"""Offline evidence producer for the scoped SG01 grammar/lexer component."""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import sys
import time
from typing import Any, Iterator, Optional, Sequence

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from evidence_identity import source_identity

from .errors import MAX_DIAGNOSTIC_CONTEXT, MAX_DIAGNOSTIC_MESSAGE, RecipeError
from .lexer import KEYWORDS, MAX_NAT, MAX_RECIPE_BYTES, Lexer, LexerConfig, Token
from .parser import DEFAULT_MAX_DEPTH, Parser, ParserConfig


SCHEMA_VERSION = 1
FIXTURE_REJECTION_CODES = [
    "retired_assignment",
    "retired_assignment",
    "expected_token",
    "reserved_handle",
    "odd_hex",
    "nat_leading_zero",
    "invalid_escape",
    "expected_token",
    "unterminated_heredoc",
    "reserved_handle",
    "expected_token",
    "trailing_input",
    "trailing_comma",
    "expected_token",
    "expected_token",
    "reserved_handle",
]


class EvidenceError(Exception):
    pass


class CaseValidationError(Exception):
    pass


@dataclass(frozen=True)
class Case:
    identity: str
    category: str
    origin: str
    source: bytes
    accept: bool
    code: Optional[str] = None
    check: Optional[str] = None
    probe: Optional[str] = None


class Evidence:
    def __init__(self, repository: Path, destination: str) -> None:
        self.repository = repository.resolve()
        self.path = Path(destination).expanduser().resolve()
        if self.path == self.repository or self.repository in self.path.parents:
            raise EvidenceError("evidence directory must be outside the repository")
        if self.path.exists():
            raise EvidenceError("evidence path must be fresh and must not already exist")
        try:
            self.path.mkdir(parents=True, mode=0o700)
        except OSError as exc:
            raise EvidenceError(f"cannot create evidence directory: {exc}") from exc

    def new_bytes(self, name: str, content: bytes) -> None:
        path = self.path / name
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(content)
        except Exception:
            try:
                os.close(descriptor)
            except OSError:
                pass
            raise

    def write_json(self, name: str, value: Any) -> None:
        payload = (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")
        self.new_bytes(name, payload)

    def artifact(self, name: str) -> dict[str, Any]:
        content = (self.path / name).read_bytes()
        return {"path": name, "sha256": hashlib.sha256(content).hexdigest(), "size_bytes": len(content)}


class RawLog:
    def __init__(self, evidence: Evidence) -> None:
        descriptor = os.open(
            evidence.path / "logs.jsonl",
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o600,
        )
        self.stream = os.fdopen(descriptor, "w", encoding="utf-8", newline="\n")

    def write(self, event: str, **fields: Any) -> None:
        record = {"event": event, **fields}
        self.stream.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")
        self.stream.flush()

    def close(self) -> None:
        self.stream.close()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def nested_indent(depth: int) -> bytes:
    return b'replace @x with ' + b'indent(" ",' * depth + b'"x"' + b")" * depth


def build_cases(repository: Path) -> list[Case]:
    fixture_path = repository / "qualification" / "grammar-cases.json"
    fixture = json.loads(fixture_path.read_bytes().decode("utf-8", "strict"))
    if not isinstance(fixture, dict) or set(fixture) != {"accept", "reject"}:
        raise EvidenceError("grammar-cases.json must contain exactly accept and reject")
    if not isinstance(fixture["accept"], list) or not all(isinstance(item, str) for item in fixture["accept"]):
        raise EvidenceError("grammar accept cases must be strings")
    if not isinstance(fixture["reject"], list) or not all(isinstance(item, str) for item in fixture["reject"]):
        raise EvidenceError("grammar reject cases must be strings")
    if len(fixture["reject"]) != len(FIXTURE_REJECTION_CODES):
        raise EvidenceError("grammar reject fixture count does not match its diagnostic oracle")

    cases: list[Case] = []
    for index, text in enumerate(fixture["accept"], 1):
        check = "exact_assignment" if index == 1 else None
        cases.append(
            Case(
                f"grammar_accept_{index:02d}",
                "grammar_fixture_accept",
                f"qualification/grammar-cases.json#accept[{index - 1}]",
                text.encode("utf-8"),
                True,
                check=check,
            )
        )
    for index, (text, code) in enumerate(zip(fixture["reject"], FIXTURE_REJECTION_CODES), 1):
        cases.append(
            Case(
                f"grammar_reject_{index:02d}",
                "grammar_fixture_reject",
                f"qualification/grammar-cases.json#reject[{index - 1}]",
                text.encode("utf-8"),
                False,
                code,
            )
        )

    example_paths = sorted((repository / "examples").glob("*.spl"))
    if not example_paths:
        raise EvidenceError("no example recipes found")
    for path in example_paths:
        cases.append(
            Case(
                f"example_{path.stem}",
                "example",
                path.relative_to(repository).as_posix(),
                path.read_bytes(),
                True,
            )
        )

    def add(
        identity: str,
        category: str,
        source: bytes,
        accept: bool,
        code: Optional[str] = None,
        check: Optional[str] = None,
        probe: Optional[str] = None,
    ) -> None:
        cases.append(Case(identity, category, f"tools/sg01/producer.py:{identity}", source, accept, code, check, probe))

    add("encoding_invalid_utf8", "encoding", b'replace @x with "\xff"', False, "invalid_utf8")
    add("encoding_bom_prefix", "encoding", b'\xef\xbb\xbfremove "x"', False, "bom")
    add("encoding_bom_inside_literal", "encoding", b'replace @x with "\xef\xbb\xbf"', False, "bom")
    add("encoding_nul_outside", "encoding", b'remove "x"\x00', False, "nul_outside_literal")
    add("encoding_nul_comment", "encoding", b'# comment\x00\nremove "x"', False, "nul_outside_literal")
    add("encoding_lone_cr", "encoding", b'remove "x"\r', False, "lone_cr")
    add("encoding_crlf", "encoding", b'edit "x"\r\n{\r\ndelete @x\r\n}', True)
    add(
        "encoding_nul_heredoc",
        "encoding",
        b"replace @x with <<'N'\n\x00\nN",
        True,
        check="nul_heredoc",
    )
    add("encoding_nonascii_name", "encoding", "delete @x.café".encode("utf-8"), False, "unexpected_byte")
    add("encoding_recipe_too_large", "limit", b" " * (MAX_RECIPE_BYTES + 1), False, "recipe_too_large")

    add(
        "string_escapes",
        "string",
        b'create "x" with "\\"\\\\\\n\\r\\t"',
        True,
        check="string_escapes",
    )
    add("string_unicode_literal", "string", 'create "café" with "café"'.encode("utf-8"), True, check="unicode_literal")
    add("string_unescaped_newline", "string", b'replace @x with "a\nb"', False, "string_newline")
    add("string_unterminated", "truncation", b'replace @x with "abc', False, "unterminated_string")
    add("string_raw_c1_control", "string", b'replace @x with "' + "\u0085".encode("utf-8") + b'"', False, "string_control")
    add("hex_empty", "hex", b"replace @x with hex\"\"", True, check="hex_empty")
    add("hex_bad_digit", "hex", b'replace @x with hex"0g"', False, "invalid_hex")
    add("rx_backslash_passthrough", "regex", b'each @r in line_matches rx"\\S+" expect 0 { delete @r }', True, check="rx_backslash")
    add("rx_physical_newline", "regex", b'each @r in line_matches rx"a\nb" expect 0 { delete @r }', False, "rx_newline")
    add(
        "rx_reserved_group",
        "reserved_word",
        b'each @r in line_matches rx"(?<expect>x)" expect 1 { delete @r }',
        False,
        "reserved_regex_group",
    )
    add(
        "rx_named_groups",
        "regex",
        b'each @r in line_matches rx"(?<verb>[a-z]+)" expect 1 { delete @r.verb }',
        True,
    )
    add("reference_reserved_group", "reserved_word", b"delete @x.expect", False, "expected_token")

    add(
        "heredoc_mixed_eol",
        "heredoc",
        b"replace @x with <<'X'\r\na\r\nb\n#c\nX\r\n",
        True,
        check="heredoc_mixed_eol",
    )
    add(
        "heredoc_close_at_eof",
        "heredoc",
        b"replace @x with <<'X'\nbody\nX",
        True,
        check="heredoc_close_at_eof",
    )
    add("heredoc_empty", "heredoc", b"replace @x with <<'X'\nX", True, check="heredoc_empty")
    add(
        "heredoc_lone_cr_content",
        "heredoc",
        b"replace @x with <<'X'\na\rb\nX",
        True,
        check="heredoc_lone_cr_content",
    )
    add("heredoc_open_without_eol", "heredoc", b"replace @x with <<'X'", False, "heredoc_open_eol")
    add("heredoc_open_lone_cr", "heredoc", b"replace @x with <<'X'\rbody", False, "lone_cr")
    add("heredoc_truncated_prefix", "truncation", b"replace @x with <<'X'\nbody\nXX", False, "unterminated_heredoc")
    add("heredoc_tag_too_long", "heredoc", b"replace @x with <<'" + b"A" * 65 + b"'\n", False, "heredoc_tag_too_long")
    add(
        "heredoc_first_delimiter_wins",
        "heredoc",
        b"replace @x with <<'X'\nfirst\nX\njunk",
        False,
        "trailing_input",
    )

    for word in sorted(KEYWORDS):
        add(
            f"reserved_handle_{word}",
            "reserved_handle",
            f'@{word} := one text "x"'.encode("ascii"),
            False,
            "reserved_handle",
        )

    add(
        "nat_u64_max",
        "numeric",
        f"delete range [{MAX_NAT},{MAX_NAT})".encode("ascii"),
        True,
        check="nat_u64_max",
    )
    add("nat_u64_overflow", "numeric", f"delete range [{MAX_NAT + 1},0)".encode("ascii"), False, "nat_overflow")
    add("nat_huge_limb", "numeric", b"delete range [" + b"9" * 4096 + b",0)", False, "nat_overflow")

    add("parser_depth_32", "depth", nested_indent(32), True, check="depth_32")
    add("parser_depth_33", "depth", nested_indent(33), False, "parser_depth")
    add("parser_depth_attack_128", "depth", nested_indent(128), False, "parser_depth")
    add("parser_empty_document", "truncation", b"", False, "expected_document")
    add("parser_truncated_indent", "truncation", b'replace @x with indent(" ",', False, "expected_piece")
    add("payload_trailing_comma", "syntax", b'replace @x with ["a",]', False, "trailing_comma")
    add("overlap_ambiguity_probe", "ambiguity", b'@x := one text "aa"', True, probe="overlapping_text")

    identities = [case.identity for case in cases]
    if len(identities) != len(set(identities)):
        raise EvidenceError("duplicate qualification case identity")
    return cases


def walk_nodes(value: Any) -> Iterator[dict[str, Any]]:
    if isinstance(value, dict):
        if "kind" in value and "span" in value:
            yield value
        for child in value.values():
            yield from walk_nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk_nodes(child)


def nodes_of_kind(ast: dict[str, Any], kind: str) -> list[dict[str, Any]]:
    return [node for node in walk_nodes(ast) if node["kind"] == kind]


def validate_structure(case: Case, tokens: list[Token], ast: Optional[dict[str, Any]], diagnostic: Optional[dict[str, Any]]) -> None:
    previous_end = 0
    for token in tokens:
        if not (0 <= token.start <= token.end <= len(case.source)):
            raise CaseValidationError(f"invalid token span in {case.identity}")
        if token.start < previous_end:
            raise CaseValidationError(f"overlapping token spans in {case.identity}")
        previous_end = token.end
    if ast is not None:
        for node in walk_nodes(ast):
            span = node["span"]
            if set(span) != {"start", "end"}:
                raise CaseValidationError(f"noncanonical AST span in {case.identity}")
            if not (0 <= span["start"] <= span["end"] <= len(case.source)):
                raise CaseValidationError(f"invalid AST span in {case.identity}")
    if diagnostic is not None:
        span = diagnostic["span"]
        if not (0 <= span["start"] <= span["end"] <= len(case.source)):
            raise CaseValidationError(f"invalid diagnostic span in {case.identity}")
        context = diagnostic["context"]
        if context["end"] - context["start"] > MAX_DIAGNOSTIC_CONTEXT:
            raise CaseValidationError(f"unbounded diagnostic context in {case.identity}")
        if len(diagnostic["message"]) > MAX_DIAGNOSTIC_MESSAGE:
            raise CaseValidationError(f"unbounded diagnostic message in {case.identity}")


def validate_special(case: Case, tokens: list[Token], ast: Optional[dict[str, Any]]) -> str:
    if case.check is None:
        return "not_requested"
    if ast is None:
        raise CaseValidationError(f"{case.check} requires an AST")
    if case.check == "exact_assignment":
        if not any(token.kind == "ASSIGN" and case.source[token.start : token.end] == b":=" for token in tokens):
            raise CaseValidationError(":= was not emitted as one ASSIGN token")
    elif case.check == "string_escapes":
        if not any(node["value"] == '"\\\n\r\t' for node in nodes_of_kind(ast, "string")):
            raise CaseValidationError("STRING escape decoding differs from the golden value")
    elif case.check == "unicode_literal":
        if not any(node["value"] == "café" for node in nodes_of_kind(ast, "string")):
            raise CaseValidationError("literal Unicode was not preserved")
    elif case.check == "hex_empty":
        if not any(node["bytes_hex"] == "" for node in nodes_of_kind(ast, "hex")):
            raise CaseValidationError("empty HEX did not survive exactly")
    elif case.check == "rx_backslash":
        if not any(node["regex"] == "\\S+" for node in nodes_of_kind(ast, "line_matches")):
            raise CaseValidationError("RX backslash was not passed through")
    elif case.check.startswith("heredoc_") or case.check in ("nul_heredoc",):
        expected = {
            "heredoc_mixed_eol": "a\r\nb\n#c\n",
            "heredoc_close_at_eof": "body\n",
            "heredoc_empty": "",
            "heredoc_lone_cr_content": "a\rb\n",
            "nul_heredoc": "\x00\n",
        }[case.check]
        if not any(node["value"] == expected for node in nodes_of_kind(ast, "heredoc")):
            raise CaseValidationError(f"{case.check} HEREDOC bytes differ from golden text")
    elif case.check == "nat_u64_max":
        ranges = nodes_of_kind(ast, "range")
        if len(ranges) != 1 or ranges[0]["low"] != MAX_NAT or ranges[0]["high"] != MAX_NAT:
            raise CaseValidationError("maximum NAT was not parsed exactly")
    elif case.check == "depth_32":
        if len(nodes_of_kind(ast, "indent")) != DEFAULT_MAX_DEPTH:
            raise CaseValidationError("depth-32 payload did not produce 32 indent nodes")
    else:
        raise CaseValidationError(f"unknown special check {case.check}")
    return "passed"


def overlap_probe() -> dict[str, Any]:
    target = b"aaa"
    needle = b"aa"
    starts = []
    position = 0
    while position <= len(target) - len(needle):
        found = target.find(needle, position)
        if found < 0:
            break
        starts.append(found)
        position = found + 1
    matches = [{"start": start, "end": start + len(needle)} for start in starts]
    overlaps = any(matches[index]["end"] > matches[index + 1]["start"] for index in range(len(matches) - 1))
    return {
        "status": "rejected" if len(matches) != 1 else "accepted",
        "code": "ambiguous_selector" if len(matches) != 1 else None,
        "target_hex": target.hex(),
        "needle_hex": needle.hex(),
        "matches": matches,
        "overlap_detected": overlaps,
        "scope": "isolated one-text overlap oracle; not general execution semantics",
    }


def parse_source(case: Case, lexer_config: LexerConfig = LexerConfig(), parser_config: ParserConfig = ParserConfig()) -> dict[str, Any]:
    tokens: list[Token] = []
    ast = None
    diagnostic = None
    status = "accepted"
    try:
        tokens = Lexer(case.source, lexer_config).tokenize()
        ast = Parser(case.source, tokens, parser_config).parse()
    except RecipeError as exc:
        tokens = exc.tokens
        diagnostic = exc.diagnostic()
        status = "rejected"
    return {"status": status, "tokens": tokens, "ast": ast, "diagnostic": diagnostic}


def execute_case(case: Case) -> dict[str, Any]:
    outcome = parse_source(case)
    tokens: list[Token] = outcome["tokens"]
    ast = outcome["ast"]
    diagnostic = outcome["diagnostic"]
    status = outcome["status"]
    record: dict[str, Any] = {
        "id": case.identity,
        "category": case.category,
        "origin": case.origin,
        "executed": True,
        "source": {"sha256": sha256(case.source), "size_bytes": len(case.source), "hex": case.source.hex()},
        "expected": {"status": "accepted" if case.accept else "rejected", "diagnostic_code": case.code},
        "observed": {"status": status, "diagnostic": diagnostic},
        "tokens": [token.golden(case.source) for token in tokens],
        "ast": ast,
        "special_check": "not_requested",
        "probe": None,
        "passed": False,
    }
    try:
        validate_structure(case, tokens, ast, diagnostic)
        if status == "accepted":
            record["special_check"] = validate_special(case, tokens, ast)
        expected_status = "accepted" if case.accept else "rejected"
        passed = status == expected_status
        if not case.accept:
            passed = passed and diagnostic is not None and diagnostic["code"] == case.code
        if case.probe == "overlapping_text":
            probe = overlap_probe()
            record["probe"] = probe
            passed = passed and probe["status"] == "rejected" and probe["overlap_detected"] and len(probe["matches"]) == 2
        record["passed"] = passed
    except (CaseValidationError, KeyError, TypeError) as exc:
        record["validation_error"] = f"{type(exc).__name__}: {exc}"
    return record


def run_mutants(cases: list[Case]) -> list[dict[str, Any]]:
    by_identity = {case.identity: case for case in cases}
    specifications = [
        (
            "parser_accepts_retired_assignment",
            "grammar_reject_02",
            LexerConfig(),
            ParserConfig(allow_retired_assignment=True),
        ),
        (
            "lexer_ignores_lone_cr",
            "encoding_lone_cr",
            LexerConfig(allow_lone_cr=True),
            ParserConfig(),
        ),
        (
            "parser_depth_limit_removed_for_33",
            "parser_depth_33",
            LexerConfig(),
            ParserConfig(max_depth=64),
        ),
        (
            "parser_accepts_trailing_comma",
            "payload_trailing_comma",
            LexerConfig(),
            ParserConfig(allow_trailing_comma=True),
        ),
    ]
    results = []
    for name, identity, lexer_config, parser_config in specifications:
        case = by_identity[identity]
        baseline = parse_source(case)
        mutant = parse_source(case, lexer_config, parser_config)
        killed = baseline["status"] == "rejected" and mutant["status"] == "accepted"
        results.append(
            {
                "name": name,
                "case_id": identity,
                "baseline_status": baseline["status"],
                "baseline_diagnostic": baseline["diagnostic"],
                "mutant_status": mutant["status"],
                "mutant_diagnostic": mutant["diagnostic"],
                "killed": killed,
                "failing_before": killed,
            }
        )
    return results


def input_manifest(repository: Path, examples: list[Path]) -> list[dict[str, Any]]:
    paths: list[tuple[Path, str]] = []
    paths.append((repository / "tools" / "check_sg01.py", "producer_source"))
    paths.extend((path, "producer_source") for path in sorted((repository / "tools" / "sg01").glob("*.py")))
    paths.extend(
        [
            (repository / "spec" / "splicery.ebnf", "normative_grammar"),
            (repository / "spec" / "language.md", "normative_language"),
            (repository / "qualification" / "grammar-cases.json", "grammar_fixture"),
            (repository / "qualification" / "gates.json", "gate_registry"),
            (repository / "qualification" / "gates.md", "gate_specification"),
            (repository / "examples" / "README.md", "example_fixture"),
        ]
    )
    paths.extend((path, "example_fixture") for path in examples)
    manifest = []
    for path, role in sorted(paths, key=lambda item: item[0].as_posix()):
        content = path.read_bytes()
        manifest.append(
            {
                "path": path.relative_to(repository).as_posix(),
                "role": role,
                "sha256": sha256(content),
                "size_bytes": len(content),
            }
        )
    return manifest


class Producer:
    def __init__(self, repository: Path, evidence: Evidence, command: Sequence[str]) -> None:
        self.repository = repository
        self.evidence = evidence
        self.command = list(command)
        self.log = RawLog(evidence)
        self.started_wall = utc_now()
        self.started_clock = time.monotonic()
        self.finished = False

    def base_receipt(self, status: str, exit_code: int) -> dict[str, Any]:
        return {
            "schema_version": SCHEMA_VERSION,
            "gate": "SG01",
            "source_identity": source_identity(self.repository),
            "component": "independent_grammar_lexer_qualification",
            "scope": {
                "syntax_and_lexing": "assessed",
                "isolated_overlap_oracle": "assessed_only_for_reported_probe",
                "semantic_correctness": "not_assessed",
                "regex_engine_correctness": "not_assessed",
                "host_integration": "not_assessed",
                "full_SG01_pass_claim": False,
            },
            "status": status,
            "exit_code": exit_code,
            "command": self.command,
            "cwd": str(Path.cwd()),
            "started_at": self.started_wall,
            "finished_at": utc_now(),
            "elapsed_seconds": round(time.monotonic() - self.started_clock, 6),
            "tool": {
                "implementation": "Python 3 standard library only; offline",
                "python_version": sys.version,
                "python_executable": sys.executable,
                "platform": platform.platform(),
            },
        }

    def finish_receipt(self, receipt: dict[str, Any]) -> None:
        self.log.close()
        names = ["inputs.json", "golden.json", "mutations.json", "logs.jsonl"]
        receipt["artifacts"] = [self.evidence.artifact(name) for name in names if (self.evidence.path / name).exists()]
        self.evidence.write_json("receipt.json", receipt)
        self.finished = True

    def execute(self) -> int:
        examples = sorted((self.repository / "examples").glob("*.spl"))
        manifest = input_manifest(self.repository, examples)
        self.evidence.write_json(
            "inputs.json",
            {
                "schema_version": SCHEMA_VERSION,
                "repository": str(self.repository),
                "hash_algorithm": "sha256",
                "inputs": manifest,
            },
        )
        cases = build_cases(self.repository)
        self.log.write("qualification_started", required_cases=len(cases), examples=len(examples))
        records = []
        for case in cases:
            try:
                record = execute_case(case)
            except Exception as exc:
                record = {
                    "id": case.identity,
                    "category": case.category,
                    "origin": case.origin,
                    "executed": True,
                    "source": {"sha256": sha256(case.source), "size_bytes": len(case.source), "hex": case.source.hex()},
                    "observed": {"status": "error"},
                    "internal_error": f"{type(exc).__name__}: {exc}",
                    "passed": False,
                }
            records.append(record)
            self.log.write(
                "case_finished",
                case_id=case.identity,
                observed=record["observed"]["status"],
                passed=record["passed"],
            )
        self.evidence.write_json(
            "golden.json",
            {
                "schema_version": SCHEMA_VERSION,
                "span_convention": "half-open byte offsets into original recipe; original EOL retained",
                "token_lexemes": "hex of exact original source bytes",
                "cases": records,
            },
        )
        mutants = run_mutants(cases)
        for mutant in mutants:
            self.log.write("mutant_finished", name=mutant["name"], killed=mutant["killed"])
        self.evidence.write_json("mutations.json", {"schema_version": SCHEMA_VERSION, "mutants": mutants})

        stable_inputs = manifest == input_manifest(self.repository, examples)
        if not stable_inputs:
            self.log.write("input_stability_failure")
        all_executed = len(records) == len(cases) and all(record.get("executed") is True for record in records)
        failed_cases = [record["id"] for record in records if not record.get("passed")]
        survived_mutants = [mutant["name"] for mutant in mutants if not mutant["killed"]]
        passed = all_executed and not failed_cases and not survived_mutants and stable_inputs
        category_counts = Counter(case.category for case in cases)
        coverage = {
            "required_cases": len(cases),
            "executed_cases": sum(1 for record in records if record.get("executed") is True),
            "passed_cases": sum(1 for record in records if record.get("passed") is True),
            "all_required_cases_executed": all_executed,
            "grammar_fixture_accepts": category_counts["grammar_fixture_accept"],
            "grammar_fixture_rejects": category_counts["grammar_fixture_reject"],
            "examples": [path.stem for path in examples],
            "reserved_handles": sorted(KEYWORDS),
            "category_counts": dict(sorted(category_counts.items())),
            "failed_cases": failed_cases,
            "mutants_total": len(mutants),
            "mutants_killed": sum(1 for mutant in mutants if mutant["killed"]),
            "survived_mutants": survived_mutants,
            "stable_input_hashes": stable_inputs,
            "numeric_range": {"minimum": 0, "maximum": MAX_NAT, "conversion": "checked decimal accumulation before AST NAT allocation"},
            "parser_depth_limit": DEFAULT_MAX_DEPTH,
        }
        status = "passed" if passed else "failed"
        exit_code = 0 if passed else 3
        receipt = self.base_receipt(status, exit_code)
        receipt["coverage"] = coverage
        receipt["assumptions"] = [
            "NAT is qualified in the unsigned 64-bit range and accumulated without arbitrary-precision conversion.",
            "Regex qualification here covers raw-token preservation and reserved named-group spelling, not regex compilation or matching.",
            "The overlap result is an isolated qualification oracle, not a claim of selector or host semantics.",
            "Every listed case ran in this invocation; a pass is limited to this component and exact input hashes.",
        ]
        self.log.write("qualification_finished", status=status, failed_cases=failed_cases, survived_mutants=survived_mutants)
        self.finish_receipt(receipt)
        return exit_code

    def abort(self, exc: Exception) -> int:
        if self.finished:
            return 4
        try:
            self.log.write("producer_error", error=f"{type(exc).__name__}: {exc}")
            receipt = self.base_receipt("error", 4)
            receipt["error"] = f"{type(exc).__name__}: {exc}"
            self.finish_receipt(receipt)
        except Exception:
            try:
                self.log.close()
            except Exception:
                pass
        return 4


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Run independent SG01 grammar/lexer qualification")
    parser.add_argument("--evidence", required=True, help="fresh output directory outside the repository")
    arguments = parser.parse_args(argv)
    repository = Path(__file__).resolve().parents[2]
    try:
        evidence = Evidence(repository, arguments.evidence)
    except EvidenceError as exc:
        print(f"SG01 evidence setup error: {exc}", file=sys.stderr)
        return 2
    command = list(getattr(sys, "orig_argv", ["python3", "-B", *sys.argv]))
    producer = Producer(repository, evidence, command)
    try:
        return producer.execute()
    except Exception as exc:
        return producer.abort(exc)


if __name__ == "__main__":
    raise SystemExit(main())
