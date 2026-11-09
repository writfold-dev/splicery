# Splicery quickstart for agents

One page instead of [the language reference](../spec/language.md). Every recipe
below is run by the language gate (`tools/check_language.py`) against the
inputs and expected bytes in
[quickstart-vectors.json](../qualification/quickstart-vectors.json); the
`# card:` line names its vector.

## Workflow

```sh
# Preview a fragment from stdin against one file: read the diff, not the file.
printf '%s\n' '@x := one text "80"' 'replace @x with "100"' |
  splicery preview --root . --file config/limits.toml --diff -
# The same recipe without --diff prints the exact candidate bytes.
```

- Selectors compare exact bytes and must match exactly once; zero or several
  matches refuse with the sites and a hint on stderr. Nothing is guessed.
- Every selector and `keep` reads the original file, so swaps and moves need
  no ordering tricks. Overlapping edits refuse the whole batch.
- Preview never writes the target. `--out NEW_FILE` writes a new file outside
  `--root`; applying an edit to a worktree belongs to Writfold.

## Idioms

Wrap an expression without retyping it:

```splicery
# card: wrap
@call := one text "transport.send(request)"
replace @call with ["trace_send(", keep(@call), ")"]
```

Swap two values; both `keep`s read the original:

```splicery
# card: swap
@soft := one text "100"
@hard := one text "80"
replace @soft with keep(@hard)
replace @hard with keep(@soft)
```

Insert a whole line after another, reusing its line ending:

```splicery
# card: insert-line
@config := one line "use crate::config::Config;"
after whole @config insert lines like @config ["use crate::policy::Policy;"]
```

Delete a whole line, terminator included:

```splicery
# card: delete-line
@stale := one line "    // TODO: remove"
delete whole @stale
```

Select a long span by two short anchors instead of byte offsets:

```splicery
# card: span
@open := one text "format!("
@close := one text "name,\n)"
@expr := from @open to @close
replace @expr with ["sanitize(", keep(@expr), ")"]
```

Disambiguate a repeated line inside a window between two marker lines:

```splicery
# card: window
@begin := one line "[server]"
@end := one line "[client]"
@section := between @begin and @end
@port := one text "port = 80" within @section
replace @port with "port = 8080"
```

## Traps

- `one line` matches a complete line body, indentation included; `one text`
  matches any part of a line.
- A bare line binding selects the body without its terminator: `after @x
  insert lines like @x [...]` joins two lines and `delete @x` leaves an empty
  line. Use `after whole @x` and `delete whole @x`.
- `from @a to @b` includes both anchors; `between @a and @b` excludes both
  marker lines. Reversed or overlapping anchors refuse.
- Reserved words cannot be handle names: `@line`, `@text`, `@from`, `@to` and
  the other keywords refuse; pick names like `@row` or `@open`.
- An ambiguous selector refuses rather than picking one:

```splicery
# card: ambiguous
replace one text "level" with "LEVEL"
```
