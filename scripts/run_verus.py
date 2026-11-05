"""Verify the actual Splicery apply_at body with pinned Verus and one mutant."""
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


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources(root: Path) -> dict[str, str]:
    files = [root / "Cargo.toml", root / "Cargo.lock", root / "rust-toolchain.toml"]
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
    record = {"schema_version": 1, "scope": "actual apply_at Verus spine only",
              "status": "failed", "sources": sources(ROOT)}
    try:
        version = subprocess.run(["verus", "--version"], capture_output=True, check=False, timeout=60)
        record["verus_version"] = version.stdout.decode("utf-8", "strict").strip()
        if version.returncode or "0.2026.09.06.8dea4a2" not in record["verus_version"]:
            raise ValueError("wrong Verus version")
        control = output / "control"
        mutant = output / "mutant"
        copy_workspace(control)
        copy_workspace(mutant)
        positive = run(output, "control", control, output / "target-control")
        record["control"] = positive
        if positive["exit_status"] != 0:
            raise ValueError("positive Verus process failed")
        apply_name = "splicery_kernel::apply_at"
        successful = [report for report in positive["reports"]
                      if report["verification"].get("success") is True
                      and report["verification"].get("errors") == 0
                      and report["verification"].get("verified", 0) > 0
                      and report["verification"].get("is-verifying-entire-crate") is True
                      and report["functions"].get(apply_name) is True]
        if len(successful) != 1:
            raise ValueError("positive Verus run did not emit exactly one apply_at crate result")
        result = successful[0]["verification"]
        record["verified"] = result["verified"]
        if positive["functions"].get(apply_name) is not True:
            raise ValueError("actual apply_at solver success is missing")
        source = mutant / "crates/splicery-kernel/src/lib.rs"
        text = source.read_text()
        good = "output.extend_from_slice(replacement);"
        broken = "output.extend_from_slice(&input[..lo]);"
        if text.count(good) != 1:
            raise ValueError("mutant did not select the exact payload copy")
        source.write_text(text.replace(good, broken))
        negative = run(output, "mutant", mutant, output / "target-mutant")
        record["mutant"] = negative
        if negative["exit_status"] == 0 or negative["functions"].get(apply_name) is not False:
            raise ValueError("payload mutant did not fail apply_at verification")
        record["status"] = "passed_verus_apply_at_component"
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(output / "verus-receipt.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(record, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if record["status"] != "passed_verus_apply_at_component":
        print(record.get("error", "Verus failed"))
        return 1
    print(json.dumps({"status": record["status"], "verified": record["verified"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
