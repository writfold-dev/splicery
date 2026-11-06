"""Deterministic offline SG06 differential and resource qualification.

Run from the repository root with the actual static musl executable:

    python3 -B tools/fuzz_offline.py --binary <path> --evidence <fresh-out-of-tree>

Every case has an isolated retained workdir. The producer makes no network calls,
never invokes a shell, and checks that the complete case tree is unchanged by the
executable. Raw candidates and overlap/encoding refusals are decided by the
independent tools/byte_reference.py model, not by reimplementing its normalizer.

The resource sample includes both sides of input, splice, candidate, payload,
regex, frozen-wire, and aggregate-work limits. Measured cases must finish within
30 seconds wall time and 256 MiB peak RSS. These intentionally conservative
ceilings allow slow qualification hosts while still rejecting unbounded growth.
GNU /usr/bin/time -v is required for those cases; no RSS estimate is substituted.
"""
from __future__ import annotations

import argparse
import copy
from dataclasses import dataclass, field
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import signal
import struct
import subprocess
import sys
import time
from typing import Any

from byte_reference import Refusal, Span, apply as byte_apply

ROOT = Path(__file__).resolve().parents[1]
MASTER_SEED = 0x534730365F4F4646
RAW_CASE_COUNT = 2048
MAX_RECIPE_BYTES = 256 * 1024
MAX_INPUT_BYTES = 8 * 1024 * 1024
MAX_CANDIDATE_BYTES = 16 * 1024 * 1024
MAX_PAYLOAD_BYTES = 16 * 1024 * 1024
MAX_SPLICES = 4096
MAX_FROZEN_BYTES = 1024 * 1024
MAX_AGGREGATE_BYTE_VISITS = 128 * 1024 * 1024
RESOURCE_WALL_CEILING_SECONDS = 30.0
RESOURCE_RSS_CEILING_KIB = 256 * 1024
CASE_TIMEOUT_SECONDS = 20.0
PROFILE_ID = "rust-regex-1.11.1/unicode-scalar-dot-posix-ascii-classes-v1"
REQUIRED_REFUSAL_DIAGNOSTICS = {
    "limit-input-over": "8 MiB input bound",
    "limit-splices-4097": "TooManySplices",
    "limit-candidate-over": "CandidateTooLarge",
    "limit-payload-at": "CandidateTooLarge",
    "limit-payload-over": "PayloadTooLarge",
    "total-work-over": "BudgetExceeded",
    "regex-automaton-limit": "AutomatonTooLarge",
    "limit-frozen-wire-over": "1 MiB",
    "limit-regex-source-over": "SourceTooLong",
}


def digest_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stable_seed(name: str) -> int:
    return int.from_bytes(hashlib.sha256(f"{MASTER_SEED}:{name}".encode()).digest()[:8], "big")


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


@dataclass
class Operation:
    kind: str
    selector: Span
    literal_before: bytes = b""
    keep: Span | None = None
    literal_after: bytes = b""

    def target(self) -> Span:
        if self.kind == "before":
            return Span(self.selector.lo, self.selector.lo)
        if self.kind == "after":
            return Span(self.selector.hi, self.selector.hi)
        return self.selector

    def replacement(self, base: bytes) -> bytes:
        copied = b"" if self.keep is None else base[self.keep.lo:self.keep.hi]
        return self.literal_before + copied + self.literal_after


@dataclass
class EditSpec:
    base: bytes
    operations: list[Operation] = field(default_factory=list)
    raw: bool = True
    path: str = "target.bin"


@dataclass
class Case:
    name: str
    family: str
    seed: int
    files: dict[str, bytes]
    arguments: list[str]
    expected_status: int
    expected_output: bytes | None
    command_kind: str = "preview"
    model_refusal: str | None = None
    measure: bool = False
    edit_spec: EditSpec | None = None
    detail: str = ""


@dataclass
class Observation:
    ok: bool
    reason: str
    exit_status: int | None
    stdout: bytes
    stderr: bytes
    elapsed_seconds: float
    timed_out: bool
    no_effect: bool
    resource: dict[str, Any] | None


def edit_recipe(spec: EditSpec) -> tuple[bytes, list[str]]:
    keep_names: dict[Span, str] = {}
    captures: list[str] = []
    lines = [f'edit "{spec.path}"' + (" as bytes" if spec.raw else "") + " {\n"]
    for operation in spec.operations:
        if operation.keep is not None and operation.keep not in keep_names:
            name = f"k{len(keep_names)}"
            keep_names[operation.keep] = name
            captures.append(f"@{name}={operation.keep.lo}:{operation.keep.hi}")
    for operation in spec.operations:
        selector = f"range [{operation.selector.lo},{operation.selector.hi})"
        if operation.kind == "delete":
            lines.append(f"delete {selector}\n")
            continue
        if operation.keep is None:
            payload = f'hex"{operation.literal_before.hex()}"'
        else:
            keep_name = keep_names[operation.keep]
            payload = (
                f'[hex"{operation.literal_before.hex()}", keep(@{keep_name}), '
                f'hex"{operation.literal_after.hex()}"]'
            )
        if operation.kind == "replace":
            lines.append(f"replace {selector} with {payload}\n")
        elif operation.kind == "before":
            lines.append(f"before {selector} insert {payload}\n")
        elif operation.kind == "after":
            lines.append(f"after {selector} insert {payload}\n")
        else:
            raise ValueError(f"unknown edit kind {operation.kind}")
    lines.append("}\n")
    return "".join(lines).encode("utf-8"), captures


def model_edit(spec: EditSpec, recipe: bytes) -> tuple[bytes | None, str | None]:
    if len(spec.base) > MAX_INPUT_BYTES:
        return None, "InputLimit"
    if len(recipe) > MAX_RECIPE_BYTES:
        return None, "RecipeLimit"
    if len(spec.operations) > MAX_SPLICES:
        return None, "SpliceLimit"
    for operation in spec.operations:
        for capture in (operation.selector, operation.keep):
            if capture is None:
                continue
            try:
                # A no-op replacement makes the independent model validate both
                # capture endpoints, including text UTF-8 scalar boundaries.
                byte_apply(spec.base, [(capture, spec.base[capture.lo:capture.hi])], raw=spec.raw)
            except Refusal as error:
                return None, str(error)
    edits = [(operation.target(), operation.replacement(spec.base)) for operation in spec.operations]
    if sum(len(replacement) for _, replacement in edits) > MAX_PAYLOAD_BYTES:
        return None, "PayloadLimit"
    try:
        candidate = byte_apply(spec.base, edits, raw=spec.raw)
    except Refusal as error:
        return None, str(error)
    if len(candidate) > MAX_CANDIDATE_BYTES:
        return None, "CandidateLimit"
    return candidate, None


def make_edit_case(name: str, spec: EditSpec, *, seed: int | None = None,
                   family: str = "directed-edit", measure: bool = False,
                   detail: str = "") -> Case:
    recipe, captures = edit_recipe(spec)
    expected, model_refusal = model_edit(spec, recipe)
    arguments = ["preview", "--root", "."]
    for capture in captures:
        arguments.extend(["--capture", capture])
    arguments.append("recipe.spl")
    return Case(
        name=name,
        family=family,
        seed=stable_seed(name) if seed is None else seed,
        files={"recipe.spl": recipe, spec.path: spec.base},
        arguments=arguments,
        expected_status=0 if expected is not None else 2,
        expected_output=expected,
        model_refusal=model_refusal,
        measure=measure,
        edit_spec=spec,
        detail=detail,
    )


def preview_case(name: str, recipe: bytes, expected: bytes | None, *,
                 base: bytes = b"", path: str = "target.txt",
                 captures: list[str] | None = None,
                 frozen: dict[str, bytes] | None = None,
                 frozen_arguments: list[str] | None = None,
                 family: str = "directed-preview", measure: bool = False,
                 model_refusal: str | None = None, detail: str = "") -> Case:
    files = {"recipe.spl": recipe, path: base}
    files.update(frozen or {})
    arguments = ["preview", "--root", "."]
    for capture in captures or []:
        arguments.extend(["--capture", capture])
    for frozen_argument in frozen_arguments or []:
        arguments.extend(["--frozen", frozen_argument])
    arguments.append("recipe.spl")
    return Case(name, family, stable_seed(name), files, arguments,
                0 if expected is not None else 2, expected,
                model_refusal=model_refusal, measure=measure, detail=detail)


def inspect_case(name: str, base: bytes, pattern: str, expected: bytes | None,
                 *, expect: int = 0, within: str | None = None,
                 family: str = "regex-wire", measure: bool = False,
                 detail: str = "") -> Case:
    arguments = ["inspect", "--root", ".", "--file", "target.txt", "--name", "@set",
                 "--expect", str(expect)]
    if within is not None:
        arguments.extend(["--within", within])
    arguments.append(pattern)
    return Case(name, family, stable_seed(name), {"target.txt": base}, arguments,
                0 if expected is not None else 2, expected, "inspect",
                model_refusal=None if expected is not None else "ExpectedRefusal",
                measure=measure, detail=detail)


def kernel_case(name: str, arguments: list[str], expected: bytes | None,
                *, family: str = "kernel-wire", detail: str = "") -> Case:
    return Case(name, family, stable_seed(name), {}, ["kernel", *arguments],
                0 if expected is not None else 1,
                b"REFUSAL\n" if expected is None else expected,
                "kernel", None if expected is not None else "ExpectedRefusal", detail=detail)


def reference_candidate(base: bytes, edits: list[tuple[Span, bytes]], raw: bool = False) -> bytes | None:
    try:
        return byte_apply(base, edits, raw=raw)
    except Refusal:
        return None


def deterministic_raw_cases() -> list[Case]:
    cases: list[Case] = []
    kinds = ["replace", "delete", "before", "after"]
    for index in range(RAW_CASE_COUNT):
        seed = int.from_bytes(hashlib.sha256(f"raw:{MASTER_SEED}:{index}".encode()).digest()[:8], "big")
        rng = random.Random(seed)
        base = bytes(rng.randrange(256) for _ in range(256 + index % 33))
        operations: list[Operation] = []
        if index % 5 == 4:
            point = 24 + index % 96
            style = (index // 5) % 3
            if style == 0:
                operations = [
                    Operation("replace", Span(point, point + 9), b"left"),
                    Operation("replace", Span(point + 4, point + 13), b"right"),
                ]
            elif style == 1:
                operations = [
                    Operation("before", Span(point, point + 2), b"first"),
                    Operation("after", Span(point - 2, point), b"second"),
                ]
            else:
                operations = [
                    Operation("replace", Span(point, point + 8), b"replacement"),
                    Operation("before", Span(point + 3, point + 4), b"inside"),
                ]
            operations.append(Operation("after", Span(200, 201), b"tail",
                                        Span(0, 17), b"keep"))
        else:
            count = 1 + index % 12
            for item in range(count):
                lo = 4 + item * 16
                hi = lo + (index + item) % 6
                kind = kinds[(index + item) % len(kinds)]
                if kind == "delete":
                    operations.append(Operation(kind, Span(lo, hi)))
                elif (index + item) % 3 == 0:
                    keep_lo = rng.randrange(len(base) + 1)
                    keep_hi = rng.randrange(keep_lo, len(base) + 1)
                    operations.append(Operation(kind, Span(lo, hi), bytes([65 + item]),
                                                Span(keep_lo, keep_hi), bytes([90 + item % 6])))
                else:
                    replacement = bytes(rng.randrange(256) for _ in range((index + item) % 9))
                    operations.append(Operation(kind, Span(lo, hi), replacement))
            if index % 3 == 0:
                rng.shuffle(operations)
            elif index % 3 == 1:
                operations.reverse()
            elif operations:
                rotation = index % len(operations)
                operations = operations[rotation:] + operations[:rotation]
        if index % 17 == 16 and operations:
            first = operations[0]
            operations[0] = Operation(first.kind, Span(first.selector.lo, len(base) + 1),
                                      first.literal_before, first.keep, first.literal_after)
        cases.append(make_edit_case(
            f"raw-{index:04d}", EditSpec(base, operations), seed=seed,
            family="deterministic-raw",
            detail="Permuted original-coordinate batch; byte_reference decides success or refusal.",
        ))
    return cases


def limit_cases() -> list[Case]:
    cases: list[Case] = []
    for label, size in [("at", MAX_INPUT_BYTES), ("over", MAX_INPUT_BYTES + 1)]:
        base = b"I" * size
        cases.append(make_edit_case(f"limit-input-{label}", EditSpec(base), measure=True,
                                    family="limit-input", detail=f"Input size {size}."))
    for count in [MAX_SPLICES, MAX_SPLICES + 1]:
        base = bytes(index % 251 for index in range(count))
        operations = [Operation("delete", Span(index, index + 1)) for index in range(count)]
        cases.append(make_edit_case(f"limit-splices-{count}", EditSpec(base, operations),
                                    measure=True, family="limit-splices",
                                    detail="One disjoint deletion per splice."))

    repeated_base = bytes((index * 37) % 256 for index in range(8192))
    at_operations = [Operation("replace", Span(4096 + index, 4097 + index), keep=Span(0, 4095))
                     for index in range(4096)]
    at_case = make_edit_case("limit-candidate-payload-at", EditSpec(repeated_base, at_operations),
                             measure=True, family="limit-candidate-payload",
                             detail="4096 repeated keeps render exactly 16 MiB; payload is 4096 bytes below its cap.")
    if at_case.expected_output is None or len(at_case.expected_output) != MAX_CANDIDATE_BYTES:
        raise ValueError("internal error: repeated-keep at-limit vector does not render exactly 16 MiB")
    cases.append(at_case)

    candidate_operations = [Operation("replace", Span(4096 + index, 4097 + index),
                                      keep=Span(0, 4096 if index == 0 else 4095))
                            for index in range(4096)]
    cases.append(make_edit_case("limit-candidate-over", EditSpec(repeated_base, candidate_operations),
                                measure=True, family="limit-candidate-payload",
                                detail="One extra repeated-keep byte makes the candidate 16 MiB + 1."))
    payload_at_operations = [Operation("replace", Span(4096 + index, 4097 + index), keep=Span(0, 4096))
                             for index in range(4096)]
    cases.append(make_edit_case("limit-payload-at", EditSpec(repeated_base, payload_at_operations),
                                measure=True, family="limit-candidate-payload",
                                detail="Payload is exactly 16 MiB; the 4096 unchanged prefix bytes then force CandidateTooLarge."))
    payload_operations = [Operation("replace", Span(4096 + index, 4097 + index),
                                    b"x" if index == 0 else b"", Span(0, 4096))
                          for index in range(4096)]
    cases.append(make_edit_case("limit-payload-over", EditSpec(repeated_base, payload_operations),
                                measure=True, family="limit-candidate-payload",
                                detail="Retained replacement material is 16 MiB + 1."))

    work_size = 128 * 1024
    work_base = b"a" * (work_size - 1) + b"b"
    for label, width in [("at", 1031), ("over", 1032)]:
        needle = b"a" * (width - 1) + b"b"
        positions = work_size - width + 1
        accounted_work = positions * (width + 1)
        if label == "at" and accounted_work + 1 > MAX_AGGREGATE_BYTE_VISITS:
            raise ValueError("internal error: aggregate-work at-limit vector is above budget")
        if label == "over" and accounted_work <= MAX_AGGREGATE_BYTE_VISITS:
            raise ValueError("internal error: aggregate-work over-limit vector is within budget")
        recipe = b'edit "target.txt" {\nreplace one text "' + needle + b'" with "x"\n}\n'
        edits: list[tuple[Span, bytes]] = [(Span(work_size - width, work_size), b"x")]
        expected = reference_candidate(work_base, edits) if label == "at" else None
        cases.append(preview_case(f"total-work-{label}", recipe, expected, base=work_base,
                                  family="total-work", measure=True,
                                  model_refusal=None if label == "at" else "BudgetExceeded",
                                  detail=f"Conservative one-text accounting is {accounted_work} byte visits plus payload."))
    return cases


def frozen_document(base: bytes) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "profile": PROFILE_ID,
        "capture_sha256": digest_bytes(base),
        "name": "set",
        "members": [
            {"lo": 0, "hi": 2, "line": "Lf", "groups": [
                {"name": "first", "lo": 0, "hi": 1},
                {"name": "rest", "lo": 1, "hi": 2},
            ]},
            {"lo": 3, "hi": 6, "line": "Lf", "groups": [
                {"name": "first", "lo": 3, "hi": 4},
                {"name": "rest", "lo": 4, "hi": 6},
            ]},
        ],
    }


def frozen_recipe() -> bytes:
    return (b'edit "target.txt" {\n'
            b' each @r in frozen @set {\n'
            b'  replace @r with [keep(@r.first), "!", keep(@r.rest)]\n'
            b' }\n}\n')


def regex_and_frozen_cases() -> list[Case]:
    cases: list[Case] = []
    base = b"ab\nabb\n"
    document = frozen_document(base)
    encoded = canonical_json(document)
    expected = reference_candidate(base, [
        (Span(0, 2), b"a!b"), (Span(3, 6), b"a!bb"),
    ])
    cases.append(preview_case("frozen-valid", frozen_recipe(), expected, base=base,
                              frozen={"frozen.json": encoded}, frozen_arguments=["@set=frozen.json"],
                              family="frozen-valid", detail="Independent exact frozen DTO and byte model."))
    inspect_expected = canonical_json(document)
    cases.append(inspect_case("inspect-valid", base, "^(?<first>a)(?<rest>b+)$", inspect_expected,
                              expect=2, detail="Exact canonical frozen wire output."))
    scoped = dict(document)
    scoped["scope"] = {"lo": 3, "hi": 6}
    scoped["members"] = [document["members"][1]]
    scoped_expected = canonical_json({
        "schema_version": scoped["schema_version"], "profile": scoped["profile"],
        "capture_sha256": scoped["capture_sha256"], "name": scoped["name"],
        "scope": scoped["scope"], "members": scoped["members"],
    })
    cases.append(inspect_case("inspect-scoped", base, "^(?<first>a)(?<rest>b+)$", scoped_expected,
                              expect=1, within="3:6", detail="Scoped match inventory and exact scope wire."))

    mutations: list[tuple[str, bytes]] = [
        ("invalid-json", b"{"),
        ("invalid-utf8", b"\xff"),
        ("duplicate-key", b'{"schema_version":1,"schema_version":1}'),
    ]
    changed = copy.deepcopy(document)
    changed["unknown"] = True
    mutations.append(("unknown-field", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["schema_version"] = 2
    mutations.append(("wrong-schema", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["profile"] = "other-regex-profile"
    mutations.append(("wrong-profile", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["capture_sha256"] = "0" * 64
    mutations.append(("wrong-digest", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["capture_sha256"] = digest_bytes(base).upper()
    mutations.append(("uppercase-digest", canonical_json(changed)))
    changed = copy.deepcopy(document)
    del changed["members"][0]["line"]
    mutations.append(("missing-line", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["groups"][0]["hi"] = None
    mutations.append(("half-null-group", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["groups"][0]["unexpected"] = 1
    mutations.append(("unknown-group-field", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"].reverse()
    mutations.append(("unsorted-members", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][1]["lo"] = 1
    mutations.append(("overlapping-members", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["hi"] = 0
    mutations.append(("zero-width-member", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["hi"] = len(base) + 1
    mutations.append(("out-of-bounds-member", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["groups"][1]["hi"] = 3
    mutations.append(("group-outside-member", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["line"] = "CrLf"
    mutations.append(("wrong-line-ending", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["members"][0]["groups"].append(copy.deepcopy(changed["members"][0]["groups"][0]))
    mutations.append(("duplicate-group", canonical_json(changed)))
    changed = copy.deepcopy(document)
    changed["name"] = "edit"
    mutations.append(("reserved-frozen-name", canonical_json(changed)))
    for label, data in mutations:
        cases.append(preview_case(f"frozen-malformed-{label}", frozen_recipe(), None, base=base,
                                  frozen={"frozen.json": data}, frozen_arguments=["@set=frozen.json"],
                                  family="frozen-malformed", model_refusal="MalformedFrozen"))
    cases.append(preview_case("frozen-alias-mismatch", frozen_recipe(), None, base=base,
                              frozen={"frozen.json": encoded}, frozen_arguments=["@other=frozen.json"],
                              family="frozen-malformed", model_refusal="AliasMismatch"))
    cases.append(preview_case("frozen-missing-file", frozen_recipe(), None, base=base,
                              frozen_arguments=["@set=absent.json"], family="frozen-malformed",
                              model_refusal="MissingFrozen"))
    at_wire = encoded + b" " * (MAX_FROZEN_BYTES - len(encoded))
    cases.append(preview_case("limit-frozen-wire-at", frozen_recipe(), expected, base=base,
                              frozen={"frozen.json": at_wire}, frozen_arguments=["@set=frozen.json"],
                              family="limit-frozen-wire", measure=True,
                              detail="Exactly 1 MiB frozen frame with legal trailing JSON whitespace."))
    cases.append(preview_case("limit-frozen-wire-over", frozen_recipe(), None, base=base,
                              frozen={"frozen.json": at_wire + b" "}, frozen_arguments=["@set=frozen.json"],
                              family="limit-frozen-wire", model_refusal="FrozenLimit",
                              detail="1 MiB + 1 frozen frame."))

    live = (b'edit "target.txt" {\n'
            b' each @r in line_matches rx"^(?<first>a)(?<rest>b+)$" expect 2 {\n'
            b'  replace @r with [keep(@r.first), ":", keep(@r.rest)]\n'
            b' }\n}\n')
    live_expected = reference_candidate(base, [(Span(0, 2), b"a:b"), (Span(3, 6), b"a:bb")])
    cases.append(preview_case("regex-live-groups", live, live_expected, base=base, family="regex-live"))
    for label, pattern in [
        ("anchor", "^"), ("star", "a*"), ("named-empty-overall", "(?<empty>)"),
    ]:
        recipe = (f'edit "target.txt" {{ each @r in line_matches rx"{pattern}" expect 0 '
                  '{ replace @r with "x" } }\n').encode()
        cases.append(preview_case(f"regex-zero-width-{label}", recipe, None, base=base,
                                  family="regex-zero-width", model_refusal="ZeroWidthMatch"))
    empty_group_base = b"b\n"
    empty_group = (b'edit "target.txt" { each @r in line_matches rx"^(?<empty>a*)b$" expect 1 '
                   b'{ replace @r with keep(@r.empty) } }\n')
    cases.append(preview_case("regex-present-empty-group", empty_group, b"\n", base=empty_group_base,
                              family="regex-groups",
                              detail="A present zero-width named group is distinct from a zero-width overall match."))
    for label, pattern in [
        ("inline-flag", "(?i)a"), ("lookaround", "(?=a)"), ("backreference", r"(?<x>a)\k<x>"),
    ]:
        cases.append(inspect_case(f"regex-unsupported-{label}", base, pattern, None, expect=1,
                                  family="regex-unsupported"))
    cases.append(inspect_case("regex-automaton-limit", b"a\n", "a{1,65535}", None, expect=1,
                              family="regex-automaton-limit", measure=True,
                              detail="Small source whose counted repetition exceeds the 1 MiB automaton ceiling."))
    source_at = "a" * 4096
    source_base = source_at.encode() + b"\n"
    source_doc = {
        "schema_version": 1, "profile": PROFILE_ID, "capture_sha256": digest_bytes(source_base),
        "name": "set", "members": [{"lo": 0, "hi": 4096, "line": "Lf", "groups": []}],
    }
    cases.append(inspect_case("limit-regex-source-at", source_base, source_at, canonical_json(source_doc),
                              expect=1, family="limit-regex-source"))
    cases.append(inspect_case("limit-regex-source-over", source_base, source_at + "a", None,
                              expect=1, family="limit-regex-source"))
    return cases


def utf8_and_selector_cases() -> list[Case]:
    cases: list[Case] = []
    base = "Aé𝄞\n".encode()
    e_lo = base.index("é".encode())
    emoji_lo = base.index("𝄞".encode())
    vectors = [
        ("utf8-replace-scalar", [Operation("replace", Span(e_lo, e_lo + 2), "ë".encode())]),
        ("utf8-delete-scalar", [Operation("delete", Span(emoji_lo, emoji_lo + 4))]),
        ("utf8-cut-continuation", [Operation("replace", Span(0, e_lo + 1), b"x")]),
        ("utf8-insert-continuation", [Operation("before", Span(e_lo + 1, e_lo + 1), b"x")]),
        ("utf8-invalid-candidate", [Operation("after", Span(len(base), len(base)), b"\xff")]),
        ("utf8-incomplete-scalar", [Operation("delete", Span(e_lo, e_lo + 1))]),
    ]
    for name, operations in vectors:
        cases.append(make_edit_case(name, EditSpec(base, operations, raw=False, path="target.txt"),
                                    family="utf8-boundary"))
    invalid_base = b"valid\xffinvalid"
    cases.append(make_edit_case("utf8-invalid-input", EditSpec(invalid_base, [], raw=False, path="target.txt"),
                                family="utf8-boundary"))

    ambiguous_base = b"aaaa\nxx\nxx\n"
    for name, recipe in [
        ("selector-ambiguous-overlap", b'edit "target.txt" { replace one text "aa" with "b" }\n'),
        ("selector-ambiguous-lines", b'edit "target.txt" { replace one line "xx" with "yy" }\n'),
        ("selector-empty-needle", b'edit "target.txt" { replace one text "" with "b" }\n'),
    ]:
        cases.append(preview_case(name, recipe, None, base=ambiguous_base,
                                  family="ambiguous-selector", model_refusal="SelectorRefusal"))
    scoped_recipe = (b'edit "target.txt" {\n @scope := range [1,3)\n'
                     b' replace one text "aa" within @scope with "b"\n}\n')
    scoped_expected = reference_candidate(ambiguous_base, [(Span(1, 3), b"b")])
    cases.append(preview_case("selector-within-disambiguates", scoped_recipe, scoped_expected,
                              base=ambiguous_base, family="ambiguous-selector"))

    between_base = b"head\r\nmiddle\ntail"
    between_recipe = (b'edit "target.txt" {\n @first := one line "head"\n @second := one line "tail"\n'
                      b" replace between @first and @second with <<'TAG'\nX\nY\nTAG\n}\n")
    between_expected = reference_candidate(between_base, [(Span(6, 13), b"X\nY\n")])
    cases.append(preview_case("language-between-heredoc-crlf", between_recipe, between_expected,
                              base=between_base, family="language-forms"))
    lines_base = b"old\r\n"
    lines_recipe = (b'edit "target.txt" { @ln := one line "old" '
                    b'replace @ln with lines like @ln ["A", "B"] }\n')
    lines_expected = reference_candidate(lines_base, [(Span(0, 3), b"A\r\nB\r\n")])
    cases.append(preview_case("language-lines-like-crlf", lines_recipe, lines_expected,
                              base=lines_base, family="language-forms"))
    indent_base = b"a\n\n b\r\n"
    indent_recipe = (f'edit "target.txt" {{ @all := range [0,{len(indent_base)}) '
                     'replace @all with indent("\\t", keep(@all)) }\n').encode()
    indent_expected = reference_candidate(indent_base, [(Span(0, len(indent_base)), b"\ta\n\n\t b\r\n")])
    cases.append(preview_case("language-indent-empty-lines", indent_recipe, indent_expected,
                              base=indent_base, family="language-forms"))
    return cases


def malformed_and_lifecycle_cases() -> list[Case]:
    cases: list[Case] = []
    valid = b'edit "target.txt" { replace range [0,0) with "x" }\n'
    malformed = [
        ("invalid-utf8", b"\xff"),
        ("bom", b"\xef\xbb\xbf" + valid),
        ("nul", b'edit "target.txt" {\x00}\n'),
        ("truncated", b'edit "target.txt" {'),
        ("odd-hex", b'edit "target.txt" { replace range [0,0) with hex"0" }\n'),
        ("leading-zero-nat", b'edit "target.txt" { replace range [00,0) with "x" }\n'),
        ("overflow-nat", b'edit "target.txt" { replace range [0,999999999999999999999999999999) with "x" }\n'),
        ("unknown-handle", b'edit "target.txt" { replace @missing with "x" }\n'),
        ("duplicate-binding", b'edit "target.txt" { @x := range [0,0) @x := range [0,0) }\n'),
        ("raw-text-selector", b'edit "target.txt" as bytes { replace one text "x" with "y" }\n'),
        ("retired-assignment", b'edit "target.txt" { @x = range [0,0) }\n'),
        ("lone-cr", b'edit "target.txt" {\r replace range [0,0) with "x" }\n'),
    ]
    for label, recipe in malformed:
        cases.append(preview_case(f"recipe-malformed-{label}", recipe, None,
                                  family="recipe-malformed", model_refusal="MalformedRecipe"))
    no_op = b'edit "target.txt" {\n}\n'
    for label, size in [("at", MAX_RECIPE_BYTES), ("over", MAX_RECIPE_BYTES + 1)]:
        comment_length = size - len(no_op) - 2
        recipe = no_op[:-2] + b"#" + b"p" * comment_length + b"\n}\n"
        if len(recipe) != size:
            raise ValueError("internal error: recipe-limit vector has wrong size")
        cases.append(preview_case(f"limit-recipe-{label}", recipe, b"" if label == "at" else None,
                                  family="limit-recipe",
                                  model_refusal=None if label == "at" else "RecipeLimit"))

    create_text = b'create "new.txt" with "hello\\n"\n'
    create_text_expected = (b'{"schema_version":1,"scope":"offline-plan","op":"create",'
                            b'"path":"new.txt","content_hex":"68656c6c6f0a"}')
    cases.append(preview_case("lifecycle-create-text", create_text, create_text_expected,
                              path="unused.txt", family="lifecycle-no-effect",
                              detail="Exact plan only; new.txt must remain absent."))
    create_raw = b'create "new.bin" as bytes with ["A", hex"00ff"]\n'
    create_raw_expected = (b'{"schema_version":1,"scope":"offline-plan","op":"create",'
                           b'"path":"new.bin","content_hex":"4100ff"}')
    cases.append(preview_case("lifecycle-create-raw", create_raw, create_raw_expected,
                              path="unused.bin", family="lifecycle-no-effect"))
    rename_expected = (b'{"schema_version":1,"scope":"offline-plan","op":"rename",'
                       b'"source":"target.txt","destination":"renamed.txt"}')
    cases.append(preview_case("lifecycle-rename", b'rename "target.txt" to "renamed.txt"\n',
                              rename_expected, base=b"source remains", family="lifecycle-no-effect"))
    remove_expected = (b'{"schema_version":1,"scope":"offline-plan","op":"remove",'
                       b'"path":"target.txt"}')
    cases.append(preview_case("lifecycle-remove", b'remove "target.txt"\n', remove_expected,
                              base=b"remove plan only", family="lifecycle-no-effect"))
    return cases


def kernel_cases() -> list[Case]:
    cases: list[Case] = []
    for index in range(128):
        rng = random.Random(stable_seed(f"kernel-{index}"))
        base = bytes(rng.randrange(256) for _ in range(rng.randrange(65)))
        replacement = bytes(rng.randrange(256) for _ in range(rng.randrange(33)))
        lo = rng.randrange(len(base) + 3)
        width = rng.randrange(len(base) + 3)
        arguments = ["apply", str(lo), str(width), base.hex(), replacement.hex()]
        try:
            output = byte_apply(base, [(Span(lo, lo + width), replacement)], raw=True)
            expected = output.hex().encode() + b"\n"
        except Refusal:
            expected = None
        cases.append(kernel_case(f"kernel-random-{index:03d}", arguments, expected,
                                 detail="Independent raw byte_reference apply differential."))
    for label, size in [("input-at", 4096), ("input-over", 4097)]:
        base = b"i" * size
        expected = base.hex().encode() + b"\n" if size == 4096 else None
        cases.append(kernel_case(f"limit-kernel-{label}", ["apply", "0", "0", base.hex(), ""], expected,
                                 family="limit-kernel-wire"))
    for label, size in [("replacement-at", 4096), ("replacement-over", 4097)]:
        replacement = b"r" * size
        expected = replacement.hex().encode() + b"\n" if size == 4096 else None
        cases.append(kernel_case(f"limit-kernel-{label}", ["apply", "0", "0", "", replacement.hex()], expected,
                                 family="limit-kernel-wire"))
    malformed = [
        ("odd-input-hex", ["apply", "0", "0", "0", ""]),
        ("bad-input-hex", ["apply", "0", "0", "gg", ""]),
        ("odd-replacement-hex", ["apply", "0", "0", "", "0"]),
        ("negative-lo", ["apply", "-1", "0", "", ""]),
        ("leading-zero-lo", ["apply", "00", "0", "", ""]),
        ("overflow-width", ["apply", "0", "999999999999999999999999999", "", ""]),
        ("missing-argument", ["apply", "0", "0", ""]),
        ("unknown-operation", ["other", "0", "0", "", ""]),
    ]
    for label, arguments in malformed:
        cases.append(kernel_case(f"kernel-malformed-{label}", arguments, None,
                                 family="kernel-malformed-wire"))
    return cases


def all_cases() -> list[Case]:
    cases = deterministic_raw_cases()
    cases.extend(limit_cases())
    cases.extend(regex_and_frozen_cases())
    cases.extend(utf8_and_selector_cases())
    cases.extend(malformed_and_lifecycle_cases())
    cases.extend(kernel_cases())
    names = [case.name for case in cases]
    if len(names) != len(set(names)):
        raise ValueError("internal error: duplicate case name")
    return cases


def validate_static_elf(binary: Path) -> dict[str, Any]:
    metadata = binary.stat()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("binary must be an executable regular file")
    with binary.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:4] != b"\x7fELF" or header[4] != 2 or header[5] != 1:
            raise ValueError("binary is not a little-endian ELF64 executable")
        elf_type, machine = struct.unpack_from("<HH", header, 16)
        if machine != 62 or elf_type not in (2, 3):
            raise ValueError("binary is not an x86-64 executable or static PIE")
        ph_offset = struct.unpack_from("<Q", header, 32)[0]
        ph_entry_size, ph_count = struct.unpack_from("<HH", header, 54)
        if ph_entry_size < 56 or ph_count == 0:
            raise ValueError("binary has no valid ELF program headers")
        has_interpreter = False
        for index in range(ph_count):
            stream.seek(ph_offset + index * ph_entry_size)
            program_header = stream.read(56)
            if len(program_header) != 56:
                raise ValueError("truncated ELF program header table")
            if struct.unpack_from("<I", program_header, 0)[0] == 3:
                has_interpreter = True
        if has_interpreter:
            raise ValueError("binary has PT_INTERP and is not static")
    return {
        "path": str(binary), "sha256": digest_file(binary), "size": metadata.st_size,
        "elf_class": 64, "elf_machine": "x86-64", "elf_type": elf_type,
        "pt_interp": False, "requirement": "static Linux x86-64 musl executable supplied by the parent build",
    }


def tree_snapshot(root: Path) -> dict[str, Any]:
    snapshot: dict[str, Any] = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        metadata = path.lstat()
        entry: dict[str, Any] = {
            "mode": metadata.st_mode, "size": metadata.st_size,
            "mtime_ns": metadata.st_mtime_ns, "ctime_ns": metadata.st_ctime_ns,
        }
        if path.is_symlink():
            entry["kind"] = "symlink"
            entry["target"] = os.readlink(path)
        elif path.is_dir():
            entry["kind"] = "directory"
        elif path.is_file():
            entry["kind"] = "file"
            entry["sha256"] = digest_file(path)
        else:
            entry["kind"] = "other"
        snapshot[relative] = entry
    return snapshot


def materialize(case: Case, work: Path) -> None:
    work.mkdir(parents=True, mode=0o700, exist_ok=False)
    for relative, data in case.files.items():
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)


def parse_elapsed(value: str) -> float:
    parts = value.strip().split(":")
    if len(parts) == 2:
        minutes, seconds = parts
        return int(minutes) * 60 + float(seconds)
    if len(parts) == 3:
        hours, minutes, seconds = parts
        return int(hours) * 3600 + int(minutes) * 60 + float(seconds)
    raise ValueError(f"unrecognized GNU time elapsed value {value!r}")


def parse_time_v(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8", errors="replace")
    elapsed = None
    maximum_rss = None
    user_seconds = None
    system_seconds = None
    for line in text.splitlines():
        if "Elapsed (wall clock) time" in line:
            elapsed = parse_elapsed(line.rsplit(": ", 1)[1])
        elif "Maximum resident set size (kbytes):" in line:
            maximum_rss = int(line.rsplit(": ", 1)[1])
        elif "User time (seconds):" in line:
            user_seconds = float(line.rsplit(": ", 1)[1])
        elif "System time (seconds):" in line:
            system_seconds = float(line.rsplit(": ", 1)[1])
    if elapsed is None or maximum_rss is None:
        raise ValueError("GNU time output lacks elapsed time or maximum RSS")
    return {
        "path": str(path), "elapsed_seconds": elapsed, "maximum_rss_kib": maximum_rss,
        "user_seconds": user_seconds, "system_seconds": system_seconds,
        "wall_ceiling_seconds": RESOURCE_WALL_CEILING_SECONDS,
        "rss_ceiling_kib": RESOURCE_RSS_CEILING_KIB,
        "within_ceiling": elapsed <= RESOURCE_WALL_CEILING_SECONDS and maximum_rss <= RESOURCE_RSS_CEILING_KIB,
    }


def invoke(command: list[str], cwd: Path, timeout: float) -> tuple[int | None, bytes, bytes, bool]:
    process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True, env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "TZ": "UTC"})
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return process.returncode, stdout, stderr, False
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
        return None, stdout, stderr, True


def execute_case(case: Case, binary: Path, work: Path, resource_dir: Path | None = None) -> Observation:
    before = tree_snapshot(work)
    command = [str(binary), *case.arguments]
    timeout = CASE_TIMEOUT_SECONDS
    metrics_path = None
    if case.measure:
        if resource_dir is None:
            raise ValueError("resource directory required for measured case")
        resource_dir.mkdir(parents=True, exist_ok=True)
        metrics_path = resource_dir / f"{case.name}.time.txt"
        command = ["/usr/bin/time", "-v", "-o", str(metrics_path), *command]
        timeout = RESOURCE_WALL_CEILING_SECONDS + 10
    started = time.monotonic()
    status, stdout, stderr, timed_out = invoke(command, work, timeout)
    elapsed = time.monotonic() - started
    after = tree_snapshot(work)
    no_effect = before == after
    resource = None
    reasons: list[str] = []
    if timed_out:
        reasons.append("timeout")
    if status != case.expected_status:
        reasons.append(f"exit status {status} != {case.expected_status}")
    expected_stdout = case.expected_output if case.expected_output is not None else b""
    if stdout != expected_stdout:
        reasons.append(f"stdout differs (observed {len(stdout)} bytes, expected {len(expected_stdout)} bytes)")
    if case.expected_status == 0 and stderr:
        reasons.append("success wrote stderr")
    if case.command_kind in ("preview", "inspect") and case.expected_status != 0 and not stderr:
        reasons.append("preview/inspect refusal had no diagnostic")
    if case.command_kind == "kernel" and case.expected_status != 0 and stderr:
        reasons.append("primitive refusal wrote stderr")
    required_diagnostic = REQUIRED_REFUSAL_DIAGNOSTICS.get(case.name)
    if required_diagnostic is not None and required_diagnostic.encode() not in stderr:
        reasons.append(f"refusal lacks required diagnostic {required_diagnostic}")
    if not no_effect:
        reasons.append("case tree changed during execution")
    if case.measure:
        try:
            if metrics_path is None:
                raise ValueError("missing metrics path")
            resource = parse_time_v(metrics_path)
            if not resource["within_ceiling"]:
                reasons.append("resource ceiling exceeded")
        except (OSError, ValueError) as error:
            reasons.append(f"resource measurement failed: {error}")
    return Observation(not reasons, "; ".join(reasons) or "exact model agreement",
                       status, stdout, stderr, elapsed, timed_out, no_effect, resource)


def case_manifest(case: Case, work: Path) -> dict[str, Any]:
    expected = case.expected_output
    return {
        "name": case.name, "family": case.family, "seed": case.seed,
        "detail": case.detail, "command_kind": case.command_kind,
        "arguments": case.arguments, "expected_status": case.expected_status,
        "expected_output_length": None if expected is None else len(expected),
        "expected_output_sha256": None if expected is None else digest_bytes(expected),
        "model_refusal": case.model_refusal, "measured": case.measure,
        "files": {name: {"length": len(data), "sha256": digest_bytes(data)}
                  for name, data in case.files.items()},
        "workdir": str(work),
        "raw_edit_count": 0 if case.edit_spec is None else len(case.edit_spec.operations),
    }


def observation_summary(case: Case, observation: Observation) -> dict[str, Any]:
    return {
        "name": case.name, "family": case.family, "seed": case.seed,
        "ok": observation.ok, "reason": observation.reason,
        "exit_status": observation.exit_status, "timed_out": observation.timed_out,
        "elapsed_seconds": round(observation.elapsed_seconds, 6),
        "stdout_length": len(observation.stdout), "stdout_sha256": digest_bytes(observation.stdout),
        "stderr_length": len(observation.stderr), "stderr_sha256": digest_bytes(observation.stderr),
        "stderr_excerpt": observation.stderr[:4096].decode("utf-8", "replace"),
        "no_effect": observation.no_effect, "resource": observation.resource,
    }


def preserve_failure(case: Case, observation: Observation, work: Path,
                     failures: Path) -> Path:
    destination = failures / case.name
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "actual-stdout.bin").write_bytes(observation.stdout)
    (destination / "actual-stderr.bin").write_bytes(observation.stderr)
    if case.expected_output is not None and len(case.expected_output) <= 1024 * 1024:
        (destination / "expected-stdout.bin").write_bytes(case.expected_output)
    write_json(destination / "failure.json", {
        "case": case_manifest(case, work), "observation": observation_summary(case, observation),
    })
    return destination


def minimize_failure(case: Case, binary: Path, original_work: Path,
                     failure_dir: Path) -> dict[str, Any]:
    if case.edit_spec is None:
        minimized = failure_dir / "minimized"
        shutil.copytree(original_work, minimized)
        return {"strategy": "directed-case-is-already-minimal", "workdir": str(minimized),
                "remaining_edits": None}
    spec = case.edit_spec
    operations = list(spec.operations)
    best_case = case
    best_work = original_work
    trial = 0
    granularity = 2
    while operations and granularity <= max(2, len(operations)):
        chunk = max(1, math.ceil(len(operations) / granularity))
        reduced = False
        for start in range(0, len(operations), chunk):
            candidate_operations = operations[:start] + operations[start + chunk:]
            if len(candidate_operations) == len(operations):
                continue
            trial += 1
            trial_spec = EditSpec(spec.base, candidate_operations, spec.raw, spec.path)
            trial_case = make_edit_case(f"min-{trial:04d}", trial_spec, seed=case.seed,
                                        family=f"minimized-{case.family}")
            trial_work = failure_dir / "trials" / f"{trial:04d}"
            materialize(trial_case, trial_work)
            observation = execute_case(trial_case, binary, trial_work)
            if not observation.ok:
                operations = candidate_operations
                best_case = trial_case
                best_work = trial_work
                granularity = max(2, granularity - 1)
                reduced = True
                break
        if not reduced:
            if granularity >= len(operations):
                break
            granularity = min(len(operations), granularity * 2)
    minimized = failure_dir / "minimized"
    shutil.copytree(best_work, minimized)
    write_json(failure_dir / "minimized-case.json", case_manifest(best_case, minimized))
    return {"strategy": "delta-debug contiguous edit removal with model rechecked per trial",
            "workdir": str(minimized), "remaining_edits": len(operations), "trials": trial}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    evidence = args.evidence.resolve()
    if evidence.exists() or evidence.is_relative_to(ROOT):
        parser.error("evidence directory must be fresh and outside the source tree")
    if not Path("/usr/bin/time").is_file() or not os.access("/usr/bin/time", os.X_OK):
        parser.error("GNU /usr/bin/time is required for peak RSS measurements")
    evidence.mkdir(parents=True, mode=0o700, exist_ok=False)
    started = time.monotonic()
    receipt: dict[str, Any] = {
        "schema_version": 1, "scope": "SG06 offline differential fuzz and bounded execution component",
        "status": "failed", "master_seed": MASTER_SEED,
        "command": sys.orig_argv, "binary": None,
        "ceilings": {"measured_wall_seconds": RESOURCE_WALL_CEILING_SECONDS,
                     "measured_peak_rss_kib": RESOURCE_RSS_CEILING_KIB,
                     "unmeasured_case_timeout_seconds": CASE_TIMEOUT_SECONDS},
        "counts": {}, "resource_observations": [], "failures": [],
    }
    try:
        receipt["binary"] = validate_static_elf(binary)
        cases = all_cases()
        cases_dir = evidence / "cases"
        failures_dir = evidence / "failures"
        resources_dir = evidence / "resources"
        seeds_path = evidence / "seeds.jsonl"
        results_path = evidence / "case-results.jsonl"
        raw_edits = 0
        with seeds_path.open("x", encoding="utf-8") as seeds, results_path.open("x", encoding="utf-8") as results:
            for index, case in enumerate(cases):
                work = cases_dir / f"{index:04d}-{case.name}"
                materialize(case, work)
                seeds.write(json.dumps(case_manifest(case, work), sort_keys=True) + "\n")
                seeds.flush()
                if case.edit_spec is not None and case.edit_spec.raw:
                    raw_edits += len(case.edit_spec.operations)
                observation = execute_case(case, binary, work, resources_dir)
                summary = observation_summary(case, observation)
                results.write(json.dumps(summary, sort_keys=True) + "\n")
                results.flush()
                if observation.resource is not None:
                    receipt["resource_observations"].append({"case": case.name, **observation.resource})
                if not observation.ok:
                    failure_dir = preserve_failure(case, observation, work, failures_dir)
                    minimization = minimize_failure(case, binary, work, failure_dir)
                    write_json(failure_dir / "minimization.json", minimization)
                    receipt["failures"].append({"case": case.name, "reason": observation.reason,
                                                "retained": str(failure_dir), "minimization": minimization})
        receipt["counts"] = {
            "cases": len(cases),
            "deterministic_raw_cases": sum(case.family == "deterministic-raw" for case in cases),
            "raw_profile_edits_in_edit_cases": raw_edits,
            "measured_cases": sum(case.measure for case in cases),
            "passed": len(cases) - len(receipt["failures"]),
            "failed": len(receipt["failures"]),
        }
        if receipt["counts"]["deterministic_raw_cases"] < 2000:
            raise ValueError("internal error: fewer than 2000 deterministic raw cases")
        if not receipt["failures"]:
            receipt["status"] = "passed_sg06_offline_fuzz_resource_component"
        else:
            receipt["error"] = f"{len(receipt['failures'])} differential/resource/no-effect failures"
    except (OSError, ValueError, struct.error) as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 6)
        write_json(evidence / "fuzz-receipt.json", receipt)
    print(json.dumps({"status": receipt["status"], "counts": receipt["counts"],
                      "error": receipt.get("error")}, indent=2, sort_keys=True))
    return 0 if receipt["status"] == "passed_sg06_offline_fuzz_resource_component" else 1


if __name__ == "__main__":
    raise SystemExit(main())
