# Offline preview client

The `splicery` executable is a static Linux x86_64 musl client for the scoped offline component. It reads an existing regular file, resolves a recipe against that immutable image, and emits either the exact candidate bytes or a bounded exact splice report. It never writes the target and does not implement a Writfold transport, Worktrunk grant, `StaleTarget` apply refusal, effect receipt, or retry reconciliation.

## Crate boundaries

| Crate | Responsibility | Dependencies |
|---|---|---|
| `splicery-domain` | Limits, checked source spans, immutable image and resolved IR | None |
| `splicery-syntax` | AST, strict byte lexer and recursive-descent parser | Domain |
| `splicery-compile` | Source-order bindings, selector resolution and lowering | Domain, syntax |
| `splicery-splice` | Pure byte-splice kernel: batch validation, canonical order, rendering, `apply_at`; proved over its Aeneas translation ([proofs](../proofs/README.md)) | None (`no_std` + `alloc`) |
| `splicery-kernel` | Maps checked domain batches onto `splicery-splice`; UTF-8 check of text candidates | Domain, splice |
| `splicery-wire` | Bounded numeric/hex and exact JSON report codecs | Domain, kernel |
| `splicery-cli` | Arguments, contained regular-file reads and stdout | All five |

The syntax, splice and kernel crates have no filesystem, Git, network, shell, database, Writfold or Weft dependency. The wire layer makes no authorization decision.

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

## Line bindings

A bare line target selects the line body without its terminator, and `after` inserts at the end of the selection ([language §3 and §7](../spec/language.md#binding-selection-and-execution)). So `after @line insert lines like @line [...]` places the new lines before `@line`'s terminator and joins the two lines, and `delete @line` leaves an empty line. To insert whole lines, insert `before` the following line (as [09-import](../examples/09-import.spl) does) or select the line together with its terminator using `one text`. The preview prints a `note:` on stderr, with the statement's recipe byte span, when a recipe contains either construct; stdout is unchanged and the meaning is not altered. Changing that meaning is a language-contract change covering the spec, the Lean surface calculus and the trusted rehearsal; it is not made by the preview.

## Primitive differential interface

```bash
splicery kernel apply 1 1 616263 5858
# 61585863
```

The command accepts `apply LO WIDTH INPUT_HEX REPLACEMENT_HEX`, with at most 4096 bytes in either byte string. Success emits lowercase candidate hex. Invalid numbers, malformed hex, invalid cuts and limit failures emit `REFUSAL` and exit 1. This is the same bounded interface as `formal/Splicery/Oracle.lean`.

## Admitted syntax and limits

The preview accepts the /1 grammar: file-edit blocks and path-bound fragments, `@name :=` bindings, references and capture groups, `one text`, `one line`, `one hex`, `between`, `range`, `replace`, `delete`, `before`, `after`, `each ... in line_matches`/`frozen` iteration, STRING, HEX, HEREDOC, `keep`, `lines like` and `indent`. It enforces strict recipe UTF-8, original-coordinate reads, unique overlapping matches, source-order/no-shadow binding, UTF-8 boundaries, overlap refusal and input/candidate/payload/match limits. `create`, `rename` and `remove` print a typed file plan and are never executed. An offline preview is a scoped component, not a complete /1 profile claim.

## Scratch runtime

`docker/Dockerfile.runtime` packages the produced executable, project MIT license,
original notice/source ZIP and its hash-bound inventory under UID/GID 65532.
See [provenance](provenance.md#offline-packaged-notices) for pinned offline
notice inputs and the distinction between a build and publication qualification:

```bash
docker buildx build --network=none --pull=false --load \
  --file docker/Dockerfile.runtime --tag splicery-runtime:local \
  "$EVIDENCE/musl-build/build"
docker run --rm --network none splicery-runtime:local kernel apply 1 1 616263 5858
```
