"""Bind qualification evidence to the exact checkout, diff, sources and tool lock."""
from __future__ import annotations

import hashlib
from pathlib import Path
import subprocess


def source_identity(root: Path) -> dict[str, object]:
    root = root.resolve()
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True, timeout=30,
    ).strip()
    dirty = subprocess.check_output(
        ["git", "diff", "--binary", "HEAD"], cwd=root, timeout=60,
    )
    manifest = {}
    ignored = {".git", ".lake", "__pycache__", "target", "dist"}
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        relative = path.relative_to(root)
        if any(part in ignored for part in relative.parts[:-1]):
            continue
        manifest[str(relative)] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {
        "source_commit": commit,
        "dirty_diff_sha256": hashlib.sha256(dirty).hexdigest(),
        "source_manifest": manifest,
        "tool_lock_sha256": hashlib.sha256((root / "toolchains.lock.json").read_bytes()).hexdigest(),
    }
