# Proofs about the shipped edit kernel

Lean 4 theorems about the Rust code Splicery ships. `crates/splicery-splice` is
the pure byte-splice kernel: batch validation, canonical ordering, rendering and
`apply_at`. Charon and Aeneas translate it to Lean
([`Extracted/SplicerySplice.lean`](Extracted/SplicerySplice.lean), committed),
and the theorems in [`Kernel/`](Kernel/) are stated about that translation.
There is no hand-written Lean copy of the kernel. Production reaches these
functions only through `splicery-kernel` (`PreparedBatch::new` → `prepare`,
`render` → `render`, `apply_at` → `apply_at`), which is also what Writfold's
adapter embeds.

```sh
docker compose run --build --rm proofs   # the gate
sh proofs/refresh.sh                     # regenerate proofs/Extracted/ after a kernel change
```

## What is proved

The theorems hold **for the Aeneas translation of `crates/splicery-splice`**.
Every theorem is a total-correctness Hoare triple `f args ⦃ r => P r ⦄`, so it
also proves termination and that the call never panics: no out-of-bounds
index, no arithmetic overflow and no failed allocation-size check.

| Theorem | Statement |
|---|---|
| `same_capture_spec` | Capture identities are compared byte for byte (all 32 bytes). |
| `char_boundary_spec` | `char_boundary input i` holds iff `i = len`, or `i < len` and `input[i]` is not a UTF-8 continuation byte. |
| `check_cut_spec` | A cut is accepted iff it names this capture, `lo ≤ hi ≤ len` and, under the text profile, both ends are boundaries. Otherwise it refuses with `OutOfBounds` (out of range or another capture) or `InvalidUtf8Boundary` (in range, off a boundary). |
| `check_edit_spec` | An edit is accepted iff its target and every `Copy` range are valid cuts. |
| `replacement_len_spec` | The replacement length is the sum of literal lengths and copy widths. It refuses with `PayloadTooLarge` iff that sum exceeds 16 MiB. |
| `insert_by_start_spec`, `sort_by_start_spec` | The sort is `List.orderedInsert` by target start (formal's `sortAuthored`). It returns a permutation of the edit positions, sorted by start. |
| `compatible_spec`, `check_canonical_spec` | `compatible` decides formal's `AuthoredCompatible`: ends at or before the next start, touching only between two nonempty targets. `check_canonical` accepts iff every consecutive pair is compatible, and otherwise refuses with `Overlap`. |
| `isChain_iff_pairwise_disjoint` | For valid cuts sorted by start, consecutive compatibility is equivalent to pairwise disjointness. |
| `prepare_spec` | `prepare` returns `Ok order` only for an admissible batch, with `order` a permutation and the targets canonical in that order. Otherwise it refuses for a reason that holds of the batch: `TooManySplices` (more than 4096 edits), `OutOfBounds`/`InvalidUtf8Boundary` (some edit is invalid; the latter only under text), `PayloadTooLarge` (total payload over 16 MiB) or `Overlap` (all edits are valid and fit, but two targets are not disjoint). It never returns `CandidateTooLarge`. |
| `prepare_ok_iff` | `prepare` accepts exactly the admissible batches. |
| `admissible_perm`, `prepare_order_independent` | Admission does not depend on authored order. Two accepted orderings of the same edits give the same canonical list, so `render` produces the same candidate and reports (formal's `normalizeAuthored_eq_of_perm`). |
| `materialize_spec` | A replacement is its parts in authored order, and every `Copy` reads the original image. |
| `render_spec`, `prepared_ready`, `prepare_render` | For an order from `prepare`, `render` yields the original with each target replaced by its replacement in canonical order. Every gap is copied from the original, never from the candidate being built. The reported replacements are exactly those bytes. `no_change` holds iff the output equals the input. It refuses, only with `CandidateTooLarge`, iff the output would exceed 16 MiB. |
| `spliced_length` | Output length + Σ target widths = input length + Σ replacement lengths. |
| `spliced_single` | A one-edit batch renders `input[..lo] ++ replacement ++ input[hi..]`. |
| `prepare_empty`, `render_empty` | An empty batch is accepted and renders the input unchanged with `no_change`. Inputs within 16 MiB qualify; production inputs are at most 8 MiB. |
| `apply_at_spec` | `apply_at` returns `input[..lo] ++ replacement ++ input[lo+width..]` iff `lo ≤ len`, `width ≤ len − lo` (a checked remainder, never a wrapped `lo + width`) and the output fits 16 MiB. Otherwise it returns `None` (formal's `applyAt`). |

The gate counts 105 audited declarations under `Splicery.Kernel`. These include
the loop lemmas and the equation lemmas that Lean generates for the reference
definitions; 25 of them are required headline claims.

**Trust base.** The Charon/Aeneas translation is faithful to the Rust
semantics of this crate, including Aeneas's models of `Vec`, slices, arrays and
`usize` arithmetic. Lean's kernel checks the proofs, and only the standard
axioms are used (`propext`, `Classical.choice`, `Quot.sound`). The audit
rejects anything else, including `sorryAx` and any Aeneas external axiom. The
Rust compiler is trusted to compile the same source that was translated.

**Not proved here** (tested, or the responsibility of another layer):

- The `splicery-kernel` facade. It maps the checked `splicery_domain` types onto
  the kernel's plain data, applies the proved order to the authored splices for
  `splices()` and the edit reports, and checks that a text-profile candidate is
  well-formed UTF-8 with `core::str::from_utf8`. All of this is tested, not
  translated.
- Text-profile boundary agreement with `str::is_char_boundary`. It relies on
  the domain invariant that a text `OriginalImage` is well-formed UTF-8
  (`OriginalImage::new` validates it). Under that invariant, a position is a
  boundary exactly when it is the end or holds a non-continuation byte.
- Capture digests (SHA-256 in `splicery-domain`). The kernel compares them; it
  does not compute them.
- Recipe syntax, lowering, frozen iteration and the wire codecs. These remain
  covered by the reference calculus in [`formal/`](../formal/), the TLA+ models
  in [`model/`](../model/) and the differential and fuzz suites.

## Relation to `formal/`

[`formal/`](../formal/) is the reference calculus: Lean v4.21.0, core/Std
only, and its own build. It is unchanged. This project uses the Aeneas
toolchain (Lean v4.35.0 with mathlib), so the specification statements it
needs are restated over the kernel's types rather than imported:
`Compatible` is `AuthoredCompatible` (`stop = hi`, `width ≠ 0 ↔ lo ≠ hi`),
`sort_by_start` is `sortAuthored`, `emit` is `Batch.apply` over gaps,
`apply_at_spec` mirrors `applyAt_eq_some_iff`, and `prepare_order_independent`
mirrors `normalizeAuthored_eq_of_perm`.

## The gate

`proofs/gate.sh` runs inside the image `proofs/Dockerfile` builds:

1. `extract.sh` translates the crate in a throwaway shadow workspace: Charon,
   then Aeneas.
2. The fresh translation must equal the committed `Extracted/` byte for byte.
3. `lake build` builds the translation, the theorems and the audit.
4. The axiom audit (`Audit.lean`) checks every theorem's axioms and requires
   the headline claims to exist.
5. Mutant calibration (`mutants.py`): 23 semantic mutants of
   `crates/splicery-splice/src/splice.rs`. Each one is re-translated and must
   break exactly the named theorem(s) and nothing else.

## Pins

These are the toolchain pins other writfold-dev projects use for this method;
they move together:

| Input | Pin |
|---|---|
| Aeneas | release `nightly-2026.11.07-c5eb4a2`, `aeneas-linux-x86_64.tar.gz` sha256 `14fbaed4ce8624cf3d668f887d6ebb237a59cc37d7498c792d9f3f97332cbb25` |
| Charon | commit `8cae8fd1905946de61147a65ecdd7122cc6b491b` (Aeneas's `charon-pin`), built from source on `nightly-2026-09-17` |
| Lean | `leanprover/lean4:v4.35.0` (`lean-toolchain`), elan `v4.2.4` |
| mathlib | `v4.35.0` → `56a005855ea17e0b0958f73ec801ae140b0a1679` (`lake-manifest.json`) |
| base image | `ubuntu:24.04@sha256:224a1869083a311ef3f13648a154ba79832fbef6364d31493642ca03082da254` |

## Shape rules the translation needs

Each rule is also recorded where it applies in the source:

- No derives in the translated module. Charon translates every impl in a
  module it starts from, and a derived `Debug` pulls in `core::fmt` as an
  axiomatized external. `Clone`, `Copy`, `PartialEq` and `Debug` are written by
  hand in `convenience.rs`, outside `[package.metadata.charon] start_from`.
- Owned data inside types. `Piece::Literal` holds a `Vec<u8>`, because a
  borrowed slice inside a borrowed type is a nested borrow.
- Explicit equality. Capture identity is the `same_capture` byte loop, not a
  derived `==` on `[u8; 32]`, and `no_change` uses the `same_bytes` loop.
- No `?`. An explicit `match` keeps the Lean side a plain case split instead of
  going through the `Try`/`FromResidual` models.
- No `RangeInclusive::contains`. Two comparisons keep `PartialOrd` out.
- Literal limits (`16_777_216`). `16 * 1024 * 1024` translates to a fallible
  `Result` computation.
- Caps checked as `amount > CAP - used`, never as a checked addition. The
  running total is kept at or below the cap.
- `render` re-checks indices and coordinates. That makes it total for any order
  it is handed; for an order from `prepare` the theorems show those refusals
  are unreachable.
- The canonical order is returned as positions (`Vec<usize>`) rather than
  moved edits. The facade applies it to the domain splices.
