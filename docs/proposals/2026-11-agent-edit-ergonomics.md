# Agent edit ergonomics · proposal, November 2026

Status: proposal for the Splicery owner. Nothing here changes the kernel, the
resolved IR or the /1 language contract; the ranked table says which items
would, and what they would need.

## Summary

The 8 November Writfold edit pilot
([bench/2026-11-08-edit-pilot](https://github.com/writfold-dev/writfold/tree/main/bench/2026-11-08-edit-pilot))
ran six small single-file edit tasks with Sonnet agents. Both arms were exact
on all six; the Splicery arm used about 12.5% more tokens, 25% more tool
calls and 53% more wall time, mostly one extra write-recipe → preview →
overwrite cycle. One agent selected a 662-byte expression with `range [103,765)`,
and a `after <line> insert lines like` recipe joined two lines.

Reading of the evidence:

- On short, unique, single-site edits in small files Splicery cannot beat an
  editor tool that already refuses non-unique matches. The overhead is the
  workflow, not the language, and the frontend/CLI can remove most of it.
- Splicery's plausible wins are long preserved text by handle, counted
  near-duplicate sites, swap/move from one snapshot, byte fidelity (CRLF, no
  final LF, binary) and staleness refusal. None of those was in the pilot.
- Two of the three cheapest improvements (actionable refusal diagnostics and a
  diff/compact output) are already owed by the spec and the gate catalogue and
  need no proof work. The one semantic gap found (body-only line selection)
  needs a small language-contract change with Lean and rehearsal updates, but
  no kernel or IR change.

Recommended order: P1 diagnostics, P2 `--diff`, P3 quickstart card, P4 inline
recipe, then P5 `whole` line selector and P6 anchored spans after the owner
agrees the spec text, then P7 standalone apply once the authority boundary
below is accepted.

## Evidence

Measured with `splicery-cli` built from `839e45e` on the pilot fixture and on a
14,681-byte Rust file:

| Observation | Measurement |
|---|---|
| Preview output for a one-word change in a 14,681-byte file | candidate 14,679 bytes; `--report` 241 bytes (hex replacement, not readable); `diff -u` 734 bytes |
| Ambiguous selector refusal (pilot T6) | stderr is `CompileDiagnostic { kind: AmbiguousSelector, origin: RecipeSpan { start: 22, end: 56 } }`; no match count, no locations, no hint |
| Missing selector refusal | `CompileDiagnostic { kind: MissingSelector, origin: ... }`; no near-miss information |
| `after @line insert lines like @line [...]` | inserts before the terminator and joins two lines (spec §3 body selection + §7 insert at hi); confirmed in `Surface.lean` `selectedCut`/`applySurface` and `tools/recipe_rehearsal.py` |
| `delete @line` | leaves an empty line |
| `replace @line with lines like @line [...]` on a terminated line | leaves an empty line; card 07 works only because its target line is unterminated |
| Pilot T2 agent recipe | `@expr := range [103,765)`; correct, fragile under any earlier byte change |
| Pilot T5 agent recipe | retyped both values instead of `keep`; correct here, wrong when a value recurs |
| Pilot arm B per trial | 5 tool calls vs 4 (recipe write, preview, overwrite); extra 3–6k tokens, roughly the spec reading plus the recipe cycle |

Commit `839e45e` already adds a stderr `note:` for the two line-body constructs
(`crates/splicery-compile/src/advisory.rs`) and pins the spec behaviour in
tests. It does not change meaning.

## Where Splicery can beat ordinary edit tools

Each claim names the mechanism, the fair control, and a task that could
falsify it. "Ordinary tools" means the agent host's Read/Edit/Write with a
uniqueness check on the old string, not `sed`.

| Claim | Mechanism | Fair control and task |
|---|---|---|
| C1 Long preserved text costs a handle, not a retype | `keep(@x)` copies original bytes; an Edit tool resends the text twice (old and new string) | Wrap, move or re-indent 0.5–4 KB expressions with a near-duplicate elsewhere. Count bytes resent and output tokens. Only fair once the selection itself needs no retype (P6); today the agent either retypes in `one text` or uses `range` |
| C2 Many near-duplicate sites with a counted inventory | `each … expect n` fails closed on 4 or 6 hits; Edit needs n calls or `replace_all`, which also hits decoys | 20–50 sites, commented decoys, one decoy differing by whitespace, mixed CRLF/LF. Measure wrong-target rate, tool calls, retries |
| C3 Swap and move read one snapshot; the batch is atomic | Both `keep`s read the original; overlap/coincidence refuses the whole batch | Swap two values where the first replacement creates a second occurrence of the other; move a block across a file; a five-edit batch whose fourth edit is ambiguous must leave the file untouched |
| C4 Staleness is refused, not silently retargeted | Captures bind to the image digest; a changed file yields `StaleTarget` in the host | Run a formatter between read and edit. Needs the Writfold host; offline, test only the digest-bound refusal in `--capture` fragments |
| C5 Byte fidelity | Kernel copies bytes; no EOL or trailing-newline normalization | CRLF files, no final LF, BOM, a binary fixture via `as bytes`. Measure byte-exact success. Expect the biggest observed gap here |
| C6 Mechanical refusal on hidden ambiguity | `one text`/`one line` scan the whole image; an agent that read 200 lines does not | 3,000-line file, duplicate far from the visible region. Fair only against hosts whose Edit does not check uniqueness; report parity otherwise |

Include control tasks where Splicery should lose (one short unique edit in a
small file) so the campaign measures the overhead too, per the
"Agent benchmark admission" rules in `qualification/gates.md`.

## Ranked proposals

Kernel/IR column: "none" means `crates/splicery-kernel`, `spec/resolved-ir.md`
and `formal/Splicery/{Core,Batch,Lowering}.lean` are untouched. "Surface" means
`spec/language.md`, `spec/splicery.ebnf`, `formal/Splicery/Surface.lean`,
`tools/recipe_rehearsal.py`, grammar cases and vectors change, re-running SG01,
SG02 (surface part) and SG04 at the new scope.

| # | Proposal | Agent effect | Cost | Risk | Kernel/IR | Where |
|---|---|---|---|---|---|---|
| P1 | Actionable refusal diagnostics | Fewer retries; ambiguity fixed in one step | S–M | Budget/grant leakage of nearby context must follow §9 and protocol rule 5 | none | `compile::CompileDiagnostic` gains a bounded typed `detail`; `cli::preview` renders text; `--report` gets a refusal object |
| P2 | `--diff` output, readable report | Agents stop re-reading whole files; ~20× fewer output bytes on a 14 KB file | S | A diff is derived; keep the candidate as the only thing that is applied, one mode per invocation, stdout stays machine-clean | none | `cli` flag; `wire::report` adds `replacement_text` when valid UTF-8 and original context |
| P3 | Agent quickstart card | Less reading per task than `spec/language.md`; the six idioms and the line-body traps | S | Drift from the spec; make the gate that exercises examples also check the card's recipes | none | `docs/agent-quickstart.md`, recipes exercised like `examples/` |
| P4 | Inline or stdin recipe | Removes the recipe-file tool call (pilot: 5 → 4) | S | Shell quoting; prefer stdin (`-`) over `-e` | none | `cli::parse_preview`; fragments with `--file` already exist |
| P5 | `whole @line` selector, [start, fullEnd) | Removes the three line-body traps; `after whole @a insert lines like @a […]`, `delete whole @x` | M | Spec text and proof update; alternative (change `after <line>` to fullEnd implicitly) rejected because it breaks suffixing a line and silently changes proven meaning | Surface | EBNF `selector`, §3/§4, `Surface.lean` `select`/`selectOneLine`, `compile::selector`, rehearsal, vectors |
| P6a | `from @a to @b` span (inclusive, text captures) | Long spans from two short anchors; no `range` | M | Must refuse reversed/overlapping anchors like `between` does | Surface | as P5 |
| P6b | `balanced "(" ")" from @open` | Select to the matching delimiter by byte depth count | M–L | Heuristic on strings/comments; document, require a unique result and offer `to @close` as the exact form | Surface | as P5 plus a counting function and its Lean definition |
| P7 | Standalone `apply` | Removes `> out && mv`; keeps no-clobber, temp+rename, digest check | M | Authority boundary (below); must never look like a Writfold receipt | none | new `cli` subcommand, own doc section; not in SG07–SG10 scope |
| P8 | Several `edit` blocks per document | One preview for a cross-file change | M–L | §6 forbids cross-file `keep`; §8 forbids cross-file transactions; apply stays per file | none per file; language text | parser, `cli`; owner decision on §6/§8 wording |

### P1 detail

Today the preview prints the `Debug` form of `CompileDiagnostic`. Spec §9 owes
"original recipe span and bounded authorized nearby context"; SG04 keeps an
AST-to-source map. Proposed text form, one line per fact so agents can parse it:

```text
refused: ambiguous selector at recipe [22,56) in src/log.rs
  one line "level = \"debug\"" matched 2 lines: 3 (bytes 36..51), 10 (bytes 118..133)
  hint: add `within` a window between two unique lines, or use `one text` with distinguishing context
```

For `MissingSelector`, report the nearest bounded near miss when cheap: same
needle ignoring CR/LF or leading whitespace, or a count when the needle occurs
only outside `within`. Cap at N locations and M context bytes, charge the budget,
keep diagnostic kind names unchanged so `tools/check_surface_differential.py`'s
substring check still holds.

### P7 detail: authority boundary

Inside Writfold the host owns every write: it resolves the run, grant and
Worktrunk binding, prepares on immutable bytes, checks the grant at the effect
boundary and records the effect (`protocol/worktree.md`, "Native materialization
contract"). A standalone `apply` must not blur that:

- It is a different subcommand with a different name in the docs: "unmanaged
  apply in a plain directory". No RunRef, request key, receipt, retry or
  reconciliation vocabulary; exit status and a digest line only.
- It adds no authority the agent lacks. Outside Writfold the agent already
  writes with its shell (`> out && mv` in the pilot prompt); `apply` replaces a
  fragile two-step with same-directory temp + fsync + rename, refuses symlinks
  and non-regular files, and compares the file's digest to the image it prepared
  against immediately before rename.
- It lives in `splicery-cli` only, is excluded from SG07–SG10 evidence, and the
  README keeps "applying an edit to a real worktree belongs to Writfold" for the
  managed flow. SG13 documents it as a component of the offline client.
- Decline it if the owner wants a single story for writes; the fallback is
  `preview --out PATH` outside the root, which still leaves the `mv` to the agent.

## Kernel and IR impact

| Touches | Items |
|---|---|
| Nothing but CLI/wire/compile frontend | P1, P2, P3, P4, P7 |
| Language text, EBNF, `Surface.lean`, rehearsal, vectors; no kernel or IR | P5, P6a, P6b (plus a new counting definition) |
| Language text on §6/§8 and the parser; no kernel or IR | P8 |
| Kernel, `resolved-ir.md`, `Core/Batch/Lowering.lean` | none |

Every Surface item keeps the lowering target a plain `[lo,hi)` range, so
`Lowering.lean` `lower_correct` and the kernel proofs are unaffected; the new
obligations are selection soundness lemmas in `Surface.lean` and the
differential against the rehearsal.

## Benchmark plan

Follow the Writfold plan's debug-pilot size and the admission rules in
`qualification/gates.md`: 12 tasks, two arms, two repeats, one pinned model,
pre-registered grader, randomized A/B order, failures in both denominators.

- Tasks: two per claim C1–C6, drawn from fixtures of 5–50 KB so re-reading
  costs are visible, plus two control tasks where ordinary tools should win.
  Each task has hidden expected bytes, a positive control recipe and a seeded
  wrong-target opportunity.
- Arms: A ordinary tools; B0 current CLI; B1 CLI with P1–P4 (frontend only,
  can run before any proof work); B2 adds P5/P6 once their calculus lands.
  B1 vs B0 isolates the workflow cost; B2 vs B1 isolates the language changes.
- Metrics: exact-bytes success, wrong-target and collateral-change counts,
  bytes resent, tool calls, provider-reported tokens by category, wall time,
  retries after a refusal. Report paired differences and raw counts first.
- Decision rule: claim a token or time win only where exact-bytes success is
  not worse; any collateral byte change blocks the headline.

## Open questions for the owner

1. P5: is `whole @line` the preferred spelling, and should an unterminated final
   line make `whole` refuse or select the body?
2. P6b: is byte-level delimiter counting acceptable as a documented heuristic
   with mandatory uniqueness, or should only the exact `from … to` form exist?
3. P7: accept an unmanaged standalone `apply` in the client, or keep writes
   exclusively in Writfold and offer `--out` only?
4. P1: what bounded context may a preview diagnostic reveal offline, given
   protocol rule 5 (diagnostics never expose ungranted text) also binds the
   managed flow?
5. P8: should §6/§8 be reworded to admit a multi-block document whose blocks
   remain independent single-file effects?
6. `docs/offline-preview.md` previously claimed `one line`, `between`, iteration,
   `lines like` and `indent` were unsupported; `839e45e` corrected it. Should
   a gate check the admitted-syntax paragraph against the grammar cases?

## Appendix: pilot tasks under proposed syntax

```splicery
# T2 with P6a (today: range [103,765))
edit "src/report.rs" {
  @open := one text "format!(\n"
  @tail := one text "    );\n    body"
  @close := one text ")" within @tail
  @expr := from @open to @close
  replace @expr with ["sanitize(", keep(@expr), ")"]
}

# T4 with P5 (today: before the following line)
edit "src/lib.rs" {
  @a := one line "// imports begin"
  @b := one line "// imports end"
  @window := between @a and @b
  @anchor := one line "use crate::config::Config;" within @window
  after whole @anchor insert lines like @anchor ["use crate::policy::Policy;"]
}

# T5 as the reference already writes it; the agent retyped values instead
edit "config/limits.toml" {
  @soft := one text "100"
  @hard := one text "80"
  replace @soft with keep(@hard)
  replace @hard with keep(@soft)
}
```

```sh
# P4 + P2: one tool call, no recipe file, no whole-file re-read
printf 'replace one text "80" with "100"' | splicery preview --root . --file config/limits.toml --diff -
```
