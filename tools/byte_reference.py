"""Pure byte model for valid fixture profiles. Offsets refer to one immutable input."""
from dataclasses import dataclass


class Refusal(ValueError):
    pass


@dataclass(frozen=True, order=True)
class Span:
    lo: int
    hi: int


def ordered(edits, size):
    for pos, _ in edits:
        if not 0 <= pos.lo <= pos.hi <= size:
            raise Refusal('Bounds')
    result = sorted(edits, key=lambda e: e[0])
    for (a, _), (b, _) in zip(result, result[1:]):
        if b.lo < a.hi or (b.lo == a.hi and (a.lo == a.hi or b.lo == b.hi)):
            raise Refusal('Overlap')
    return result


def apply(base, edits, raw=False):
    if not raw:
        try:
            base.decode('utf8')
        except UnicodeError as exc:
            raise Refusal('Encoding') from exc
    result, cursor = [], 0
    for pos, replacement in ordered(edits, len(base)):
        if not raw:
            for i in [pos.lo, pos.hi]:
                if i < len(base) and (base[i] & 0xC0) == 0x80:
                    raise Refusal('Boundary')
        result.extend((base[cursor:pos.lo], replacement))
        cursor = pos.hi
    out = b''.join([*result, base[cursor:]])
    if not raw:
        try:
            out.decode('utf8')
        except UnicodeError as exc:
            raise Refusal('Encoding') from exc
    return out


def lines(data):
    offset = 0
    while offset < len(data):
        newline = data.find(b'\n', offset)
        end = len(data) if newline < 0 else newline + 1
        hi = end if newline < 0 else newline
        if newline >= 0 and hi > offset and data[hi-1] == 13:
            hi -= 1
        yield Span(offset, hi), data[hi:end]
        offset = end
