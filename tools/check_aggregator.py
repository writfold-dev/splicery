"""Exercise strict release aggregation with synthetic, non-product receipts."""
from __future__ import annotations

import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def identity() -> dict:
    loader = importlib.machinery.SourceFileLoader(
        "splicery_identity", str(ROOT / "scripts/evidence_identity.py"))
    spec = importlib.util.spec_from_loader("splicery_identity", loader)
    if spec is None:
        raise ImportError("cannot load identity helper")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module.source_identity(ROOT)


def invoke(index: Path, evidence: Path, receipt: Path) -> dict:
    receipt = receipt.parent / f"attempt-{receipt.stem}" / "receipt.json"
    result = subprocess.run([
        sys.executable, "-B", str(ROOT / "scripts/aggregate.py"),
        "--index", str(index), "--evidence-root", str(evidence),
        "--receipt", str(receipt), "--allow-blocked",
    ], capture_output=True, check=False, timeout=120)
    payload = json.loads(receipt.read_text()) if receipt.exists() else {}
    return {"exit_status": result.returncode, "receipt": payload,
            "stderr": result.stderr.decode("utf-8", "replace")}


def write_receipt(evidence: Path, gate: str, status: str, identity_value: dict) -> dict:
    path = evidence / f"{gate}.json"
    document = {
        "schema_version": 1,
        "gate": gate,
        "status": status,
        "source_identity": identity_value,
        "cases": [{"name": f"{gate}-control"}],
    }
    path.write_text(json.dumps(document))
    return {"path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "scope": "synthetic-control", "source_commit": identity_value["source_commit"]}


def make_index(evidence: Path, identity_value: dict) -> dict:
    return {
        "schema_version": 1,
        "source_identity": identity_value,
        "gates": {
            f"SG{number:02}": {
                "status": "pass",
                "receipts": [write_receipt(evidence, f"SG{number:02}", "pass", identity_value)],
                "required_controls": [f"SG{number:02}-control"],
            }
            for number in range(15)
        },
        "scenarios": {
            f"C{number:02}": {
                "status": "pass",
                "receipts": [write_receipt(evidence, f"C{number:02}", "pass", identity_value)],
            }
            for number in range(1, 25)
        },
    }


def main() -> int:
    evidence = Path(sys.argv[1]).resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    identity_value = identity()
    index = make_index(evidence, identity_value)
    index_path = evidence / "index.json"
    index_path.write_text(json.dumps(index))
    results = []
    valid = invoke(index_path, evidence, evidence / "valid-receipt.json")
    if valid["exit_status"] != 0 or valid["receipt"].get("status") != "pass":
        raise ValueError(f"valid synthetic aggregation failed: {valid}")
    results.append({"name": "valid", **valid})

    blocked = make_index(evidence, identity_value)
    for number in range(8, 15):
        gate = blocked["gates"][f"SG{number:02}"]
        gate["status"] = "blocked"
        gate["receipts"] = []
        gate["blockers"] = ["synthetic-owner-unavailable"]
    blocked_path = evidence / "blocked.json"
    blocked_path.write_text(json.dumps(blocked))
    observed = invoke(blocked_path, evidence, evidence / "blocked-receipt.json")
    if observed["exit_status"] == 0 or observed["receipt"].get("status") != "blocked_external_dependencies":
        raise ValueError(f"blocked release disposition was wrong: {observed}")
    results.append({"name": "external_block", **observed})

    scenario_blocked = make_index(evidence, identity_value)
    scenario_blocked["scenarios"]["C10"] = {
        "status": "blocked", "blockers": ["synthetic-owner-unavailable"]}
    scenario_blocked_path = evidence / "scenario-blocked.json"
    scenario_blocked_path.write_text(json.dumps(scenario_blocked))
    observed = invoke(scenario_blocked_path, evidence, evidence / "scenario-blocked-receipt.json")
    if (observed["exit_status"] == 0
            or observed["receipt"].get("status") != "blocked_external_dependencies"
            or observed["receipt"].get("release_status") == "qualified"):
        raise ValueError(f"blocked scenario disposition was wrong: {observed}")
    results.append({"name": "scenario_external_block", **observed})

    component = make_index(evidence, identity_value)
    component["gates"]["SG02"]["status"] = "component_pass"
    component_path = evidence / "component-not-release.json"
    component_path.write_text(json.dumps(component))
    observed = invoke(component_path, evidence, evidence / "component-not-release-receipt.json")
    if (observed["exit_status"] == 0
            or observed["receipt"].get("status") != "blocked_external_dependencies"
            or observed["receipt"].get("release_status") == "qualified"):
        raise ValueError(f"component-only evidence was marked qualified: {observed}")
    results.append({"name": "component_not_release", **observed})

    variants = []
    missing = make_index(evidence, identity_value)
    missing["gates"]["SG03"]["receipts"][0]["path"] = "missing.json"
    variants.append(("missing_receipt", missing))
    duplicate = make_index(evidence, identity_value)
    duplicate["gates"]["SG02"]["receipts"].append(dict(duplicate["gates"]["SG02"]["receipts"][0]))
    variants.append(("duplicate_receipt", duplicate))
    stale = make_index(evidence, identity_value)
    stale["gates"]["SG04"]["receipts"][0]["source_commit"] = "1" * 40
    variants.append(("stale_receipt", stale))
    unknown = make_index(evidence, identity_value)
    unknown["gates"]["SG06"]["status"] = "skipped"
    variants.append(("skipped_backend", unknown))
    scenario = make_index(evidence, identity_value)
    del scenario["scenarios"]["C07"]
    variants.append(("missing_scenario", scenario))
    offline = make_index(evidence, identity_value)
    offline["gates"]["SG07"]["status"] = "blocked"
    offline["gates"]["SG07"]["blockers"] = ["synthetic-proof-gap"]
    variants.append(("offline_gate_blocked", offline))
    controls = make_index(evidence, identity_value)
    controls["gates"]["SG05"]["required_controls"].append("missing-mutant")
    variants.append(("missing_control", controls))
    wrong_status = make_index(evidence, identity_value)
    wrong_status["gates"]["SG01"]["receipts"][0]["path"] = "wrong-status.json"
    (evidence / "wrong-status.json").write_text(json.dumps({
        "schema_version": 1, "gate": "SG01", "status": "failed",
        "source_identity": identity_value,
    }))
    variants.append(("wrong_receipt_status", wrong_status))
    wrong_identity = make_index(evidence, identity_value)
    wrong_identity["gates"]["SG00"]["receipts"][0]["path"] = "wrong-identity.json"
    (evidence / "wrong-identity.json").write_text(json.dumps({
        "schema_version": 1, "gate": "SG00", "status": "pass", "source_identity": {},
    }))
    variants.append(("wrong_source_identity", wrong_identity))

    for name, value in variants:
        path = evidence / f"{name}.json"
        path.write_text(json.dumps(value))
        observed = invoke(path, evidence, evidence / f"{name}-receipt.json")
        if observed["exit_status"] == 0 or observed["receipt"].get("status") != "failed":
            raise ValueError(f"{name} did not refuse: {observed}")
        results.append({"name": name, **observed})

    placeholder = evidence / "placeholder.json"
    placeholder.write_text("sorryAx")
    bad = make_index(evidence, identity_value)
    bad["gates"]["SG01"]["receipts"][0]["path"] = "placeholder.json"
    path = evidence / "placeholder-index.json"
    path.write_text(json.dumps(bad))
    observed = invoke(path, evidence, evidence / "placeholder-receipt.json")
    if observed["exit_status"] == 0 or observed["receipt"].get("status") != "failed":
        raise ValueError(f"placeholder evidence was accepted: {observed}")
    results.append({"name": "placeholder", **observed})
    print(json.dumps({"status": "passed_aggregator_controls", "cases": len(results)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
