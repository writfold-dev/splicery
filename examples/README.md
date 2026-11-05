# Agent recipe cards

All .spl files in this directory are exercised from original recipe text. Exact inputs, expected bytes or refusals are in [the vectors](../qualification/recipe-vectors.json). Run python3 -B tools/check_handover.py from the archive root. The rehearsal is an independent bounded implementation for the trusted examples.

| Card | What to look for |
|---|---|
| 01-await | Suffix inside existing punctuation; edit does not resend original call |
| 02-wrap | Keep exact multi-line expression, comment, mixed endings and Unicode |
| 03-policy | Between whole-line markers; quoted multiline literal |
| 04-routes | Named captures, frozen match cardinality, original CRLF/LF/no terminal LF |
| 05-swap | Both sides read the original snapshot |
| 06-raw | Non-UTF-8 byte keep/replacement in an explicit raw profile |
| 07-lines | Deliberately choose a nearby line terminator for new lines |
| 08-indent | Prefix only nonempty bodies; preserve each terminator |
| 09-import | Disambiguate duplicate text inside an explicitly selected window |
| 10-move | Move a comment and its line as a single captured unit |
| 11-absence | Reject use of an absent optional group |

Interactive fragment: **after @call insert ".await"**. The bound client supplies the file, capture epoch and authorized target. In a recipe, **@call := one text "transport.send(request)"** creates a local name. Use inspect/peek to avoid spelling a long original passage; pass a durable exact binding when crossing a session boundary. On Ambiguous inspect the bounded candidates and narrow with within. On StaleTarget reread the bounded current target and prepare a new edit with a new request key. On Unknown look up/reconcile the original effect. Do not retry a write merely because its acknowledgement was lost.

File lifecycle examples in grammar-cases.json have syntax coverage only. Implement the full owner-side absence/path/ownership and crash fixtures before enabling these operations. Agent-ergonomics qualification measures end-to-end outcomes as well as tokens; an accepted grammar string alone tells you little about whether authoring a change is pleasant.
