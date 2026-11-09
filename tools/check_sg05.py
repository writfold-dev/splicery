"""SG05 producer under owner amendment SG05-A1 (qualification/gate-amendments.json).

The shipping-Rust refinement evidence is the Aeneas proofs gate over
crates/splicery-splice: a fresh Charon/Aeneas translation with a byte-identical
drift check against proofs/Extracted, the Lean build, the axiom audit and mutant
calibration. The unchanged SG05 differential criterion is met by fresh primitive
and surface Lean/Rust differential receipts that must carry this checkout's
exact source identity.

Run on the admitted workstation from a clean checkout (the proofs gate builds
its own pinned image through Compose):

    python3 -B tools/check_sg05.py --evidence "$EVIDENCE/sg05-NNN" \
      --differential "$EVIDENCE/differential-NNN/differential-receipt.json" \
      --surface-differential "$EVIDENCE/surface-differential-NNN/surface-differential-receipt.json"
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from evidence_identity import source_identity

AMENDMENT = "SG05-A1"
MARKERS = (
    "extract: charon",
    "drift check against proofs/Extracted",
    "lake build",
    "axiom audit",
    "mutant calibration",
    "proofs gate: OK",
)
DIFFERENTIALS = {
    "differential": "passed_primitive_differential_component",
    "surface_differential": "passed_direct_lean_surface_rust_differential",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_new(path: Path, data: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(data)


def amendment() -> dict:
    path = ROOT / "qualification/gate-amendments.json"
    entries = json.loads(path.read_text(encoding="utf-8"))["amendments"]
    matching = [entry for entry in entries if entry.get("id") == AMENDMENT]
    if len(matching) != 1 or matching[0].get("status") != "accepted" or matching[0].get("gate") != "SG05":
        raise ValueError(f"owner amendment {AMENDMENT} is missing or not accepted")
    return {"id": AMENDMENT, "registry": "qualification/gate-amendments.json", "sha256": digest(path)}


def differential(path: Path, expected_status: str, identity: dict) -> dict:
    receipt = json.loads(path.read_text(encoding="utf-8"))
    if receipt.get("gate") != "SG05" or receipt.get("status") != expected_status:
        raise ValueError(f"{path.name} is not a passing SG05 differential receipt")
    if receipt.get("error") is not None:
        raise ValueError(f"{path.name} records an error")
    if receipt.get("source_identity") != identity:
        raise ValueError(f"{path.name} is bound to a different source identity")
    return {"path": str(path), "sha256": digest(path), "status": receipt["status"],
            "scope": receipt.get("scope"), "rust_binary_sha256": receipt.get("rust_binary_sha256")}


def proofs_image() -> dict:
    """The image Compose built for the `proofs` service (project `splicery`)."""
    name = "splicery-proofs"
    inspected = subprocess.run(
        ["docker", "image", "inspect", "--format", "{{.Id}}", name], cwd=ROOT,
        capture_output=True, text=True, check=False, timeout=60,
    )
    if inspected.returncode != 0 or not inspected.stdout.strip().startswith("sha256:"):
        raise ValueError("cannot identify the proofs image the gate ran in")
    return {"tag": name, "image_id": inspected.stdout.strip()}


def parse_log(text: str) -> dict:
    missing = [marker for marker in MARKERS if marker not in text]
    if missing:
        raise ValueError(f"proofs gate log lacks stages: {missing}")
    if "FAIL:" in text:
        raise ValueError("proofs gate log reports a failure")
    audit = re.search(r"AXIOM_AUDIT ok: (\d+) theorems, (\d+) required claims, axioms ⊆ (\[[^\]]*\]|\{[^}]*\}|#\[[^\]]*\])", text)
    calibrations = re.search(r"PROOF_CALIBRATIONS=(\d+)", text)
    rejected = re.findall(r"^calibration: (\S+) rejected by (.+)$", text, re.MULTILINE)
    if not audit or not calibrations:
        raise ValueError("proofs gate log lacks the axiom audit or calibration summary")
    declared = re.findall(r'^    Mutant\("([^"]+)"', (ROOT / "proofs/mutants.py").read_text(), re.MULTILINE)
    if (int(calibrations.group(1)) != len(rejected) or not rejected
            or sorted(name for name, _ in rejected) != sorted(declared)):
        raise ValueError("mutant calibration did not reject exactly the declared mutants")
    axioms = {name.strip(" `") for name in re.split(r"[,\s]+", audit.group(3).strip("#[]{}")) if name.strip(" `")}
    if not axioms <= {"propext", "Classical.choice", "Quot.sound"}:
        raise ValueError(f"axiom audit admitted nonstandard axioms: {sorted(axioms)}")
    return {
        "audited_theorems": int(audit.group(1)),
        "required_claims": int(audit.group(2)),
        "allowed_axioms": sorted(axioms),
        "calibrated_mutants": [{"name": name, "rejected_by": by.split(", ")} for name, by in rejected],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--differential", required=True, type=Path)
    parser.add_argument("--surface-differential", required=True, type=Path)
    args = parser.parse_args()
    evidence = args.evidence.expanduser().resolve()
    if evidence.is_relative_to(ROOT) or evidence.exists():
        parser.error("use a fresh evidence directory outside the source checkout")
    evidence.mkdir(parents=True, mode=0o700)
    identity = source_identity(ROOT)
    record: dict = {
        "schema_version": 1, "gate": "SG05", "status": "failed", "source_identity": identity,
        "scope": "SG05 under owner amendment SG05-A1: Aeneas proofs gate over crates/splicery-splice "
                 "plus fresh primitive and surface Lean/Rust differentials",
        "not_claimed": [
            "splicery-kernel facade mapping and UTF-8 candidate check (tested, not proved)",
            "refinement between formal/ (Lean 4.21 calculus) and proofs/ (Aeneas translation)",
        ],
    }
    command = ["docker", "compose", "run", "--build", "--rm", "proofs"]
    try:
        record["amendment"] = amendment()
        record["differentials"] = {
            "differential": differential(args.differential.resolve(), DIFFERENTIALS["differential"], identity),
            "surface_differential": differential(
                args.surface_differential.resolve(), DIFFERENTIALS["surface_differential"], identity),
        }
        record["translated_sources"] = {
            name: checksum for name, checksum in identity["source_manifest"].items()
            if name.startswith(("crates/splicery-splice/", "proofs/"))
        }
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                check=False, timeout=7200)
        log = evidence / "proofs-gate.log"
        write_new(log, result.stdout)
        record["proofs_gate"] = {
            "command": command, "exit_status": result.returncode,
            "elapsed_seconds": round(time.monotonic() - started, 1),
            "log": {"path": log.name, "sha256": digest(log), "bytes": log.stat().st_size},
        }
        if result.returncode != 0:
            raise ValueError("the proofs gate failed; inspect proofs-gate.log")
        record["proofs_gate"].update(parse_log(result.stdout.decode("utf-8", "replace")))
        record["proofs_gate"]["image"] = proofs_image()
        if source_identity(ROOT) != identity:
            raise ValueError("checkout identity changed during SG05 qualification")
        record["status"] = "passed_aeneas_shipping_refinement_component"
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    write_new(evidence / "sg05-receipt.json", (json.dumps(record, indent=2, sort_keys=True) + "\n").encode())
    print(json.dumps({"status": record["status"], "error": record.get("error")}, indent=2))
    return 0 if record["status"] == "passed_aeneas_shipping_refinement_component" else 1


if __name__ == "__main__":
    raise SystemExit(main())
