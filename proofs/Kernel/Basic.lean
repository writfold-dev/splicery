import Extracted.SplicerySplice

/-!
# Shared definitions and library facts

The reference vocabulary the kernel theorems are stated in, over the Aeneas
translation of `crates/splicery-splice` (`Extracted/SplicerySplice.lean`):
byte lists, cut validity, piece/part denotations and the canonical-order
relation. `formal/` (the reference calculus, Lean v4.21.0) states the same
notions over its own `AuthoredSplice`; they are restated here rather than
imported because the two projects pin different Lean toolchains.
-/

open Aeneas Aeneas.Std Result
open splicery_splice

namespace Splicery.Kernel

/-! ## Library facts about the Aeneas models used by the kernel -/

@[simp] theorem deref_val {α : Type} (v : alloc.vec.Vec α) :
    (alloc.vec.Vec.deref v).val = v.val := by
  simp [alloc.vec.Vec.deref, Slice.from_val]

@[simp] theorem max_payload_val : splice.MAX_PAYLOAD_BYTES.val = 16777216 := by
  simp [splice.MAX_PAYLOAD_BYTES]

@[simp] theorem max_candidate_val : splice.MAX_CANDIDATE_BYTES.val = 16777216 := by
  simp [splice.MAX_CANDIDATE_BYTES]

@[simp] theorem max_splices_val : splice.MAX_SPLICES.val = 4096 := by
  simp [splice.MAX_SPLICES]

@[simp] theorem capture_bytes_val : splice.CAPTURE_BYTES.val = 32 := by
  simp [splice.CAPTURE_BYTES]

/-- `extend_from_slice` over bytes appends the slice, when the result fits. -/
theorem extend_spec (v : alloc.vec.Vec U8) (s : Slice U8)
    (h : v.val.length + s.val.length ≤ Usize.max) :
    alloc.vec.Vec.extend_from_slice core.clone.CloneU8 v s ⦃ w => w.val = v.val ++ s.val ⦄ := by
  unfold alloc.vec.Vec.extend_from_slice
  have h' : v.length + s.length ≤ Usize.max := by simpa using h
  simp only [h', ↓reduceDIte]
  apply WP.spec_bind (List.clone_spec (by intro x _; rfl))
  intro l hl
  simp [hl]

/-- The bytes `lo..hi` of a list (Aeneas's `List.slice`). -/
theorem slice_length (l : List U8) (lo hi : Nat) (h : hi ≤ l.length) :
    (l.slice lo hi).length = hi - lo := by
  simp [List.slice]
  omega

end Splicery.Kernel
