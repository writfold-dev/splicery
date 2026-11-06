"""Exercise SG00 preflight refusals without altering operator storage or caches."""
from __future__ import annotations

import argparse
import importlib.util
import importlib.machinery
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import preflight  # noqa: E402
from evidence_identity import source_identity  # noqa: E402

loader = importlib.machinery.SourceFileLoader("splicery_gate", str(ROOT / "scripts/gate"))
spec = importlib.util.spec_from_loader("splicery_gate", loader)
if spec is None:
    raise ImportError("cannot load scripts/gate")
gate = importlib.util.module_from_spec(spec)
loader.exec_module(gate)  # noqa: E402


def run(command: list[str], cwd: Path) -> dict:
    result = subprocess.run(command, cwd=cwd, capture_output=True, check=False, timeout=120)
    return {"command": command, "exit_status": result.returncode,
            "stdout": result.stdout.decode("utf-8", "replace"),
            "stderr": result.stderr.decode("utf-8", "replace")}


def expect_refusal(record: dict, name: str, function) -> None:
    try:
        function()
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        record["cases"].append({"name": name, "refused": True,
                                "reason": f"{type(error).__name__}: {error}"})
        return
    raise ValueError(f"{name} did not refuse")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    record = {"schema_version": 1, "gate": "SG00", "source_identity": source_identity(ROOT),
              "scope": "SG00 preflight negative controls only",
              "status": "failed", "cases": []}
    original_env = dict(os.environ)
    try:
        record["positive"] = preflight.observe(ROOT)
        os.environ["SPLICERY_CI_INFRA"] = str(output / "missing-infra")
        expect_refusal(record, "missing_ci_infra", lambda: preflight.observe(ROOT))
        os.environ.pop("SPLICERY_CI_INFRA", None)
        os.environ["SPLICERY_CI_STATE"] = str(output / "missing-state")
        expect_refusal(record, "missing_storage_binding", lambda: preflight.observe(ROOT))

        binding = json.loads((Path.home() / ".local/share/buildbot-ci/storage.json").read_text())
        binding["drive_letter"] = "Z"
        state = output / "wrong-drive"
        state.mkdir()
        (state / "storage.json").write_text(json.dumps(binding))
        os.environ["SPLICERY_CI_STATE"] = str(state)
        expect_refusal(record, "mismatched_external_drive", lambda: preflight.observe(ROOT))
        os.environ.pop("SPLICERY_CI_STATE", None)

        lock = json.loads((ROOT / "toolchains.lock.json").read_text())
        first = next(iter(lock["tools"]))
        lock["tools"][first] = "0" * 64
        (output / "tampered-tool").mkdir()
        expect_refusal(record, "tampered_tool_digest", lambda: gate.verify_tools(
            "splicery-tools:qualification", output / "tampered-tool", lock,
        ))

        empty = output / "empty-registry"
        empty.mkdir()
        result = run([
            "docker", "run", "--rm", "--network", "none", "--workdir", "/work/splicery",
            "-v", f"{ROOT}:/work/splicery:ro", "-v", f"{empty}:/opt/cargo/registry",
            "splicery-tools:qualification", "cargo", "metadata", "--locked", "--offline",
            "--format-version", "1",
        ], ROOT)
        record["cases"].append({"name": "empty_offline_registry", **result})
        if result["exit_status"] == 0:
            raise ValueError("empty offline registry unexpectedly resolved dependencies")
        if record["source_identity"] != source_identity(ROOT):
            raise ValueError("checkout identity changed during preflight qualification")
        record["status"] = "passed_preflight_negative_component"
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        os.environ.clear()
        os.environ.update(original_env)
        (output / "preflight-receipt.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    if record["status"] != "passed_preflight_negative_component":
        print(record.get("error", "preflight controls failed"))
        return 1
    print(json.dumps({"status": record["status"], "cases": len(record["cases"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
