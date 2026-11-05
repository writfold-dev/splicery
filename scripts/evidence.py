"""Small source-bound private evidence helpers; no qualification status is inferred."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import zipfile

IGNORED_DIRECTORIES = {".git", ".lake", "__pycache__", "target", "dist"}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def manifest(root: Path, inputs: list[str]) -> dict[str, str]:
    result: dict[str, str] = {}
    for relative in inputs:
        relative_path = Path(relative)
        if relative_path.is_absolute() or ".." in relative_path.parts:
            raise ValueError(f"source input must stay within its root: {relative}")
        path = root / relative_path
        ancestor = path
        while ancestor != root:
            if ancestor.is_symlink():
                raise ValueError(f"symlink is not an admitted source input: {ancestor}")
            ancestor = ancestor.parent
        if not path.exists():
            raise ValueError(f"missing required source input: {relative}")
        files = sorted(path.rglob("*")) if path.is_dir() else [path]
        for candidate in files:
            if candidate.is_symlink():
                raise ValueError(f"symlink is not an admitted source input: {candidate}")
            relative_candidate = candidate.relative_to(root)
            if (candidate.is_file()
                    and not IGNORED_DIRECTORIES.intersection(relative_candidate.parts[:-1])):
                result[str(relative_candidate)] = digest(candidate)
    return result


def write_new(path: Path, value: object) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write("\n")


def run_logged(
    args: list[str], cwd: Path, evidence: Path, label: str,
    *, env: dict[str, str] | None = None, timeout: int = 600,
) -> dict[str, object]:
    stdout_path = evidence / f"{label}.stdout"
    stderr_path = evidence / f"{label}.stderr"
    started = time.monotonic()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        try:
            process = subprocess.Popen(
                args, cwd=cwd, env=env, stdout=stdout, stderr=stderr,
                start_new_session=True,
            )
            try:
                exit_code = process.wait(timeout=timeout)
                failure = None
            except subprocess.TimeoutExpired as error:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                exit_code = None
                failure = f"{type(error).__name__}: {error}"
        except OSError as error:
            exit_code = None
            failure = f"{type(error).__name__}: {error}"
    record = {
        "argv": args, "cwd": str(cwd), "exit_code": exit_code,
        "failure": failure, "elapsed_seconds": time.monotonic() - started,
        "stdout": {"path": stdout_path.name, "sha256": digest(stdout_path)},
        "stderr": {"path": stderr_path.name, "sha256": digest(stderr_path)},
    }
    write_new(evidence / f"{label}.command.json", record)
    print(f"{label}: exit={exit_code}; logs={evidence}", flush=True)
    return record


def require_success(record: dict[str, object]) -> None:
    if record["exit_code"] != 0:
        raise ValueError(f"command failed; inspect retained logs: {record['argv']}")


def process_env(**updates: str) -> dict[str, str]:
    result = dict(os.environ)
    result.update(updates)
    return result


def capture_sources(root: Path, inputs: list[str], archive: Path) -> dict[str, object]:
    sources = manifest(root, inputs)
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
        for relative, expected in sources.items():
            data = (root / relative).read_bytes()
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError(f"source changed while capturing: {relative}")
            bundle.writestr(relative, data)
    return {"path": archive.name, "sha256": digest(archive), "sources": sources}
