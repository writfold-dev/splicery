# Offline preview client

The `splicery` executable is a static Linux x86_64 musl client for the scoped offline component. It reads an existing regular file, resolves a recipe against that immutable image, and emits the exact candidate bytes, a bounded exact splice report or a unified diff. It never writes the target and does not implement a Writfold transport, Worktrunk grant, `StaleTarget` apply refusal, effect receipt, or retry reconciliation; applying an edit to a worktree belongs to Writfold.

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

`--diff` writes a unified diff (`--- a/PATH`, `+++ b/PATH`, three lines of context) derived from the same exact splices; no change prints nothing. It is a view for reading: the candidate remains the only thing a host applies. Lines without a final newline carry the usual `\ No newline at end of file` marker, and a diff over 16 KiB refuses rather than truncating. `--report` and `--diff` are exclusive; each invocation prints one view.

A recipe argument of `-` reads the recipe from standard input (at most 256 KiB), so an agent can preview without writing a recipe file:

```bash
printf 'replace one text "80" with "100"' |
  splicery preview --root /path/to/worktree --file config/limits.toml --diff -
```

`--out NEW_FILE` writes the selected view to a new file instead of stdout. The file must not exist and its directory must be outside `--root`; preview never replaces a file and never writes inside the worktree. Moving a candidate into place stays with the caller or, in the managed flow, with Writfold.

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

A bare line target selects the line body without its terminator, and `after` inserts at the end of the selection ([language §3 and §7](../spec/language.md#binding-selection-and-execution)). So `after @line insert lines like @line [...]` places the new lines before `@line`'s terminator and joins the two lines, and `delete @line` leaves an empty line. `whole @name` selects a line binding together with its terminator, `[start, fullEnd)`: `after whole @a insert lines like @a ["new"]` inserts a complete line after `@a`, and `delete whole @a` removes the line. On an unterminated final line `whole` selects the body through the end of the file. The preview prints a `note:` on stderr, with the statement's recipe byte span, when a recipe uses a bare line binding in either trap; stdout is unchanged and the meaning is not altered.

## Refusals

A refusal exits 2 with stdout empty and one fact per stderr line. The first line keeps the typed kind name and the recipe span, for example:

```text
refused: AmbiguousSelector at recipe [20,47) in src/log.rs
  matched 2 sites: line 2 bytes [2,11); line 4 bytes [14,23)
  line 2: level = 1
  line 4: level = 1
  hint: narrow it with `within` a window (`between` two unique lines, or `from @a to @b`), or add distinguishing context to the needle
```

Ambiguous selectors list the match count and at most four sites. A missing selector reports, when the work budget allows, lines that match after trimming spaces and tabs, matches with LF and CRLF exchanged, or matches outside its `within` window. Misordered `between` and `from … to` anchors name both anchors. Sites are line numbers and original byte ranges. File bytes (`line N: …`, at most 120 bytes per site) are shown only when the caller named the target file on the command line with `--file`; a recipe that names its own file gets positions and hints only. This is offline-preview policy; in the managed flow, diagnostics follow the host's grant rules (protocol rule 5) and this client adds nothing to them.

## Primitive differential interface

```bash
splicery kernel apply 1 1 616263 5858
# 61585863
```

The command accepts `apply LO WIDTH INPUT_HEX REPLACEMENT_HEX`, with at most 4096 bytes in either byte string. Success emits lowercase candidate hex. Invalid numbers, malformed hex, invalid cuts and limit failures emit `REFUSAL` and exit 1. This is the same bounded interface as `formal/Splicery/Oracle.lean`.

## Admitted syntax and limits

The preview accepts the /1 grammar: `edit` blocks (raw files `as bytes`) and path-bound fragments, `@name :=` bindings, references and capture groups, `one text`, `one line` and `one hex` selectors (optionally `within` a capture), `between @a and @b`, `from @a to @b`, `whole @a`, `range`, `replace … with`, `delete`, `before … insert`, `after … insert`, `each … in line_matches … expect N` and `frozen` iteration, STRING, HEX, HEREDOC, `keep`, `lines like` and `indent`. It enforces strict recipe UTF-8, original-coordinate reads, unique overlapping matches, source-order/no-shadow binding, UTF-8 boundaries, overlap refusal and input/candidate/payload/match limits. `create`, `rename … to` and `remove` print a typed file plan and are never executed. An offline preview is a scoped component, not a complete /1 profile claim. The language gate (tools/check_language.py) checks this paragraph against the grammar: every EBNF keyword must appear here, and every keyword must be exercised by a grammar accept case, an example or a quickstart recipe.

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
