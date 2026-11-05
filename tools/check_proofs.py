"""Clean Splicery Lean proof/oracle audit; run inside the admitted tool image."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PERMITTED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}
FORBIDDEN = re.compile(r"\b(sorry|admit|native_decide)\b|^\s*(?:axiom|unsafe|opaque)\b", re.MULTILINE)
THEOREM = re.compile(r"^theorem\s+([A-Za-z_][A-Za-z0-9_'.]*)", re.MULTILINE)
NAMESPACE = re.compile(r"^namespace\s+([A-Za-z_][A-Za-z0-9_.']*)", re.MULTILINE)
END_NAMESPACE = re.compile(r"^end\s+([A-Za-z_][A-Za-z0-9_.']*)", re.MULTILINE)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(output: Path, name: str, args: list[str], cwd: Path, timeout: int = 600) -> dict:
    result = subprocess.run(
        args, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        check=False, timeout=timeout,
    )
    (output / f"{name}.log").write_bytes(result.stdout)
    return {
        "command": args,
        "exit_status": result.returncode,
        "output": result.stdout.decode("utf-8", "replace"),
    }


def checked(output: Path, name: str, args: list[str], cwd: Path) -> dict:
    result = run(output, name, args, cwd)
    if result["exit_status"]:
        raise ValueError(f"{name} failed; inspect retained log")
    return result


def lean_code(text: str) -> str:
    """Remove comments/string contents while preserving declaration coordinates."""
    output: list[str] = []
    index = 0
    depth = 0
    string = False
    while index < len(text):
        pair = text[index:index + 2]
        char = text[index]
        if depth:
            if pair == "/-":
                depth += 1
                output.extend("  ")
                index += 2
            elif pair == "-/":
                depth -= 1
                output.extend("  ")
                index += 2
            else:
                output.append("\n" if char == "\n" else " ")
                index += 1
        elif string:
            if char == "\\" and index + 1 < len(text):
                output.extend("  ")
                index += 2
            else:
                string = char != '"'
                output.append("\n" if char == "\n" else " ")
                index += 1
        elif pair == "--":
            end = text.find("\n", index)
            if end < 0:
                output.extend(" " * (len(text) - index))
                break
            output.extend(" " * (end - index))
            index = end
        elif pair == "/-":
            depth = 1
            output.extend("  ")
            index += 2
        elif char == '"':
            string = True
            output.append(" ")
            index += 1
        else:
            output.append(char)
            index += 1
    if depth or string:
        raise ValueError("unterminated Lean comment or string")
    return "".join(output)


def public_theorems(project: Path) -> list[str]:
    names: list[str] = []
    for source in sorted(project.glob("Splicery/**/*.lean")):
        namespaces: list[str] = []
        for line in lean_code(source.read_text()).splitlines():
            opened = NAMESPACE.match(line)
            if opened:
                namespaces.append(opened.group(1))
                continue
            closed = END_NAMESPACE.match(line)
            if closed and namespaces and namespaces[-1] == closed.group(1):
                namespaces.pop()
                continue
            theorem = THEOREM.match(line)
            if theorem:
                prefix = ".".join(namespaces)
                name = theorem.group(1)
                names.append(f"{prefix}.{name}" if prefix else name)
    if len(names) != len(set(names)):
        raise ValueError("duplicate public theorem names")
    if not names:
        raise ValueError("no public theorem declarations found")
    return sorted(names)


def copy_project(source: Path, destination: Path) -> None:
    for item in sorted(source.rglob("*")):
        if not item.is_file() or ".lake" in item.parts or item.name == "lake-manifest.json":
            continue
        target = destination / item.relative_to(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(item, target)


def theorem_line(source: Path, name: str) -> int:
    text = source.read_text()
    return text[:text.index(f"theorem {name}")].count("\n") + 1


def require_intended_failure(record: dict, output: Path, control: Path, mutant: Path,
                             module: str, name: str, good: str, broken: str,
                             mutation_name: str) -> None:
    source = mutant / module
    original = source.read_text()
    if original.count(good) != 1:
        raise ValueError(f"{mutation_name} did not select exactly one source expression")
    source.write_text(original.replace(good, broken))
    line = theorem_line(source, name)
    result = run(output, mutation_name, ["lake", "build"], mutant)
    diagnostics = [int(value) for value in re.findall(
        rf"(?:error:\s*)?{re.escape(Path(module).name)}:(\d+):\d+:", result["output"],
    )]
    if result["exit_status"] == 0 or not diagnostics or any(value < line for value in diagnostics):
        raise ValueError(f"{mutation_name} did not fail at {name}")
    record.setdefault("mutants", []).append({
        "name": mutation_name,
        "target": name,
        "diagnostic_lines": diagnostics,
        "source_sha256": digest(source),
    })
    shutil.copyfile(control / module, source)


def batch_conflict_mutant(record: dict, output: Path, control: Path) -> None:
    source = control / "BatchConflictMutant.lean"
    source.write_text("""import Splicery.Batch
import Lean.Elab.Tactic.Omega

namespace Splicery

/-- Deliberately drops destination-conflict checks for oracle calibration. -/
def buildWithoutConflicts (suffix : Bytes) (offset : Nat) :
    List AuthoredSplice → Option Batch
  | [] => some (.tail suffix)
  | a :: rest =>
      if offset ≤ a.start then
        match cut suffix (a.start - offset) a.width with
        | none => none
        | some c =>
            match buildWithoutConflicts c.post a.stop rest with
            | none => none
            | some more => some (.step c.pre c.selected a.replacement more)
      else none

def normalizeWithoutConflicts (input : Bytes) (edits : List AuthoredSplice) : Option Batch :=
  buildWithoutConflicts input 0 (sortAuthored edits)

-- This expected refusal must fail only because coincident insertions were admitted.
example : normalizeWithoutConflicts [0, 1] [⟨1, 0, [9]⟩, ⟨1, 0, [8]⟩] = none := by
  decide

end Splicery
""")
    target_line = source.read_text().splitlines().index(
        "example : normalizeWithoutConflicts [0, 1] [⟨1, 0, [9]⟩, ⟨1, 0, [8]⟩] = none := by"
    ) + 2
    result = run(output, "mutant-ignore-batch-conflict", ["lake", "env", "lean", str(source)], control)
    diagnostics = [int(value) for value in re.findall(
        r"BatchConflictMutant\.lean:(\d+):\d+:", result["output"]
    )]
    if result["exit_status"] == 0 or diagnostics != [target_line]:
        raise ValueError("batch conflict mutant did not fail only at the coincident-insertion oracle")
    record.setdefault("mutants", []).append({
        "name": "mutant-ignore-batch-conflict",
        "target": "coincident-insertion refusal oracle",
        "diagnostic_lines": diagnostics,
        "source_sha256": digest(source),
    })


def oracle(output: Path, project: Path) -> list[dict]:
    cases = [
        (["apply", "1", "1", "616263", "5858"], 0, "61585863"),
        (["apply", "0", "0", "", "00"], 0, "00"),
        (["apply", "3", "0", "616263", ""], 0, "616263"),
        (["apply", "4", "0", "616263", ""], 1, "REFUSAL"),
        (["apply", "2", "2", "616263", ""], 1, "REFUSAL"),
        (["apply", "1", "1", "61626", "58"], 1, "REFUSAL"),
        (["apply", "1", "1", "616263", "5"], 1, "REFUSAL"),
        (["apply", "-1", "0", "616263", ""], 1, "REFUSAL"),
        (["apply", "0", "0", "00" * 4097, ""], 1, "REFUSAL"),
    ]
    results = []
    for index, (arguments, status, stdout) in enumerate(cases):
        result = run(output, f"oracle-{index:02}", [
            "lake", "env", "lean", "--run", "Splicery/Oracle.lean", *arguments,
        ], project)
        observed = result["output"].strip()
        if result["exit_status"] != status or observed != stdout:
            raise ValueError(f"oracle case {index} disagreed: {result['exit_status']} {observed!r}")
        results.append({"arguments": arguments, "exit_status": status, "stdout": stdout})
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    destination = args.evidence.expanduser().resolve()
    if destination.is_relative_to(ROOT) or destination.exists():
        parser.error("use a fresh evidence directory outside the source checkout")
    destination.mkdir(parents=True, exist_ok=False)
    record: dict = {"schema_version": 1, "scope": "Splicery Lean calculus and bounded primitive oracle",
                    "status": "failed", "mutants": []}
    try:
        lean = shutil.which("lean")
        lake = shutil.which("lake")
        if not lean or not lake:
            raise ValueError("missing admitted Lean/Lake")
        version = checked(destination, "lean-version", [lean, "--version"], destination)["output"]
        if not re.search(r"\b4\.21\.0\b", version):
            raise ValueError("wrong Lean version")
        record["version"] = version.strip()
        formal = ROOT / "formal"
        record["sources"] = {
            str(path.relative_to(ROOT)): digest(path)
            for path in sorted(formal.rglob("*"))
            if path.is_file() and ".lake" not in path.parts and path.name != "lake-manifest.json"
        }
        for path in formal.glob("Splicery/**/*.lean"):
            escaped = FORBIDDEN.search(lean_code(path.read_text()))
            if escaped:
                raise ValueError(f"forbidden proof escape {escaped.group()!r} in {path}")
        control = destination / "control"
        mutant = destination / "mutant"
        copy_project(formal, control)
        copy_project(formal, mutant)
        symbols = public_theorems(control)
        record["symbols"] = symbols
        checked(destination, "control-build", [lake, "build"], control)
        audit = control / "AxiomAudit.lean"
        audit.write_text("import Splicery\n" + "".join(
            f"#print axioms {symbol}\n" for symbol in symbols
        ))
        result = checked(destination, "control-axioms", [lake, "env", "lean", str(audit)], control)
        for symbol in symbols:
            match = re.search(
                rf"'{re.escape(symbol)}' (?:does not depend on any axioms|depends on axioms:\s*\[([^\]]*)\])",
                result["output"],
            )
            if not match:
                raise ValueError(f"missing axiom report for {symbol}")
            dependencies = {item.strip() for item in (match.group(1) or "").split(",") if item.strip()}
            if not dependencies <= PERMITTED_AXIOMS:
                raise ValueError(f"unapproved axiom for {symbol}: {sorted(dependencies)}")
            record.setdefault("observed_axioms", {})[symbol] = sorted(dependencies)
        record["oracle"] = oracle(destination, control)
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Lowering.lean", "lower_correct",
            "| .after ps => ⟨(c.pre ++ c.selected).length, 0, render ps⟩",
            "| .after ps => ⟨c.pre.length, 0, render ps⟩", "mutant-after-to-before",
        )
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Core.lean", "cutRange_sound",
            "if hi < lo then none else cut xs lo (hi - lo)",
            "cut xs lo (hi - lo)", "mutant-accept-reversed-range",
        )
        batch_conflict_mutant(record, destination, control)
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Authority.lean", "read_only_grant_cannot_authorize",
            "grant.write && grant.scope = input.ir.image.scope &&",
            "true && grant.scope = input.ir.image.scope &&", "mutant-ignore-write-grant",
        )
        final_sources = {
            str(path.relative_to(ROOT)): digest(path)
            for path in sorted(formal.rglob("*"))
            if path.is_file() and ".lake" not in path.parts and path.name != "lake-manifest.json"
        }
        if record["sources"] != final_sources:
            record["source_drift"] = {
                relative: {"before": record["sources"].get(relative), "after": final_sources.get(relative)}
                for relative in sorted(set(record["sources"]) | set(final_sources))
                if record["sources"].get(relative) != final_sources.get(relative)
            }
            raise ValueError("proof sources changed during qualification")
        record["status"] = "passed_calculus_component"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        (destination / "proof-receipt.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    if record["status"] != "passed_calculus_component":
        raise SystemExit(record.get("error", "proof audit failed"))
    print(json.dumps({"status": record["status"], "theorems": len(record["symbols"]),
                      "oracle_cases": len(record["oracle"]), "mutants": len(record["mutants"])}, indent=2))


if __name__ == "__main__":
    main()
