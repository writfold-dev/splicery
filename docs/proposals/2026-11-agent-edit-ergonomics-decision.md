# Agent edit ergonomics · owner decision, 9 November 2026

Decision by the Splicery owner (Tarek) on the proposal
[docs/proposals/2026-11-agent-edit-ergonomics.md](https://github.com/writfold-dev/splicery/pull/1)
(PR #1, branch `proposal/agent-edit-ergonomics`). The proposal itself is merged
by the owner through that PR; this record states what was accepted, what was
deferred and how each accepted item was implemented and qualified.

## Accepted

| Item | Decision | Implementation |
|---|---|---|
| P1 actionable refusal diagnostics | Accepted. Offline preview may show bounded context only from the file the caller passed on the command line. The managed-flow rule 5 (diagnostics never expose ungranted text) is untouched. Diagnostic kind names are unchanged. | `splicery-compile` attaches a bounded `SelectorDetail` (match count, at most four original-image sites, or a near miss: whitespace-trimmed lines, LF/CRLF exchanged, matches outside `within`, misordered anchors) to a refusal; near-miss searches run only on the refusal path and charge the work budget. `splicery-cli` prints `refused: <Kind> at recipe [s,e) in <path>`, the sites as line numbers and byte ranges, and a hint. Line text (at most 120 bytes per site) appears only with `--file`. |
| P2 `--diff` | Accepted. | `splicery-wire::unified_diff` derives a unified diff from the exact splices (trimmed to changed lines, three lines of context, no-newline markers, 16 KiB bound). `--report` and `--diff` are exclusive; the candidate stays the only applied artifact. |
| P3 agent quickstart card | Accepted; its recipes must be exercised by the gate that exercises the examples. | [docs/agent-quickstart.md](../agent-quickstart.md) with [quickstart-vectors.json](../../qualification/quickstart-vectors.json). The SG04 language gate (`tools/check_language.py`) runs every card recipe through the real binary from stdin, with and without `--diff`, and refuses a card without a vector or a vector without a card. |
| P4 stdin recipe | Accepted as `-`. | `preview … -` reads at most 256 KiB + 1 bytes from stdin; an oversized recipe refuses with the usual `Limit` diagnostic. |
| P5 `whole @line` | Accepted: `[start, fullEnd)`; on an unterminated final line `whole` selects through end of file. | Spec §3, EBNF `"whole" reference`, `Surface.lean` (`select` branch, `select_whole_sound`, `whole_requires_line`, examples), `splicery-compile::anchors::resolve_whole`, rehearsal, grammar cases, example 12 and its Lean differential vector. Non-line captures refuse with `InvalidCapture`. |
| P6a `from @a to @b` | Accepted; reversed or overlapping anchors refuse. | Spec §4, EBNF `"from" reference "to" reference`, `Surface.lean` (`select_fromTo_sound`, `fromTo_refuses_misordered`, examples, mutant `mutant-accept-overlapping-from-to`), `splicery-compile::anchors::resolve_from_to`, rehearsal, grammar cases, example 13 and its Lean differential vector. Coincident, reversed and overlapping anchors refuse with `MissingSelector` (as `between` does) and name both anchors. |
| Open question 6 | Accepted: a gate checks the admitted-syntax paragraph against the grammar cases. | `tools/language_cards.py::admitted_syntax`, run by the SG04 language gate: the paragraph in [offline-preview.md](../offline-preview.md#admitted-syntax-and-limits) must name every EBNF keyword and nothing outside the grammar, and every keyword must be exercised by a grammar accept case, an example or a quickstart recipe. |

Both Surface items lower to a plain `[lo,hi)` range: `selectedCut_plain` shows
every selection is one checked cut with prefix length `lo` and selection length
`hi - lo`, and the soundness lemmas bound both new spans by the image. The
kernel (`crates/splicery-splice`), the resolved IR and
`formal/Splicery/{Core,Batch,Lowering}.lean` are unchanged.

`whole`, `from` and `to` are reserved words: `@whole` and `@from` are no longer
valid handle names (`to` already was).

## Deferred

- **P6b** `balanced "(" ")" from @open`: a byte-depth heuristic. Not
  implemented; `from @a to @b` is the exact form.
- **P7** standalone `apply`: deferred. Writes stay with Writfold, consistent
  with the family's single authority boundary. The proposal's fallback,
  `preview --out PATH`, did not exist and is offered: it writes the selected view
  to a new file outside `--root`, refuses an existing file, and never writes
  inside the worktree.
- **P8** several `edit` blocks per document: deferred; §6 and §8 are unchanged.

## SG05 amendment

Accepted. SG05's Verus criterion is amended to admit the Aeneas proofs gate as
the shipping-Rust refinement evidence: the drift check, the axiom audit and
mutant calibration over `crates/splicery-splice`. The amendment is recorded as
`SG05-A1` in [gate-amendments.json](../../qualification/gate-amendments.json),
which quotes the original criterion verbatim, states the amended criterion,
maps the SG05 negative controls to mutants and lists what remains unproved
(the `splicery-kernel` facade and its UTF-8 check are tested, not proved).
`qualification/gates.json` and `qualification/gates.md` are the immutable
handover registry and are not edited. The remaining SG05 criterion (Lean/Rust
differential on success and refusal, including input bytes, normalized IR,
after-image and origins) is unchanged and still required.

The receipt is produced by `tools/check_sg05.py`, which runs the proofs gate,
checks its drift/audit/calibration markers, binds the log and image identity,
and requires fresh primitive and surface differential receipts for the same
source identity. Scenario C21 is re-evaluated from that fresh evidence; the
retired Verus receipt is not reused. Status and receipts are in
[gate-status.json](../../qualification/gate-status.json) and
[scenario-status.json](../../qualification/scenario-status.json).
