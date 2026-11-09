import Kernel.Basic

/-!
# Cut and edit validation

Theorems over `splice::same_capture`, `char_boundary`, `check_cut`,
`check_edit`, `piece_len` and `replacement_len`:

- a capture identity is compared byte for byte (`same_capture_spec`);
- a cut is accepted exactly when it names this image's capture, lies inside
  it (`lo ≤ hi ≤ len`, never a wrapped sum) and, under the text profile,
  starts and ends on character boundaries (`check_cut_spec`);
- an edit is accepted exactly when its target and every copied range are
  (`check_edit_spec`);
- a replacement's length is the sum of its literal lengths and copy widths,
  refused exactly when that sum exceeds `MAX_PAYLOAD_BYTES`
  (`replacement_len_spec`).
-/

open Aeneas Aeneas.Std Result
open splicery_splice

namespace Splicery.Kernel

/-! ## Reference vocabulary -/

/-- A UTF-8 continuation byte, `10xxxxxx`. -/
def Continuation (b : U8) : Prop := 128 ≤ b.val ∧ b.val ≤ 191

/-- A character boundary of well-formed UTF-8: the end, or a position whose
byte does not continue a character. -/
def Boundary (input : List U8) (i : Nat) : Prop :=
  i ≤ input.length ∧ ∀ h : i < input.length, ¬ Continuation input[i]

/-- A cut bound to this capture and inside the image (formal/Core.lean's
`cutRange` succeeds exactly when `lo ≤ hi ≤ len`). -/
def InRange (input capture : List U8) (c : splice.Cut) : Prop :=
  c.capture.val = capture ∧ c.lo.val ≤ c.hi.val ∧ c.hi.val ≤ input.length

/-- A cut the kernel accepts. -/
def CutValid (input : List U8) (text : Bool) (capture : List U8) (c : splice.Cut) : Prop :=
  InRange input capture c ∧ (text = true → Boundary input c.lo.val ∧ Boundary input c.hi.val)

/-- Literals are always valid; a copy must be a valid cut. -/
def PieceValid (input : List U8) (text : Bool) (capture : List U8) : splice.Piece → Prop
  | .Literal _ => True
  | .Copy c => CutValid input text capture c

/-- An edit the kernel accepts: its target and every copy are valid cuts. -/
def EditValid (input : List U8) (text : Bool) (capture : List U8) (e : splice.Edit) : Prop :=
  CutValid input text capture e.target ∧ ∀ p ∈ e.parts.val, PieceValid input text capture p

/-- Bytes a piece contributes to its replacement. -/
def pieceSize : splice.Piece → Nat
  | .Literal b => b.val.length
  | .Copy c => c.hi.val - c.lo.val

/-- Total bytes of a replacement. -/
def partsSize (ps : List splice.Piece) : Nat := (ps.map pieceSize).sum

/-- A copy is not reversed. -/
def PieceOrdered : splice.Piece → Prop
  | .Literal _ => True
  | .Copy c => c.lo.val ≤ c.hi.val

/-! ## Capture identity -/

theorem same_capture_loop_spec (a b : Array U8 32#usize) (i : Usize)
    (hi : i.val ≤ 32)
    (hprefix : ∀ j, j < i.val → a.val[j]? = b.val[j]?) :
    splice.same_capture_loop a b i ⦃ r => r ↔ a.val = b.val ⦄ := by
  unfold splice.same_capture_loop
  have ha := a.property
  have hb := b.property
  apply loop.spec_decr_nat (fun i : Usize => 32 - i.val)
    (fun i : Usize => i.val ≤ 32 ∧ ∀ j, j < i.val → a.val[j]? = b.val[j]?)
  · intro i ⟨hi, hprefix⟩
    unfold splice.same_capture_loop.body
    simp only [capture_bytes_val, UScalar.lt_equiv] at *
    split_ifs with hlt
    · step as ⟨ x, hx ⟩
      step as ⟨ y, hy ⟩
      split_ifs with hne
      · simp only [WP.spec_ok, Bool.false_eq_true, false_iff]
        intro heq
        rw [bne_iff_ne] at hne
        apply hne
        rw [hx, hy]
        exact List.getElem_of_eq heq _
      · rw [bne_iff_ne, not_not] at hne
        apply WP.spec_bind (Usize.add_spec (by scalar_tac))
        intro i' hi'
        simp only [WP.spec_ok]
        and_intros
        all_goals try scalar_tac
        intro j hj
        by_cases hji : j < i.val
        · exact hprefix j hji
        · have : j = i.val := by scalar_tac
          subst this
          rw [List.getElem?_eq_getElem (by scalar_tac), List.getElem?_eq_getElem (by scalar_tac),
            ← hx, ← hy, hne]
    · simp only [WP.spec_ok, true_iff]
      apply List.ext_getElem?
      intro j
      by_cases hj : j < 32
      · exact hprefix j (by scalar_tac)
      · rw [List.getElem?_eq_none (by scalar_tac), List.getElem?_eq_none (by scalar_tac)]
  · exact ⟨hi, hprefix⟩

/-- `same_capture` is byte equality of the two 32-byte identities. -/
theorem same_capture_spec (a b : Array U8 32#usize) :
    splice.same_capture a b ⦃ r => r ↔ a.val = b.val ⦄ := by
  unfold splice.same_capture
  apply same_capture_loop_spec
  · simp
  · intro j hj; simp at hj

/-! ## Character boundaries -/

theorem is_continuation_spec (b : U8) :
    splice.is_continuation b ⦃ r => r ↔ Continuation b ⦄ := by
  unfold splice.is_continuation Continuation
  split_ifs with h <;> simp_all <;> scalar_tac

theorem char_boundary_spec (input : Slice U8) (i : Usize) :
    splice.char_boundary input i ⦃ r => r ↔ Boundary input.val i.val ⦄ := by
  unfold splice.char_boundary Boundary
  dsimp only
  split_ifs with h1 h2
  · simp only [WP.spec_ok, true_iff]
    have : i.val = input.val.length := by scalar_tac
    refine ⟨by omega, fun h => by omega⟩
  · simp only [WP.spec_ok, Bool.false_eq_true, false_iff, not_and]
    intro h; scalar_tac
  · have hlt : i.val < input.val.length := by scalar_tac
    step as ⟨ x, hx ⟩
    apply WP.spec_bind (is_continuation_spec x)
    intro b hb
    simp only [WP.spec_ok, decide_not, Bool.not_eq_eq_eq_not, Bool.not_true]
    constructor
    · intro hf
      refine ⟨by omega, fun _ => ?_⟩
      rw [← hx]
      intro hc
      have := hb.mpr hc
      simp_all
    · rintro ⟨_, hnc⟩
      cases b
      · rfl
      · exact absurd (hx ▸ hb.mp rfl) (hnc hlt)

/-! ## Cuts -/

/-- A cut is accepted exactly when it is valid; otherwise the refusal says
whether it is out of range (or bound to another capture) or off a character
boundary under the text profile. -/
theorem check_cut_spec (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (cut : splice.Cut) :
    splice.check_cut input text capture cut ⦃ r =>
      (r = .Ok () ↔ CutValid input.val text capture.val cut) ∧
      (r = .Err .OutOfBounds ↔ ¬ InRange input.val capture.val cut) ∧
      (r = .Err .InvalidUtf8Boundary ↔
        (InRange input.val capture.val cut ∧ text = true ∧
          ¬ (Boundary input.val cut.lo.val ∧ Boundary input.val cut.hi.val))) ⦄ := by
  unfold splice.check_cut
  apply WP.spec_bind (same_capture_spec _ _)
  intro b hb
  by_cases hcap : cut.capture.val = capture.val
  · have hbt : b = true := hb.mpr hcap
    subst hbt
    simp only [↓reduceIte]
    split_ifs with hlo hhi htext
    · simp [CutValid, InRange]; scalar_tac
    · simp [CutValid, InRange]; scalar_tac
    · apply WP.spec_bind (char_boundary_spec _ _)
      intro b1 hb1
      have hin : InRange input.val capture.val cut := ⟨hcap, by scalar_tac, by scalar_tac⟩
      split_ifs with h1
      · apply WP.spec_bind (char_boundary_spec _ _)
        intro b2 hb2
        split_ifs with h2
        · simp_all [CutValid]
        · simp_all [CutValid]
      · simp_all [CutValid]
    · have hin : InRange input.val capture.val cut := ⟨hcap, by scalar_tac, by scalar_tac⟩
      simp_all [CutValid]
  · have hbf : b = false := by
      cases b
      · rfl
      · exact absurd (hb.mp rfl) hcap
    subst hbf
    simp [CutValid, InRange, hcap]

/-- Copy validation over the parts from position `k`. -/
theorem check_edit_loop_spec (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (parts : alloc.vec.Vec splice.Piece) (k : Usize) (hk : k.val ≤ parts.val.length)
    (hprefix : ∀ j (h : j < parts.val.length), j < k.val →
      PieceValid input.val text capture.val parts.val[j]) :
    splice.check_edit_loop input text capture parts k ⦃ r =>
      (r = .Ok () ↔ ∀ p ∈ parts.val, PieceValid input.val text capture.val p) ∧
      (∀ e, r = .Err e → e = .OutOfBounds ∨ (e = .InvalidUtf8Boundary ∧ text = true)) ⦄ := by
  unfold splice.check_edit_loop
  apply loop.spec_decr_nat (fun k : Usize => parts.val.length - k.val)
    (fun k : Usize => k.val ≤ parts.val.length ∧ ∀ j (h : j < parts.val.length), j < k.val →
      PieceValid input.val text capture.val parts.val[j])
  · intro k ⟨hk, hprefix⟩
    unfold splice.check_edit_loop.body
    simp only [alloc.vec.Vec.index_slice_index]
    split_ifs with hlt
    · have hlt' : k.val < parts.val.length := by scalar_tac
      step as ⟨ p, hp ⟩
      have hstep : ∀ j (h : j < parts.val.length), j < k.val + 1 →
          PieceValid input.val text capture.val p →
          PieceValid input.val text capture.val parts.val[j] := by
        intro j hj hjk hpv
        by_cases hjk' : j < k.val
        · exact hprefix j hj hjk'
        · have : j = k.val := by omega
          subst this; rw [← hp]; exact hpv
      cases hpc : p with
      | Literal b =>
        simp only
        apply WP.spec_bind (Usize.add_spec (by scalar_tac))
        intro k' hk'
        simp only [WP.spec_ok]
        refine ⟨by scalar_tac, fun j hj hjk => hstep j hj (by scalar_tac) (by simp [hpc, PieceValid]),
          by scalar_tac⟩
      | Copy c =>
        simp only
        apply WP.spec_bind (check_cut_spec _ _ _ _)
        intro r ⟨hok, hoob, hutf⟩
        cases r with
        | Ok u =>
          cases u
          simp only
          apply WP.spec_bind (Usize.add_spec (by scalar_tac))
          intro k' hk'
          simp only [WP.spec_ok]
          refine ⟨by scalar_tac, fun j hj hjk =>
            hstep j hj (by scalar_tac) (by simpa [hpc, PieceValid] using hok.mp rfl), by scalar_tac⟩
        | Err e =>
          simp only [WP.spec_ok]
          refine ⟨⟨by simp, fun hall => ?_⟩, ?_⟩
          · have hv : PieceValid input.val text capture.val p := by
              rw [hp]; exact hall _ (List.getElem_mem _)
            rw [hpc] at hv
            have := hok.mpr hv
            simp at this
          · intro e' he'
            cases he'
            cases e <;> simp_all [CutValid]
    · simp only [WP.spec_ok, true_iff, reduceCtorEq, false_implies, implies_true, and_true]
      intro p hmem
      obtain ⟨j, hj, rfl⟩ := List.getElem_of_mem hmem
      exact hprefix j hj (by scalar_tac)
  · exact ⟨hk, hprefix⟩

/-- An edit is accepted exactly when it is valid; a refusal is
`OutOfBounds`, or `InvalidUtf8Boundary` under the text profile. -/
theorem check_edit_spec (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (edit : splice.Edit) :
    splice.check_edit input text capture edit ⦃ r =>
      (r = .Ok () ↔ EditValid input.val text capture.val edit) ∧
      (∀ e, r = .Err e → e = .OutOfBounds ∨ (e = .InvalidUtf8Boundary ∧ text = true)) ⦄ := by
  unfold splice.check_edit
  apply WP.spec_bind (check_cut_spec _ _ _ _)
  intro r ⟨hok, hoob, hutf⟩
  cases r with
  | Ok u =>
    cases u
    simp only
    apply WP.spec_mono (check_edit_loop_spec _ _ _ _ _ (by simp) (by intro j _ hj; simp at hj))
    intro r ⟨h1, h2⟩
    refine ⟨?_, h2⟩
    rw [h1]
    simp only [EditValid]
    have := hok.mp rfl
    tauto
  | Err e =>
    simp only [WP.spec_ok]
    refine ⟨⟨by simp, fun hv => ?_⟩, ?_⟩
    · have := hok.mpr hv.1
      simp at this
    · intro e' he'
      cases he'
      cases e <;> simp_all [CutValid]

/-! ## Replacement length -/

theorem piece_len_spec (p : splice.Piece) :
    splice.piece_len p ⦃ r =>
      (PieceOrdered p → ∃ n : Usize, r = .Ok n ∧ n.val = pieceSize p) ∧
      (¬ PieceOrdered p → r = .Err .OutOfBounds) ⦄ := by
  unfold splice.piece_len
  cases p with
  | Literal b => simp [PieceOrdered, pieceSize]
  | Copy c =>
    simp only
    split_ifs with h
    · simp only [WP.spec_ok, PieceOrdered]
      refine ⟨fun h' => by scalar_tac, fun _ => by simp⟩
    · apply WP.spec_bind (Usize.sub_spec (by scalar_tac))
      intro n hn
      simp only [WP.spec_ok, PieceOrdered, pieceSize]
      refine ⟨fun _ => ⟨n, rfl, by scalar_tac⟩, fun h' => by scalar_tac⟩

theorem partsSize_take_succ (ps : List splice.Piece) (k : Nat) (h : k < ps.length) :
    partsSize (ps.take (k + 1)) = partsSize (ps.take k) + pieceSize ps[k] := by
  unfold partsSize
  rw [List.take_add_one, List.getElem?_eq_getElem h, Option.toList_some, List.map_append,
    List.sum_append]
  simp

theorem partsSize_take_le (ps : List splice.Piece) (k : Nat) :
    partsSize (ps.take k) ≤ partsSize ps := by
  unfold partsSize
  rw [List.map_take]
  have h := congrArg List.sum (List.take_append_drop k (ps.map pieceSize))
  rw [List.sum_append] at h
  omega

theorem replacement_len_loop_spec (edit : splice.Edit) (length k : Usize)
    (hk : k.val ≤ edit.parts.val.length)
    (hlen : length.val = partsSize (edit.parts.val.take k.val))
    (hcap : length.val ≤ 16777216) :
    splice.replacement_len_loop edit length k ⦃ r =>
      (∀ p ∈ edit.parts.val, PieceOrdered p) →
        ((partsSize edit.parts.val ≤ 16777216 →
            ∃ n : Usize, r = .Ok n ∧ n.val = partsSize edit.parts.val) ∧
          (16777216 < partsSize edit.parts.val → r = .Err .PayloadTooLarge)) ⦄ := by
  unfold splice.replacement_len_loop
  apply loop.spec_decr_nat
    (fun s : Usize × Usize => edit.parts.val.length - s.2.val)
    (fun s : Usize × Usize => s.2.val ≤ edit.parts.val.length ∧
      s.1.val = partsSize (edit.parts.val.take s.2.val) ∧ s.1.val ≤ 16777216)
  · rintro ⟨length, k⟩ ⟨hk, hlen, hcap⟩
    dsimp only at hk hlen hcap
    unfold splice.replacement_len_loop.body
    simp only [alloc.vec.Vec.index_slice_index]
    split_ifs with hlt
    · have hlt' : k.val < edit.parts.val.length := by scalar_tac
      step as ⟨ p, hp ⟩
      apply WP.spec_bind (piece_len_spec p)
      intro r ⟨hord, hunord⟩
      have hsucc := partsSize_take_succ edit.parts.val k.val hlt'
      rw [← hp] at hsucc
      by_cases hpo : PieceOrdered p
      · obtain ⟨n, rfl, hn⟩ := hord hpo
        simp only [max_payload_val]
        apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
        intro room hroom
        simp only [max_payload_val] at hroom
        split_ifs with hover
        · simp only [WP.spec_ok]
          intro _
          refine ⟨fun hle => ?_, fun _ => by simp⟩
          have := partsSize_take_le edit.parts.val (k.val + 1)
          scalar_tac
        · apply WP.spec_bind (Usize.add_spec (by scalar_tac))
          intro length' hlength'
          apply WP.spec_bind (Usize.add_spec (by scalar_tac))
          intro k' hk'
          simp only [WP.spec_ok]
          and_intros
          · scalar_tac
          · rw [hk', hlength', hlen, hn, ← hsucc]; simp
          · scalar_tac
          · scalar_tac
      · rw [hunord hpo]
        simp only [WP.spec_ok]
        intro hall
        exact absurd (hall p (by rw [hp]; exact List.getElem_mem _)) hpo
    · simp only [WP.spec_ok]
      have : k.val = edit.parts.val.length := by scalar_tac
      rw [this, List.take_length] at hlen
      intro _
      exact ⟨fun _ => ⟨length, rfl, hlen⟩, fun h => by omega⟩
  · exact ⟨hk, hlen, hcap⟩

/-- The replacement length is the sum of literal lengths and copy widths,
refused exactly when it exceeds `MAX_PAYLOAD_BYTES` (for unreversed copies,
which validation guarantees). -/
theorem replacement_len_spec (edit : splice.Edit) :
    splice.replacement_len edit ⦃ r =>
      (∀ p ∈ edit.parts.val, PieceOrdered p) →
        ((partsSize edit.parts.val ≤ 16777216 →
            ∃ n : Usize, r = .Ok n ∧ n.val = partsSize edit.parts.val) ∧
          (16777216 < partsSize edit.parts.val → r = .Err .PayloadTooLarge)) ⦄ := by
  unfold splice.replacement_len
  apply replacement_len_loop_spec
  · simp
  · simp [partsSize]
  · simp

/-- A valid edit's copies are unreversed. -/
theorem EditValid.ordered {input : List U8} {text : Bool} {capture : List U8} {e : splice.Edit}
    (h : EditValid input text capture e) : ∀ p ∈ e.parts.val, PieceOrdered p := by
  intro p hp
  have := h.2 p hp
  cases p with
  | Literal _ => trivial
  | Copy c => exact this.1.2.1

end Splicery.Kernel
