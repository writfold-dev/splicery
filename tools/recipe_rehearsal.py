"""Trusted-fixture oracle for single-file recipes. No IO, wire, or authority backend."""
import re
from dataclasses import dataclass, field
from byte_reference import Span, Refusal, apply, lines
from grammar import parse


@dataclass(frozen=True)
class Capture:
    pos: Span
    eol: bytes | None = None
    groups: dict = field(default_factory=dict)


def run(source, base):
    root = parse(source).value[0]
    if root.kind != 'file_edit':
        raise Refusal('ExampleProfile')
    raw = any(t.kind == 'bytes' for t in root.value)
    env, edits = {}, []

    def value(capture):
        return base[capture.pos.lo:capture.pos.hi]

    def reference(node):
        tokens = node.value
        if tokens[0].value not in env:
            raise Refusal('Unbound')
        cap = env[tokens[0].value]
        if len(tokens) == 3:
            cap = cap.groups.get(tokens[2].value)
            if cap is None:
                raise Refusal('AbsentGroup')
        return cap

    def select(node):
        t = node.value
        if t[0].kind == 'reference':
            return reference(t[0])
        if t[0].kind == 'range':
            span = Span(t[2].value, t[4].value)
            if not 0 <= span.lo <= span.hi <= len(base):
                raise Refusal('Bounds')
            return Capture(span)
        if t[0].kind == 'between':
            a, b = reference(t[1]), reference(t[3])
            if not a.eol or b.eol is None or a.pos.hi+len(a.eol) > b.pos.lo:
                raise Refusal('Markers')
            return Capture(Span(a.pos.hi+len(a.eol), b.pos.lo))
        window = reference(t[-1]).pos if t[-1].kind == 'reference' else Span(0, len(base))
        kind = t[1].kind
        if raw and kind != 'HEX':
            raise Refusal('Profile')
        needle = bytes.fromhex(t[1].value) if kind == 'HEX' else t[2].value.encode()
        if kind == 'line':
            hits = [Capture(s, end) for s, end in lines(base)
                    if window.lo <= s.lo <= s.hi <= window.hi and base[s.lo:s.hi] == needle]
        else:
            if not needle:
                raise Refusal('EmptyNeedle')
            hits, pos = [], window.lo
            while (pos := base.find(needle, pos, window.hi)) >= 0:
                hits.append(Capture(Span(pos, pos+len(needle))))
                pos += 1
        if len(hits) != 1:
            raise Refusal('Cardinality')
        return hits[0]

    def payload(node):
        if node.kind == 'payload':
            return b''.join(payload(t) for t in node.value if t.kind == 'piece')
        t = node.value
        if t[0].kind in ('STRING', 'HEREDOC'):
            return t[0].value.encode()
        if t[0].kind == 'HEX':
            return bytes.fromhex(t[0].value)
        if t[0].kind == 'keep':
            return value(reference(t[2]))
        if raw:
            raise Refusal('Profile')
        if t[0].kind == 'lines':
            eol = reference(t[2]).eol
            if not eol or any('\n' in x.value or '\r' in x.value for x in t if x.kind == 'STRING'):
                raise Refusal('Lines')
            return b''.join(x.value.encode()+eol for x in t if x.kind == 'STRING')
        prefix = t[2].value.encode()
        if prefix.strip(b' \t'):
            raise Refusal('Indent')
        data = payload(t[4])
        return b''.join((prefix if s.hi > s.lo else b'') + data[s.lo:s.hi]+end
                        for s, end in lines(data))

    def change(node):
        t = node.value
        s = select(t[1]).pos
        if t[0].kind == 'before':
            s = Span(s.lo, s.lo)
        elif t[0].kind == 'after':
            s = Span(s.hi, s.hi)
        edits.append((s, b'' if t[0].kind == 'delete' else payload(t[-1])))

    def members(t):
        if raw or t[0].kind != 'line_matches':
            raise Refusal('ExampleProfile')
        # Python regex is only an independent oracle for the listed trusted examples.
        pattern = re.compile(re.sub(r'\(\?<([A-Za-z_]\w*)>', r'(?P<\1>', t[1].value), re.ASCII)
        limit = reference(t[3]).pos if len(t) == 6 else Span(0, len(base))
        result = []
        for s, _ in lines(base):
            row = base[s.lo:s.hi].decode('utf8')
            offsets = [s.lo]
            for scalar in row:
                offsets.append(offsets[-1] + len(scalar.encode()))
            for m in pattern.finditer(row):
                pos = Span(offsets[m.start()], offsets[m.end()])
                if not limit.lo <= pos.lo <= pos.hi <= limit.hi:
                    continue
                if pos.lo == pos.hi:
                    raise Refusal('EmptyMatch')
                groups = {n: Capture(Span(offsets[m.start(n)], offsets[m.end(n)]))
                          for n in pattern.groupindex if m.start(n) >= 0}
                result.append(Capture(pos, groups=groups))
        if len(result) != t[-1].value:
            raise Refusal('Cardinality')
        return result

    for st in [x.value[0] for x in root.value if x.kind == 'statement']:
        t = st.value
        if st.kind == 'change':
            change(st)
        elif st.kind == 'binding':
            if t[0].value in env:
                raise Refusal('Duplicate')
            env[t[0].value] = select(t[2])
        else:
            name = t[1].value
            if name in env:
                raise Refusal('Duplicate')
            for m in members(t[3].value):
                env[name] = m
                for cmd in (c for c in t if c.kind == 'change'):
                    change(cmd)
                del env[name]
    return apply(base, edits, raw)
