"""Small EBNF recognizer for trusted handover fixtures, with parse trees."""
import ast
import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GRAMMAR_TOKEN = re.compile(r'"[^"]*"|[A-Za-z_][A-Za-z_0-9]*|[][(){}=|;]')
NAME = r'[A-Za-z_][A-Za-z_0-9]*'


@dataclass(frozen=True)
class Node:
    kind: str
    value: object


def read_grammar():
    source = (ROOT / 'spec/splicery.ebnf').read_text()
    if GRAMMAR_TOKEN.sub('', source).strip():
        raise ValueError('invalid EBNF character')
    tokens = iter(GRAMMAR_TOKEN.findall(source))
    current = next(tokens, None)

    def take():
        nonlocal current
        item, current = current, next(tokens, None)
        return item

    def expect(token):
        if take() != token:
            raise ValueError('malformed EBNF, expected ' + token)

    def choice():
        branches = []
        while True:
            terms = []
            while current not in (None, ';', '|', ']', '}', ')'):
                item = take()
                if item in ['(', '[', '{']:
                    term = choice()
                    expect({'(': ')', '[': ']', '{': '}'}[item])
                    terms.append(({'(': 'group', '[': 'opt', '{': 'rep'}[item], term))
                elif item.startswith('"'):
                    terms.append(('literal', ast.literal_eval(item)))
                else:
                    terms.append(('name', item))
            if not terms:
                raise ValueError('unexpected empty production')
            branches.append(('seq', tuple(terms)))
            if current != '|':
                return ('alt', tuple(branches))
            take()

    result = {}
    while current:
        name = take()
        if name in result:
            raise ValueError('duplicate rule')
        expect('=')
        result[name] = choice()
        expect(';')
    return result, set(re.findall(r'"([a-z_]+)"', source))


RULES, KEYWORDS = read_grammar()
STRING = re.compile(r'"(?:\\["\\nrt]|[^"\\\x00-\x1f])*"')
TRIVIA = re.compile(r'(?:[ \t\n]|\r\n|#[^\r\n]*)+')
WORDS = re.compile(NAME)


def lex(source):
    tokens, pos = [], 0
    while pos < len(source):
        tail = source[pos:]
        m = TRIVIA.match(tail)
        if m:
            pos += m.end()
            continue
        if tail.startswith("<<'"):
            m = re.match("<<'(" + NAME + r")'(\r?\n)", tail)
            if not m or len(m[1]) > 64:
                raise ValueError('bad heredoc opener')
            end = re.search('(?m)^' + re.escape(m[1]) + r'(?:\r?\n|\Z)', tail[m.end():])
            if not end:
                raise ValueError('unterminated heredoc')
            kind, value, length = 'HEREDOC', tail[m.end():m.end()+end.start()], m.end()+end.end()
        elif tail.startswith('"'):
            m = STRING.match(tail)
            if not m:
                raise ValueError('bad string')
            import json
            kind, value, length = 'STRING', json.loads(m[0]), m.end()
        elif tail.startswith(('rx"', 'hex"')):
            m = re.match(r'(rx|hex)"([^"\r\n]*)"', tail)
            if not m:
                raise ValueError('bad literal')
            kind, value, length = ('RX' if m[1] == 'rx' else 'HEX'), m[2], m.end()
            if kind == 'HEX' and not re.fullmatch(r'(?:[0-9a-fA-F]{2})*', value):
                raise ValueError('bad hex')
        elif tail.startswith('@'):
            m = re.match('@(' + NAME + ')', tail)
            if not m or m[1] in KEYWORDS:
                raise ValueError('bad handle')
            kind, value, length = 'HANDLE', m[1], m.end()
        elif (m := WORDS.match(tail)):
            kind = m[0] if m[0] in KEYWORDS else 'NAME'
            value, length = m[0], m.end()
        elif (m := re.match(r'[0-9]+', tail)):
            if len(m[0]) > 1 and m[0][0] == '0':
                raise ValueError('bad natural')
            kind, value, length = 'NAT', int(m[0]), m.end()
        elif (m := re.match(r':=|[][{},.()]', tail)):
            kind, value, length = m[0], m[0], m.end()
        else:
            raise ValueError('unexpected source near ' + repr(tail[:16]))
        tokens.append(Node(kind, value))
        pos += length
    return tokens


def parse(source):
    tokens = lex(source)

    @lru_cache(None)
    def visit(expr, pos):
        op, value = expr
        if op == 'name' and value in RULES:
            return tuple(((Node(value, ns),), end) for ns, end in visit(RULES[value], pos))
        if op in ('name', 'literal'):
            return (((tokens[pos],), pos+1),) if pos < len(tokens) and tokens[pos].kind == value else ()
        if op == 'alt':
            return tuple(pair for item in value for pair in visit(item, pos))
        if op == 'group':
            return visit(value, pos)
        if op == 'opt':
            return (((), pos),) + visit(value, pos)
        if op == 'seq':
            states = [((), pos)]
            for item in value:
                states = [(ns + extra, stop) for ns, i in states for extra, stop in visit(item, i)]
            return tuple(states)
        states, layer = [((), pos)], [((), pos)]
        while layer:
            layer = [(ns + extra, stop) for ns, i in layer for extra, stop in visit(value, i) if stop > i]
            states.extend(layer)
        return tuple(states)

    matches = [ns[0] for ns, end in visit(('name', 'document'), 0) if end == len(tokens)]
    if len(matches) != 1:
        raise ValueError('expected exactly one complete parse, got ' + str(len(matches)))
    return matches[0]
