"""Differential-check the static Rust client against the executable Lean oracle."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CASES = [
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


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(command: list[str], cwd: Path) -> dict:
    result = subprocess.run(command, cwd=cwd, capture_output=True, check=False, timeout=120)
    return {"command": command, "exit_status": result.returncode,
            "stdout": result.stdout.decode("utf-8", "strict").strip(),
            "stderr": result.stderr.decode("utf-8", "strict").strip()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-binary", required=True, type=Path)
    parser.add_argument("--lean-project", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    binary = args.rust_binary.resolve()
    project = args.lean_project.resolve()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    record = {"schema_version": 1, "scope": "Lean/Rust primitive success-and-refusal differential",
              "status": "failed", "rust_binary_sha256": digest(binary), "cases": []}
    try:
        for arguments, expected_status, expected_stdout in CASES:
            lean = invoke(["lake", "env", "lean", "--run", "Splicery/Oracle.lean", *arguments], project)
            rust = invoke([str(binary), "kernel", *arguments], ROOT)
            case = {"arguments": arguments, "expected": [expected_status, expected_stdout],
                    "lean": lean, "rust": rust}
            if ((lean["exit_status"], lean["stdout"]) != (expected_status, expected_stdout)
                    or (rust["exit_status"], rust["stdout"]) != (expected_status, expected_stdout)):
                record["cases"].append(case)
                raise ValueError(f"differential disagreement for {arguments}")
            record["cases"].append(case)
        record["status"] = "passed_primitive_differential_component"
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(output / "differential-receipt.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(record, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if record["status"] != "passed_primitive_differential_component":
        print(record.get("error", "differential failed"))
        return 1
    print(json.dumps({"status": record["status"], "cases": len(record["cases"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
