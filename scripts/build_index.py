"""Build a source-bound gate/scenario evidence index without inferring release status."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path

from evidence_identity import source_identity

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def relative(root: Path, value: str) -> Path:
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"evidence path escapes root: {value}")
    result = root / path
    if not result.is_file():
        raise ValueError(f"missing evidence receipt: {value}")
    return result


def write_new(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    evidence_root = args.evidence_root.resolve()
    identity = source_identity(ROOT)
    commit = identity["source_commit"]
    gates_input = json.loads((ROOT / "qualification/gate-status.json").read_text())["gates"]
    scenarios_input = json.loads((ROOT / "qualification/scenario-status.json").read_text())["scenarios"]
    gates = {}
    for gate_id, entry in gates_input.items():
        result = {"status": entry["status"]}
        if entry["status"] == "blocked":
            result["blockers"] = entry["blockers"]
        else:
            result["required_controls"] = entry.get("required_controls", [])
            result["receipts"] = [
                {"path": value, "sha256": digest(relative(evidence_root, value)),
                 "scope": "source-bound-component", "source_commit": commit}
                for value in entry.get("receipts", [])
            ]
        gates[gate_id] = result
    scenarios = {}
    for scenario_id, entry in scenarios_input.items():
        if entry["status"] == "blocked":
            scenarios[scenario_id] = {"status": "blocked", "blockers": entry["blockers"]}
            continue
        receipt_path = Path("scenario-receipts") / f"{scenario_id}.json"
        write_new(evidence_root / receipt_path, {
            "schema_version": 1,
            "gate": scenario_id,
            "status": "pass",
            "scope": "offline scenario component",
            "source_identity": identity,
            "evidence": entry["evidence"],
            "cases": entry.get("cases", []),
        })
        scenarios[scenario_id] = {
            "status": "pass",
            "receipts": [{"path": str(receipt_path), "sha256": digest(evidence_root / receipt_path),
                          "scope": "offline-scenario-component", "source_commit": commit}],
        }
    write_new(args.output, {"schema_version": 1, "source_identity": identity,
                            "gates": gates, "scenarios": scenarios})
    print(json.dumps({"status": "index_built", "gates": len(gates), "scenarios": len(scenarios)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
