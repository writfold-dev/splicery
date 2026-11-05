"""A byte-oriented lexer for Splicery /1, written directly from spec/language.md."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional
import unicodedata

from .errors import RecipeError


MAX_RECIPE_BYTES = 256 * 1024
MAX_HEREDOC_TAG = 64
MAX_NAT = (1 << 64) - 1

KEYWORDS = frozenset(
    {
        "edit",
        "as",
        "bytes",
        "create",
        "with",
        "rename",
        "to",
        "remove",
        "replace",
        "before",
        "after",
        "insert",
        "delete",
        "each",
        "in",
        "line_matches",
        "within",
        "expect",
        "frozen",
        "one",
        "text",
        "line",
        "between",
        "and",
        "range",
        "keep",
        "lines",
        "like",
        "indent",
        "hex",
        "rx",
    }
)

SYMBOLS = {
    b":=": "ASSIGN",
    b"{": "LBRACE",
    b"}": "RBRACE",
    b"[": "LBRACKET",
    b"]": "RBRACKET",
    b"(": "LPAREN",
    b")": "RPAREN",
    b",": "COMMA",
    b".": "DOT",
    b"=": "EQUAL",
}


@dataclass(frozen=True)
class LexerConfig:
    allow_lone_cr: bool = False


@dataclass(frozen=True)
class Token:
    kind: str
    start: int
    end: int
    value: Optional[Any] = None

    def golden(self, source: bytes) -> dict[str, Any]:
        item: dict[str, Any] = {
            "kind": self.kind,
            "span": {"start": self.start, "end": self.end},
            "lexeme_hex": source[self.start : self.end].hex(),
        }
        if self.value is not None:
            item["value"] = self.value
        return item


def _name_start(byte: int) -> bool:
    return 65 <= byte <= 90 or 97 <= byte <= 122 or byte == 95


def _name_continue(byte: int) -> bool:
    return _name_start(byte) or 48 <= byte <= 57


def _utf8_width(byte: int) -> int:
    if byte < 0x80:
        return 1
    if 0xC2 <= byte <= 0xDF:
        return 2
    if 0xE0 <= byte <= 0xEF:
        return 3
    if 0xF0 <= byte <= 0xF4:
        return 4
    return 1


class Lexer:
    def __init__(self, source: bytes, config: LexerConfig = LexerConfig()) -> None:
        self.source = source
        self.config = config
        self.pos = 0
        self.tokens: list[Token] = []

    def fail(self, phase: str, code: str, message: str, start: int, end: int) -> None:
        raise RecipeError(
            phase, code, message, self.source, start, end, self.tokens
        )

    def emit(self, kind: str, start: int, end: int, value: Any = None) -> None:
        self.tokens.append(Token(kind, start, end, value))

    def tokenize(self) -> list[Token]:
        if len(self.source) > MAX_RECIPE_BYTES:
            self.fail(
                "limit",
                "recipe_too_large",
                "recipe exceeds the 256 KiB /1 source limit",
                0,
                min(len(self.source), 64),
            )
        try:
            self.source.decode("utf-8", "strict")
        except UnicodeDecodeError as exc:
            self.fail(
                "encoding",
                "invalid_utf8",
                "recipe is not strict UTF-8",
                exc.start,
                exc.end,
            )
        bom = self.source.find(b"\xef\xbb\xbf")
        if bom >= 0:
            self.fail(
                "encoding",
                "bom",
                "Unicode BOM is forbidden in recipe source",
                bom,
                bom + 3,
            )

        while self.pos < len(self.source):
            byte = self.source[self.pos]
            if byte in (0x20, 0x09, 0x0A):
                self.pos += 1
                continue
            if byte == 0x0D:
                if self.pos + 1 < len(self.source) and self.source[self.pos + 1] == 0x0A:
                    self.pos += 2
                    continue
                if self.config.allow_lone_cr:
                    self.pos += 1
                    continue
                self.fail("lexical", "lone_cr", "lone CR outside a literal", self.pos, self.pos + 1)
            if byte == 0:
                self.fail(
                    "lexical",
                    "nul_outside_literal",
                    "NUL is only permitted inside a literal",
                    self.pos,
                    self.pos + 1,
                )
            if byte == 0x23:
                self._comment()
                continue
            if self.source.startswith(b"<<", self.pos):
                self._heredoc()
                continue
            if self.source.startswith(b'hex"', self.pos):
                self._hex()
                continue
            if self.source.startswith(b'rx"', self.pos):
                self._rx()
                continue
            if byte == 0x22:
                self._string()
                continue
            if byte == 0x40:
                self._handle()
                continue
            if _name_start(byte):
                self._identifier()
                continue
            if 48 <= byte <= 57:
                self._nat()
                continue
            matched = False
            for spelling, kind in SYMBOLS.items():
                if self.source.startswith(spelling, self.pos):
                    start = self.pos
                    self.pos += len(spelling)
                    self.emit(kind, start, self.pos)
                    matched = True
                    break
            if matched:
                continue
            self.fail(
                "lexical",
                "unexpected_byte",
                "byte cannot begin a Splicery token",
                self.pos,
                self.pos + 1,
            )
        self.emit("EOF", len(self.source), len(self.source))
        return self.tokens

    def _comment(self) -> None:
        self.pos += 1
        while self.pos < len(self.source) and self.source[self.pos] not in (0x0A, 0x0D):
            if self.source[self.pos] == 0:
                self.fail(
                    "lexical",
                    "nul_outside_literal",
                    "NUL is only permitted inside a literal",
                    self.pos,
                    self.pos + 1,
                )
            self.pos += 1

    def _identifier(self) -> None:
        start = self.pos
        self.pos += 1
        while self.pos < len(self.source) and _name_continue(self.source[self.pos]):
            self.pos += 1
        name = self.source[start : self.pos].decode("ascii")
        kind = "KW_" + name.upper() if name in KEYWORDS else "NAME"
        self.emit(kind, start, self.pos, name)

    def _handle(self) -> None:
        start = self.pos
        self.pos += 1
        if self.pos >= len(self.source) or not _name_start(self.source[self.pos]):
            self.fail(
                "lexical",
                "invalid_handle",
                "HANDLE requires @ followed by an ASCII name",
                start,
                min(self.pos + 1, len(self.source)),
            )
        name_start = self.pos
        self.pos += 1
        while self.pos < len(self.source) and _name_continue(self.source[self.pos]):
            self.pos += 1
        name = self.source[name_start : self.pos].decode("ascii")
        if name in KEYWORDS:
            self.fail(
                "lexical",
                "reserved_handle",
                "reserved words cannot be used as HANDLE names",
                start,
                self.pos,
            )
        self.emit("HANDLE", start, self.pos, name)

    def _string(self) -> None:
        start = self.pos
        self.pos += 1
        decoded = bytearray()
        while self.pos < len(self.source):
            byte = self.source[self.pos]
            if byte == 0x22:
                self.pos += 1
                value = decoded.decode("utf-8", "strict")
                self.emit("STRING", start, self.pos, value)
                return
            if byte == 0x5C:
                escape_start = self.pos
                self.pos += 1
                if self.pos >= len(self.source):
                    self.fail("lexical", "unterminated_string", "unterminated STRING escape", start, self.pos)
                escape = self.source[self.pos]
                replacements = {
                    0x22: b'"',
                    0x5C: b"\\",
                    0x6E: b"\n",
                    0x72: b"\r",
                    0x74: b"\t",
                }
                if escape not in replacements:
                    self.fail(
                        "lexical",
                        "invalid_escape",
                        "STRING escape must be one of quote, backslash, n, r, t",
                        escape_start,
                        self.pos + 1,
                    )
                decoded.extend(replacements[escape])
                self.pos += 1
                continue
            if byte in (0x0A, 0x0D):
                self.fail(
                    "lexical",
                    "string_newline",
                    "physical line breaks are forbidden in STRING",
                    self.pos,
                    self.pos + 1,
                )
            if byte < 0x20 or byte == 0x7F:
                self.fail(
                    "lexical",
                    "string_control",
                    "unescaped control characters are forbidden in STRING",
                    self.pos,
                    self.pos + 1,
                )
            if byte >= 0x80:
                width = _utf8_width(byte)
                char = self.source[self.pos : self.pos + width].decode("utf-8", "strict")
                if unicodedata.category(char) == "Cc":
                    self.fail(
                        "lexical",
                        "string_control",
                        "unescaped control characters are forbidden in STRING",
                        self.pos,
                        self.pos + width,
                    )
                decoded.extend(self.source[self.pos : self.pos + width])
                self.pos += width
                continue
            decoded.append(byte)
            self.pos += 1
        self.fail("lexical", "unterminated_string", "unterminated STRING", start, self.pos)

    def _hex(self) -> None:
        start = self.pos
        self.pos += 4
        body_start = self.pos
        while self.pos < len(self.source):
            byte = self.source[self.pos]
            if byte == 0x22:
                digits = self.source[body_start : self.pos]
                if len(digits) % 2:
                    self.fail("lexical", "odd_hex", "HEX requires complete byte pairs", self.pos - 1, self.pos)
                self.pos += 1
                self.emit("HEX", start, self.pos, digits.decode("ascii").lower())
                return
            if not (48 <= byte <= 57 or 65 <= byte <= 70 or 97 <= byte <= 102):
                self.fail(
                    "lexical",
                    "invalid_hex",
                    "HEX body accepts ASCII hexadecimal digits only",
                    self.pos,
                    self.pos + 1,
                )
            self.pos += 1
        self.fail("lexical", "unterminated_hex", "unterminated HEX", start, self.pos)

    def _rx(self) -> None:
        start = self.pos
        self.pos += 3
        body_start = self.pos
        while self.pos < len(self.source):
            byte = self.source[self.pos]
            if byte == 0x22:
                body_end = self.pos
                self.pos += 1
                self._check_regex_group_names(body_start, body_end)
                value = self.source[body_start:body_end].decode("utf-8", "strict")
                self.emit("RX", start, self.pos, value)
                return
            if byte in (0x0A, 0x0D):
                self.fail("lexical", "rx_newline", "RX must remain on one physical line", self.pos, self.pos + 1)
            self.pos += 1
        self.fail("lexical", "unterminated_rx", "unterminated RX", start, self.pos)

    def _check_regex_group_names(self, start: int, end: int) -> None:
        pos = start
        in_class = False
        while pos < end:
            byte = self.source[pos]
            if byte == 0x5C:
                pos = min(pos + 2, end)
                continue
            if byte == 0x5B and not in_class:
                in_class = True
                pos += 1
                continue
            if byte == 0x5D and in_class:
                in_class = False
                pos += 1
                continue
            if not in_class and self.source.startswith(b"(?<", pos):
                name_start = pos + 3
                cursor = name_start
                if cursor < end and _name_start(self.source[cursor]):
                    cursor += 1
                    while cursor < end and _name_continue(self.source[cursor]):
                        cursor += 1
                    if cursor < end and self.source[cursor] == 0x3E:
                        name = self.source[name_start:cursor].decode("ascii")
                        if name in KEYWORDS:
                            self.fail(
                                "lexical",
                                "reserved_regex_group",
                                "reserved words cannot declare regex groups",
                                name_start,
                                cursor,
                            )
                        pos = cursor + 1
                        continue
            pos += 1

    def _nat(self) -> None:
        start = self.pos
        self.pos += 1
        while self.pos < len(self.source) and 48 <= self.source[self.pos] <= 57:
            self.pos += 1
        digits = self.source[start : self.pos]
        if len(digits) > 1 and digits[0] == 48:
            self.fail("lexical", "nat_leading_zero", "NAT has a leading zero", start, self.pos)
        value = 0
        overflow = False
        for byte in digits:
            digit = byte - 48
            if not overflow:
                if value > (MAX_NAT - digit) // 10:
                    overflow = True
                else:
                    value = value * 10 + digit
        if overflow:
            self.fail(
                "limit",
                "nat_overflow",
                "NAT exceeds the checked unsigned 64-bit range",
                start,
                min(self.pos, start + 32),
            )
        self.emit("NAT", start, self.pos, digits.decode("ascii"))

    def _heredoc(self) -> None:
        start = self.pos
        if not self.source.startswith(b"<<'", self.pos):
            self.fail("lexical", "heredoc_syntax", "HEREDOC begins with <<'TAG'", start, min(start + 3, len(self.source)))
        self.pos += 3
        tag_start = self.pos
        if self.pos >= len(self.source) or not _name_start(self.source[self.pos]):
            self.fail("lexical", "heredoc_tag", "HEREDOC tag must be an ASCII identifier", tag_start, self.pos + 1)
        self.pos += 1
        while self.pos < len(self.source) and _name_continue(self.source[self.pos]):
            self.pos += 1
        tag_end = self.pos
        if tag_end - tag_start > MAX_HEREDOC_TAG:
            self.fail("lexical", "heredoc_tag_too_long", "HEREDOC tag exceeds 64 ASCII bytes", tag_start, tag_end)
        if self.pos >= len(self.source) or self.source[self.pos] != 0x27:
            self.fail("lexical", "heredoc_tag", "HEREDOC tag is missing its closing quote", tag_start, self.pos)
        self.pos += 1
        if self.pos >= len(self.source):
            self.fail("lexical", "heredoc_open_eol", "HEREDOC opener requires LF or CRLF", start, self.pos)
        if self.source[self.pos] == 0x0A:
            self.pos += 1
        elif self.source.startswith(b"\r\n", self.pos):
            self.pos += 2
        elif self.source[self.pos] == 0x0D:
            self.fail("lexical", "lone_cr", "lone CR after HEREDOC opener", self.pos, self.pos + 1)
        else:
            self.fail("lexical", "heredoc_open_eol", "HEREDOC opener requires immediate LF or CRLF", self.pos, self.pos + 1)

        tag = self.source[tag_start:tag_end]
        content_start = self.pos
        line_start = self.pos
        while line_start < len(self.source):
            lf = self.source.find(b"\n", line_start)
            if lf < 0:
                if self.source[line_start:] == tag:
                    content = self.source[content_start:line_start]
                    self.pos = len(self.source)
                    self.emit(
                        "HEREDOC",
                        start,
                        self.pos,
                        {"tag": tag.decode("ascii"), "text": content.decode("utf-8", "strict")},
                    )
                    return
                break
            body_end = lf - 1 if lf > line_start and self.source[lf - 1] == 0x0D else lf
            if self.source[line_start:body_end] == tag:
                content = self.source[content_start:line_start]
                self.pos = lf + 1
                self.emit(
                    "HEREDOC",
                    start,
                    self.pos,
                    {"tag": tag.decode("ascii"), "text": content.decode("utf-8", "strict")},
                )
                return
            line_start = lf + 1
        self.fail("lexical", "unterminated_heredoc", "HEREDOC closing delimiter is absent", start, min(content_start, start + 64))
