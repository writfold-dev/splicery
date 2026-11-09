#!/bin/sh
# Translate the kernel crate to Lean: Charon (Rust → LLBC) then Aeneas
# (LLBC → Lean), exactly as the `proofs` gate and the mutant calibration do.
#
#   sh proofs/extract.sh <kernel-crate-dir> <dest-dir>
#
# The crate is copied into a throwaway shadow workspace that carries the
# real root's `[workspace.package]` and `[workspace.lints]` values (the
# kernel inherits them) and nothing else, so Charon never loads the full
# workspace and every run sees the same relative paths
# (crates/splicery-splice/src/...), which Aeneas records in its output.
set -eu
crate="$(cd "$1" && pwd)"
dest="$2"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

python3 - "$root/Cargo.toml" "$crate/Cargo.toml" "$work/Cargo.toml" <<'PY'
import json, sys, tomllib
root, kernel, out = sys.argv[1], sys.argv[2], sys.argv[3]
with open(root, "rb") as handle:
    workspace = tomllib.load(handle)["workspace"]
with open(kernel, "rb") as handle:
    manifest = tomllib.load(handle)
inherited = [key for key, value in manifest["package"].items()
             if isinstance(value, dict) and value.get("workspace")]
if manifest.get("dependencies"):
    raise SystemExit("the kernel crate must have no dependencies")

def value(item):
    # TOML basic strings and JSON strings agree on the escapes used here.
    if isinstance(item, bool):
        return "true" if item else "false"
    if isinstance(item, str):
        return json.dumps(item)
    if isinstance(item, dict):
        return "{ " + ", ".join(f"{k} = {value(v)}" for k, v in item.items()) + " }"
    if isinstance(item, list):
        return "[" + ", ".join(value(v) for v in item) + "]"
    raise SystemExit(f"unsupported workspace value {item!r}")

lines = ['[workspace]', 'members = ["crates/splicery-splice"]', 'resolver = "3"', '', '[workspace.package]']
lines += [f'{key} = {value(workspace["package"][key])}' for key in inherited]
for table, entries in workspace.get("lints", {}).items():
    lines += ['', f'[workspace.lints.{table}]']
    lines += [f'{key} = {value(entry)}' for key, entry in entries.items()]
open(out, "w").write("\n".join(lines) + "\n")
PY
mkdir -p "$work/crates"
cp -R "$crate" "$work/crates/splicery-splice"
rm -rf "$work/crates/splicery-splice/target"

cd "$work/crates/splicery-splice"
# --preset=aeneas is the option set Aeneas's own test-suite drives Charon
# with; the rest of the configuration is [package.metadata.charon].
charon cargo --preset=aeneas --dest-file "$work/splicery_splice.llbc" -- --lib

rm -rf "$dest"
mkdir -p "$dest"
aeneas -backend lean -no-progress-bar -abort-on-error -dest "$dest" "$work/splicery_splice.llbc"
echo "extracted: $(ls "$dest" | tr '\n' ' ')"
