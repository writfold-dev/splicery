"""Build a real Splicery Cargo workspace with locked, offline, static musl tools."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-unknown-linux-musl"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(output: Path, name: str, args: list[str]) -> subprocess.CompletedProcess[bytes]:
    result = subprocess.run(args, cwd=ROOT, capture_output=True, check=False)
    (output / f"{name}.stdout").write_bytes(result.stdout)
    (output / f"{name}.stderr").write_bytes(result.stderr)
    return result


def checked(output: Path, name: str, args: list[str]) -> subprocess.CompletedProcess[bytes]:
    result = run(output, name, args)
    if result.returncode != 0:
        raise ValueError(f"{name} failed; inspect the retained logs")
    return result


def require_static_elf(path: Path) -> None:
    data = path.read_bytes()
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise ValueError(f"executable is not little-endian ELF64: {path}")
    if struct.unpack_from("<H", data, 18)[0] != 62:
        raise ValueError(f"executable is not x86-64: {path}")
    program_offset = struct.unpack_from("<Q", data, 32)[0]
    entry_size = struct.unpack_from("<H", data, 54)[0]
    entry_count = struct.unpack_from("<H", data, 56)[0]
    if entry_size < 56 or program_offset + entry_size * entry_count > len(data):
        raise ValueError(f"invalid ELF program header table: {path}")
    for index in range(entry_count):
        kind = struct.unpack_from("<I", data, program_offset + index * entry_size)[0]
        if kind == 3:  # PT_INTERP: a dynamically loaded userspace executable.
            raise ValueError(f"musl executable is not static (PT_INTERP present): {path}")


def collect_artifacts(stdout: bytes) -> list[dict[str, object]]:
    artifacts: dict[str, dict[str, object]] = {}
    for line in stdout.decode("utf-8").splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact":
            continue
        executable = message.get("executable")
        filenames = list(message.get("filenames", []))
        if executable is not None:
            filenames.append(executable)
        for filename in filenames:
            path = Path(filename)
            if path.suffix == ".d" or not path.is_file():
                continue
            key = str(path)
            record = artifacts.setdefault(key, {
                "path": key,
                "sha256": digest(path),
                "size_bytes": path.stat().st_size,
                "packages": [],
            })
            package = message.get("package_id")
            if package not in record["packages"]:
                record["packages"].append(package)
            if executable is not None and path == Path(executable):
                require_static_elf(path)
                record["executable"] = True
                record["static_elf_x86_64"] = True
    return sorted(artifacts.values(), key=lambda item: item["path"])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--rust-version", required=True)
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o755, exist_ok=False)
    record: dict[str, object] = {
        "schema_version": 1,
        "scope": "locked offline static musl workspace build only",
        "status": "failed",
        "target": TARGET,
        "locked": True,
        "offline": True,
    }
    try:
        cargo_toml = ROOT / "Cargo.toml"
        cargo_lock = ROOT / "Cargo.lock"
        if not cargo_toml.is_file() or not cargo_lock.is_file():
            raise ValueError("no real Cargo workspace: root Cargo.toml and Cargo.lock are required")
        document = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
        if not isinstance(document.get("workspace"), dict) and not isinstance(document.get("package"), dict):
            raise ValueError("no real Cargo workspace: Cargo.toml has neither [workspace] nor [package]")
        cargo = shutil.which("cargo")
        rustc = shutil.which("rustc")
        if not cargo or not rustc:
            raise ValueError("missing admitted Cargo/Rust compiler")
        record["cargo_manifest_sha256"] = digest(cargo_toml)
        record["cargo_lock_sha256"] = digest(cargo_lock)
        cargo_version = checked(output, "cargo-version", [cargo, "--version"]).stdout.decode().strip()
        rust_version = checked(output, "rustc-version", [rustc, "--version"]).stdout.decode().strip()
        if args.rust_version not in cargo_version or args.rust_version not in rust_version:
            raise ValueError("Cargo/Rust version differs from the tool lock")
        record["cargo_version"] = cargo_version
        record["rustc_version"] = rust_version
        command = [
            cargo, "build", "--workspace", "--release", "--locked", "--offline",
            "--target", TARGET, "--message-format=json-render-diagnostics",
        ]
        record["command"] = command
        result = checked(output, "cargo-build", command)
        record["artifacts"] = collect_artifacts(result.stdout)
        executables = [artifact for artifact in record["artifacts"]
                       if artifact.get("executable") is True]
        if len(executables) != 1:
            raise ValueError("build must produce exactly one Splicery client executable")
        source = Path(executables[0]["path"])
        client = output / "splicery"
        shutil.copyfile(source, client)
        client.chmod(0o755)
        record["client"] = {"path": client.name, "sha256": digest(client),
                            "size_bytes": client.stat().st_size, "static_elf_x86_64": True}
        smoke = checked(output, "runtime-smoke", [
            str(client), "kernel", "apply", "1", "1", "616263", "5858",
        ])
        if smoke.stdout != b"61585863\n":
            raise ValueError("static client returned the wrong kernel smoke result")
        record["runtime_smoke"] = {"command": smoke.args, "stdout_hex": smoke.stdout.hex()}
        record["status"] = "passed_locked_offline_static_musl_build_only"
    except (OSError, UnicodeError, ValueError, KeyError, struct.error) as error:
        record["reason"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(output / "build-results.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(record, stream, indent=2)
            stream.write("\n")
    if record["status"] != "passed_locked_offline_static_musl_build_only":
        print(record.get("reason", "build failed"), file=sys.stderr)
        return 1
    print("Built the Cargo workspace with locked offline static musl tools; product gates remain separate.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
