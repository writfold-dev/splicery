# First work session

Objective: deliver a harness-independent way to edit exact bytes in one run-owned Worktrunk worktree. The first useful demo wraps an already captured multiline call using @call := … and keep(@call), preserves every other byte and reports an exact diff. A formatter change between prepare and apply produces StaleTarget. A lost response uses the original request key and retrieves/reconciles the existing effect.

1. Read this page, the EBNF, language rules, gate catalogue, then the owner protocol. Preserve the input archive and manifest. Inspect current target checkout instructions and Writfold's exact owner-published contracts. Record any drift from [the pins](guide/sources.md) before selecting a baseline. Import as a separate Splicery workstream in the operator-designated repository/worktree. Proposed home: writfold-dev/splicery. Repo creation, publishing or edits to either sibling require their own authorization. No guessed live endpoints.
2. In the unpacked root run **python3 -B tools/check_handover.py**. It consumes the actual grammar, examples, vectors and gate DAG. Record an initial commit/branch or immutable source tree, and a separate private evidence destination. Keep authoring evidence distinct from product status.
3. First PR: preserve contracts and fixture inputs; establish a pinned Compose/BuildKit offline qualification environment (SG00). Bring up a clean Lean project from formal/, build the included proof bodies, audit axiom output and fix elaboration in that commit. Add a failing-before negative mutant and an executable Lean oracle. Complete SG01+SG02 at the frozen scope, or report exactly what remains open. A skeleton Cargo workspace may be created; shipping semantic implementation waits for the proof gates.
4. Second increment: finish identity/authority assumptions SG03; implement the frontend, normalized IR and Rust spine for one regular UTF-8 file. The pure kernel is proved over its Aeneas translation ([proofs](proofs/README.md)); this replaced the original Verus plan for those bodies. Start with exact text and an interactive captured span, replace, before, after, delete and keep. Run offline differential positive/refusal vectors. An offline preview can be demonstrated at this stage with an explicit component label.
5. Expand within the SAME syntax/profile to the bulk/byte/file-set vocabulary as staged below. Do not advertise /1-complete until every required /1 gate passes. The first write-enabled demonstration needs the real Writfold host, claimed effect, native materializer and applicable protocol/refinement/fault gates. Avoid enabling a subset through a fake production transport.

| Stage | Deliverable | Exit condition |
|---|---|---|
| A · establish | provenance, immutable spec, external drive, reproducible verifier image | SG00–01 |
| B · prove | reference calculus, full syntax meaning, typed binding and effect premises | SG02–03; no production semantic Rust before corresponding calculus |
| C · implement locally | compiler, checked nominal IR, pure kernel, bounded reports | SG04–06; pack an explicitly scoped read-only preview if desired |
| D · integrate first write | owner-approved stdio contract, Worktrunk-owned file, stale/retry handling | SG07–10 under the file-update scope, real host only |
| E · complete /1 | file lifecycle, multi-repo reporting, comparative evaluation, packaging | SG11–14 at full scope |

Start/resume order within an increment: compare exact owner refs, inspect current source and failed receipts, select earliest unclosed predecessor, execute smallest vertical slice, retain logs, update current status, leave next action. Time-boxed exploration or an externally blocked integration may continue in independent gates. Never substitute a mock for the owed host receipt.

**Done** means a clean source-bound release aggregate, static Linux x86_64 client, versioned host adapter contract, reproducible proofs and a fresh WSL execution on the admitted storage profile. A grammar rehearsal, model check, proof run or green unit suite is a component claim with its own scope. Keep the immutable handover and mutable implementation status separate.
