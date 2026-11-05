# Release gates SG00–SG14

Mandatory for the full v1 profile. The [registry](gates.json) starts every product gate at not_run. Put new evidence and dispositions in a separate, versioned implementation ledger. A subset can be labelled by exact component/capability and input revision only. A passing example, an exit code, a counterexample on a deliberately broken model or an inherited sibling receipt cannot complete a product gate.

Implement a gate producer, e.g. scripts/gate SG02 --receipt <fresh-path>, and a release-required aggregator. These commands are the implementing agent's contract; only tools/check_handover.py ships as an executed handover check. A producer refuses missing tools, stale/unbound inputs, unknown fields, missing mandatory scenarios, duplicate receipts, skipped backends, placeholder proofs and missing negative controls. A receipt records exact source tree incl uncommitted material, dirty diff digest where applicable, tool/container and dependency hashes, target/features, command, scoped case IDs, gate name, exit status, raw logs, duration, resource observations, produced digests, assumptions, blocked items and any retained prior failures. Never overwrite an attempt. Redact secrets and keep private source in its realm.

## SG00 · Provenance and local build

**Owner** Build owner. **Predecessors** none.

**Pass.** All source/tool/dependency pins resolved; external drive and floors verified; offline clean bootstrap reproduces tool identities; exact musl/Verus strategy admitted.

**Break it.** Detach/mismatch external volume, move a cache to the system disk, change tool digest, run offline with missing artifact. Preflight refuses before allocating heavy jobs.

**Keep.** Tool lock, storage attestation, source/Compose closure, fresh offline and negative logs.

## SG01 · Language freeze and grammar

**Owner** Language owner. **Predecessors** SG00.

**Pass.** Normative EBNF, lexer, semantics, example inputs, spans, ambiguity rules and /1 capability registry agree. EBNF positive/negative corpus and wholly independent parser tests; exact @name := spelling.

**Break it.** Retired assignment, invalid UTF-8, mixed EOL/heredoc delimiter, truncated input, reserved handle, huge limb, overlapping text ambiguity and parser-depth attacks.

**Keep.** Grammar and lexer identities, full golden tokens/AST/diagnostics with byte spans, rejected mutants.

## SG02 · Lean byte and surface calculus

**Owner** Proof owner. **Predecessors** SG01.

**Pass.** Clean Lean build, L01–L07 across enabled profile; independent surface denotation, lexer/resolution assumptions, sound lowering including refusal, batch normalization and exact original-source keep. Audit every public theorem and run executable oracle.

**Break it.** Move after to start; sequentially read a modified capture; clamp invalid bound; drop a missing optional group; normalize CRLF; remove overlap refusal. Each compiles as a mutant where applicable and fails intended theorem or calibrated oracle.

**Keep.** Clean Lake logs, source/tool identity, symbol and permitted axiom inventory, mutation reports, oracle artifact.

## SG03 · Lean authority and preparation

**Owner** Proof and host owners. **Predecessors** SG02.

**Pass.** L08–L09, typed namespace/generation separation, stable inputs, preparation nonmutation, guarded lifecycle semantics, no authority gain by receipt/citation. Enumerate trusted crypto/OS/origin assumptions.

**Break it.** Collide read and edit kinds, reuse alias from another run, omit prepared input field, accept stale or unsupported profile. One legitimate derivation per safety family.

**Keep.** Theorem inventory, traces of pass/refusal/nonvacuity, axiom audit, reviewed assumptions.

## SG04 · Frontend, profile and nominal APIs

**Owner** Compiler owner. **Predecessors** SG02.

**Pass.** Raw input→checked AST→resolved IR works through CLI/library. Strict UTF-8, raw bytes, groups, scopes, paths and budgets; reproducible canonical vectors. Compile-fail tests for checked types.

**Break it.** Unknown/duplicate keys, offset type swap, wrong image, overflow, trace truncation, regex engine exhaustion, aliased same text in other scope.

**Keep.** Compiled fixture logs, public-path output, AST-to-source map, per-phase max/budget counters.

## SG05 · Shipping Rust refinement

**Owner** Kernel owner. **Predecessors** SG03, SG04.

**Pass.** Verus verifies actual selected production bodies and checked conversions/unaltered outside bytes for normalized batch. Audit bridges from proof definitions to exact shipping configuration. Differential compare Lean and Rust on success/refusal including input bytes, normalized IR, complete after-image and origins.

**Break it.** Wrong coordinate order, unchecked length add, accidental Unicode normalization, sequential swap, deletion of one source linkage, zero-length boundary bug.

**Keep.** Verus logs and selected function mapping, trusted-base inventory, independent Lean↔Rust vectors, mutation results.

## SG06 · Structural quality and bounded execution

**Owner** Independent reviewer. **Predecessors** SG04, SG05.

**Pass.** Enforce engineering.md budgets, dependencies, lints, nominal types, module ownership, regex resource profile, no hidden writes. Fuzz lexer/wire/normalizer and adversarial large batches in a controlled environment; measure memory and total work at and above limits.

**Break it.** Oversize module/macro, forbidden dependency/re-export, macro panic, wrong type deserialized, quadratic repeated visits, large output via repeated keep.

**Keep.** AST/graph/lint and compile-fail receipts, memory/time ceilings, fuzz seeds/crashes, reviewed narrowly scoped exceptions.

## SG07 · Operational model and inductive proofs

**Owner** Protocol owner. **Predecessors** SG03.

**Pass.** Extended TLA+ model, TLC finite fault schedules and TLAPS inductive safety plus implementation-action bridge; the supplied seed alone cannot pass. Separate safety/liveness and fairness assumptions.

**Break it.** Blind retry Unknown; grant checked only in prepare; stale generation writes; duplicate key with changed intent; log-confirm before file observation. Retain actual counterexample trace for every named invariant.

**Keep.** TLC config/versions/configuration coverage/deadlock policy, TLAPS proofs/backends, transition-to-code map, mutants.

## SG08 · Owner service and authority

**Owner** Writfold and Splicery owners. **Predecessors** SG06, SG07.

**Pass.** Owner-published pinned schema and vectors. Real initialized stdio, verified grant/scope and dirty-worktree snapshot; replay/validate exact recipe for attributed receipts. Real plan/run/Worktrunk binding. Report actual existing owner gate prerequisites and blockers.

**Break it.** Cross-realm/cross-client handles, forged recipe-to-IR, mocked grant, read-only capture as write target, wrong repo, expired prepare/owner and unsupported capability.

**Keep.** Actual installable artifacts and owner receipts, handshake transcripts sanitized for secrets, valid and denied runs.

## SG09 · Native WSL file effects

**Owner** Adapter owner. **Predecessors** SG08.

**Pass.** Actual descriptor-contained read/compare/install, mode-safe replacement, correct temp/rename/fsync lifecycle, fresh handle/receipt and no-change/no-write result. Verify effective reservation and quiescence under the declared writer model.

**Break it.** Symlink race, hard link, special file, stale digest with unchanged excerpt, path escape, old FD, ENOSPC/short write, read-only parent, formatter drift, unsafe metadata profile.

**Keep.** Real WSL filesystem and Worktrunk evidence, before/after bytes/metadata and syscall fault evidence.

## SG10 · Crash, retry and Run Watch

**Owner** Host owner. **Predecessors** SG09.

**Pass.** Kill actual adapter/host at each durable boundary, restart and reconcile under same EffectId or block. Acks and TUI bind to exact outcome and head; selection/rendering has zero effect. Different sessions reuse only verified durable bindings.

**Break it.** Lost response, two concurrent applies with same key, reused key with changed intent, altered host generation, crash after rename/before dir sync, lagged activity projection, stale receipt.

**Keep.** Fault matrix, event/ref counts, zero second logical dispatch, retained unknown paths, independently observed content.

## SG11 · File lifecycle and multi-repository visibility

**Owner** Adapter and QA owners. **Predecessors** SG10.

**Pass.** Open real absent/existing cases for create/remove/rename with explicit path and freshness guards. Multi-file/repo recipes orchestrated as distinct effects, report exact applied prefix. Demonstrate one failed file does not turn run green.

**Break it.** Create races with another create; rename target appears, cross-device rename, stale removal, ACL/linked file, denied repo B after A succeeded, unknown first effect on replay.

**Keep.** Native outcomes, partial/failure receipts and TUI snapshots, authority and retention checks.

## SG12 · Whole-language conformance and agent usability

**Owner** Evaluation owner. **Predecessors** SG06, SG10, SG11.

**Pass.** Freeze complete task corpus and labels before trial. Compare agent's current editor/patch tooling against Splicery on matched held-out tasks. Every normative syntax form exercised from raw recipe to actual observed file or documented refusal. Include all 5 anchor tasks, multiple file formats and a genuine binary fixture.

**Break it.** Seed duplicate regions, stale formatter edit, Unicode/CRLF/no final LF, tempting regeneration of comments, misordered marker pair, hidden unrelated file. Hidden checks must detect a corrupted candidate.

**Keep.** Exact task/seed/model/harness/token meter, successful completions, wrong-target rate, retries, bytes resent, input/output tokens, time and correction logs.

## SG13 · Artifact, runtime and offline reproduction

**Owner** Release engineer. **Predecessors** SG06, SG08, SG11.

**Pass.** Static Linux x86_64 musl client, exact pinned compatible adapter, SBOM/licenses, reproducible source inputs, deterministic outputs or explained differences, cold offline rebuild, clean runtime smoke without build tools.

**Break it.** Invoke with missing/degraded host, incompatible protocol, stale cached artifact, compromised tool lock, missing external storage or proof tools on runtime PATH. No fallback to shell writer.

**Keep.** Binary and image hashes, ELF/runtime inspection, install smoke, SBOM, cold build and disclosure of any reproducibility variance.

## SG14 · Independent release decision

**Owner** Release owner. **Predecessors** SG00, SG01, SG02, SG03, SG04, SG05, SG06, SG07, SG08, SG09, SG10, SG11, SG12, SG13.

**Pass.** Every mandatory gate at matching /1 scope passed against the release closure. Independent reviewer traces a recipe to actual retained bytes, theorem and receipt. All failing/blocked paths and counterexamples have a disposition.

**Break it.** Remove/alter a predecessor receipt, change crate feature/fixture/profile, replace test binary with mock, mark skipped gate pass. Aggregate must refuse.

**Keep.** Signed/admitted release decision as applicable, immutable evidence index and compatibility/limits note.

## Agent benchmark admission

Use at least 20 held-out tasks spanning the five anchor families in examples/README.md and mixed repo/text formats; repeat paired runs under a fixed model/harness, budget, permissions and equivalent context. Use clean worktrees, randomized tool order, disjoint development data and an outcome grader blinded to method when feasible. Pre-register how failures and timeouts enter the denominator. A wrong-target success or silent collateral byte change blocks write enablement regardless of speed. Require every mandatory semantic task family to succeed with exact intended bytes or an appropriate explicit refusal, and a usable recovery route from seeded ambiguity/staleness. Establish the production ceiling and acceptable completion-rate regression before running SG12. Publish per-task results and paired distributions; claim token or time savings only when the defined measure improves without trading away correctness/completion. Describe any workflow that still requires an external transformation and why.
