"""Verify the shipping Splicery batch bodies and calibrate production mutants."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "splicery_kernel::apply_at",
    "splicery_kernel::spec::replacement_len",
    "splicery_kernel::spec::candidate_len",
    "splicery_kernel::spec::materialize",
    "splicery_kernel::spec::emit_gaps",
)
MUTANTS = {
    "payload-copy": (
        "output.extend_from_slice(replacement);",
        "output.extend_from_slice(&input[..lo]);",
        "splicery_kernel::apply_at",
        Path("crates/splicery-kernel/src/lib.rs"),
    ),
    "unchecked-length": (
        "let next = match length.checked_add(amount) {",
        "let next = match Some(length + amount) {",
        "splicery_kernel::spec::replacement_len",
        Path("crates/splicery-kernel/src/spec.rs"),
    ),
    "coordinate-order": (
        "bytes.extend_from_slice(&input[cursor..splice.target.lo()]);",
        "bytes.extend_from_slice(&input[splice.target.lo()..cursor]);",
        "splicery_kernel::spec::emit_gaps",
        Path("crates/splicery-kernel/src/spec.rs"),
    ),
    "sequential-swap": (
        "let splice = &splices[i];\n        let replacement = materialize(input, splice)?;",
        "let splice = &splices[i];\n        let replacement = materialize(&bytes, splice)?;",
        "splicery_kernel::spec::emit_gaps",
        Path("crates/splicery-kernel/src/spec.rs"),
    ),
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources(root: Path) -> dict[str, str]:
    files = [root / "Cargo.toml", root / "Cargo.lock", root / "rust-toolchain.toml"]
    files += [root / "scripts/run_verus.py", root / "qualification/verus-trusted-base.json"]
    files += sorted((root / "crates").glob("*/src/**/*.rs"))
    return {str(path.relative_to(root)): digest(path) for path in files}


def copy_workspace(destination: Path) -> None:
    destination.mkdir(parents=True)
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        shutil.copyfile(ROOT / name, destination / name)
    shutil.copytree(ROOT / "crates", destination / "crates")


def run(output: Path, name: str, workspace: Path, target: Path) -> dict:
    environment = dict(os.environ)
    environment.update({"CARGO_TARGET_DIR": str(target), "CARGO_NET_OFFLINE": "true"})
    command = ["cargo", "verus", "verify", "-p", "splicery-kernel", "--locked", "--offline",
               "--message-format=json", "--", "--output-json", "--time-expanded"]
    result = subprocess.run(command, cwd=workspace, env=environment, capture_output=True,
                            check=False, timeout=1800)
    log = result.stdout + result.stderr
    (output / f"{name}.log").write_bytes(log)
    text = log.decode("utf-8", "replace")
    records = []
    decoder = json.JSONDecoder()
    position = 0
    for match in re.finditer(r"(?m)^\s*\{", text):
        if match.start() < position:
            continue
        start = text.index("{", match.start())
        try:
            value, position = decoder.raw_decode(text, start)
        except json.JSONDecodeError as error:
            raise ValueError("truncated or malformed Verus JSON record") from error
        if isinstance(value, dict):
            records.append(value)
    functions = {}
    reports = []
    for record in records:
        local_functions = {}
        for module in record.get("times-ms", {}).get("smt", {}).get("smt-run-module-times", []):
            for entry in module.get("function-breakdown", []):
                name = entry.get("function")
                if isinstance(name, str) and type(entry.get("success")) is bool:
                    local_functions[name] = local_functions.get(name, True) and entry["success"]
                    functions[name] = functions.get(name, True) and entry["success"]
        if isinstance(record.get("verification-results"), dict):
            reports.append({"verification": record["verification-results"],
                            "functions": local_functions})
    verification = [report["verification"] for report in reports]
    return {"command": command, "exit_status": result.returncode,
            "verification": verification, "functions": functions, "reports": reports}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o755, exist_ok=False)
    record = {"schema_version": 2, "scope": "actual normalized-batch Verus production bodies",
              "status": "failed", "sources": sources(ROOT)}
    try:
        trusted_path = ROOT / "qualification/verus-trusted-base.json"
        trusted = json.loads(trusted_path.read_text(encoding="utf-8"))
        if set(trusted.get("verified_functions", {})) != set(REQUIRED):
            raise ValueError("trusted-base verified-function inventory does not match required bodies")
        if trusted.get("hidden_trusted_extensions") != []:
            raise ValueError("trusted-base inventory declares hidden extensions")
        kernel_text = "\n".join(
            path.read_text(encoding="utf-8")
            for path in sorted((ROOT / "crates/splicery-kernel/src").glob("*.rs"))
        )
        forbidden = ("#[verifier::assume]", "assume_specification", "assume(", "admit(", "axiom")
        present = [token for token in forbidden if token in kernel_text]
        declarations = {
            "#[verifier::external_type_specification]": 5,
            "#[verifier::external_fn_specification]": 6,
            "pub uninterp spec fn": 4,
        }
        present += [f"unexpected {token} count" for token, count in declarations.items()
                    if kernel_text.count(token) != count]
        opaque_span = ("#[verifier::external_type_specification] #[verifier::external_body]\n"
                       "pub struct ExSourceSpan(SourceSpan);")
        opaque_target = ("#[verifier::external_type_specification] #[verifier::external_body]\n"
                         "pub struct ExEditTarget(EditTarget);")
        if (kernel_text.count(opaque_span) != 1 or kernel_text.count(opaque_target) != 1
                or kernel_text.count("#[verifier::external_body]") != 2):
            present.append("external_body outside the inventoried opaque SourceSpan/EditTarget")
        if present:
            raise ValueError(f"uninventoried trusted kernel extensions: {present}")
        record["trusted_base_sha256"] = digest(trusted_path)
        shutil.copyfile(trusted_path, output / "verus-trusted-base.json")
        version = subprocess.run(["verus", "--version"], capture_output=True, check=False, timeout=60)
        record["verus_version"] = version.stdout.decode("utf-8", "strict").strip()
        if version.returncode or "0.2026.09.06.8dea4a2" not in record["verus_version"]:
            raise ValueError("wrong Verus version")
        control = output / "control"
        copy_workspace(control)
        positive = run(output, "control", control, output / "target-control")
        record["control"] = positive
        if positive["exit_status"] != 0:
            raise ValueError("positive Verus process failed")
        successful = [report for report in positive["reports"]
                      if report["verification"].get("success") is True
                      and report["verification"].get("errors") == 0
                      and report["verification"].get("verified", 0) >= len(REQUIRED)
                      and report["verification"].get("is-verifying-entire-crate") is True
                      and all(report["functions"].get(name) is True for name in REQUIRED)]
        if len(successful) != 1:
            raise ValueError("positive run lacks one report containing every required production body")
        result = successful[0]["verification"]
        record["verified"] = result["verified"]
        record["verified_functions"] = {name: positive["functions"].get(name) for name in REQUIRED}
        if any(record["verified_functions"][name] is not True for name in REQUIRED):
            raise ValueError("a required production body has no solver success")
        record["mutants"] = {}
        for mutant_name, (good, broken, target_function, mutant_source) in MUTANTS.items():
            if kernel_text.count(good) != 1:
                raise ValueError(f"{mutant_name} did not select exactly one production statement")
            workspace = output / f"mutant-{mutant_name}"
            copy_workspace(workspace)
            source = workspace / mutant_source
            source.write_text(source.read_text(encoding="utf-8").replace(good, broken), encoding="utf-8")
            negative = run(output, f"mutant-{mutant_name}", workspace,
                           output / f"target-mutant-{mutant_name}")
            record["mutants"][mutant_name] = negative
            failed = [report for report in negative["reports"]
                      if report["verification"].get("success") is False
                      and report["verification"].get("errors", 0) > 0
                      and report["verification"].get("encountered-vir-error") is False
                      and report["verification"].get("is-verifying-entire-crate") is True
                      and report["functions"].get(target_function) is False]
            if negative["exit_status"] == 0 or len(failed) != 1:
                raise ValueError(f"{mutant_name} did not fail {target_function} as intended")
        record["status"] = "passed_verus_normalized_batch_component"
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(output / "verus-receipt.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(record, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if record["status"] != "passed_verus_normalized_batch_component":
        print(record.get("error", "Verus failed"))
        return 1
    print(json.dumps({"status": record["status"], "verified": record["verified"],
                      "functions": list(REQUIRED)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
