"""Independent finite paper protocol simulator. TLC and TLAPS are separate gates."""
from collections import deque
from dataclasses import dataclass, replace


@dataclass(frozen=True)
class State:
    phase: str = 'idle'
    file: int = 0
    base: int = 0
    owner: int = 0
    stamp: int = 0
    grant: bool = True
    writes: int = 0
    observed: bool = False
    unauthorized_write: bool = False


def successors(s, mutant):
    def step(label, **changes):
        return label, replace(s, **changes)

    live = s.grant and s.stamp == s.owner and s.file == s.base
    if s.phase == 'idle' and s.grant and s.file != 2:
        yield step('prepare', phase='ready', base=s.file, stamp=s.owner)
    if s.phase in ('idle', 'ready') and s.file == 0:
        yield step('drift', file=1)
    if s.grant:
        yield step('revoke', grant=False)
    if s.owner == 0 and (s.phase in ('idle', 'ready', 'confirmed', 'refused') or mutant == 'unfenced'):
        yield step('transfer', owner=1)
    if s.phase == 'ready' and live:
        yield step('claim', phase='claimed')
    if s.phase == 'claimed' and (live or mutant == 'skip_live'):
        yield step('write', phase='wrote', file=2, writes=s.writes+1, unauthorized_write=not live)
    if s.phase in ('ready', 'claimed') and not live:
        yield step('refuse', phase='refused')
    if s.phase in ('claimed', 'wrote'):
        yield step('lost', phase='unknown')
    if s.phase in ('wrote', 'unknown') and (s.file == 2 or mutant == 'confirm_timeout'):
        yield step('confirm', phase='confirmed', observed=True)
    if mutant == 'blind_reissue' and s.phase == 'unknown' and s.file == 2:
        yield step('unconditional_retry', phase='wrote', file=2, writes=s.writes+1, unauthorized_write=not live)


def explore(mutant=None):
    q, seen, edges = deque([(State(), ())]), {State()}, 0
    while q:
        s, trace = q.popleft()
        problem = None
        if s.writes > 1:
            problem = 'double_write'
        elif s.phase == 'confirmed' and not (s.observed and s.writes == 1 and s.file == 2):
            problem = 'false_confirmation'
        elif s.phase in ('claimed', 'wrote', 'unknown') and s.owner != s.stamp:
            problem = 'lost_fence'
        elif s.unauthorized_write:
            problem = 'unchecked_write'
        if problem:
            return len(seen), edges, problem, trace
        for label, new in successors(s, mutant):
            edges += 1
            if new not in seen:
                seen.add(new)
                q.append((new, trace+(label,)))
    return len(seen), edges, None, ()
