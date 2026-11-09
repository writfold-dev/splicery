#!/usr/bin/env python3
"""Mutant calibration for the `proofs` gate.

Every proved kernel function gets at least one semantic mutation of its Rust
source. For each: copy the crate, apply the mutation, translate it exactly as
the gate does (proofs/extract.sh), build the theorems against the mutated
translation in a scratch copy of the lake project, and require the build to
FAIL inside the named theorem(s) — no other theorem may fail, and the
translation itself must still type-check. A proof that survives its mutant
proves nothing; a failure anywhere else is noise, not calibration.
"""
from dataclasses import dataclass
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PROOFS = ROOT / "proofs"
KERNEL = ROOT / "crates" / "splicery-splice"


@dataclass(frozen=True)
class Mutant:
    name: str
    file: str
    before: str
    after: str
    fails: tuple[str, ...]


K = "Splicery.Kernel."

MUTANTS = (
    # capture identity is byte-exact: ignoring the last byte is another capture
    Mutant("capture-prefix", "splice.rs",
           "    while i < CAPTURE_BYTES {\n        if a[i] != b[i] {",
           "    while i + 1 < CAPTURE_BYTES {\n        if a[i] != b[i] {",
           (K + "same_capture_loop_spec",)),
    # a cut bound to another capture is refused even when its range fits
    Mutant("foreign-capture-accepted", "splice.rs",
           "    if !same_capture(&cut.capture, capture) {\n        return Err(Refusal::OutOfBounds);",
           "    if false && !same_capture(&cut.capture, capture) {\n        return Err(Refusal::OutOfBounds);",
           (K + "check_cut_spec",)),
    # a reversed cut refuses; it never wraps into a huge width
    Mutant("reversed-cut-accepted", "splice.rs",
           "    if cut.lo > cut.hi || cut.hi > input.len() {\n        return Err(Refusal::OutOfBounds);\n    }\n    if text",
           "    if cut.hi > input.len() {\n        return Err(Refusal::OutOfBounds);\n    }\n    if text",
           (K + "check_cut_spec",)),
    # both ends of a text cut sit on character boundaries
    Mutant("text-end-unchecked", "splice.rs",
           "    if text && !(char_boundary(input, cut.lo) && char_boundary(input, cut.hi)) {",
           "    if text && !char_boundary(input, cut.lo) {",
           (K + "check_cut_spec",)),
    # 0xC0 starts a character; it is not a continuation byte
    Mutant("continuation-range", "splice.rs",
           "    byte >= 0x80 && byte <= 0xBF",
           "    byte >= 0x80 && byte <= 0xC0",
           (K + "is_continuation_spec",)),
    # every copied range is checked, including the first part. (Mutants that
    # drop the copy check, or its `text` argument, remove a parameter from
    # the translated loop and so break `check_edit_spec` and the loop lemma
    # together; ignoring a copy's refusal does not translate: Aeneas
    # c5eb4a2 fails to join the match arms.)
    Mutant("copy-check-skips-first-part", "splice.rs",
           "    let mut k = 0;\n    while k < edit.parts.len() {\n        match &edit.parts[k] {\n"
           "            Piece::Literal(_) => {}",
           "    let mut k = 1;\n    while k < edit.parts.len() {\n        match &edit.parts[k] {\n"
           "            Piece::Literal(_) => {}",
           (K + "check_edit_spec",)),
    # a replacement of exactly MAX_PAYLOAD_BYTES is admitted
    Mutant("payload-cap-off-by-one", "splice.rs",
           "        if amount > MAX_PAYLOAD_BYTES - length {",
           "        if amount >= MAX_PAYLOAD_BYTES - length {",
           (K + "replacement_len_loop_spec",)),
    # the batch payload, not each edit alone, is capped
    Mutant("batch-payload-uncapped", "splice.rs",
           "        if amount > MAX_PAYLOAD_BYTES - payload {",
           "        if amount > MAX_PAYLOAD_BYTES {",
           (K + "prepare_loop_spec",)),
    # exactly MAX_SPLICES edits are admitted
    Mutant("splice-count-off-by-one", "splice.rs",
           "    if edits.len() > MAX_SPLICES {",
           "    if edits.len() >= MAX_SPLICES {",
           (K + "prepare_spec",)),
    # overlapping targets are not compatible
    Mutant("overlap-compatible", "splice.rs",
           "    if left.hi > right.lo {\n        return false;",
           "    if left.hi > right.hi {\n        return false;",
           (K + "compatible_spec",)),
    # an insertion at the start of a following replacement is refused
    Mutant("touching-insertion-compatible", "splice.rs",
           "    if left.hi == right.lo && (left.lo == left.hi || right.lo == right.hi) {",
           "    if left.hi == right.lo && left.lo == left.hi {",
           (K + "compatible_spec",)),
    # the first consecutive pair is checked too
    Mutant("canonical-skips-first-pair", "splice.rs",
           "    let mut k = 1;\n    while k < order.len() {",
           "    let mut k = 2;\n    while k < order.len() {",
           (K + "check_canonical_spec",)),
    # positions are ordered by ascending start
    Mutant("sort-descending", "splice.rs",
           "        if !placed && edits[i].target.lo <= edits[order[k]].target.lo {",
           "        if !placed && edits[i].target.lo >= edits[order[k]].target.lo {",
           (K + "insert_by_start_loop_spec",)),
    # the inserted position is never dropped
    Mutant("sort-drops-last", "splice.rs",
           "    if !placed {\n        out.push(i);\n    }",
           "    if placed {\n        out.push(i);\n    }",
           (K + "insert_by_start_spec",)),
    # a copy reads exactly its range of the original
    Mutant("copy-wrong-range", "splice.rs",
           "                out.extend_from_slice(&input[cut.lo..cut.hi]);",
           "                out.extend_from_slice(&input[cut.lo..cut.lo]);",
           (K + "materialize_loop_spec",)),
    # the untouched gap before a target is kept
    Mutant("render-drops-gap", "splice.rs",
           "        bytes.extend_from_slice(&input[cursor..edit.target.lo]);",
           "        bytes.extend_from_slice(&input[edit.target.lo..edit.target.lo]);",
           (K + "render_loop_spec",)),
    # copies read the original, never the candidate being built (sequential swap)
    Mutant("render-copies-candidate", "splice.rs",
           "        let replacement = match materialize(input, edit) {",
           "        let replacement = match materialize(&bytes, edit) {",
           (K + "render_loop_spec",)),
    # the cursor moves past the replaced bytes
    Mutant("render-cursor-stays", "splice.rs",
           "        cursor = edit.target.hi;",
           "        cursor = edit.target.lo;",
           (K + "render_loop_spec",)),
    # the final gap reaches the end of the original
    Mutant("render-drops-tail", "splice.rs",
           "    bytes.extend_from_slice(&input[cursor..]);",
           "    bytes.extend_from_slice(&input[input.len()..]);",
           (K + "render_loop_spec",)),
    # the candidate cap counts the bytes already emitted
    Mutant("render-cap-ignores-emitted", "splice.rs",
           "        if edit.target.lo - cursor > MAX_CANDIDATE_BYTES - bytes.len() {",
           "        if edit.target.lo - cursor > MAX_CANDIDATE_BYTES {",
           (K + "render_loop_spec",)),
    # no_change is equality with the input, not its negation
    Mutant("render-no-change-inverted", "splice.rs",
           "    let no_change = same_bytes(&bytes, input);",
           "    let no_change = !same_bytes(&bytes, input);",
           (K + "render_loop_spec",)),
    # apply_at writes the replacement, not a second copy of the prefix
    Mutant("apply-at-payload-copy", "splice.rs",
           "    output.extend_from_slice(replacement);",
           "    output.extend_from_slice(&input[..lo]);",
           (K + "apply_at_spec",)),
    # apply_at checks the width against the remainder after lo
    Mutant("apply-at-unchecked-width", "splice.rs",
           "    if width > input.len() - lo {",
           "    if width > input.len() {",
           (K + "apply_at_spec",)),
)

# lake reports `error: <path>.lean:<line>:<col>: <message>`
ERROR = re.compile(r"^error:\s*(?P<file>[^\s:]+\.lean):(?P<line>\d+):(?P<col>\d+):", re.M)
DECLARATION = re.compile(r"^(?:@\[[^\]]*\]\s*)?(?:private\s+|protected\s+)?(?:theorem|lemma|def|abbrev|instance)\s+(?P<name>[^\s:(]+)", re.M)
NAMESPACE = re.compile(r"^namespace\s+(?P<name>\S+)", re.M)


def declarations(path: Path) -> list[tuple[int, str]]:
    """(line, fully qualified name) of every declaration in a theorem file."""
    namespace = ""
    found = []
    for number, line in enumerate(path.read_text().splitlines(), start=1):
        if match := NAMESPACE.match(line):
            namespace = match.group("name") + "."
            continue
        if line.startswith("end "):
            namespace = ""
            continue
        if match := DECLARATION.match(line):
            found.append((number, namespace + match.group("name")))
    return found


def attribute(file: Path, line: int) -> str | None:
    enclosing = None
    for start, name in declarations(file):
        if start <= line:
            enclosing = name
    return enclosing


def failing_theorems(output: str, project: Path) -> tuple[set[str], list[str]]:
    failures: set[str] = set()
    unattributed: list[str] = []
    for match in ERROR.finditer(output):
        file = (project / match.group("file")).resolve()
        try:
            relative = file.relative_to(project)
        except ValueError:
            unattributed.append(match.group(0))
            continue
        if relative.parts[0] != "Kernel" or not file.exists():
            unattributed.append(str(relative))
            continue
        name = attribute(file, int(match.group("line")))
        if name is None:
            unattributed.append(str(relative))
        else:
            failures.add(name)
    return failures, unattributed


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          text=True, timeout=1800)


def calibrate(mutant: Mutant, scratch: Path) -> None:
    crate = scratch / "crate"
    shutil.copytree(KERNEL, crate, ignore=shutil.ignore_patterns("target"))
    source = crate / "src" / mutant.file
    text = source.read_text()
    if text.count(mutant.before) != 1:
        raise SystemExit(f"FAIL {mutant.name}: mutation site does not match exactly once")
    source.write_text(text.replace(mutant.before, mutant.after))

    extraction = run(["sh", str(PROOFS / "extract.sh"), str(crate), str(scratch / "Extracted")], ROOT)
    if extraction.returncode != 0:
        print(extraction.stdout)
        raise SystemExit(f"FAIL {mutant.name}: the mutated kernel did not translate")

    project = scratch / "proofs"
    project.mkdir()
    for name in ("lakefile.toml", "lean-toolchain", "lake-manifest.json", "Kernel.lean", "Audit.lean"):
        shutil.copy2(PROOFS / name, project / name)
    shutil.copytree(PROOFS / "Kernel", project / "Kernel")
    shutil.copytree(scratch / "Extracted", project / "Extracted")
    (project / ".lake").mkdir()
    os.symlink(PROOFS / ".lake" / "packages", project / ".lake" / "packages")

    build = run(["lake", "build"], project)
    if build.returncode == 0:
        raise SystemExit(f"FAIL {mutant.name}: the proofs still build against the mutated kernel")
    failures, unattributed = failing_theorems(build.stdout, project)
    if unattributed:
        print(build.stdout)
        raise SystemExit(f"FAIL {mutant.name}: errors outside the theorem files: {unattributed}")
    expected = set(mutant.fails)
    if not failures or not expected <= failures:
        print(build.stdout)
        print(f"attributed failures: {sorted(failures)}")
        raise SystemExit(f"FAIL {mutant.name}: expected {sorted(expected)} to fail, got {sorted(failures)}")
    if failures - expected:
        print(build.stdout)
        raise SystemExit(f"FAIL {mutant.name}: unexpected failing theorems {sorted(failures - expected)}")
    print(f"calibration: {mutant.name} rejected by {', '.join(sorted(failures))}", flush=True)


def main() -> None:
    selected = set(sys.argv[1:])
    mutants = [m for m in MUTANTS if not selected or m.name in selected]
    if selected and len(mutants) != len(selected):
        raise SystemExit(f"unknown mutant(s): {sorted(selected - {m.name for m in mutants})}")
    # Every mutant runs even after a failure, so one gate run reports them all.
    failed: list[str] = []
    for mutant in mutants:
        with tempfile.TemporaryDirectory(prefix=f"mutant-{mutant.name}-") as directory:
            try:
                calibrate(mutant, Path(directory))
            except SystemExit as error:
                print(error, flush=True)
                failed.append(mutant.name)
    if failed:
        raise SystemExit(f"FAIL: {len(failed)} mutant(s) not calibrated: {failed}")
    print(f"PROOF_CALIBRATIONS={len(mutants)}", flush=True)


if __name__ == "__main__":
    main()
