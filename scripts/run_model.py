#!/usr/bin/env python3
"""Run the source-bound SG07 TLC mutant and TLAPS evidence producers."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from typing import Sequence

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from evidence_identity import source_identity
MODEL = ROOT / "model"
REQUIRED_THEOREMS = (
    "InitInductive",
    "NextInductive",
    "SafetyFromInv",
    "SG07Safety",
    "ReplayObservational",
    "LookupObservational",
    "RevokedStaleNoWrite",
)
ASSUMPTIONS = (
    "private worktrees or cooperating writers while a claim is reserved",
    "owner-controlled directory handles with contained no-symlink resolution",
    "durable ledger journaling before target IO and old-process quiescence",
    "native Linux rename/fsync semantics; DrvFS and network mounts are excluded",
    "collision-resistant domain-separated digests and exact predecessor identity",
    "authenticated grants, stable request history, and discriminating effect tokens",
)


@dataclass(frozen=True)
class Mutation:
    name: str
    invariant: str
    replacements: tuple[tuple[str, str], ...]


MUTATIONS = (
    Mutation(
        "remove-live-at-write",
        "AuthorizedWrite",
        ((
            '  /\\ LiveAtWrite(k, f)\n'
            '  /\\ authorityAtWrite_k0_f0\' = IF f = "f0" THEN LiveAtWrite(k, f) ELSE authorityAtWrite_k0_f0',
            '  /\\ TRUE\n'
            '  /\\ authorityAtWrite_k0_f0\' = IF f = "f0" THEN LiveAtWrite(k, f) ELSE authorityAtWrite_k0_f0',
        ),),
    ),
    Mutation(
        "dispatch-unknown",
        "NoBlindRedispatch",
        ((
            '  /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "idle"\n'
            '  /\\ PrepKey(p) = k /\\ PrepClient(p) = c',
            '  /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \\in {"idle", "unknown"}\n'
            '  /\\ PrepKey(p) = k /\\ PrepClient(p) = c',
        ),),
    ),
    Mutation(
        "release-active-claim",
        "ActiveClaimRetention",
        ((
            '  /\\ k \\in Keys /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \\in {"confirmed", "refused", "partial"}\n'
            '  /\\ k = "k0" /\\ step \\in {6, 8, 9, 10, 15}\n'
            '  /\\ (IF k = "k0" THEN live_k0 ELSE live_k1) /\\ resources = 1',
            '  /\\ k \\in Keys /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \\in {"claimed", "unknown", "confirmed", "refused", "partial"}\n'
            '  /\\ k = "k0" /\\ step \\in {6, 8, 9, 10, 15}\n'
            '  /\\ (IF k = "k0" THEN live_k0 ELSE live_k1) /\\ resources = 1',
        ),),
    ),
    Mutation(
        "confirm-timeout",
        "ConfirmationProvenance",
        ((
            '  /\\ (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) = "pending" /\\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) = "pending"\n'
            '  /\\ phase_k0\' = "unknown" /\\ phase_k1\' = phase_k1\n'
            '  /\\ everUnknown_k0\' = TRUE /\\ everUnknown_k1\' = everUnknown_k1',
            '  /\\ (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) = "pending" /\\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) = "pending"\n'
            '  /\\ phase_k0\' = "confirmed" /\\ phase_k1\' = phase_k1\n'
            '  /\\ everUnknown_k0\' = TRUE /\\ everUnknown_k1\' = everUnknown_k1',
        ),),
    ),
    Mutation(
        "foreign-observation",
        "ConfirmationProvenance",
        ((
            '  /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsKey_k0_f0 ELSE obsKey_k0_f1) ELSE (IF f = "f0" THEN obsKey_k1_f0 ELSE obsKey_k1_f1)) = k /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsWriter_k0_f0 ELSE obsWriter_k0_f1) ELSE (IF f = "f0" THEN obsWriter_k1_f0 ELSE obsWriter_k1_f1)) = k',
            '  /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsKey_k0_f0 ELSE obsKey_k0_f1) ELSE (IF f = "f0" THEN obsKey_k1_f0 ELSE obsKey_k1_f1)) # NoKey /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsWriter_k0_f0 ELSE obsWriter_k0_f1) ELSE (IF f = "f0" THEN obsWriter_k1_f0 ELSE obsWriter_k1_f1)) = k',
        ),),
    ),
    Mutation(
        "aba-rival-content-only",
        "ConfirmationProvenance",
        (
            (
                '  /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle" /\\ ~FileReserved(f) /\\ step \\in {2, 5} /\\ k = "k0"\n'
                '  /\\ fileState_f0\' = IF f = "f0" THEN (IF k = "k0" THEN after_k0_f0 ELSE after_k1_f0) ELSE fileState_f0',
                '  /\\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle" /\\ step \\in {2, 5} /\\ k = "k0"\n'
                '  /\\ fileState_f0\' = IF f = "f0" THEN (IF k = "k0" THEN after_k0_f0 ELSE after_k1_f0) ELSE fileState_f0',
            ),
            (
                '  /\\ (IF k = "k0" THEN journal_k0 ELSE journal_k1) /\\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 1\n'
                '  /\\ (IF f = "f0" THEN fileState_f0 ELSE fileState_f1) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1)) /\\ (IF f = "f0" THEN fileWriter_f0 ELSE fileWriter_f1) = k\n'
                '  /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsKey_k0_f0 ELSE obsKey_k0_f1) ELSE (IF f = "f0" THEN obsKey_k1_f0 ELSE obsKey_k1_f1)) = k /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsWriter_k0_f0 ELSE obsWriter_k0_f1) ELSE (IF f = "f0" THEN obsWriter_k1_f0 ELSE obsWriter_k1_f1)) = k\n'
                '  /\\ (IF k = "k0" THEN (IF f = "f0" THEN obsDigest_k0_f0 ELSE obsDigest_k0_f1) ELSE (IF f = "f0" THEN obsDigest_k1_f0 ELSE obsDigest_k1_f1)) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1))',
                '  /\\ (IF k = "k0" THEN journal_k0 ELSE journal_k1) /\\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 1\n'
                '  /\\ (IF f = "f0" THEN fileState_f0 ELSE fileState_f1) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1))',
            ),
        ),
    ),
)


def sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def source_files() -> list[Path]:
    return sorted(
        [path for path in MODEL.rglob("*") if path.is_file()]
        + [Path(__file__).resolve()],
        key=lambda path: str(path),
    )


def source_manifest() -> dict[str, str]:
    return {str(path.relative_to(ROOT)): sha256(path) for path in source_files()}


def write_json_new(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def run_logged(
    argv: Sequence[str], cwd: Path, attempt: Path, timeout: int = 1800
) -> dict[str, object]:
    started = time.time()
    try:
        result = subprocess.run(
            list(argv), cwd=cwd, capture_output=True, check=False, timeout=timeout
        )
        code = result.returncode
        stdout = result.stdout
        stderr = result.stderr
        timed_out = False
    except subprocess.TimeoutExpired as error:
        code = 124
        stdout = error.stdout or b""
        stderr = (error.stderr or b"") + b"\nproducer timeout\n"
        timed_out = True
    (attempt / "command.stdout").write_bytes(stdout)
    (attempt / "command.stderr").write_bytes(stderr)
    record: dict[str, object] = {
        "argv": list(argv),
        "cwd": str(cwd),
        "exit_code": code,
        "timed_out": timed_out,
        "elapsed_seconds": round(time.time() - started, 3),
        "stdout_sha256": sha256(attempt / "command.stdout"),
        "stderr_sha256": sha256(attempt / "command.stderr"),
    }
    write_json_new(attempt / "command.json", record)
    return record


def output_text(attempt: Path) -> str:
    return (attempt / "command.stdout").read_text(errors="replace") + "\n" + (
        attempt / "command.stderr"
    ).read_text(errors="replace")


def prepare_attempt(parent: Path, name: str, source_model: Path) -> tuple[Path, Path]:
    attempt = parent / name
    work = attempt / "work"
    work.mkdir(parents=True)
    shutil.copytree(source_model, work / "model")
    return attempt, work / "model"


def tlc_command(jar: Path, config: str = "SG07.cfg") -> list[str]:
    return [
        "java",
        "-XX:+UseParallelGC",
        "-Xmx4g",
        "-cp",
        str(jar),
        "tlc2.TLC",
        "-workers",
        "2",
        "-dump",
        "dot,actionlabels",
        "states.dot",
        "-config",
        config,
        "SG07.tla",
    ]


def check_base(record: dict[str, object], attempt: Path) -> dict[str, object]:
    text = output_text(attempt)
    if record["exit_code"] != 0:
        raise ValueError("base TLC failed; inspect its retained output")
    if "Model checking completed. No error has been found." not in text:
        raise ValueError("base TLC did not retain an explicit successful completion")
    match = re.search(
        r"([\d,]+) states generated, ([\d,]+) distinct states found", text
    )
    if not match:
        raise ValueError("base TLC state counts are missing")
    graph = attempt / "work" / "model" / "states.dot"
    if not graph.is_file() or graph.stat().st_size == 0:
        raise ValueError("base TLC state graph is missing")
    graph_text = graph.read_text(errors="replace")
    if "digraph" not in graph_text:
        raise ValueError("base TLC state graph is not a DOT graph")
    return {
        "states_generated": int(match.group(1).replace(",", "")),
        "distinct_states": int(match.group(2).replace(",", "")),
        "state_graph": str(graph.relative_to(attempt)),
        "state_graph_sha256": sha256(graph),
        "invariants": ["StrengthenedInv"],
        "config_sha256": sha256(attempt / "work" / "model" / "SG07.cfg"),
        "deadlock_check": False,
    }


def mutate(work: Path, mutation: Mutation) -> None:
    path = work / "SG07.tla"
    source = path.read_text()
    for old, new in mutation.replacements:
        count = source.count(old)
        if count != 1:
            raise ValueError(
                f"{mutation.name}: mutation anchor count is {count}, expected exactly one"
            )
        source = source.replace(old, new, 1)
    path.write_text(source)
    (work / "SG07-mutant.cfg").write_text(
        f"SPECIFICATION Spec\nINVARIANT {mutation.invariant}\nCHECK_DEADLOCK FALSE\n"
    )


def check_mutant(
    mutation: Mutation, record: dict[str, object], attempt: Path
) -> dict[str, object]:
    text = output_text(attempt)
    if record["exit_code"] == 0 or record["timed_out"]:
        raise ValueError(f"{mutation.name}: TLC did not produce the required failure")
    if not re.search(
        rf"Invariant\s+{re.escape(mutation.invariant)}\s+is violated", text
    ):
        raise ValueError(
            f"{mutation.name}: missing counterexample for {mutation.invariant}"
        )
    if not re.search(r"State\s+1:", text) or not re.search(r"State\s+2:", text):
        raise ValueError(f"{mutation.name}: retained schedule is incomplete")
    work = attempt / "work" / "model"
    return {
        "name": mutation.name,
        "expected_invariant": mutation.invariant,
        "counterexample_retained": True,
        "mutant_source_sha256": sha256(work / "SG07.tla"),
        "mutant_config_sha256": sha256(work / "SG07-mutant.cfg"),
        "schedule_log": "command.stdout",
    }


def check_tlaps(record: dict[str, object], attempt: Path, source: Path) -> dict[str, object]:
    text = output_text(attempt)
    proof_source = source.read_text()
    if re.search(r"\b(?:OMITTED|OBVIOUS)\b", proof_source):
        raise ValueError("expanded proof source contains an unchecked proof step")
    for theorem in REQUIRED_THEOREMS:
        if not re.search(rf"\bTHEOREM\s+{theorem}\b", proof_source):
            raise ValueError(f"required theorem {theorem} is absent")
    backend_steps = len(re.findall(r"\bBY\s+SMT\b", proof_source))
    if backend_steps < 35:
        raise ValueError("expanded proof source is missing an SMT action/backend case")
    if record["exit_code"] != 0 or record["timed_out"]:
        raise ValueError("TLAPS failed or timed out; inspect retained obligations")
    if re.search(
        r"(?i)(?:\[ERROR\]|backend errors|could not prove|obligations? failed|"
        r"zenon error|smt error|ending abnormally)",
        text,
    ):
        raise ValueError("full TLAPS output contains an unproved obligation/backend error")
    if "fingerprints written" not in text:
        raise ValueError("full TLAPS output lacks its successful fingerprint completion marker")
    return {
        "theorem_names": list(REQUIRED_THEOREMS),
        "completion": "full TLAPS exit 0; fingerprints written; no failed obligation diagnostics",
        "backend": "SMT",
        "backend_proof_steps": backend_steps,
        "proof_source_sha256": sha256(source),
        "obligation_log": ["command.stdout", "command.stderr"],
    }


def verify_tools(jar: Path, tlapm: Path, evidence: Path) -> dict[str, object]:
    if not jar.is_file():
        raise ValueError(f"TLC jar is absent: {jar}")
    if not tlapm.is_file() or not os.access(tlapm, os.X_OK):
        raise ValueError(f"TLAPM executable is absent: {tlapm}")
    records = []
    for name, argv in (
        ("java-version", ["java", "-version"]),
        ("tlc-version", ["java", "-cp", str(jar), "tlc2.TLC", "-h"]),
        ("tlapm-version", [str(tlapm), "--version"]),
    ):
        attempt = evidence / "tools" / name
        attempt.mkdir(parents=True)
        record = run_logged(argv, evidence, attempt, timeout=60)
        records.append(record)
        if record["exit_code"] not in (0, 1):
            raise ValueError(f"tool probe failed: {name}")
        if name == "tlc-version" and "Version 2.19" not in output_text(attempt):
            raise ValueError("unexpected TLC version")
    return {
        "tla2tools_jar": str(jar),
        "tla2tools_sha256": sha256(jar),
        "tlapm": str(tlapm),
        "tlapm_sha256": sha256(tlapm),
        "probes": records,
    }


def produce(evidence: Path) -> dict[str, object]:
    evidence.mkdir(parents=True, mode=0o755, exist_ok=False)
    evidence.chmod(0o755)
    manifest = source_manifest()
    identity = source_identity(ROOT)
    snapshot = evidence / "source"
    snapshot.mkdir()
    shutil.copytree(MODEL, snapshot / "model")
    shutil.copyfile(Path(__file__).resolve(), snapshot / "run_model.py")
    write_json_new(snapshot / "source-manifest.json", manifest)

    jar = Path(os.environ.get("TLA2TOOLS_JAR", "/opt/tla/tla2tools.jar"))
    tlapm = Path(os.environ.get("TLAPM", "/opt/tlapm/bin/tlapm"))
    tools = verify_tools(jar, tlapm, evidence)
    errors: list[str] = []
    base_result: dict[str, object] | None = None
    mutant_results: list[dict[str, object]] = []

    base_attempt, base_work = prepare_attempt(evidence / "attempts", "base", snapshot / "model")
    base_record = run_logged(tlc_command(jar), base_work, base_attempt)
    try:
        base_result = check_base(base_record, base_attempt)
    except Exception as error:  # retain all later independent attempts
        errors.append(f"base: {error}")

    for mutation in MUTATIONS:
        attempt, work = prepare_attempt(
            evidence / "attempts", f"mutant-{mutation.name}", snapshot / "model"
        )
        try:
            mutate(work, mutation)
            record = run_logged(
                tlc_command(jar, "SG07-mutant.cfg"), work, attempt
            )
            mutant_results.append(check_mutant(mutation, record, attempt))
        except Exception as error:
            errors.append(f"{mutation.name}: {error}")

    proof_attempt, proof_work = prepare_attempt(
        evidence / "attempts", "tlaps-expanded", snapshot / "model"
    )
    proof_record = run_logged(
        [str(tlapm), "--strict", "SG07Safety.tla"], proof_work, proof_attempt
    )
    proof_result: dict[str, object] | None = None
    try:
        proof_result = check_tlaps(
            proof_record, proof_attempt, proof_work / "SG07Safety.tla"
        )
    except Exception as error:
        errors.append(f"tlaps: {error}")

    if source_manifest() != manifest:
        errors.append("model or producer sources changed during evidence production")
    if len(mutant_results) != len(MUTATIONS):
        errors.append("not every required mutant produced a retained counterexample")
    if identity != source_identity(ROOT):
        errors.append("checkout identity changed during model qualification")
    status = "failed" if errors else "passed_sg07_bounded_operational_model"
    return {
        "schema_version": 1,
        "gate": "SG07",
        "source_identity": identity,
        "status": status,
        "scope": "bounded operational safety model, mutant schedules, and expanded TLAPS proofs",
        "source_manifest": manifest,
        "tools": tools,
        "base": {"command": base_record, "result": base_result},
        "mutants": mutant_results,
        "tlaps": {"command": proof_record, "result": proof_result},
        "assumptions": list(ASSUMPTIONS),
        "liveness": {
            "proved": False,
            "module": "SG07Liveness.tla",
            "requirements": "weak fairness plus stable grant, availability, old-process quiescence and working IO",
        },
        "exclusions": [
            "no production host or WSL filesystem qualification",
            "no liveness claim from the safety run",
            "no Lean-to-TLA equivalence claim",
            "seed results are historical and do not satisfy SG07",
        ],
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    evidence = args.evidence.expanduser().resolve()
    if evidence.is_relative_to(ROOT):
        parser.error("evidence must be outside the source checkout")
    if evidence.exists():
        parser.error("evidence directory must be fresh; prior evidence is never overwritten")
    try:
        receipt = produce(evidence)
    except Exception as error:
        if not evidence.exists():
            evidence.mkdir(parents=True, mode=0o700)
        receipt = {
            "schema_version": 1,
            "gate": "SG07",
            "status": "failed",
            "errors": [f"producer: {type(error).__name__}: {error}"],
        }
    write_json_new(evidence / "model-receipt.json", receipt)
    return 1 if receipt["status"] == "failed" else 0


if __name__ == "__main__":
    sys.dont_write_bytecode = True
    raise SystemExit(main())
