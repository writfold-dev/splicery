# Proof-to-shipping trace and review tasks

Names under “production responsibility” are suggested names in a new repository. Freeze final symbol locations and digests as part of each receipt. Coverage is a many-to-many mapping; a high line-coverage number cannot replace a missing semantic boundary.

| Semantics | Production responsibility | Qualification witness |
|---|---|---|
| L01/L02 exact slice and splice | CheckedRange::new, checked_end, kernel::emit_splice | Verus pre/post for bounds, integer widths, no wraparound, termination and prefix/suffix equality. Run both profiles and exact compiled artifact. C04/C06 |
| L03/L07 surface semantics | compile::lower_change, lower_payload, lexer origin mapping | Differential direct Lean meaning vs all public raw-recipe outputs, not Rust vs itself. Independent +/- vectors for each EBNF branch and diagnostic class. C01/C02/C06 |
| L04 worktree identity and selection | resolve::select_one, expand_handle, read/write constructor boundary | Wrong-kind and cross-epoch compile-fail/refusal; exact image/selection digests bound before exposing a target. C03/C09/C11 |
| L05 disjoint normalized batch | kernel::validate_batch, normalize, emit_batch, output budget arithmetic | Verus loop invariants: cursor monotonic, untouched gap exact, stored slices refer to original only, output length equals sum of checked segments. Never verify only a specification clone. C07/C08 |
| L06 frozen iteration / cognition | select::line_matches, freeze, bind_group, limit tracker | External matcher produces untrusted candidate spans/or qualified matcher evidence. Verify mapping, boundaries, completeness policy and cap; bind profile and parent capture. Test 0/1/n/absent/ambiguous/omitted cases. C03/C05/C08 |
| L08–09 domain/bridge | prepare::validate, stamp::claim, adapter::publish | Verus/checked predicates for scope and staleness; bridge to actual coordinator/FS events in TLC/TLAPS, crash campaigns on real Linux/WSL. C10–20 |
| Fileset mutation | create_absent, remove_expected, rename_no_replace | Integrated actual same-filesystem operator behavior, metadata and grants. Compare exact prior names after fault, preserve unresolved status. C14/C15/C18 |

For each proof, document the public property, formal judgment, modeled domains, checked symbol, trusted boundary, source/configuration digest, positive example, a specific bug that would violate it, and the observed counterexample when that bug is planted. All successful release variants must execute the checked body or have a documented verified replacement. Keep parser-generator output, regex-engine/profile assumptions, native filesystem and external evidence on the assurance inventory.

Example first review: mutate lower(after) to select the start. Isolate the mutant, build it with its own cache identity, then require the after-vs-before property or public regression to fail with the intended call selection. Revert and rerun a clean control. A compile error or unchanged proof artifact counts as an invalid calibration attempt.
