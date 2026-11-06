"""Cold-rebuild the static client twice and compare exact artifact identities."""
from __future__ import annotations

import argparse
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def load_evidence():
    loader = importlib.machinery.SourceFileLoader("splicery_evidence", str(ROOT / "scripts/evidence.py"))
    spec = importlib.util.spec_from_loader("splicery_evidence", loader)
    if spec is None:
        raise ImportError("cannot load evidence helper")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def run(command: list[str], log: Path) -> None:
    result = subprocess.run(command, capture_output=True, check=False, timeout=1800)
    log.write_bytes(result.stdout + result.stderr)
    if result.returncode:
        raise ValueError(f"command failed: {command}; inspect {log}")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    evidence = load_evidence()
    inputs = evidence.manifest(ROOT, ["."])
    record = {"schema_version": 1, "scope": "two cold offline musl rebuilds only",
              "status": "failed", "source_manifest": inputs}
    try:
        registry = output / "registry"
        registry.mkdir()
        run([
            "docker", "run", "--rm", "--network", "none",
            "-v", "~/.cargo/registry:/seed:ro",
            "-v", f"{registry}:/registry",
            "splicery-tools:qualification", "sh", "-c", "cp -a /seed/. /registry/",
        ], output / "seed-registry.log")
        hashes = []
        for name in ("a", "b"):
            (output / f"target-{name}").mkdir()
            command = [
                "docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
                "--cpus", "8", "--memory", "12884901888", "--pids-limit", "1024",
                "--workdir", "/work/splicery",
                "-v", f"{ROOT}:/work/splicery:ro", "-v", f"{output}:/evidence",
                "-v", f"{registry}:/opt/cargo/registry",
                "-e", f"CARGO_TARGET_DIR=/evidence/target-{name}",
                "splicery-tools:qualification", "python3", "-B", "scripts/run_build.py",
                "--evidence", f"/evidence/out-{name}", "--rust-version", "1.98.0",
            ]
            run(command, output / f"build-{name}.log")
            result = json.loads((output / f"out-{name}/build-results.json").read_text())
            if result["status"] != "passed_locked_offline_static_musl_build_only":
                raise ValueError(f"cold build {name} did not pass")
            hashes.append(digest(output / f"out-{name}/splicery"))
        if hashes[0] != hashes[1]:
            raise ValueError(f"cold builds differ: {hashes}")
        if inputs != evidence.manifest(ROOT, ["."]):
            raise ValueError("source changed during cold reproduction")
        record.update({"status": "passed_cold_offline_reproduction_component",
                       "artifact_sha256": hashes[0], "rebuilt_artifact_sha256": hashes[1]})
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        (output / "reproduction-receipt.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    if record["status"] != "passed_cold_offline_reproduction_component":
        print(record.get("error", "cold reproduction failed"))
        return 1
    print(json.dumps({"status": record["status"], "artifact_sha256": record["artifact_sha256"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
