"""Aggregate Splicery gate evidence without converting blocked gates into passes."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
OFFLINE_GATES = {f"SG{number:02}" for number in range(0, 8)}
HOST_GATES = {f"SG{number:02}" for number in range(8, 15)}
ALLOWED_STATUS = {"pass", "component_pass", "blocked", "not_run"}
PLACEHOLDER = re.compile(r"(?i)(sorryAx|\badmit\b|\bunsafe\b|placeholder|skipped backend)")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fail(message: str) -> None:
    raise ValueError(message)


def validate_receipt(root: Path, entry: dict, source_commit: str) -> dict:
    if not isinstance(entry, dict):
        fail("receipt entry is not an object")
    required = {"path", "sha256", "scope", "source_commit"}
    if not required <= entry.keys():
        fail("receipt is missing mandatory fields")
    if entry["source_commit"] != source_commit:
        fail("receipt source commit differs from release closure")
    path = Path(entry["path"])
    if path.is_absolute() or ".." in path.parts:
        fail("receipt path must be relative to the evidence root")
    evidence = root / path
    if not evidence.is_file() or digest(evidence) != entry["sha256"]:
        fail(f"missing or mismatched receipt: {path}")
    text = evidence.read_text(errors="replace")
    if PLACEHOLDER.search(text):
        fail(f"placeholder or forbidden evidence: {path}")
    return entry


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--allow-blocked", action="store_true")
    args = parser.parse_args()
    index_path = args.index.resolve()
    evidence_root = args.evidence_root.resolve()
    receipt_path = args.receipt.resolve()
    if receipt_path.is_relative_to(ROOT) or receipt_path.exists():
        parser.error("receipt must be fresh and outside the source checkout")
    record = {"schema_version": 1, "status": "failed", "offline_complete": False,
              "release_status": "not_qualified", "errors": []}
    try:
        index = json.loads(index_path.read_text())
        if index.get("schema_version") != 1 or not isinstance(index.get("gates"), dict):
            fail("invalid evidence index")
        source_commit = index.get("source_commit")
        if not isinstance(source_commit, str) or len(source_commit) != 40:
            fail("invalid release source commit")
        expected = {f"SG{number:02}" for number in range(15)}
        if set(index["gates"]) != expected:
            fail("evidence index must contain every SG00-SG14 gate")
        scenarios = index.get("scenarios")
        if not isinstance(scenarios, dict) or set(scenarios) != {f"C{number:02}" for number in range(1, 25)}:
            fail("evidence index must assign every C01-C24 scenario")
        offline_complete = True
        for gate_id in sorted(expected):
            gate = index["gates"][gate_id]
            if not isinstance(gate, dict) or gate.get("status") not in ALLOWED_STATUS:
                fail(f"invalid status for {gate_id}")
            status = gate["status"]
            if status == "blocked" and gate_id in OFFLINE_GATES:
                offline_complete = False
                fail(f"required offline gate is blocked: {gate_id}")
            if status == "not_run":
                if gate_id in OFFLINE_GATES:
                    offline_complete = False
                continue
            if status == "blocked":
                blockers = gate.get("blockers")
                if not isinstance(blockers, list) or not blockers or not all(
                        isinstance(item, str) and item for item in blockers):
                    fail(f"missing blockers for {gate_id}")
                continue
            receipts = gate.get("receipts")
            if not isinstance(receipts, list) or not receipts:
                fail(f"missing receipts for {gate_id}")
            paths = [entry.get("path") for entry in receipts if isinstance(entry, dict)]
            if len(paths) != len(set(paths)):
                fail(f"duplicate receipts for {gate_id}")
            for entry in receipts:
                validate_receipt(evidence_root, entry, source_commit)
        for scenario_id, scenario in scenarios.items():
            if not isinstance(scenario, dict) or not isinstance(scenario.get("status"), str):
                fail(f"invalid scenario {scenario_id}")
            if scenario["status"] == "pass" and not scenario.get("receipts"):
                fail(f"scenario {scenario_id} has no receipt")
        record["offline_complete"] = offline_complete
        host_blocked = any(index["gates"][gate]["status"] == "blocked" for gate in HOST_GATES)
        all_pass = all(index["gates"][gate]["status"] in {"pass", "component_pass"}
                       for gate in expected)
        if all_pass:
            record.update({"status": "pass", "release_status": "qualified_offline_component_only"
                           if host_blocked else "qualified"})
        elif args.allow_blocked and offline_complete:
            record.update({"status": "blocked_external_dependencies",
                           "release_status": "not_qualified"})
        else:
            record["errors"].append("release predecessors are incomplete")
        record["source_commit"] = source_commit
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        record["errors"].append(f"{type(error).__name__}: {error}")
    record["rejected"] = record["status"] != "pass"
    receipt_path.parent.mkdir(parents=True, mode=0o700, exist_ok=False)
    descriptor = os.open(receipt_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(record, stream, indent=2, sort_keys=True)
        stream.write("\n")
    print(json.dumps({"status": record["status"], "offline_complete": record["offline_complete"],
                      "release_status": record["release_status"]}, indent=2))
    return 0 if record["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
