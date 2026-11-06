"""Exercise release aggregator validation with synthetic, non-product receipts."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COMMIT = "0" * 40


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


def make_index(evidence: Path) -> dict:
    entry = {"path": "component.json", "sha256": hashlib.sha256((evidence / "component.json").read_bytes()).hexdigest(), "scope": "synthetic-control",
             "source_commit": COMMIT}
    return {
        "schema_version": 1,
        "source_commit": COMMIT,
        "gates": {
            f"SG{number:02}": {"status": "component_pass", "receipts": [entry]}
            for number in range(15)
        },
        "scenarios": {
            f"C{number:02}": {"status": "pass", "receipts": ["component.json"]}
            for number in range(1, 25)
        },
    }


def main() -> int:
    evidence = Path(sys.argv[1]).resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    component = evidence / "component.json"
    component.write_text('{"status":"synthetic-control"}\n')
    entry = {"path": "component.json", "sha256": hashlib.sha256((evidence / "component.json").read_bytes()).hexdigest(), "scope": "synthetic-control",
             "source_commit": COMMIT}
    index = make_index(evidence)
    for gate in index["gates"].values():
        gate["receipts"] = [dict(entry)]
    index_path = evidence / "index.json"
    index_path.write_text(json.dumps(index))
    results = []
    valid = invoke(index_path, evidence, evidence / "valid-receipt.json")
    if valid["exit_status"] != 0 or valid["receipt"].get("status") != "pass":
        raise ValueError(f"valid synthetic aggregation failed: {valid}")
    results.append({"name": "valid", **valid})

    blocked = make_index(evidence)
    for number in range(8, 15):
        blocked["gates"][f"SG{number:02}"]["status"] = "blocked"
        blocked["gates"][f"SG{number:02}"]["receipts"] = []
        blocked["gates"][f"SG{number:02}"]["blockers"] = ["synthetic-owner-unavailable"]
    blocked_path = evidence / "blocked.json"
    blocked_path.write_text(json.dumps(blocked))
    observed = invoke(blocked_path, evidence, evidence / "blocked-receipt.json")
    if observed["exit_status"] == 0 or observed["receipt"].get("status") != "blocked_external_dependencies":
        raise ValueError(f"blocked release disposition was wrong: {observed}")
    results.append({"name": "external_block", **observed})

    variants = []
    missing = make_index(evidence)
    missing["gates"]["SG03"]["receipts"][0]["path"] = "missing.json"
    variants.append(("missing_receipt", missing))
    duplicate = make_index(evidence)
    duplicate["gates"]["SG02"]["receipts"].append(dict(entry))
    variants.append(("duplicate_receipt", duplicate))
    stale = make_index(evidence)
    stale["gates"]["SG04"]["receipts"][0]["source_commit"] = "1" * 40
    variants.append(("stale_receipt", stale))
    unknown = make_index(evidence)
    unknown["gates"]["SG06"]["status"] = "skipped"
    variants.append(("skipped_backend", unknown))
    scenario = make_index(evidence)
    del scenario["scenarios"]["C07"]
    variants.append(("missing_scenario", scenario))
    offline = make_index(evidence)
    offline["gates"]["SG07"]["status"] = "blocked"
    offline["gates"]["SG07"]["receipts"] = []
    variants.append(("offline_gate_blocked", offline))

    for name, value in variants:
        path = evidence / f"{name}.json"
        path.write_text(json.dumps(value))
        observed = invoke(path, evidence, evidence / f"{name}-receipt.json")
        if observed["exit_status"] == 0 or observed["receipt"].get("status") != "failed":
            raise ValueError(f"{name} did not refuse: {observed}")
        results.append({"name": name, **observed})

    placeholder = evidence / "placeholder.json"
    placeholder.write_text("sorryAx")
    bad = make_index(evidence)
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
