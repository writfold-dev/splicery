"""Hand-written recursive-descent parser for every production in splicery.ebnf."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

from .errors import RecipeError
from .lexer import Token


DEFAULT_MAX_DEPTH = 32


@dataclass(frozen=True)
class ParserConfig:
    allow_retired_assignment: bool = False
    allow_trailing_comma: bool = False
    max_depth: int = DEFAULT_MAX_DEPTH


class Parser:
    def __init__(
        self,
        source: bytes,
        tokens: list[Token],
        config: ParserConfig = ParserConfig(),
    ) -> None:
        self.source = source
        self.tokens = tokens
        self.config = config
        self.index = 0

    @property
    def current(self) -> Token:
        return self.tokens[self.index]

    @property
    def previous(self) -> Token:
        return self.tokens[self.index - 1]

    def fail(self, code: str, message: str, token: Optional[Token] = None) -> None:
        at = token if token is not None else self.current
        raise RecipeError("syntax", code, message, self.source, at.start, at.end, self.tokens)

    def advance(self) -> Token:
        token = self.current
        if token.kind != "EOF":
            self.index += 1
        return token

    def match(self, kind: str) -> Optional[Token]:
        if self.current.kind == kind:
            return self.advance()
        return None

    def expect(self, kind: str) -> Token:
        if self.current.kind != kind:
            self.fail(
                "expected_token",
                f"expected {kind}, found {self.current.kind}",
            )
        return self.advance()

    @staticmethod
    def node(kind: str, start: int, end: int, **fields: Any) -> dict[str, Any]:
        return {"kind": kind, "span": {"start": start, "end": end}, **fields}

    def parse(self) -> dict[str, Any]:
        if self.current.kind == "EOF":
            self.fail("expected_document", "expected a file operation or statement")
        if self.current.kind == "KW_EDIT":
            body = self.file_edit()
        elif self.current.kind == "KW_CREATE":
            body = self.create_file()
        elif self.current.kind == "KW_RENAME":
            body = self.rename_file()
        elif self.current.kind == "KW_REMOVE":
            body = self.remove_file()
        else:
            body = self.fragment()
        if self.current.kind != "EOF":
            self.fail("trailing_input", f"unexpected {self.current.kind} after document")
        return self.node("document", body["span"]["start"], body["span"]["end"], body=body)

    def file_edit(self) -> dict[str, Any]:
        start = self.expect("KW_EDIT").start
        path = self.expect("STRING")
        raw = False
        if self.match("KW_AS"):
            self.expect("KW_BYTES")
            raw = True
        self.expect("LBRACE")
        statements = []
        while self.statement_start():
            statements.append(self.statement(0))
        close = self.expect("RBRACE")
        return self.node(
            "file_edit",
            start,
            close.end,
            path=path.value,
            raw_bytes=raw,
            statements=statements,
        )

    def create_file(self) -> dict[str, Any]:
        start = self.expect("KW_CREATE").start
        path = self.expect("STRING")
        raw = False
        if self.match("KW_AS"):
            self.expect("KW_BYTES")
            raw = True
        self.expect("KW_WITH")
        payload = self.payload(0)
        return self.node(
            "create_file",
            start,
            payload["span"]["end"],
            path=path.value,
            raw_bytes=raw,
            payload=payload,
        )

    def rename_file(self) -> dict[str, Any]:
        start = self.expect("KW_RENAME").start
        source = self.expect("STRING")
        self.expect("KW_TO")
        destination = self.expect("STRING")
        return self.node(
            "rename_file",
            start,
            destination.end,
            source=source.value,
            destination=destination.value,
        )

    def remove_file(self) -> dict[str, Any]:
        start = self.expect("KW_REMOVE").start
        path = self.expect("STRING")
        return self.node("remove_file", start, path.end, path=path.value)

    def fragment(self) -> dict[str, Any]:
        if (
            self.current.kind == "NAME"
            and self.current.value == "let"
            and self.index + 2 < len(self.tokens)
            and self.tokens[self.index + 1].kind == "HANDLE"
            and self.tokens[self.index + 2].kind in ("EQUAL", "ASSIGN")
        ):
            end = self.tokens[self.index + 2].end
            self.fail(
                "retired_assignment",
                "retired let/@name = binding; use @name := selector",
                Token("RETIRED", self.current.start, end),
            )
        if not self.statement_start():
            self.fail("expected_statement", "fragment requires at least one statement")
        start = self.current.start
        statements = []
        while self.statement_start():
            statements.append(self.statement(0))
        return self.node("fragment", start, statements[-1]["span"]["end"], statements=statements)

    def statement_start(self) -> bool:
        return self.current.kind in (
            "HANDLE",
            "KW_REPLACE",
            "KW_BEFORE",
            "KW_AFTER",
            "KW_DELETE",
            "KW_EACH",
        )

    def statement(self, depth: int) -> dict[str, Any]:
        if self.current.kind == "HANDLE":
            return self.binding()
        if self.current.kind == "KW_EACH":
            return self.iteration(depth)
        return self.change(depth)

    def binding(self) -> dict[str, Any]:
        handle = self.expect("HANDLE")
        if self.current.kind == "EQUAL" and not self.config.allow_retired_assignment:
            self.fail(
                "retired_assignment",
                "retired @name = binding; use @name := selector",
            )
        if self.current.kind == "EQUAL" and self.config.allow_retired_assignment:
            self.advance()
        else:
            self.expect("ASSIGN")
        selector = self.selector()
        return self.node(
            "binding",
            handle.start,
            selector["span"]["end"],
            name=handle.value,
            selector=selector,
        )

    def change_start(self) -> bool:
        return self.current.kind in ("KW_REPLACE", "KW_BEFORE", "KW_AFTER", "KW_DELETE")

    def change(self, depth: int) -> dict[str, Any]:
        operation = self.advance()
        if operation.kind not in ("KW_REPLACE", "KW_BEFORE", "KW_AFTER", "KW_DELETE"):
            self.fail("expected_change", "expected replace, before, after, or delete", operation)
        selector = self.selector()
        payload = None
        if operation.kind == "KW_REPLACE":
            self.expect("KW_WITH")
            payload = self.payload(depth)
        elif operation.kind in ("KW_BEFORE", "KW_AFTER"):
            self.expect("KW_INSERT")
            payload = self.payload(depth)
        end = payload["span"]["end"] if payload is not None else selector["span"]["end"]
        names = {
            "KW_REPLACE": "replace",
            "KW_BEFORE": "before",
            "KW_AFTER": "after",
            "KW_DELETE": "delete",
        }
        return self.node(
            "change",
            operation.start,
            end,
            operation=names[operation.kind],
            selector=selector,
            payload=payload,
        )

    def iteration(self, depth: int) -> dict[str, Any]:
        start = self.expect("KW_EACH").start
        binder = self.expect("HANDLE")
        self.expect("KW_IN")
        collection = self.collection()
        self.expect("LBRACE")
        changes = []
        while self.change_start():
            changes.append(self.change(depth))
        close = self.expect("RBRACE")
        return self.node(
            "iteration",
            start,
            close.end,
            binder=binder.value,
            collection=collection,
            changes=changes,
        )

    def collection(self) -> dict[str, Any]:
        if self.match("KW_FROZEN"):
            start = self.previous.start
            reference = self.reference()
            return self.node("frozen", start, reference["span"]["end"], reference=reference)
        start = self.expect("KW_LINE_MATCHES").start
        regex = self.expect("RX")
        within = None
        if self.match("KW_WITHIN"):
            within = self.reference()
        self.expect("KW_EXPECT")
        count = self.expect("NAT")
        return self.node(
            "line_matches",
            start,
            count.end,
            regex=regex.value,
            within=within,
            expect=int(count.value),
            expect_decimal=count.value,
        )

    def reference(self) -> dict[str, Any]:
        handle = self.expect("HANDLE")
        group = None
        end = handle.end
        if self.match("DOT"):
            group_token = self.expect("NAME")
            group = group_token.value
            end = group_token.end
        return self.node("reference", handle.start, end, name=handle.value, group=group)

    def selector(self) -> dict[str, Any]:
        if self.current.kind == "HANDLE":
            return self.reference()
        if self.current.kind == "KW_ONE":
            start = self.advance().start
            if self.match("KW_TEXT"):
                mode = "text"
                needle = self.expect("STRING")
                value = needle.value
                end = needle.end
            elif self.match("KW_LINE"):
                mode = "line"
                needle = self.expect("STRING")
                value = needle.value
                end = needle.end
            elif self.current.kind == "HEX":
                mode = "hex"
                needle = self.advance()
                value = needle.value
                end = needle.end
            else:
                self.fail("expected_selector", "one requires text STRING, line STRING, or HEX")
            within = None
            if self.match("KW_WITHIN"):
                within = self.reference()
                end = within["span"]["end"]
            return self.node("one", start, end, mode=mode, needle=value, within=within)
        if self.current.kind == "KW_BETWEEN":
            start = self.advance().start
            first = self.reference()
            self.expect("KW_AND")
            second = self.reference()
            return self.node("between", start, second["span"]["end"], first=first, second=second)
        if self.current.kind == "KW_RANGE":
            start = self.advance().start
            self.expect("LBRACKET")
            low = self.expect("NAT")
            self.expect("COMMA")
            high = self.expect("NAT")
            close = self.expect("RPAREN")
            return self.node(
                "range",
                start,
                close.end,
                low=int(low.value),
                high=int(high.value),
                low_decimal=low.value,
                high_decimal=high.value,
            )
        self.fail("expected_selector", "expected a reference, one, between, or range")

    def payload(self, depth: int) -> dict[str, Any]:
        if depth > self.config.max_depth:
            self.fail(
                "parser_depth",
                f"payload nesting exceeds depth {self.config.max_depth}",
            )
        if self.current.kind == "LBRACKET":
            start = self.advance().start
            pieces = []
            if self.current.kind != "RBRACKET":
                pieces.append(self.piece(depth))
                while self.match("COMMA"):
                    if self.current.kind == "RBRACKET":
                        if not self.config.allow_trailing_comma:
                            self.fail("trailing_comma", "payload list cannot end with a comma")
                        break
                    pieces.append(self.piece(depth))
            close = self.expect("RBRACKET")
            return self.node("payload", start, close.end, pieces=pieces)
        piece = self.piece(depth)
        return self.node("payload", piece["span"]["start"], piece["span"]["end"], pieces=[piece])

    def piece(self, depth: int) -> dict[str, Any]:
        token = self.current
        if token.kind == "STRING":
            self.advance()
            return self.node("string", token.start, token.end, value=token.value)
        if token.kind == "HEREDOC":
            self.advance()
            return self.node(
                "heredoc",
                token.start,
                token.end,
                tag=token.value["tag"],
                value=token.value["text"],
            )
        if token.kind == "HEX":
            self.advance()
            return self.node("hex", token.start, token.end, bytes_hex=token.value)
        if token.kind == "KW_KEEP":
            start = self.advance().start
            self.expect("LPAREN")
            reference = self.reference()
            close = self.expect("RPAREN")
            return self.node("keep", start, close.end, reference=reference)
        if token.kind == "KW_LINES":
            start = self.advance().start
            self.expect("KW_LIKE")
            reference = self.reference()
            self.expect("LBRACKET")
            lines = [self.expect("STRING").value]
            while self.match("COMMA"):
                lines.append(self.expect("STRING").value)
            close = self.expect("RBRACKET")
            return self.node("lines_like", start, close.end, reference=reference, lines=lines)
        if token.kind == "KW_INDENT":
            start = self.advance().start
            self.expect("LPAREN")
            prefix = self.expect("STRING").value
            self.expect("COMMA")
            nested = self.payload(depth + 1)
            close = self.expect("RPAREN")
            return self.node("indent", start, close.end, prefix=prefix, payload=nested)
        self.fail("expected_piece", "expected a payload piece")
