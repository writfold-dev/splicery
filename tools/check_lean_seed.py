"""Run on the admitted workstation inside its pinned offline Lean tool container.

This producer only checks the supplied Lean seed. It cannot pass SG02/SG03.
The evidence path must be fresh and on storage already admitted by SG00.
"""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SYMBOLS = [
    'splitExact_sound','splitExact_append','cut_sound','cut_of_parts','apply_local',
    'keep_identity','lower_correct','batch_preserves_gaps','swap_from_original'
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(output, name, args, cwd):
    res = subprocess.run(args, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    (output/(name+'.log')).write_bytes(res.stdout)
    return res.returncode, res.stdout.decode('utf8','replace')


def checked(output, name, args, cwd):
    code, text = run(output,name,args,cwd)
    if code:
        raise ValueError(name+' failed; inspect the retained log')
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence',required=True,type=Path)
    dest = parser.parse_args().evidence.resolve()
    if dest.is_relative_to(ROOT) or dest.exists():
        raise SystemExit('Use a fresh evidence directory outside the source tree.')
    dest.mkdir(parents=True,exist_ok=False)
    record = {'scope':'Lean seed only','status':'failed','symbols':SYMBOLS}
    try:
        lean = shutil.which('lean')
        lake = shutil.which('lake')
        if not lean or not lake:
            raise ValueError('Missing admitted Lean/Lake')
        files = [p for p in (ROOT/'formal').rglob('*') if p.is_file()
                 and '.lake' not in p.parts and p.name != 'lake-manifest.json']
        record['sources'] = {str(p.relative_to(ROOT)):digest(p) for p in sorted(files)}
        # Run within the copied pinned project so elan resolves the intended toolchain.
        for name in ['control','mutant']:
            copy = dest/name
            for src in files:
                dst = copy/src.relative_to(ROOT/'formal')
                dst.parent.mkdir(parents=True,exist_ok=True)
                shutil.copyfile(src,dst)
        version = checked(dest,'lean-version',[lean,'--version'],dest/'control')
        if not re.search(r'\b4\.21\.0\b',version):
            raise ValueError('Wrong Lean version')
        record['version']=version.strip()
        checked(dest,'control-build',[lake,'build'],dest/'control')
        audit = (dest/'control'/'SeedAxiomAudit.lean')
        audit.write_text('import Splicery\n'+''.join('#print axioms Splicery.'+s+'\n' for s in SYMBOLS))
        result = checked(dest,'control-axioms',[lake,'env','lean',str(audit)],dest/'control')
        permitted = {'propext','Classical.choice','Quot.sound'}
        for symbol in SYMBOLS:
            match = re.search(r"'Splicery\."+re.escape(symbol)+
                              r"' (?:does not depend on any axioms|depends on axioms:\s*\[([^\]]*)\])",result)
            if not match:
                raise ValueError('Missing or unsupported axiom report for '+symbol)
            deps = {x.strip() for x in (match.group(1) or '').split(',') if x.strip()}
            if not deps <= permitted:
                raise ValueError('Unapproved axiom for '+symbol)
            record.setdefault('observed_axioms',{})[symbol]=sorted(deps)
        src = dest/'mutant'/'Splicery'/'Lowering.lean'
        original = src.read_text()
        good = '| .after ps => ⟨(c.pre ++ c.selected).length, 0, render ps⟩'
        broken = '| .after ps => ⟨c.pre.length, 0, render ps⟩'
        if original.count(good)!=1:
            raise ValueError('Mutant failed to select its source')
        src.write_text(original.replace(good,broken))
        theorem_line = original[:original.index('theorem lower_correct')].count('\n')+1
        code,out = run(dest,'mutant-build',[lake,'build'],dest/'mutant')
        locations = [int(i) for i in re.findall(r'Lowering\.lean:(\d+):\d+:\s*error:',out)]
        if code==0 or not locations or any(i<theorem_line for i in locations):
            raise ValueError('The chosen lower_correct negative control did not fail as required')
        record['semantic_mutant']={'changed':'after lowered to start','diagnostics_lines':locations,
                                  'mutated_source_sha256':digest(src)}
        record['status']='passed_seed_only'
    except (OSError,ValueError) as exc:
        record['reason']=str(exc)
    finally:
        (dest/'seed-receipt.json').write_text(json.dumps(record,indent=2)+'\n')
    if record['status']!='passed_seed_only':
        sys.exit(record.get('reason','Failed'))
    print('Checked Lean seed. Full Splicery gates remain separate.')


if __name__=='__main__':
    main()
