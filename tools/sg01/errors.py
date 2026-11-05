"""Typed, bounded diagnostics for the independent SG01 reader."""

from __future__ import annotations

from typing import Any, Optional


MAX_DIAGNOSTIC_MESSAGE = 160
MAX_DIAGNOSTIC_CONTEXT = 64


class RecipeError(Exception):
    """A source rejection with a half-open byte span into the original recipe."""

    def __init__(
        self,
        phase: str,
        code: str,
        message: str,
        source: bytes,
        start: int,
        end: int,
        tokens: Optional[list[Any]] = None,
    ) -> None:
        super().__init__(message)
        size = len(source)
        self.phase = phase
        self.code = code
        self.message = message[:MAX_DIAGNOSTIC_MESSAGE]
        self.start = max(0, min(start, size))
        self.end = max(self.start, min(end, size))
        self.source = source
        self.tokens = list(tokens) if tokens is not None else []

    def diagnostic(self) -> dict[str, Any]:
        lo = max(0, self.start - MAX_DIAGNOSTIC_CONTEXT // 2)
        hi = min(len(self.source), lo + MAX_DIAGNOSTIC_CONTEXT)
        lo = max(0, hi - MAX_DIAGNOSTIC_CONTEXT)
        return {
            "phase": self.phase,
            "code": self.code,
            "message": self.message,
            "span": {"start": self.start, "end": self.end},
            "context": {
                "start": lo,
                "end": hi,
                "hex": self.source[lo:hi].hex(),
                "truncated": lo > 0 or hi < len(self.source),
            },
        }
