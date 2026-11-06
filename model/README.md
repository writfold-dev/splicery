# Operational protocol qualification

## Seed and expanded SG07 scope

`Edit.tla` and `EditSafety.tla` are the historical one-effect seed, not SG07 evidence. The seed `Init` now states `grant = TRUE` explicitly; this fixes both TLC 2.19's bare-variable Init parsing problem and the ill-typed initiation obligation. The original failed sources are preserved read-only under `seed-original/`. A seed pass is never treated as an expanded-model result.

`SG07.tla`, `SG07.cfg`, and `SG07Safety.tla` are the expanded bounded safety model. It has prepared IDs `p0,p1`, request keys `k0,k1`, clients `c0,c1`, and files `f0,f1`. Both prepared edits address both files. The transition relation covers independent captures, same-key/different-intent conflict, cancellation, revocation, alias epoch rotation, predecessor drift and ABA, durable claim before IO, rename before directory durability, post-rename/pre-fsync loss, interrupted stdio, historical lookup after revoke, competing observers, and per-file partial outcomes. Capacity one makes claim exhaustion observable; unused prepared material can be evicted; an owner can be blocked and unblocked without releasing its reservation. Captures are one-shot events and aliases rotate once per client. Response-only replay, conflict, historical lookup, eviction refusal, exhaustion, and pre-claim interruption are explicit stuttering actions: their enabled guards denote the typed external response without adding redundant reply-history state.

A visible `step` cursor defines the finite fault-schedule domain rather than relying on a TLC depth constraint. Steps 0–2 cover capture/eviction and both keyed dispatches; steps 2–3 cover stale authority, blocked owner, claim, first write/refusal, and timeout/interruption; steps 4–5 cover first-file fsync or power loss, observation, rival/foreign reconciliation, and confirmation; steps 6–11 cover the second file, partial/full settlement, and quiescence. Steps 12–16 cover blind-dispatch rejection, two no-effect observations, settlement, and release after timeout or active interruption. This retains each required fault and mutant schedule while excluding only redundant interleavings and repeated history. Digests are binary because prepared IDs, request keys, clients, and intent IDs already discriminate identity; writer/effect tokens remain distinct.

`StrengthenedInv` combines `TypeOK`, bounded write counters, resource/unique-claim coherence, claim retention and journaling, phase/predecessor and metadata coherence, sticky authority/preimage evidence, confirmation provenance, and `NoBlindRedispatch`. `Confirm` requires request key, effect token, and after-digest evidence. Matching bytes alone are insufficient. Unknown may remain forever, and deadlock checking is intentionally disabled for terminal and unresolved-unknown states.

## Proofs and fault schedules

`SG07Safety.tla` uses the SMT backend and contains no omitted proof steps. Its named theorems are:

- `InitInductive`: `Init => StrengthenedInv`.
- `NextInductive`: initiation is preserved by every one of the 30 parameterized actions and by stuttering, in separate proof steps. Each non-stuttering action is decomposed into the individual `StrengthenedInv` components so no monolithic backend search hides a failed obligation. Types, guards, update expressions, and invariant components use direct scalar cells over the fixed `k0`/`k1` and `f0`/`f1` schedule domain, avoiding unsupported function-view and primed-function-update reasoning without changing reachable semantics.
- `SafetyFromInv` and `SG07Safety`: the strengthened invariant implies the safety conjunction on every `Spec` behavior.
- `ReplayObservational` and `LookupObservational`: acknowledged same-key repeats and historical lookups leave all effect state unchanged.
- `RevokedStaleNoWrite`: `Write` is impossible with stale predecessor or revoked/stale/cancelled/blocked authority.

The producer creates isolated source copies for the base and each mutant. The base must complete TLC successfully and retain its state counts and action-labelled DOT state graph. Each mutant must parse, run, and produce a retained multi-state counterexample to its named invariant:

| Mutant | Required discriminating schedule | Intended invariant |
|---|---|---|
| `remove-live-at-write` | claim, revoke, then write; sticky authority/preimage are recorded at write | `AuthorizedWrite` |
| `dispatch-unknown` | claim, timeout, then dispatch the unknown key | `NoBlindRedispatch` |
| `release-active-claim` | release after per-file effects but before request settlement | `ActiveClaimRetention` |
| `confirm-timeout` | turn the pre-IO timeout into global confirmation | `ConfirmationProvenance` |
| `foreign-observation` | reconcile with another key's observation | `ConfirmationProvenance` |
| `aba-rival-content-only` | rival replaces the writer token while bytes cycle to the same after-image, then content-only confirmation | `ConfirmationProvenance` |

Any missing command, full TLAPS completion, proof, state graph, counterexample, or source/tool identity fails the producer closed. TLAPS runs with `--strict` and without `--summary`: success requires exit zero, newly written fingerprints in the fresh proof tree, and no failed-obligation/backend diagnostics in the complete output. Strict mode also rejects missing proof steps rather than silently generating no obligation. Raw stdout, stderr, exit status, elapsed time, source and tool hashes, generated mutant source/configuration, TLC schedules, and TLAPS obligations are retained. `model-receipt.json` binds those results to the complete model/producer source manifest; release evidence must compare that manifest with the release closure rather than infer equivalence from another proof job.

## Implementation-action bridge

This is the refinement contract for the planned Writfold/Worktrunk adapter, not a claim that unimplemented host code has been proved. `Init` abstracts an empty durable idempotency/claim ledger, admitted prepared objects, valid grants, and no target writes or completed capture events. Every host phase must implement the listed model action, or leave all model variables unchanged (stutter) while doing validation, diagnostics, parsing, preview, or pure preparation.

| Actual planned host phase | Model refinement |
|---|---|
| authenticated initialized stdio intake; schema/report/repository/grant validation | stutter until a committed `Dispatch`; refusal before ledger admission stutters |
| lazy independent file inspection and immutable capture | `Capture(f0)` and `Capture(f1)` independently |
| pure recipe replay, immutable alias expansion, prepare materialization | stutter; the two admitted prepared IDs are initialized by `Init` |
| stable-key committed RunRef/intake and exact predecessor binding | `Dispatch` |
| identical acknowledged retry, changed intent, post-revoke lookup | `Replay`, `Conflict`, `HistoricalLookup`; all are read-only with respect to effects |
| prepare-cache pressure or claim-capacity refusal | `Evict`, `RejectEvicted`, `Exhaust` |
| cancellation, grant revocation, owner/alias generation rotation, owner blockage | `Cancel`, `Revoke`, `RotateAlias`, `BlockOwner`, `UnblockOwner` |
| stale no-effect refusal before or after a claim | `RefuseReady` or per-file `Refuse` |
| exact resource reservation followed by durable dispatch-claim commit | one atomic `Claim`; no target IO occurs in this action |
| exclusive same-directory temp, data/metadata fsync, checked predecessor and atomic rename | `Write`; sticky authority and preimage are captured at the effect boundary |
| required file/directory synchronization after rename | `Fsync` |
| crash after rename but before required directory fsync | `PowerLoss`, retaining the claim and applied bytes |
| pre-IO timeout, interrupted pre-claim stdio, interrupted active stdio | `Timeout`, `InterruptBeforeClaim`, `InterruptActive` |
| exact current read and effect-token observation; another key's competing observation | `Observe`, `CompetingObserve` |
| discriminating reconciliation and any still-required directory fsync before receipt | `Confirm` |
| discriminating proof that an uncertain pre-IO effect did not occur | `ResolveNoEffect` |
| aggregate independent per-file receipts without global rollback | `Settle`, producing `confirmed`, `refused`, or `partial` |
| old-process quiescence and reservation release | `Quiesce` |
| external predecessor write or same-content rival outside reservation | `Drift`, `RivalWrite` |

Temp creation, exclusive locking, no-clobber replacement, owned-directory resolution, metadata checks, and durable ledger IO are separate implementation obligations within the mapped atomic boundaries. They must not be “implemented” by an unrelated model transition. Unsupported ACL/xattr/hardlink/symlink/cross-device cases must refuse before effects; those deployment checks are outside this abstraction and therefore stutter only when no model-visible state changes.

## Assumptions and exclusions

1. **Writers:** the worktree is private or every writer cooperates with the owner reservation for the entire compare/install interval. A local generation, ordinary lock, or rename cannot fence an unconditional external writer. `Drift` and `RivalWrite` are disabled while a reservation is live; the ABA mutant deliberately violates this boundary and is rejected.
2. **Directory handles and OS:** resolution starts at owner-held directory descriptors, rejects symlink/magic-link escape and nonregular/multiply linked targets, and uses native Linux same-filesystem atomic replacement. Both affected directory handles are synced where required. DrvFS, network mounts, and other filesystems need separate evidence; no WSL production assumption is proved by this model.
3. **Persistence:** the claim journal is durable before target IO; request-key/intent history outlives client loss; a claimed reservation survives cancellation and interruption until quiescence/reconciliation. Durable visibility, durable file data, durable directory entry, and durable acknowledgement remain distinct.
4. **Evidence and hashes:** hashes are domain-separated and collision resistant; exact metadata/predecessor identity and writer/effect tokens supplement bytes. A matching after-hash alone never attributes authorship in an open worktree. Observations are meaningful only after old writers are quiescent.
5. **Authority:** clients are authenticated; grants, run/repository/binding scope, owner generation, procedure step, cancellation, and resource ownership are checked at dispatch and the effect boundary. Identical replay is observational, while changed intent conflicts permanently under the original key.
6. **Scope:** this is finite safety evidence, not host implementation, deployment, or liveness qualification. It makes no Lean-to-TLA equivalence claim.

`SG07Liveness.tla` separately defines weak fairness for unblocking, writes, synchronization, observation, reconciliation, settling, and release. A progress proof additionally needs eventually stable authority/availability, responsive observers, working storage/fsync, and old-process quiescence. Safety `Spec` contains none of these fairness assumptions, and no liveness theorem is asserted.

## Exact producer commands

From an admitted SG00 tool container with `/opt/tla/tla2tools.jar` and `/opt/tlapm/bin/tlapm`, and with a fresh `/evidence` mount:

```sh
python3 -B scripts/run_model.py --evidence /evidence/sg07
```

The equivalent host-side invocation uses the stored admitted image ID and a fresh out-of-tree attempt directory:

```sh
export SPLICERY_ATTEMPT="$(mktemp -d /tmp/splicery-sg07.XXXXXX)"
export SPLICERY_TOOL_IMAGE="$(python3 -c 'import json,pathlib; print(json.loads((pathlib.Path.home()/".local/share/splicery/tool-admission.json").read_text())["image_id"])')"
docker compose run --rm --name "splicery-sg07-$(basename "$SPLICERY_ATTEMPT")" qualify python3 -B scripts/run_model.py --evidence /evidence/sg07
```

The receipt is `$SPLICERY_ATTEMPT/sg07/model-receipt.json`. Existing evidence is never overwritten. Direct historical seed checks, if separately needed, remain `java -cp "$TLA2TOOLS_JAR" tlc2.TLC -config Edit.cfg Edit.tla` and `tlapm EditSafety.tla` in an isolated writable copy of `model/`; they are not part of an SG07 pass.
