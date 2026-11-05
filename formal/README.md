# Lean work order

Target Lean 4.21.0 with core/Std, matching the observed sibling proof tool. Run inside the admitted tool image:

    cd formal
    lake build
    lake env lean Splicery.lean

Once SG00 admits the actual external storage and pinned image, the included producer automates the clean build, a per-theorem axiom audit and an after→before lowering mutant. From the package root inside the offline container run:

    python3 -B tools/check_lean_seed.py --evidence "$EVIDENCE/splicery-lean-seed-001"

Use a new evidence directory outside the checkout; retain each failed attempt. The script checks the seed only. The SG02/SG03 assurance inventory remains larger.

The supplied sources contain proof terms/tactics for exact splitting, checked cuts, unchanged exterior bytes, keep identity, the four resolved command forms, gap preservation for normalized batches, and a two-span swap. These source files are starter work. Capture a clean compiler and printed-axioms receipt before marking a theorem checked. The tool was unavailable to the authoring host. Resolve any elaboration failures here first. The rehearsal is neither the full calculus nor parser/host qualification.

| ID | Obligation & quantified subject | Next acceptance evidence |
|---|---|---|
| L01 | Successful validated cut exactly decomposes original bytes with target start and width; every valid decomposition succeeds; negative/reversed/overflowed external bounds refuse | Core.lean + checked lo/hi→Nat-width bridge; add rejection and totality lemmas |
| L02 | Successful splice equals exact prefix ++ rendered replacement ++ suffix; keep identity; unchanged exterior and exact new length | Core.lean + length and refusal extension |
| L03 | After resolution and profile checks, denotation of replace/delete/before/after equals primitive execution | Lowering.lean + nontrivial empty/EOF and invalid-input reference tests |
| L04 | All reads, including captured regex groups, refer to the authorized original image and units; keep cannot manufacture a target or use a stale binding | Add nominal snapshot/capture evidence, resolution/refusal and taint/realm relations. Current Piece inputs are already resolved |
| L05 | Canonicalization/refusal of numeric batches: gaps reconstruct exactly the input, disjoint valid permutations give same output, all listed overlaps fail, no partial result | Batch.lean supplies a normalized relation; add positional validation, sorting, reconstruction and swap/move lowering |
| L06 | Lexical/scoping judgments, frozen membership, expect/one cardinality over complete scope; truncation/budget failure never yields a candidate | Define independent surface semantics for raw/text/line/range/between/within/iteration/groups and refusing derivations |
| L07 | Byte preservation for quoted strings, heredocs, UTF-8 boundaries, CRLF, lines-like, indent and raw hex; entire accepted surface lowering refines the denotation | Extend L03 to all enabled productions. Keep decoder, regex assumptions, origin mapping, resource accounting explicit |
| L08 | Prepared identity binds exact inputs/profile/IR/candidate; replacing a value, scope or predecessor cannot authorize a different write | Formalize domain-separated identity and injectivity/collision assumptions; bind to host wire vector tests |
| L09 | Pure preparation has no filesystem/ledger effect; bounded deterministic success or typed refusal; file operations have separate preconditions | Prove on the public result type and specify separate adapter assumptions. No abstract proof of a physical disk |

Gate policy: freeze independent surface semantics before lowering; different definitions for specification and implementation under test. Under SG02 prove the exact mandatory profile (L01–L07), before shipping semantic Rust work starts. SG03 completes L08–L09 before effect implementation. Narrow offline experiments stay labelled. A newly introduced constructor requires local lemmas, nonvacuous positive/negative examples and a target mutant before promotion.

Axiom audit: enumerate *all* registered public theorem declarations by name, verify source dependency closure, reject sorryAx, added axioms, admit, skipped verification, unreviewed unsafe/opaque escape hatches and stale oleans. Record any standard Lean logical axioms individually with rationale. From a clean checkout, delete owned build outputs, compile actual changed sources; ensure a semantic mutant fails at the named property and does not fail earlier for unrelated reasons. Build an executable Lean oracle over a bounded wire format and compare success AND refusal outputs to the shipping path. Do not count a plausible proof sketch or differential tests as a refinement theorem.

Trust boundary: finite immutable byte sequences, exact input provenance supplied by host, bounded and complete matcher evidence, named encoding, current active grant and a cooperating exclusive worktree. Prove which preconditions each layer discharges. Never infer parse correctness of edited source or semantic equivalence of the user's program from a checked splice.
