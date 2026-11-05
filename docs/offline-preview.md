# Offline preview client

The `splicery` executable is a static Linux x86_64 musl client for the scoped offline component. It reads an existing regular file, resolves a recipe against that immutable image, and emits either the exact candidate bytes or a bounded exact splice report. It never writes the target and does not implement a Writfold transport, Worktrunk grant, `StaleTarget` apply refusal, effect receipt, or retry reconciliation.

## Crate boundaries

| Crate | Responsibility | Dependencies |
|---|---|---|
| `splicery-domain` | Limits, checked source spans, immutable image and resolved IR | None |
| `splicery-syntax` | AST, strict byte lexer and recursive-descent parser | Domain |
| `splicery-compile` | Source-order bindings, selector resolution and lowering | Domain, syntax |
| `splicery-kernel` | Checked batches, exact rendering and Verus `apply_at` | Domain, `vstd` |
| `splicery-wire` | Bounded numeric/hex and exact JSON report codecs | Domain, kernel |
| `splicery-cli` | Arguments, contained regular-file reads and stdout | All five |

The syntax and kernel crates have no filesystem, Git, network, shell, database, Writfold or Weft dependency. The wire layer makes no authorization decision.

## Build

```bash
python3 -B scripts/gate bootstrap --receipt "$EVIDENCE/bootstrap/receipt.json"
python3 -B scripts/gate build --receipt "$EVIDENCE/musl-build/receipt.json"
```

The client and its SHA-256-bound build record are under `$EVIDENCE/musl-build/build/`. The build uses the locked offline dependency closure and runs a kernel smoke case against the produced executable.

## Preview

Given `sdk/src/client.rs` containing `transport.send(request)`, this recipe wraps the only exact occurrence:

```splicery
edit "sdk/src/client.rs" {
  @call := one text "transport.send(request)"
  replace @call with ["trace_send(", keep(@call), ")"]
}
```

```bash
splicery preview --root /path/to/worktree wrap.spl > candidate.rs
splicery preview --root /path/to/worktree --report wrap.spl
```

Default preview writes only candidate bytes to stdout. `--report` instead writes one JSON object with `schema_version`, `scope`, `path`, `no_change`, `candidate_length`, and exact `edits`. Every edit includes the original `[lo,hi)` target, complete replacement hex and recipe byte span. A report over 16 KiB refuses rather than truncating identity information.

For a fragment and an already captured span:

```splicery
replace @call with ["trace_send(", keep(@call), ")"]
```

```bash
splicery preview --root /path/to/worktree \
  --file sdk/src/client.rs --capture @call=12:35 fragment.spl
```

Offsets are zero-based bytes into the original image, not UTF-16 or editor columns. Captures are offline aliases only; they confer no authority.

## Primitive differential interface

```bash
splicery kernel apply 1 1 616263 5858
# 61585863
```

The command accepts `apply LO WIDTH INPUT_HEX REPLACEMENT_HEX`, with at most 4096 bytes in either byte string. Success emits lowercase candidate hex. Invalid numbers, malformed hex, invalid cuts and limit failures emit `REFUSAL` and exit 1. This is the same bounded interface as `formal/Splicery/Oracle.lean`.

## Admitted syntax and limits

The preview accepts file-edit blocks and path-bound fragments, `@name :=` bindings, references, `one text`, `one hex`, `range`, `replace`, `delete`, `before`, `after`, STRING, HEX, HEREDOC and `keep`. It enforces strict recipe UTF-8, original-coordinate reads, unique overlapping matches, source-order/no-shadow binding, UTF-8 boundaries, overlap refusal and input/candidate/payload limits.

Other /1 productions return `Unsupported`: file lifecycle operations, `one line`, `between`, iteration, regex/frozen sets, capture groups, `lines like` and `indent`. These omissions are not hidden by a fallback and are not a complete /1 profile claim.

## Scratch runtime

`docker/Dockerfile.runtime` packages only the produced executable under UID/GID 65532:

```bash
docker buildx build --network=none --pull=false --load \
  --file docker/Dockerfile.runtime --tag splicery-runtime:local \
  "$EVIDENCE/musl-build/build"
docker run --rm --network none splicery-runtime:local kernel apply 1 1 616263 5858
```
