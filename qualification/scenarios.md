# Adversarial acceptance matrix

Assign these IDs to concrete cases/receipts; implement through public entrypoints. Each row requires a positive control that exercises the same path, a named refusal or exact output, and snapshots of writes, content identity and effect state where applicable. Split multiple attacks into separately recorded case IDs in the implementation ledger.

| ID | Input or intervention | Expected observation | Gates |
|---|---|---|---|
| C01 | @call := valid binding; duplicate, forward, shadow, reserved, old assignment | Only valid scopes resolve; source byte diagnostic identifies offending token | SG01,04 |
| C02 | Quoted block contains braces, dollar, Unicode, CRLF; tag prefix, missing tag, raw CR outside tokens | Exact literal byte mapping; wrong delimiter refuses; no partial recipe | SG01,02,04 |
| C03 | Equal selected text in two files, two sessions or two occurrences incl overlapping starts | No cross-file authority, epoch reuse or implicit first-match | SG02–04,08 |
| C04 | Between adjacent/reversed/duplicate markers; final unterminated begin; UTF-8 offsets across a scalar | Empty explicit range or indicated refusal; no edits to marker bytes | SG02,04,05 |
| C05 | Match 3 actual groups, expect 2; group absent vs present empty; truncated result; zero-width global match | Exact cardinality/absence semantics; incomplete cannot pass | SG02,04,05 |
| C06 | wrap multiline mixed-EOL call; CRLF line replacement, no terminal LF, binary NUL, invalid UTF-8 | Exact raw goldens for protected bytes and new bytes; profile-specific refusal | SG02,04,05 |
| C07 | Swaps, disjoint moves and permutations; overlapping ranges, same-anchor inserts, insert on replacement edge | Same frozen source across ops; conflicting batch entirely refused | SG02,05 |
| C08 | Limit ±1 for recipe, input, output, nesting, cumulative captures, compiled regex and total work | Complete success at admitted boundary or a typed whole-request refusal; bounded memory | SG04,06 |
| C09 | Newtype misuse at Rust API and decoder; wrong hash algorithm/kind, swapped units, forged Prepared ID | Compile-fail or validated decode refusal; no raw re-export escape | SG03,04,06 |
| C10 | Weft report names a different plan; valid report with expired grant; public citation offered as target | Run intake/attachment or effect refuses under its own rules; no authority escalation | SG03,08 |
| C11 | Dirty worktree differs from HEAD, including same span with changed context; format before apply | Inspect captures actual file; stale apply refuses; original prepared bytes remain inspectable with grant | SG08,09 |
| C12 | Valid first write and identical same-key retry; different payload with same key, including after edit-revert ABA | Single logical effect, replay/lookup or conflict; no second target write | SG07,08,10 |
| C13 | Kill at claim, write, rename, directory sync, acknowledgement; repeat every interruption | Confirm only on discriminating evidence; otherwise Unknown under same reservation | SG07,09,10 |
| C14 | Parent symlink swapped, hardlink, FIFO, directory, file owner/mode/ACL drift, cross-device destination | Refuse at contained resolution/materialization, preserving unrelated files | SG09,11 |
| C15 | Real ENOSPC before temp completed and fsync failure after rename | Proven no-effect or Unknown/partial according to observed point; no false success | SG09,10 |
| C16 | Second writer or child survives cancellation, owner transfer while held, old process replies late | No effect under stale ownership; hold until fenced/reaped; no forged confirmation | SG07,09,10 |
| C17 | Multiple repos, first applied, second denied or in doubt | Per-file effects/actual repo vector visible; never global success/rollback | SG10,11 |
| C18 | Create at absent name, race to create, failed compare-remove, rename onto a live target | Real absence and preimage checks; overwritten user files count is zero | SG09,11 |
| C19 | Host only advertises read, stale response/alias epoch, partial frame, malformed/trailing JSON or giant request | Versioned typed response at transport boundary, clean stdout, no fallback | SG04,08,10 |
| C20 | Observer or KB query arrives while bytes change; one reader denied; activity projection lags | Authorized historical outcome and watermark; read/peek/watch does not acquire edit permission | SG03,08,10 |
| C21 | Module/lint/cargo feature/macro cheats or a proof escape in active or generated source | Quality/axiom/graph checks fail for intended reason; valid siblings compile | SG02,05,06 |
| C22 | Cold offline build, wrong image/tool hash, external disk absent, release missing one receipt | Bootstrap/aggregator refuses; active cache/drive and user files untouched | SG00,13,14 |
| C23 | Wrap, precise import, config section, bulk captures and comment-preserving move under two harnesses | Measured complete tasks, exact final code, retries/bytes/time; no hidden task bypass | SG12 |
| C24 | Empty/no-change recipe, stale preview or approval, access revoked; same input on another worktree | No synthetic successes, stale authority or permission from equal bytes | SG03,07–10 |

**Assurance trace required for the release reviewer.** Pick one normal and one adversarial case per syntax family. Follow original recipe bytes → lexical locations → typed selection → original capture → normalized primitive → compared before/after bytes → materializer observation → effect receipt → Run Watch projection, alongside obligations/assumptions and refusals at each arrow. Capture the real binary and lock digests. The prototype may illuminate that chain without closing any missing shipping link.
