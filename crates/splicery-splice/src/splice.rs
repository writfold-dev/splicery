//! Batch validation, canonical ordering and exact rendering of byte splices
//! over one immutable original image.
//!
//! Proved over the Aeneas translation of this module (proofs/Kernel/):
//!
//! - `prepare` accepts exactly the admissible batches: at most
//!   `MAX_SPLICES` edits, every target and copy range inside the image,
//!   bound to its capture and (text profile) on character boundaries, total
//!   payload within `MAX_PAYLOAD_BYTES`, and every pair of targets disjoint
//!   (touching only between two nonempty targets). An accepted order is a
//!   permutation of the edits whose targets are canonical (each ends at or
//!   before the next starts);
//! - `render` of an accepted batch is the original with each target replaced
//!   by its materialized parts, in canonical order: every untouched gap is
//!   copied from the original, every `Copy` part reads the original (never
//!   the candidate being built), and the output length is the input length
//!   minus all target widths plus all replacement lengths; it refuses with
//!   `CandidateTooLarge` exactly when that length exceeds the cap;
//! - `apply_at` replaces exactly `width` bytes at `lo` or refuses.
//!
//! Shape rules for the translation (each one is recorded where it applies):
//! no `?` (explicit `match`), no iterators (`while` loops), no casts, no
//! derives (see `convenience.rs`), owned data inside every type, and
//! equality that a theorem depends on written as a kernel function.
// `?` translates through the `Try`/`FromResidual` trait models; an explicit
// `match` keeps the Lean side a plain case split. Likewise two comparisons
// instead of `RangeInclusive::contains`, which goes through `PartialOrd`.
#![allow(clippy::question_mark, clippy::manual_range_contains)]
use alloc::vec::Vec;

// The limits are literals: `16 * 1024 * 1024` translates to a fallible
// `Result` computation in Lean, a literal to a plain value. The facade's
// tests pin each one to the `splicery_domain` constant of the same name.

/// Most edits one batch may carry (`splicery_domain::MAX_SPLICES`).
pub const MAX_SPLICES: usize = 4096;
/// Most replacement bytes one batch may materialize, 16 MiB
/// (`splicery_domain::MAX_PAYLOAD_BYTES`).
pub const MAX_PAYLOAD_BYTES: usize = 16_777_216;
/// Largest candidate a batch may render, 16 MiB
/// (`splicery_domain::MAX_CANDIDATE_BYTES`).
pub const MAX_CANDIDATE_BYTES: usize = 16_777_216;
/// Width of a capture identity (a SHA-256 digest of the original image).
pub const CAPTURE_BYTES: usize = 32;

/// A half-open byte range `lo..hi` of the original image identified by
/// `capture`. Coordinates are unchecked here; `prepare` checks them.
pub struct Cut {
    pub capture: [u8; CAPTURE_BYTES],
    pub lo: usize,
    pub hi: usize,
}

/// One piece of a replacement: literal bytes, or a copy of original bytes.
/// Literals are owned (`Vec<u8>`): a borrowed slice inside a type that is
/// itself borrowed is a nested borrow, which Aeneas does not translate.
pub enum Piece {
    Literal(Vec<u8>),
    Copy(Cut),
}

/// Replace `target` with the concatenation of `parts`.
pub struct Edit {
    pub target: Cut,
    pub parts: Vec<Piece>,
}

/// Why a batch was refused. No refusal carries partial output.
pub enum Refusal {
    OutOfBounds,
    Overlap,
    TooManySplices,
    PayloadTooLarge,
    CandidateTooLarge,
    InvalidUtf8Boundary,
}

/// A rendered candidate: the new bytes, each edit's materialized
/// replacement in canonical order, and whether the bytes equal the input.
pub struct Rendered {
    pub bytes: Vec<u8>,
    pub replacements: Vec<Vec<u8>>,
    pub no_change: bool,
}

/// Byte-exact capture identity, as a loop rather than a derived
/// `PartialEq` on `[u8; 32]`, so the proved call graph stays inside the
/// Aeneas standard-library models.
pub fn same_capture(a: &[u8; CAPTURE_BYTES], b: &[u8; CAPTURE_BYTES]) -> bool {
    let mut i = 0;
    while i < CAPTURE_BYTES {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A UTF-8 continuation byte (`10xxxxxx`), written as a range so no bit
/// operation enters the proofs.
pub fn is_continuation(byte: u8) -> bool {
    byte >= 0x80 && byte <= 0xBF
}

/// `i` is a character boundary of `input`: the end, or a position whose byte
/// does not continue a character. For the well-formed UTF-8 that
/// `splicery_domain::OriginalImage` admits under the text profile this is
/// exactly `str::is_char_boundary`.
pub fn char_boundary(input: &[u8], i: usize) -> bool {
    if i == input.len() {
        return true;
    }
    if i > input.len() {
        return false;
    }
    !is_continuation(input[i])
}

/// A cut is valid for this image: same capture, `lo <= hi <= len`, and under
/// the text profile both ends on character boundaries.
pub fn check_cut(
    input: &[u8],
    text: bool,
    capture: &[u8; CAPTURE_BYTES],
    cut: &Cut,
) -> Result<(), Refusal> {
    if !same_capture(&cut.capture, capture) {
        return Err(Refusal::OutOfBounds);
    }
    if cut.lo > cut.hi || cut.hi > input.len() {
        return Err(Refusal::OutOfBounds);
    }
    if text && !(char_boundary(input, cut.lo) && char_boundary(input, cut.hi)) {
        return Err(Refusal::InvalidUtf8Boundary);
    }
    Ok(())
}

/// Check the target, then every copied range, in authored order.
pub fn check_edit(
    input: &[u8],
    text: bool,
    capture: &[u8; CAPTURE_BYTES],
    edit: &Edit,
) -> Result<(), Refusal> {
    match check_cut(input, text, capture, &edit.target) {
        Ok(()) => {}
        Err(refusal) => return Err(refusal),
    }
    let mut k = 0;
    while k < edit.parts.len() {
        match &edit.parts[k] {
            Piece::Literal(_) => {}
            Piece::Copy(cut) => match check_cut(input, text, capture, cut) {
                Ok(()) => {}
                Err(refusal) => return Err(refusal),
            },
        }
        k += 1;
    }
    Ok(())
}

/// Bytes a piece contributes. A reversed copy refuses rather than wraps.
pub fn piece_len(piece: &Piece) -> Result<usize, Refusal> {
    match piece {
        Piece::Literal(bytes) => Ok(bytes.len()),
        Piece::Copy(cut) => {
            if cut.lo > cut.hi {
                Err(Refusal::OutOfBounds)
            } else {
                Ok(cut.hi - cut.lo)
            }
        }
    }
}

/// The replacement length of one edit, capped at `MAX_PAYLOAD_BYTES`. The
/// running total never exceeds the cap, so the comparison is written as a
/// subtraction from the cap rather than a checked addition.
pub fn replacement_len(edit: &Edit) -> Result<usize, Refusal> {
    let mut length = 0;
    let mut k = 0;
    while k < edit.parts.len() {
        let amount = match piece_len(&edit.parts[k]) {
            Ok(amount) => amount,
            Err(refusal) => return Err(refusal),
        };
        if amount > MAX_PAYLOAD_BYTES - length {
            return Err(Refusal::PayloadTooLarge);
        }
        length += amount;
        k += 1;
    }
    Ok(length)
}

/// Insert edit `i` into `order` (indices sorted by target start) before the
/// first index whose target starts at or after it: `List.orderedInsert` on
/// start positions, the step of the reference model's `sortAuthored`.
pub fn insert_by_start(edits: &[Edit], order: Vec<usize>, i: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut placed = false;
    let mut k = 0;
    while k < order.len() {
        if !placed && edits[i].target.lo <= edits[order[k]].target.lo {
            out.push(i);
            placed = true;
        }
        out.push(order[k]);
        k += 1;
    }
    if !placed {
        out.push(i);
    }
    out
}

/// Insertion sort of the edit indices by target start, last index first,
/// exactly as the reference model's `sortAuthored` folds from the right.
pub fn sort_by_start(edits: &[Edit]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    let mut i = edits.len();
    while i > 0 {
        i -= 1;
        order = insert_by_start(edits, order, i);
    }
    order
}

/// Two consecutive canonical targets: `left` ends at or before `right`
/// starts, and they may touch only when both are nonempty, so coincident
/// insertions and an insertion at either end of a replacement refuse.
pub fn compatible(left: &Cut, right: &Cut) -> bool {
    if left.hi > right.lo {
        return false;
    }
    if left.hi == right.lo && (left.lo == left.hi || right.lo == right.hi) {
        return false;
    }
    true
}

/// Every consecutive pair in `order` is compatible.
pub fn check_canonical(edits: &[Edit], order: &[usize]) -> Result<(), Refusal> {
    let mut k = 1;
    while k < order.len() {
        if !compatible(&edits[order[k - 1]].target, &edits[order[k]].target) {
            return Err(Refusal::Overlap);
        }
        k += 1;
    }
    Ok(())
}

/// Validate a batch against its original image and return the canonical
/// order (positions into `edits`). Refusal precedence: too many edits, then
/// each edit in authored order (target, copies, payload), then overlap.
pub fn prepare(
    input: &[u8],
    text: bool,
    capture: &[u8; CAPTURE_BYTES],
    edits: &[Edit],
) -> Result<Vec<usize>, Refusal> {
    if edits.len() > MAX_SPLICES {
        return Err(Refusal::TooManySplices);
    }
    let mut payload = 0;
    let mut i = 0;
    while i < edits.len() {
        match check_edit(input, text, capture, &edits[i]) {
            Ok(()) => {}
            Err(refusal) => return Err(refusal),
        }
        let amount = match replacement_len(&edits[i]) {
            Ok(amount) => amount,
            Err(refusal) => return Err(refusal),
        };
        if amount > MAX_PAYLOAD_BYTES - payload {
            return Err(Refusal::PayloadTooLarge);
        }
        payload += amount;
        i += 1;
    }
    let order = sort_by_start(edits);
    match check_canonical(edits, &order) {
        Ok(()) => Ok(order),
        Err(refusal) => Err(refusal),
    }
}

/// The replacement bytes of one edit: its parts in authored order, every
/// copy read from the immutable original `input`.
pub fn materialize(input: &[u8], edit: &Edit) -> Result<Vec<u8>, Refusal> {
    let mut out: Vec<u8> = Vec::new();
    let mut k = 0;
    while k < edit.parts.len() {
        match &edit.parts[k] {
            Piece::Literal(bytes) => {
                if bytes.len() > MAX_PAYLOAD_BYTES - out.len() {
                    return Err(Refusal::PayloadTooLarge);
                }
                out.extend_from_slice(bytes);
            }
            Piece::Copy(cut) => {
                if cut.lo > cut.hi || cut.hi > input.len() {
                    return Err(Refusal::OutOfBounds);
                }
                if cut.hi - cut.lo > MAX_PAYLOAD_BYTES - out.len() {
                    return Err(Refusal::PayloadTooLarge);
                }
                out.extend_from_slice(&input[cut.lo..cut.hi]);
            }
        }
        k += 1;
    }
    Ok(out)
}

/// Byte equality of two slices.
pub fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Render a prepared batch: copy each untouched gap of the original, then
/// each materialized replacement, in canonical `order`, then the final gap.
/// Every append is checked against `MAX_CANDIDATE_BYTES` before it happens,
/// so a refusal never follows a partial allocation beyond the cap and the
/// running length cannot overflow.
pub fn render(input: &[u8], edits: &[Edit], order: &[usize]) -> Result<Rendered, Refusal> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut replacements: Vec<Vec<u8>> = Vec::new();
    let mut cursor = 0;
    let mut k = 0;
    while k < order.len() {
        // Every index and coordinate is re-checked, so `render` is total
        // (never panics) whatever order it is handed; for an order from
        // `prepare` none of these refusals is reachable.
        if order[k] >= edits.len() {
            return Err(Refusal::OutOfBounds);
        }
        let edit = &edits[order[k]];
        if cursor > edit.target.lo
            || edit.target.lo > edit.target.hi
            || edit.target.hi > input.len()
        {
            return Err(Refusal::OutOfBounds);
        }
        if edit.target.lo - cursor > MAX_CANDIDATE_BYTES - bytes.len() {
            return Err(Refusal::CandidateTooLarge);
        }
        bytes.extend_from_slice(&input[cursor..edit.target.lo]);
        let replacement = match materialize(input, edit) {
            Ok(replacement) => replacement,
            Err(refusal) => return Err(refusal),
        };
        if replacement.len() > MAX_CANDIDATE_BYTES - bytes.len() {
            return Err(Refusal::CandidateTooLarge);
        }
        bytes.extend_from_slice(&replacement);
        replacements.push(replacement);
        cursor = edit.target.hi;
        k += 1;
    }
    if input.len() - cursor > MAX_CANDIDATE_BYTES - bytes.len() {
        return Err(Refusal::CandidateTooLarge);
    }
    bytes.extend_from_slice(&input[cursor..]);
    let no_change = same_bytes(&bytes, input);
    Ok(Rendered {
        bytes,
        replacements,
        no_change,
    })
}

/// Replace exactly `width` bytes at `lo`, refusing a cut outside `input`
/// (checked remainder, never a wrapping `lo + width`) or an output beyond
/// `MAX_CANDIDATE_BYTES`.
pub fn apply_at(input: &[u8], lo: usize, width: usize, replacement: &[u8]) -> Option<Vec<u8>> {
    if lo > input.len() {
        return None;
    }
    if width > input.len() - lo {
        return None;
    }
    if replacement.len() > MAX_CANDIDATE_BYTES {
        return None;
    }
    if input.len() - width > MAX_CANDIDATE_BYTES - replacement.len() {
        return None;
    }
    let mut output: Vec<u8> = Vec::new();
    output.extend_from_slice(&input[..lo]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&input[lo + width..]);
    Some(output)
}
