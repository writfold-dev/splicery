"""Aggregate Splicery gate evidence without converting components into release passes."""
from __future__ import annotations

import argparse
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OFFLINE_GATES = {f"SG{number:02}" for number in range(0, 8)}
HOST_GATES = {f"SG{number:02}" for number in range(8, 15)}
ALLOWED_STATUS = {"pass", "component_pass", "blocked", "not_run"}
PLACEHOLDER = re.compile(r"(?i)(sorryAx|\badmit\b|\bunsafe\b|placeholder|skipped backend)")


def load_identity_helper():
    loader = importlib.machinery.SourceFileLoader(
        "splicery_identity", str(ROOT / "scripts/evidence_identity.py"))
    spec = importlib.util.spec_from_loader("splicery_identity", loader)
    if spec is None:
        raise ImportError("cannot load source identity helper")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fail(message: str) -> None:
    raise ValueError(message)


def control_ids(receipt: dict) -> set[str]:
    result = set()
    for key in ("mutants", "cases", "controls"):
        values = receipt.get(key, [])
        if isinstance(values, list):
            for value in values:
                if isinstance(value, dict) and isinstance(value.get("name"), str):
                    result.add(value["name"])
                elif isinstance(value, str):
                    result.add(value)
    return result


def validate_receipt(root: Path, entry: dict, gate_id: str, source_identity: dict,
                     required_controls: list[str]) -> None:
    if not isinstance(entry, dict) or not {"path", "sha256", "scope", "source_commit"} <= entry.keys():
        fail(f"invalid receipt entry for {gate_id}")
    if entry["source_commit"] != source_identity["source_commit"]:
        fail(f"stale receipt for {gate_id}")
    path = Path(entry["path"])
    if path.is_absolute() or ".." in path.parts:
        fail(f"receipt path escapes evidence root: {path}")
    evidence = root / path
    if not evidence.is_file() or digest(evidence) != entry["sha256"]:
        fail(f"missing or mismatched receipt for {gate_id}: {path}")
    if PLACEHOLDER.search(evidence.read_text(errors="replace")):
        fail(f"placeholder evidence for {gate_id}: {path}")
    receipt = json.loads(evidence.read_text())
    if not isinstance(receipt, dict) or type(receipt.get("schema_version")) is not int:
        fail(f"receipt schema is missing for {gate_id}: {path}")
    if receipt.get("gate") != gate_id:
        fail(f"receipt gate mismatch for {gate_id}: {path}")
    status = receipt.get("status")
    if status != "pass" and not (isinstance(status, str) and status.startswith("passed_")):
        fail(f"receipt is not passing for {gate_id}: {path}: {status}")
    if receipt.get("error") is not None or receipt.get("errors") not in (None, []):
        fail(f"receipt contains failures for {gate_id}: {path}")
    if receipt.get("source_identity") != source_identity:
        fail(f"receipt source/tool identity differs for {gate_id}: {path}")
    missing = set(required_controls) - control_ids(receipt)
    if missing:
        fail(f"missing controls for {gate_id}: {sorted(missing)}")


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
        identity_helper = load_identity_helper()
        source_identity = identity_helper.source_identity(ROOT)
        index = json.loads(index_path.read_text())
        if index.get("schema_version") != 1 or not isinstance(index.get("gates"), dict):
            fail("invalid evidence index")
        if index.get("source_identity") != source_identity:
            fail("index source/tool identity differs from actual checkout")
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
            if status == "blocked":
                blockers = gate.get("blockers")
                if not isinstance(blockers, list) or not blockers or not all(
                        isinstance(item, str) and item for item in blockers):
                    fail(f"missing blockers for {gate_id}")
                if gate_id in OFFLINE_GATES:
                    offline_complete = False
                continue
            if status == "not_run":
                if gate_id in OFFLINE_GATES:
                    offline_complete = False
                continue
            receipts = gate.get("receipts")
            if not isinstance(receipts, list) or not receipts:
                fail(f"missing receipts for {gate_id}")
            paths = [entry.get("path") for entry in receipts if isinstance(entry, dict)]
            if len(paths) != len(set(paths)):
                fail(f"duplicate receipts for {gate_id}")
            required_controls = gate.get("required_controls", [])
            if not isinstance(required_controls, list) or not all(
                    isinstance(item, str) and item for item in required_controls):
                fail(f"invalid required controls for {gate_id}")
            for entry in receipts:
                validate_receipt(evidence_root, entry, gate_id, source_identity, required_controls)
        scenario_blocked = False
        for scenario_id, scenario in scenarios.items():
            if not isinstance(scenario, dict):
                fail(f"invalid scenario: {scenario_id}")
            scenario_status = scenario.get("status")
            if scenario_status == "blocked":
                blockers = scenario.get("blockers")
                if not isinstance(blockers, list) or not blockers or not all(
                        isinstance(item, str) and item for item in blockers):
                    fail(f"missing blockers for scenario {scenario_id}")
                scenario_blocked = True
                continue
            if scenario_status != "pass":
                fail(f"scenario is not passing: {scenario_id}")
            receipts = scenario.get("receipts")
            if not isinstance(receipts, list) or not receipts:
                fail(f"scenario {scenario_id} has no receipts")
            for entry in receipts:
                validate_receipt(evidence_root, entry, scenario_id, source_identity, [])
        record["offline_complete"] = offline_complete
        all_pass = (all(index["gates"][gate]["status"] == "pass" for gate in expected)
                    and not scenario_blocked)
        if all_pass:
            record.update({"status": "pass", "release_status": "qualified"})
        elif args.allow_blocked and offline_complete:
            record.update({"status": "blocked_external_dependencies", "release_status": "not_qualified"})
        else:
            record["errors"].append("release predecessors are incomplete")
        record["source_identity"] = source_identity
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
