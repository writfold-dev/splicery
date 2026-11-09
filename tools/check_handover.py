"""Offline authoring qualification. Invoke with python3 -B, without -O."""
import hashlib
import itertools
import json
import re
import sys
from pathlib import Path

from byte_reference import Span, Refusal, apply
from grammar import parse
from protocol_rehearsal import explore
from recipe_rehearsal import run

ROOT = Path(__file__).resolve().parents[1]


def read(relpath):
    return json.loads((ROOT/relpath).read_text())


def check_gate_graph():
    gates = read('qualification/gates.json')['gates']
    seen = set()
    for g in gates:
        assert g['id'] not in seen and g['status'] == 'not_run' and g['required']
        assert set(g['requires']) <= seen
        assert all(g[k] for k in ('pass_criteria','negative_controls','receipt_material','owner'))
        seen.add(g['id'])
    assert seen == {f'SG{i:02}' for i in range(15)}
    assert set(gates[-1]['requires']) == seen-{'SG14'}
    return len(seen)


def check_grammar():
    cases = read('qualification/grammar-cases.json')
    for s in cases['accept']:
        parse(s)
    for s in cases['reject']:
        try:
            parse(s)
        except ValueError:
            continue
        raise AssertionError('syntax unexpectedly accepted: '+s)
    return {k: len(v) for k, v in cases.items()}


def check_recipes():
    cases = read('qualification/recipe-vectors.json')
    assert {p.stem for p in (ROOT/'examples').glob('*.spl')} == {c['name'] for c in cases}
    for c in cases:
        src = (ROOT/'examples'/(c['name']+'.spl')).read_bytes().decode()
        base = bytes.fromhex(c['input_hex']) if 'input_hex' in c else c['input_text'].encode()
        try:
            out = run(src, base)
        except Refusal as exc:
            assert c.get('refusal') == str(exc), (c['name'], exc)
        else:
            assert 'refusal' not in c
            expected = bytes.fromhex(c['output_hex']) if 'output_hex' in c else c['output_text'].encode()
            assert out == expected, (c['name'],out,expected)
    extras = [
        ('@a := one text "aa" delete @a',b'aaa','Cardinality'),
        ('delete range [1,2)', 'é'.encode(),'Boundary'),
        ('delete range [2,1)',b'abc','Bounds'),
        ('replace range [0,2) with hex"ff"',b'ab','Encoding'),
        ('before range [1,2) insert "a" after range [0,1) insert "b"',b'ab','Overlap'),
        ('delete range [0,2) before range [2,2) insert "x"',b'ab','Overlap'),
        ('@a := one line "foo" @a := one line "foo"',b'foo','Duplicate'),
        ('replace @missing with "x"',b'foo','Unbound'),
        ('each @r in line_matches rx"z" expect 1 { delete @r }',b'x','Cardinality'),
        ('@a := one text "a" delete whole @a',b'a\n','Whole'),
        ('@a := range [2,3) @b := range [0,1) delete from @a to @b',b'abc','Markers'),
        ('@a := range [0,2) @b := range [1,3) delete from @a to @b',b'abc','Markers'),
        ('@a := range [1,1) @b := range [1,1) delete from @a to @b',b'abc','Markers')]
    for fragment, data, error in extras:
        try:
            run('edit "r/f" { '+fragment+' }',data)
        except Refusal as exc:
            assert str(exc) == error,(fragment,exc)
        else:
            raise AssertionError(fragment)
    return {'recipe_files':len(cases),'extra_refusals':len(extras)}


def check_ir():
    references = {v['name']: v for v in read('qualification/recipe-vectors.json')}
    ir = read('qualification/ir-vectors.json')
    for case in ir:
        recipe = references[case['recipe']]
        original = recipe['input_text'].encode()
        edits = []
        for op in case['splices']:
            pieces = []
            for item in op['parts']:
                if 'copy' in item:
                    lo, hi = item['copy']
                    assert 0 <= lo <= hi <= len(original)
                    pieces.append(original[lo:hi])
                else:
                    pieces.append(bytes.fromhex(item['literal_hex']))
            edits.append((Span(*op['target']), b''.join(pieces)))
        assert apply(original, edits) == recipe['output_text'].encode()
    return len(ir)


def check_splices():
    singles, pairs = 0, 0
    for n in range(6):
        for tup in itertools.product([0,255], repeat=n):
            data = bytes(tup)
            spans = [Span(i,j) for i in range(n+1) for j in range(i,n+1)]
            for s in spans:
                for repl in [b'', b'Z', b'\x00\xff', data[s.lo:s.hi]]:
                    out = apply(data, [(s,repl)], True)
                    assert out == data[:s.lo]+repl+data[s.hi:]
                    singles += 1
            for a,b in itertools.product(spans, repeat=2):
                # Independent oracle: model occupied bytes plus closed boundaries
                # for an insertion. Ranges with positive widths may be adjacent.
                if a.lo == a.hi:
                    conflict = b.lo <= a.lo <= b.hi
                elif b.lo == b.hi:
                    conflict = a.lo <= b.lo <= a.hi
                else:
                    conflict = max(a.lo,b.lo) < min(a.hi,b.hi)
                try:
                    out = apply(data, [(a,b'X'),(b,b'Y')],True)
                except Refusal as e:
                    assert conflict and str(e) == 'Overlap'
                else:
                    assert not conflict
                    by_pos = sorted([(a,b'X'),(b,b'Y')],key=lambda v:v[0])
                    # Apply right-to-left to original coordinates (different algorithm).
                    expected = data
                    for s, repl in reversed(by_pos):
                        expected = expected[:s.lo]+repl+expected[s.hi:]
                    assert out == expected
                pairs += 1
    return {'one_op':singles,'two_op':pairs}


def check_files():
    missing=[]
    for p in ROOT.rglob('*.md'):
        for url in re.findall(r'\[[^]\n]*\]\(([^)\s]+)\)',p.read_text()):
            if '://' in url or url.startswith('#'):
                continue
            if not (p.parent/url.split('#')[0]).exists():
                missing.append((p.name,url))
    assert not missing,missing
    manifest = ROOT/'MANIFEST.sha256'
    if manifest.exists():
        expected = {}
        for line in manifest.read_text().splitlines():
            digest, name = line.split('  ',1)
            assert name not in expected
            expected[name] = digest
        actual = {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in ROOT.rglob('*') if p.is_file() and p != manifest and '__pycache__' not in p.parts}
        assert actual == expected,'manifest member or digest mismatch'


def main():
    if not __debug__:
        raise SystemExit('Assertions required. Run without -O.')
    count, edges, err, _ = explore()
    assert err is None
    faults = {}
    for m,expected in [('skip_live','unchecked_write'),('unfenced','lost_fence'),
                       ('confirm_timeout','false_confirmation'),('blind_reissue','double_write')]:
        _, _, reason, trace = explore(m)
        assert reason == expected,(m,reason,trace)
        faults[m] = {'invariant_failure':reason,'trace':trace}
    result = {'grammar':check_grammar(),'recipes':check_recipes(),'byte_cases':check_splices(),
              'ir_vector_count':check_ir(), 'gates':check_gate_graph(),'protocol_python_only':{'states':count,'edges':edges,'mutants':faults}}
    check_files()
    print(json.dumps(result,indent=2))


if __name__ == '__main__':
    main()
