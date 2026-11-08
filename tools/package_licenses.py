"""Collect original notices and conservative source coverage bound to an observed link."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "AUTHORS")
TARGET = "x86_64-unknown-linux-musl"
RUST_COMMIT = "88d9e12ae178fab0fb5cc050a94da85685d449ea"
LLVM_COMMIT = "52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04"
GCC_COMMIT = "13c83c4cc679ad5383ed57f359e53e8d518b7842"
VERUS_CARGO_COMMIT = "53ec5c4b0770df8ba5526adb823929fb85eb0c4b"
VERUS_CARGO_PATHS = {
    "verus_builtin": "source/builtin",
    "verus_builtin_macros": "source/builtin_macros",
    "verus_state_machines_macros": "source/state_machines_macros",
    "vstd": "source/vstd",
}
# Exact supplemental upstream bytes, not SPDX templates or moving release URLs.
# Values are (URL, SHA-256); these files are staged outside the offline collector.
RUNTIME_SOURCES = {
    "verus/LICENSE": (
        f"https://raw.githubusercontent.com/verus-lang/verus/{VERUS_CARGO_COMMIT}/LICENSE",
        "ab78fc657425500bc64f617e36bb6819c6b0d039c573642f603dd594b12fe0d3"),
    "rust/library/compiler-builtins/LICENSE.txt": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/library/compiler-builtins/LICENSE.txt",
        "ab6eec6caf0fa5775e411c7a8bc6a45c4ef2956b0980b157ab74fc5cd62a928b"),
    "rust/library/compiler-builtins/libm/LICENSE.txt": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/library/compiler-builtins/libm/LICENSE.txt",
        "3823dda7cf046602f4b4e77ec8e227863dc4736037cc85bb33d9f19febe16bb7"),
    "rust/library/Cargo.lock": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/library/Cargo.lock",
        "d1c5dbdf53bfebd7de60f26a171819db3b28ebfd75f3fa99c3286893a5a7b7a6"),
    "rust/src/ci/docker/scripts/musl-toolchain.sh": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/src/ci/docker/scripts/musl-toolchain.sh",
        "faae0de27e4ca1661a030ce3623e4959f34f5d1b6f795ea9b5b84c10c2086b2e"),
    "rust/src/ci/docker/scripts/musl-cve-2026-6042.diff": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/src/ci/docker/scripts/musl-cve-2026-6042.diff",
        "444fa70e52ca158fb7d4bad560637790bbf8f72e80b82fff840dd66fa83091e3"),
    "rust/src/ci/docker/scripts/musl-cve-2026-40200.diff": (
        f"https://raw.githubusercontent.com/rust-lang/rust/{RUST_COMMIT}/src/ci/docker/scripts/musl-cve-2026-40200.diff",
        "1ee29f64f9ca8e8ad7c349779d661ff6b52126a27575d3586981357a52c406fb"),
    "llvm/libunwind/LICENSE.TXT": (
        f"https://raw.githubusercontent.com/rust-lang/llvm-project/{LLVM_COMMIT}/libunwind/LICENSE.TXT",
        "b5efebcaca80879234098e52d1725e6d9eb8fb96a19fce625d39184b705f7b6d"),
    "llvm/compiler-rt/LICENSE.TXT": (
        f"https://raw.githubusercontent.com/rust-lang/llvm-project/{LLVM_COMMIT}/compiler-rt/LICENSE.TXT",
        "1a8f1058753f1ba890de984e48f0242a3a5c29a6a8f2ed9fd813f36985387e8d"),
    "llvm/compiler-rt/CREDITS.TXT": (
        f"https://raw.githubusercontent.com/rust-lang/llvm-project/{LLVM_COMMIT}/compiler-rt/CREDITS.TXT",
        "a9901f47a089da41e4690682d00ce4cedaa2baf41fedbe79beee366d43ac2461"),
    "gcc-9.4.0/COPYING3": (
        f"https://raw.githubusercontent.com/gcc-mirror/gcc/{GCC_COMMIT}/COPYING3",
        "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903"),
    "gcc-9.4.0/COPYING.RUNTIME": (
        f"https://raw.githubusercontent.com/gcc-mirror/gcc/{GCC_COMMIT}/COPYING.RUNTIME",
        "9d6b43ce4d8de0c878bf16b54d8e7a10d9bd42b75178153e3af6a815bdc90f74"),
    "gcc-9.4.0/libgcc/crtstuff.c": (
        f"https://raw.githubusercontent.com/gcc-mirror/gcc/{GCC_COMMIT}/libgcc/crtstuff.c",
        "a463c3605a66623aff73295ab83fd28e825010c71a4ce1bd615776acf0bf92b4"),
    "musl-1.2.5/COPYRIGHT": (
        "https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?id=0784374d561435f7c787a555aeab8ede699ed298",
        "f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af"),
    "musl-cross-make/Makefile": (
        "https://raw.githubusercontent.com/richfelker/musl-cross-make/3635262e4524c991552789af6f36211a335a77b3/Makefile",
        "6ba8e8f0d7d46a32e4644d5de2e537c33c854c8956990b8bd475c1c310ff9a6d"),
}
# Whole-library sources retain inline notices without guessing which LTO source
# lines survived. Only the stated LLVM subtrees are copied, never compiler tools.
RUNTIME_ARCHIVES = {
    "rust-src.tar.xz": (
        "https://static.rust-lang.org/dist/2026-08-20/rust-src-1.98.0.tar.xz",
        "0e977492aed5ff137815bf9b4b796e8a85b80fb2367ed64e82d5c5839870f2bf",
        "rust-src-1.98.0", ("",)),
    "musl-1.2.5.tar.gz": (
        "https://musl.libc.org/releases/musl-1.2.5.tar.gz",
        "a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4",
        "musl-1.2.5", ("",)),
    "llvm-project.tar.gz": (
        f"https://codeload.github.com/rust-lang/llvm-project/tar.gz/{LLVM_COMMIT}",
        "27f31a6639023d90cd754c1de42d8f9d77cc0c3deff37896f88c1346c6c3f396",
        f"llvm-project-{LLVM_COMMIT}", ("libunwind/", "compiler-rt/lib/builtins/")),
    "musl-cross-make.tar.gz": (
        "https://codeload.github.com/richfelker/musl-cross-make/tar.gz/3635262e4524c991552789af6f36211a335a77b3",
        "60bed670d689d5c2164020960df36b80189dc5617e9763e023672f98a99d567e",
        "musl-cross-make-3635262e4524c991552789af6f36211a335a77b3", ("",)),
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def input_file(path: Path, root: Path) -> None:
    if not path.is_absolute():
        raise ValueError(f"non-absolute notice input: {path}")
    if not path.is_relative_to(root) or ".." in path.parts:
        raise ValueError(f"escaping notice input: {path}")
    for ancestor in (path, *path.parents):
        if ancestor.is_symlink():
            raise ValueError(f"symlink notice input: {path}")
    if not path.is_file():
        raise ValueError(f"missing or unsupported input: {path}")


def regular(path: Path, root: Path, max_size: int = 1_048_576) -> bytes:
    input_file(path, root)
    if path.stat().st_size > max_size:
        raise ValueError(f"oversized notice input: {path}")
    data = path.read_bytes()
    if not data.strip() or b"\0" in data:
        raise ValueError(f"empty or binary notice input: {path}")
    data.decode("utf-8")
    return data


def refuse_walk_error(error: OSError) -> None:
    raise error


def file_identity(path: Path, root: Path) -> dict:
    input_file(path, root)
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"source_path": str(path), "sha256": digest, "size_bytes": path.stat().st_size}


def collect_source_archives(notices: dict[str, bytes], sources: Path) -> list[dict]:
    records = []
    total = 0
    for filename, (url, expected, prefix, trees) in RUNTIME_ARCHIVES.items():
        path = sources / filename
        identity = file_identity(path, sources)
        if identity["size_bytes"] > 300_000_000 or identity["sha256"] != expected:
            raise ValueError(f"runtime source archive differs from pin: {path}")
        members = []
        with tarfile.open(path, "r:*") as archive:
            for entry in archive:
                parts = PurePosixPath(entry.name).parts
                if not parts or parts[0] != prefix or ".." in parts or entry.name.startswith("/"):
                    raise ValueError(f"unsafe source archive entry: {entry.name}")
                relative = "/".join(parts[1:])
                if entry.isdir() or not any(relative.startswith(tree) for tree in trees):
                    continue
                if not entry.isfile() or entry.size > 16_777_216:
                    raise ValueError(f"unsupported source archive entry: {entry.name}")
                total += entry.size
                if total > 134_217_728:
                    raise ValueError("runtime source coverage exceeds its admitted bound")
                stream = archive.extractfile(entry)
                if stream is None:
                    raise ValueError(f"missing source archive contents: {entry.name}")
                with stream:
                    data = stream.read()
                member = f"runtime/source/{prefix}/{relative}"
                if member in notices or len(data) != entry.size:
                    raise ValueError(f"duplicate or short source archive entry: {entry.name}")
                # Keep every original byte, including inline notices and data files.
                notices[member] = data
                members.append({"member": member, "sha256": sha256(data), "size_bytes": len(data)})
        if not members:
            raise ValueError(f"empty runtime source coverage: {path}")
        records.append({**identity, "upstream_url": url, "source_root": prefix,
                        "included_trees": trees, "files": members})
    return records


def attribute_link(notices: dict[str, bytes], receipt_path: Path, sysroot: Path,
                   installed: list[dict], package_names: set[str]) -> dict:
    raw = regular(receipt_path, receipt_path.parent, 16_777_216)
    receipt = json.loads(raw)
    if receipt.get("status") != "pass":
        raise ValueError("notice attribution requires a successful link receipt")
    if receipt.get("image") != "sha256:adcbd1fe3aa2692ad84f3bd7e61777b97cb8557c7d1b59775498af46a1a1e1f5":
        raise ValueError("link receipt differs from the inspected tools image")
    for relative, expected in receipt["source"].items():
        if file_identity(ROOT / relative, ROOT)["sha256"] != expected:
            raise ValueError(f"link receipt source changed: {relative}")
    for required in ("Cargo.lock", "Cargo.toml", "toolchains.lock.json"):
        if required not in receipt["source"]:
            raise ValueError(f"link receipt omits source identity: {required}")
    retained = {}
    for key in ("map", "executable"):
        entry = receipt[key]
        path = Path(entry["path"])
        identity = file_identity(path, receipt_path.parent)
        if identity["sha256"] != entry["sha256"]:
            raise ValueError(f"link receipt {key} hash differs")
        retained[key] = identity
    map_path = Path(receipt["map"]["path"])
    map_bytes = regular(map_path, receipt_path.parent, 16_777_216)
    text = map_bytes.decode()
    output = re.search(r"^OUTPUT\((\S+) elf64-x86-64\)$", text, re.MULTILINE)
    if output is None or file_identity(Path(output[1]), Path("/"))["sha256"] != retained["executable"]["sha256"]:
        raise ValueError("link map output is not the retained final executable")
    commands = receipt["commands"]
    if not any(c.get("exit_code") == 0 and "rustc" in c["argv"] and
               "--bin" in c["argv"] and "splicery-cli" in c["argv"] and
               f"link-arg=-Wl,-Map,{map_path}" in c["argv"] for c in commands):
        raise ValueError("link receipt omits the successful explicit binary/map command")
    notices["runtime/provenance/link-evidence.json"] = raw
    notices["runtime/provenance/splicery-link.map"] = map_bytes
    for index, entry in enumerate(receipt["logs"]):
        path = Path(entry["path"])
        identity = file_identity(path, receipt_path.parent)
        if identity["sha256"] != entry["sha256"] or identity["size_bytes"] > 16_777_216:
            raise ValueError("link receipt log differs")
        notices[f"runtime/provenance/link-log-{index}.txt"] = path.read_bytes()
    identities = {entry["source_path"]: entry for entry in installed}
    rust_crates = {Path(path).name[3:-5]: entry for path, entry in identities.items()
                  if path.endswith(".rlib")}
    native = {
        "libc.a": "musl-1.2.5",
        "libunwind.a": "llvm-libunwind",
        "rcrt1.o": "musl-1.2.5",
        "crti.o": "musl-1.2.5",
        "crtn.o": "musl-1.2.5",
        "crtbeginS.o": "gcc-9.4.0-crt",
        "crtendS.o": "gcc-9.4.0-crt",
    }
    loads = re.findall(r"^LOAD (\S+)$", text, re.MULTILINE)
    if not loads:
        raise ValueError("link map has no input records")
    selected = []
    target_dir = Path(os.environ["CARGO_TARGET_DIR"]) / TARGET / "release/deps"
    for name in loads:
        path = Path(name)
        if path.parent == sysroot / f"lib/rustlib/{TARGET}/lib/self-contained":
            if path.name not in native or name not in identities:
                raise ValueError(f"unreviewed native runtime input: {name}")
            family = native[path.name]
            members = sorted(set(re.findall(r"^" + re.escape(name) + r"\(([^)]+)\)$",
                                            text, re.MULTILINE)))
            selected.append({**identities[name], "source_family": family, "archive_members": members})
        elif name in identities and path.suffix == ".rlib":
            selected.append({**identities[name], "source_family": "rust-library-and-builtins"})
        elif path.parent == target_dir and re.fullmatch(r"splicery_cli-[0-9a-f]+\..+\.rcgu\.o", path.name):
            matched = next((key for key in rust_crates if f".{key}." in path.name), None)
            if matched is not None:
                selected.append({"source_path": name, "source_family": "rust-library-and-builtins",
                                 "installed_rlib": rust_crates[matched],
                                 "attribution": "whole-library source coverage; no LTO source-line claim"})
            else:
                crate = re.search(r"\.([a-zA-Z0-9_]+)-[0-9a-f]{16}\.", path.name)
                if crate is not None and crate[1] not in package_names:
                    raise ValueError(f"LTO input has no Cargo/source attribution: {name}")
                if crate is None and not re.fullmatch(
                        r"splicery_cli-[0-9a-f]+\.(?:[a-z0-9]+|splicery_cli\.[0-9a-f]+-cgu\.[0-9]+)\.rcgu\.o",
                        path.name):
                    raise ValueError(f"unrecognized binary LTO input: {name}")
                selected.append({"source_path": name, "source_family": "cargo-and-project",
                                 "attribution": "complete Cargo notice inventory and receipt source manifest"})
        else:
            raise ValueError(f"unreviewed linked input outside admitted runtime/Cargo families: {name}")
    if not all(any(Path(item["source_path"]).name == required for item in selected)
               for required in native):
        raise ValueError("link map lacks the reviewed musl/unwind/CRT family")
    return {"receipt_sha256": sha256(raw), **retained, "selected_inputs": selected,
            "scope": "Observed link inputs mapped conservatively to retained original whole-library sources/notices; not a legal clearance or release admission."}


def collect_runtime(notices: dict[str, bytes], sources: Path | None,
                    link_receipt: Path | None, package_names: set[str]) -> dict:
    lock_bytes = regular(ROOT / "toolchains.lock.json", ROOT)
    lock = json.loads(lock_bytes)
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    rustc = file_identity(sysroot / "bin/rustc", sysroot)
    if rustc["sha256"] != lock["tools"].get(str(sysroot / "bin/rustc")):
        raise ValueError("runtime notices require the admitted rustc executable")
    version = subprocess.check_output(["rustc", "-vV"])
    if f"commit-hash: {RUST_COMMIT}\n".encode() not in version:
        raise ValueError("runtime notice source commit differs from rustc")
    files = []

    def retain(path: Path, root: Path, member: str, expected: str | None = None,
               url: str | None = None) -> bytes:
        # Rust's original COPYRIGHT.html is 15 MB; do not truncate or rewrite it.
        data = regular(path, root, max_size=16_777_216)
        digest = sha256(data)
        if expected is not None and digest != expected:
            raise ValueError(f"runtime notice/provenance checksum differs: {path}")
        if member in notices:
            raise ValueError(f"duplicate runtime notice member: {member}")
        notices[member] = data
        files.append({"source_path": str(path), "member": member,
                      "sha256": digest, "upstream_url": url})
        return data

    notices["runtime/provenance/rustc-vV.txt"] = version
    notices["runtime/provenance/toolchains.lock.json"] = lock_bytes
    channel = retain(
        sysroot / "lib/rustlib/multirust-channel-manifest.toml", sysroot,
        "runtime/provenance/multirust-channel-manifest.toml",
        "b17aaaeded2ac77b968b406613ec319b001f105793900a6f852a72a03e241ba9")
    manifest = retain(
        sysroot / f"lib/rustlib/manifest-rust-std-{TARGET}", sysroot,
        f"runtime/provenance/manifest-rust-std-{TARGET}",
        "0b4cfaa07888b5f96f42feb9364d4ab7e9cec1c9ca8daed9d203596139f82429")
    installed = []
    for line in manifest.decode().splitlines():
        kind, relative = line.split(":", 1)
        if kind != "file" or not relative.startswith(f"lib/rustlib/{TARGET}/lib/"):
            raise ValueError("unsupported target distribution manifest entry")
        installed.append(file_identity(sysroot / relative, sysroot))
    doc = sysroot / "share/doc/rust"
    for name, expected in (
        ("COPYRIGHT-library.html", "68129500b616d5838629e68f55ff3aed5e096dacf60ce9eb41bbe599a563afa6"),
        ("COPYRIGHT.html", "5b0c93fc4e6d4b072eaa521b1762d1e746f4c368f65e81a0e5308e2772dac85e"),
    ):
        retain(doc / name, sysroot, f"runtime/rust/{name}", expected)
    licenses = doc / "licenses"
    if licenses.is_symlink() or not licenses.is_dir():
        raise ValueError("missing or symlink Rust license directory")
    for path in sorted(licenses.iterdir()):
        retain(path, sysroot, f"runtime/rust/licenses/{path.name}")
    # Distro candidates are deliberately separate from Rust's bundled musl/GCC.
    for relative in ("usr/share/doc/musl/copyright", "usr/share/doc/gcc-13-base/copyright"):
        retain(Path("/") / relative, Path("/"), f"runtime/distro/{relative}")
    for relative in ("usr/bin/x86_64-linux-musl-gcc", "usr/lib/x86_64-linux-musl/musl-gcc.specs"):
        retain(Path("/") / relative, Path("/"), f"runtime/provenance/{relative}")
    distro = subprocess.check_output([
        "dpkg-query", "-W", "-f", "${Package} ${Version} ${source:Package} ${source:Version}\n",
        "musl", "musl-dev", "musl-tools", "gcc-13", "libgcc-13-dev",
    ])
    notices["runtime/provenance/distro-packages.txt"] = distro
    missing = []
    for relative, (url, expected) in RUNTIME_SOURCES.items():
        if sources is None:
            missing.append({"relative_path": relative, "url": url, "sha256": expected})
        else:
            retain(sources / relative, sources, f"runtime/upstream/{relative}", expected, url)
    distribution = tomllib.loads(channel.decode())
    source_archives = collect_source_archives(notices, sources) if sources is not None else []
    link = attribute_link(notices, link_receipt, sysroot, installed, package_names) if link_receipt else None
    covered = bool(source_archives and link)
    return {
        "status": "source_covered_observed_link" if covered else "unresolved",
        "target": TARGET,
        "collection": "image_notices_and_pinned_supplements" if sources is not None else "image_notices_only",
        "rustc": rustc,
        "rustc_vV_sha256": sha256(version),
        "sysroot": str(sysroot),
        "toolchains_lock_sha256": sha256(lock_bytes),
        "distribution_date": distribution["date"],
        "target_distribution": distribution["pkg"]["rust-std"]["target"][TARGET],
        "installed_target_files": installed,
        "files": files,
        "missing_supplements": missing,
        "source_archives": source_archives,
        "missing_source_archives": [] if sources is not None else list(RUNTIME_ARCHIVES),
        "observed_link": link,
        "attribution_notes": [
            "Rust std/alloc/core and builtins: original Rust distribution COPYRIGHT files, licenses, complete rust-src, and compiler-builtins/libm originals; conservative whole-library coverage avoids invented LTO source precision.",
            "musl libc/CRT: complete original 1.2.5 source including inline notices, complete pinned cross-make patches plus Rust's two CVE diffs. No distro musl notice substitution.",
            "LLVM unwind and compiler builtins: complete libunwind and compiler-rt/lib/builtins source subtrees plus original licenses and compiler-rt credits. libunwind/CREDITS.TXT is absent at the pin; its original LICENSE copyright/conditions/disclaimer are preserved verbatim, with no substituted credits.",
            "GCC CRT: original 9.4.0 crtstuff.c attribution, GPL-3 and Runtime Library Exception 3.1, plus complete pinned cross-make patch provenance. This records notices, not a legal determination about exception eligibility.",
        ],
        "required_evidence": ([] if covered else
                              (["Supply pinned supplemental texts and whole-library source archives."] if sources is None else []) +
                              (["Supply a successful source-bound final-link receipt and the exact map/executable/log bytes."] if link is None else [])),
        "reason": "Coverage is limited to the hashed observed executable and reviewed source families; no blanket redistribution clearance or public-ready claim." if covered else "Available originals retained; source coverage or final-link binding is absent.",
    }


def registry_notice_bytes(root: Path, name: str, version: str, checksum: str,
                          candidates: set[Path]) -> dict[Path, bytes]:
    registry = root.parents[2]
    archive_path = registry / "cache" / root.parent.name / f"{name}-{version}.crate"
    identity = file_identity(archive_path, registry)
    if identity["size_bytes"] > 67_108_864 or identity["sha256"] != checksum:
        raise ValueError(f"registry archive differs from Cargo.lock: {name}")
    result = {}
    with tarfile.open(archive_path, "r:gz") as archive:
        members = {}
        for entry in archive:
            if entry.name in members:
                raise ValueError(f"duplicate registry archive member: {entry.name}")
            members[entry.name] = entry
        for path in sorted(candidates):
            data = regular(path, root)
            relative = path.relative_to(root).as_posix()
            member = members.get(f"{name}-{version}/{relative}")
            if member is None or not member.isfile() or member.size != len(data):
                raise ValueError(f"notice absent or changed in locked registry archive: {path}")
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError(f"missing registry archive contents: {path}")
            with stream:
                if stream.read() != data:
                    raise ValueError(f"notice differs from locked registry archive: {path}")
            result[path] = data
    return result


def verus_cargo_notice(package: dict, root: Path, checksum: str,
                       sources: Path | None, notices: dict[str, bytes]) -> list[dict]:
    name, version = package["name"], package["version"]
    if (version != "0.0.0-2026-09-06-0133" or package["license"] != "MIT" or
            package["repository"] != "https://github.com/verus-lang/verus"):
        raise ValueError("unadmitted Verus Cargo notice source")
    vcs_path = root / ".cargo_vcs_info.json"
    vcs_bytes = registry_notice_bytes(root, name, version, checksum, {vcs_path})[vcs_path]
    vcs = json.loads(vcs_bytes)
    if (vcs["git"]["sha1"] != VERUS_CARGO_COMMIT or
            vcs["path_in_vcs"] != VERUS_CARGO_PATHS[name]):
        raise ValueError("Verus Cargo notice differs from original archive VCS identity")
    if sources is None:
        return []
    url, expected = RUNTIME_SOURCES["verus/LICENSE"]
    path = sources / "verus/LICENSE"
    data = regular(path, sources)
    if sha256(data) != expected:
        raise ValueError("Verus Cargo original license checksum differs")
    member = f"licenses/{name}-{version}/LICENSE"
    notices[member] = data
    return [{"source_path": str(path), "member": member, "sha256": expected,
             "upstream_url": url, "crate_vcs_sha256": sha256(vcs_bytes),
             "crate_vcs": vcs}]


def runtime_package_ids(metadata: dict) -> set[str]:
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [package["id"] for package in packages.values()
             if any(target["name"] == "splicery-cli" and target["kind"] == ["bin"]
                    for target in package["targets"])]
    if len(roots) != 1:
        raise ValueError("notice scope requires exactly one Splicery CLI target")
    result = set()
    pending = roots.copy()
    while pending:
        identity = pending.pop()
        if identity in result:
            continue
        package = packages[identity]
        if any("proc-macro" in target["kind"] for target in package["targets"]):
            continue
        result.add(identity)
        # Keep every target-conditioned normal edge conservatively. Build/dev
        # edges and procedural-macro host dependencies are not runtime linkage.
        pending.extend(dependency["pkg"] for dependency in nodes[identity]["deps"]
                       if any(kind["kind"] is None for kind in dependency["dep_kinds"]))
    return result


def collect(output: Path, runtime_sources: Path | None = None,
            link_receipt: Path | None = None) -> dict:
    # Run where Cargo's manifest_path entries and registry contents are accessible.
    result = subprocess.run(["cargo", "metadata", "--locked", "--offline",
                             "--format-version", "1"], cwd=ROOT,
                            capture_output=True, check=False)
    (output / "cargo-metadata.json").open("xb").write(result.stdout)
    (output / "cargo-metadata.stderr").open("xb").write(result.stderr)
    if result.returncode:
        raise ValueError("offline Cargo metadata failed; retained cargo-metadata.stderr")
    metadata = json.loads(result.stdout)
    runtime_ids = runtime_package_ids(metadata)
    lock_bytes = regular(ROOT / "Cargo.lock", ROOT)
    locked = {(p["name"], p["version"], p.get("source")): p
              for p in tomllib.loads(lock_bytes.decode())["package"]}
    records = []
    notices = {}
    project = regular(ROOT / "LICENSE", ROOT)
    notices["licenses/splicery/LICENSE"] = project
    for package in sorted(metadata["packages"], key=lambda p: p["id"]):
        name, version, source = package["name"], package["version"], package.get("source")
        if not all(re.fullmatch(r"[A-Za-z0-9_.+-]+", value) for value in (name, version)):
            raise ValueError("unsupported package archive identity")
        pin = locked[(name, version, source)]
        manifest = Path(package["manifest_path"])
        root = manifest.parent
        manifest_bytes = regular(manifest, ROOT if source is None else root)
        files = []
        if source is None:
            if package.get("license") != "MIT":
                raise ValueError(f"unadmitted project license: {name}")
            if package.get("license_file"):
                declared = regular(root / package["license_file"], ROOT)
                if declared != project:
                    raise ValueError(f"unsupported distinct project license_file: {name}")
            files.append({"source_path": str(ROOT / "LICENSE"),
                          "package_relative_path": os.path.relpath(ROOT / "LICENSE", root),
                          "member": "licenses/splicery/LICENSE", "sha256": sha256(project)})
        else:
            if source != "registry+https://github.com/rust-lang/crates.io-index":
                raise ValueError(f"unsupported Cargo source: {source}")
            checksum = pin.get("checksum")
            if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
                raise ValueError(f"missing locked package checksum: {name}")
            candidates = set()
            for directory, dirs, names in os.walk(root, followlinks=False, onerror=refuse_walk_error):
                for entry in dirs + names:
                    path = Path(directory) / entry
                    if path.is_symlink() and entry in dirs:
                        raise ValueError(f"unsupported symlink package directory: {path}")
                    relative = path.relative_to(root)
                    if any(part.upper().startswith(PREFIXES) for part in relative.parts):
                        if path.is_symlink():
                            raise ValueError(f"symlink notice input: {path}")
                        if not path.is_dir():
                            candidates.add(path)
            if package.get("license_file"):
                candidates.add(root / package["license_file"])
            if not candidates:
                if name in VERUS_CARGO_PATHS:
                    files.extend(verus_cargo_notice(package, root, checksum, runtime_sources, notices))
                elif package["id"] in runtime_ids:
                    raise ValueError(f"no actual notice texts found: {name} {version}")
            for path, data in registry_notice_bytes(root, name, version, checksum, candidates).items():
                relative = path.relative_to(root).as_posix()
                member = f"licenses/{name}-{version}/{relative}"
                if member in notices:
                    raise ValueError(f"duplicate notice member: {member}")
                notices[member] = data
                files.append({"source_path": str(path), "package_relative_path": relative,
                              "member": member, "sha256": sha256(data)})
        records.append({"id": package["id"], "name": name, "version": version,
                        "source": source, "locked_package_sha256": pin.get("checksum"),
                        "manifest_path": str(manifest), "manifest_sha256": sha256(manifest_bytes),
                        "license": package.get("license"), "files": files,
                        "dependency_scope": "runtime" if package["id"] in runtime_ids else "build_or_test_only",
                        "notice_status": ("originals_retained" if files else
                                          "missing_originals_runtime" if package["id"] in runtime_ids else
                                          "missing_originals_nonruntime")})
    runtime = collect_runtime(notices, runtime_sources, link_receipt,
                              {p["name"].replace("-", "_") for p in metadata["packages"]
                               if p["id"] in runtime_ids})
    missing_runtime = [package["id"] for package in records
                       if package["notice_status"] == "missing_originals_runtime"]
    if missing_runtime:
        runtime["status"] = "unresolved"
        runtime["required_evidence"].append("Supply original pinned Verus Cargo license texts.")
    runtime["missing_cargo_notice_packages"] = missing_runtime
    archive = output / "third-party-licenses.zip"
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
        for member, data in sorted(notices.items()):
            info = zipfile.ZipInfo(member, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            bundle.writestr(info, data)
    (output / "LICENSE").open("xb").write(project)
    document = {
        "schema_version": 3,
        "scope": "Project/Cargo and runtime original notices/source coverage; observed-link attribution is conditional on runtime_notice_closure.status, not blanket legal clearance.",
        "cargo_metadata_sha256": sha256(result.stdout),
        "cargo_lock_sha256": sha256(lock_bytes),
        "collector_sha256": sha256(Path(__file__).read_bytes()),
        "archive_sha256": sha256(archive.read_bytes()),
        "packages": records,
        "nonruntime_missing_notice_packages": [package["id"] for package in records
                                               if package["notice_status"] == "missing_originals_nonruntime"],
        "runtime_notice_closure": runtime,
    }
    with (output / "third-party-licenses.json").open("x", encoding="utf-8") as stream:
        json.dump(document, stream, indent=2, sort_keys=True)
        stream.write("\n")
    return document


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--runtime-sources", type=Path, default=os.environ.get("SPLICERY_RUNTIME_SOURCES"),
                        help="absolute offline directory containing RUNTIME_SOURCES and RUNTIME_ARCHIVES")
    parser.add_argument("--link-evidence", type=Path,
                        help="successful source-bound receipt from scripts/run_build.py")
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, exist_ok=False)
    document = collect(output, args.runtime_sources, args.link_evidence)
    print(f"Collected notice evidence: {document['runtime_notice_closure']['status']}; no release admission.")


if __name__ == "__main__":
    main()
