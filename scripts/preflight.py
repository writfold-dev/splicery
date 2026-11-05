"""Observe the operator's existing storage policy; never replace its binding."""
from __future__ import annotations

import hashlib
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def observe(root: Path) -> dict[str, object]:
    infra = Path(os.environ.get("SPLICERY_CI_INFRA", str(root.parent / "ci-infra"))).resolve()
    state = Path(os.environ.get(
        "SPLICERY_CI_STATE", str(Path.home() / ".local/share/buildbot-ci"),
    )).resolve()
    storage_source = infra / "ci_infra/storage.py"
    binding = state / "storage.json"
    if not storage_source.is_file():
        raise ValueError("set SPLICERY_CI_INFRA to the operator's installed ci-infra checkout")
    if not binding.is_file():
        raise ValueError("missing operator storage binding; no fallback is permitted")
    source_sha256 = _digest(storage_source)
    binding_sha256 = _digest(binding)
    sys.path.insert(0, str(infra))
    try:
        storage = importlib.import_module("ci_infra.storage").check(state)
    except ImportError as error:
        raise ValueError("cannot import the operator's storage admission") from error
    finally:
        sys.path.pop(0)
    if storage is None:
        raise ValueError("storage observation did not admit the configured binding")
    if source_sha256 != _digest(storage_source) or binding_sha256 != _digest(binding):
        raise ValueError("storage admission inputs changed during observation")
    capacity = json.loads(subprocess.check_output([
        "docker", "info", "--format",
        '{"cpus":{{.NCPU}},"memory_bytes":{{.MemTotal}}}',
    ], text=True, timeout=30))
    if capacity["cpus"] < 8 or capacity["memory_bytes"] < 12884901888:
        raise ValueError("Docker capacity is below the existing admitted resource bound")
    return {
        "storage": storage,
        "storage_binding_sha256": binding_sha256,
        "storage_implementation_sha256": source_sha256,
        "capacity": capacity,
    }
