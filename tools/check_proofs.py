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
REQUIRED_CONTROLS = {
    "mutant-after-to-before": "Splicery.lower_correct",
    "mutant-sequential-modified-capture": "Splicery.Surface.readCapture_original",
    "mutant-accept-reversed-range": "Splicery.cutRange_sound",
    "mutant-drop-missing-optional-group": "Splicery.Surface.resolveRef_unmatched_group",
    "mutant-normalize-crlf": "Splicery.Surface.indent_preserves_recorded_eols",
    "mutant-ignore-batch-conflict": "Splicery.canonical_rejects_incompatible",
    "mutant-collide-read-edit-kinds": "Splicery.read_stamp_is_not_edit_stamp",
    "mutant-reuse-alias-across-runs": "Splicery.foreign_run_cannot_prepare",
    "mutant-omit-prepared-input-field": "Splicery.stamp_binds_input",
    "mutant-accept-stale-unsupported-profile": "Splicery.stale_or_unsupported_profile_cannot_prepare",
    "mutant-ignore-write-grant": "Splicery.read_only_grant_cannot_authorize",
}


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


def validated_audit_map(project: Path, symbols: list[str]) -> tuple[dict, dict[str, str]]:
    path = project / "audit-map.json"
    if not path.is_file():
        raise ValueError("missing formal/audit-map.json")
    try:
        audit = json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid audit map JSON: {error}") from error
    if audit.get("schema_version") != 1:
        raise ValueError("unsupported audit map schema")
    obligations = audit.get("obligations")
    if not isinstance(obligations, list):
        raise ValueError("audit map obligations must be a list")
    expected_ids = [f"L{number:02}" for number in range(1, 10)]
    if [entry.get("id") for entry in obligations if isinstance(entry, dict)] != expected_ids:
        raise ValueError("audit map must order exactly L01 through L09")
    declared: list[str] = []
    mapped_controls: list[str] = []
    required_fields = {"id", "property", "lean_declarations", "production_rust_symbols",
                       "positive_examples", "negative_examples", "mutant_ids"}
    for entry in obligations:
        if not isinstance(entry, dict) or not required_fields <= entry.keys():
            raise ValueError("incomplete obligation in audit map")
        for field in ("lean_declarations", "production_rust_symbols", "positive_examples",
                      "negative_examples", "mutant_ids"):
            if not isinstance(entry[field], list) or not all(
                    isinstance(item, str) and item for item in entry[field]):
                raise ValueError(f"{entry['id']} {field} must be a list of nonempty strings")
        for field in ("lean_declarations", "positive_examples", "negative_examples"):
            if not entry[field]:
                raise ValueError(f"{entry['id']} has empty {field}")
        declared.extend(entry["lean_declarations"])
        mapped_controls.extend(entry["mutant_ids"])
    if len(declared) != len(set(declared)):
        raise ValueError("public declaration mapped to more than one obligation")
    if set(declared) != set(symbols):
        raise ValueError("audit map does not partition the complete public theorem inventory")
    controls = audit.get("controls")
    if not isinstance(controls, dict) or set(controls) != set(REQUIRED_CONTROLS):
        raise ValueError("audit map must name every required control and no unexecuted controls")
    if (len(mapped_controls) != len(set(mapped_controls)) or
            set(mapped_controls) != set(REQUIRED_CONTROLS)):
        raise ValueError("obligation mutant IDs must partition exactly the required controls")
    control_targets: dict[str, str] = {}
    for name, target in REQUIRED_CONTROLS.items():
        entry = controls[name]
        if not isinstance(entry, dict) or entry.get("target") != target:
            raise ValueError(f"audit map has wrong target for {name}")
        if name not in mapped_controls:
            raise ValueError(f"{name} is not mapped to an obligation")
        control_targets[name] = target
    for field in ("trusted_assumptions", "excluded_host_claims"):
        if not isinstance(audit.get(field), list) or not audit[field] or not all(
                isinstance(item, str) and item for item in audit[field]):
            raise ValueError(f"audit map requires nonempty {field}")
    return audit, control_targets


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


def source_line(text: str, marker: str) -> int:
    if text.splitlines().count(marker) != 1:
        raise ValueError(f"calibration marker is not unique: {marker!r}")
    return text.splitlines().index(marker) + 1


def error_locations(output: str, filename: str) -> list[int]:
    locations = []
    for line in output.splitlines():
        if "error:" not in line:
            continue
        match = re.search(rf"{re.escape(filename)}:(\d+):\d+:", line)
        if match:
            locations.append(int(match.group(1)))
    return locations


def require_intended_failure(record: dict, output: Path, control: Path, mutant: Path,
                             module: str, name: str, good: str, broken: str,
                             mutation_name: str, allowed_late: tuple[int, ...] = ()) -> None:
    source = mutant / module
    original = source.read_text()
    if original.count(good) != 1:
        raise ValueError(f"{mutation_name} did not select exactly one source expression")
    source.write_text(original.replace(good, broken))
    line = theorem_line(source, name)
    later_theorems = [match.start() for match in THEOREM.finditer(source.read_text())
                      if source.read_text()[:match.start()].count("\n") + 1 > line]
    end = (source.read_text()[:min(later_theorems)].count("\n") + 1
           if later_theorems else len(source.read_text().splitlines()) + 1)
    result = run(output, mutation_name, ["lake", "build"], mutant)
    diagnostics = error_locations(result["output"], Path(module).name)
    if (result["exit_status"] == 0 or not diagnostics or
            any(value < line or (value >= end and value not in allowed_late)
                for value in diagnostics)):
        raise ValueError(f"{mutation_name} did not fail only at {name}")
    record.setdefault("mutants", []).append({
        "name": mutation_name,
        "target": f"{NAMESPACE.search(lean_code(original)).group(1)}.{name}",
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
        "target": "Splicery.canonical_rejects_incompatible",
        "diagnostic_lines": diagnostics,
        "source_sha256": digest(source),
    })


def calibrated_standalone_mutant(record: dict, output: Path, control: Path,
                                 mutation_name: str, target: str, filename: str,
                                 source_text: str, diagnostic_line: int) -> None:
    source = control / filename
    source.write_text(source_text)
    result = run(output, mutation_name, ["lake", "env", "lean", str(source)], control)
    diagnostics = error_locations(result["output"], filename)
    if result["exit_status"] == 0 or diagnostics != [diagnostic_line]:
        raise ValueError(f"{mutation_name} did not fail only at its calibrated {target} example")
    record.setdefault("mutants", []).append({
        "name": mutation_name,
        "target": target,
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
        audit_map, control_targets = validated_audit_map(control, symbols)
        record["audit_map"] = {
            "path": "formal/audit-map.json",
            "sha256": digest(control / "audit-map.json"),
            "obligations": [entry["id"] for entry in audit_map["obligations"]],
        }
        record["required_controls"] = control_targets
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
            "cut xs lo (hi - lo)", "mutant-accept-reversed-range", allowed_late=(220, 362),
        )
        batch_conflict_mutant(record, destination, control)
        sequential_capture = """import Splicery.Surface

namespace Splicery.Surface.Mutants

def mutantOracle : TextOracle where
  UTF8 := fun _ => True
  Boundary := fun _ _ => True
  valid := fun _ => true
  boundary := fun _ _ => true
  valid_sound := by intros; trivial
  boundary_sound := by intros; trivial

def capture : Capture := { region := ⟨7, 0, 1, true, true⟩ }
def original : Image := ⟨7, [97], true, true⟩
def modified : Image := ⟨7, [98], true, true⟩

def sequentialReadModifiedCapture : Option Bytes :=
  let _before := readCapture original .raw mutantOracle capture
  readCapture modified .raw mutantOracle capture

example : sequentialReadModifiedCapture = some [97] := by
  decide

end Splicery.Surface.Mutants
"""
        calibrated_standalone_mutant(
            record, destination, control, "mutant-sequential-modified-capture",
            "Splicery.Surface.readCapture_original", "SequentialCaptureMutant.lean",
            sequential_capture, source_line(sequential_capture, "  decide"),
        )
        missing_group = """import Splicery.Surface

namespace Splicery.Surface.Mutants

def parentCapture : Capture :=
  { region := ⟨7, 0, 1, true, true⟩, groups := [⟨[2], none⟩] }
def lexicalEnv : Env := [([1], parentCapture)]

def resolveDroppingMissing : Option Capture :=
  match lookup lexicalEnv [1] with
  | none => none
  | some c =>
      match findGroup c.groups [2] with
      | none => none
      | some _ => some c

example : resolveDroppingMissing = none := by
  decide

end Splicery.Surface.Mutants
"""
        calibrated_standalone_mutant(
            record, destination, control, "mutant-drop-missing-optional-group",
            "Splicery.Surface.resolveRef_unmatched_group", "MissingGroupMutant.lean",
            missing_group, source_line(missing_group, "  decide"),
        )
        normalize_crlf = """import Splicery.Surface

namespace Splicery.Surface.Mutants

def normalizeCRLF : Bytes → Bytes
  | [] => []
  | 13 :: 10 :: rest => 10 :: normalizeCRLF rest
  | byte :: rest => byte :: normalizeCRLF rest

example : normalizeCRLF [97, 13, 10, 98] = [97, 13, 10, 98] := by
  decide

end Splicery.Surface.Mutants
"""
        calibrated_standalone_mutant(
            record, destination, control, "mutant-normalize-crlf",
            "Splicery.Surface.indent_preserves_recorded_eols", "CrlfMutant.lean",
            normalize_crlf, source_line(normalize_crlf, "  decide"),
        )
        collide_kinds = """import Splicery.Authority

namespace Splicery.Mutants

def collideKinds (encoder : CanonicalEncoder) (_domain : IdentityDomain)
    (input : PreparedInput) : PreparedIdentity :=
  ⟨.readCapture, encoder.encode input⟩

example (encoder : CanonicalEncoder) (input : PreparedInput) :
    collideKinds encoder .readCapture input ≠ collideKinds encoder .editPreparation input := by
  intro collided
  injection collided with kind _
  contradiction

end Splicery.Mutants
"""
        calibrated_standalone_mutant(
            record, destination, control, "mutant-collide-read-edit-kinds",
            "Splicery.read_stamp_is_not_edit_stamp", "CollideKindsMutant.lean",
            collide_kinds, source_line(collide_kinds, "  contradiction"),
        )
        omitted_field = """import Splicery.Authority

namespace Splicery.Mutants

def stampWithoutCandidate (encoder : CanonicalEncoder) (domain : IdentityDomain)
    (input : PreparedInput) : PreparedIdentity :=
  ⟨domain, encoder.encode { input with candidate := [] }⟩

example (encoder : CanonicalEncoder) (left right : PreparedInput)
    (h : stampWithoutCandidate encoder .editPreparation left =
      stampWithoutCandidate encoder .editPreparation right) : left = right := by
  apply encoder.encode_injective
  injection h with _ encoding

end Splicery.Mutants
"""
        calibrated_standalone_mutant(
            record, destination, control, "mutant-omit-prepared-input-field",
            "Splicery.stamp_binds_input", "OmittedFieldMutant.lean", omitted_field,
            source_line(omitted_field, "example (encoder : CanonicalEncoder) (left right : PreparedInput)") + 2,
        )
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Authority.lean", "foreign_run_cannot_prepare",
            "&&\n    preparationScopeOK policy input", "&&\n    true",
            "mutant-reuse-alias-across-runs",
        )
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Authority.lean",
            "stale_or_unsupported_profile_cannot_prepare",
            "authorizes grant input && preparationProfileOK policy input &&",
            "authorizes grant input && true &&", "mutant-accept-stale-unsupported-profile",
        )
        require_intended_failure(
            record, destination, control, mutant, "Splicery/Authority.lean", "read_only_grant_cannot_authorize",
            "grant.write && grant.scope = input.ir.image.scope &&",
            "true && grant.scope = input.ir.image.scope &&", "mutant-ignore-write-grant",
            allowed_late=(232,),
        )
        observed_controls = {entry["name"]: entry["target"] for entry in record["mutants"]}
        if observed_controls != control_targets:
            raise ValueError("executed controls disagree with the required audit-map controls")
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
