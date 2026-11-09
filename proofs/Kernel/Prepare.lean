import Kernel.Order

/-!
# Batch preparation

The theorem over `splice::prepare` (`prepare_spec`): every result is one of

- `Ok order` — the batch is admissible (at most `MAX_SPLICES` edits, every
  edit valid, total payload within `MAX_PAYLOAD_BYTES`, every pair of
  targets disjoint), `order` is a permutation of the edit positions, and the
  targets in that order are canonical (formal/Batch.lean's
  `CanonicalPositions`: each compatible with the next);
- a refusal whose reason holds of the batch: `TooManySplices` — more than
  `MAX_SPLICES` edits; `OutOfBounds`/`InvalidUtf8Boundary` — some edit is
  invalid (the latter only under the text profile); `PayloadTooLarge` — the
  total payload exceeds the cap; `Overlap` — every edit is valid and the
  payload fits, but two targets are not disjoint. `CandidateTooLarge` is
  never returned.

Hence `prepare` accepts exactly the admissible batches (`prepare_ok_iff`),
and acceptance does not depend on the authored order (`Admissible` is
invariant under permutation, `admissible_perm`).
-/

open Aeneas Aeneas.Std Result
open splicery_splice

namespace Splicery.Kernel

/-- Total replacement bytes of a batch. -/
def payloadSize (edits : List splice.Edit) : Nat := (edits.map (fun e => partsSize e.parts.val)).sum

/-- A batch the kernel must accept. -/
def Admissible (input : List U8) (text : Bool) (capture : List U8) (edits : List splice.Edit) :
    Prop :=
  edits.length ≤ 4096 ∧ (∀ e ∈ edits, EditValid input text capture e) ∧
  payloadSize edits ≤ 16777216 ∧ edits.Pairwise (fun a b => Disjoint a.target b.target)

/-- What `prepare` guarantees about its result. -/
def PrepareOutcome (input : List U8) (text : Bool) (capture : List U8) (edits : List splice.Edit) :
    core.result.Result (alloc.vec.Vec Usize) splice.Refusal → Prop
  | .Ok order => Admissible input text capture edits ∧
      (order.val.map (·.val)).Perm (List.range edits.length) ∧
      List.IsChain (fun a b : splice.Edit => Compatible a.target b.target) (pick edits order.val)
  | .Err .TooManySplices => 4096 < edits.length
  | .Err .OutOfBounds => edits.length ≤ 4096 ∧ ∃ e ∈ edits, ¬ EditValid input text capture e
  | .Err .InvalidUtf8Boundary =>
      edits.length ≤ 4096 ∧ text = true ∧ ∃ e ∈ edits, ¬ EditValid input text capture e
  | .Err .PayloadTooLarge => edits.length ≤ 4096 ∧ 16777216 < payloadSize edits
  | .Err .Overlap => edits.length ≤ 4096 ∧ (∀ e ∈ edits, EditValid input text capture e) ∧
      payloadSize edits ≤ 16777216 ∧ ¬ edits.Pairwise (fun a b => Disjoint a.target b.target)
  | .Err .CandidateTooLarge => False

/-! ## List facts -/

theorem filterMap_range_getElem? {α : Type} (l : List α) :
    (List.range l.length).filterMap (fun j => l[j]?) = l := by
  induction l with
  | nil => simp
  | cons a l ih =>
    rw [List.length_cons, List.range_succ_eq_map, List.filterMap_cons, List.filterMap_map]
    simp only [List.getElem?_cons_zero]
    congr 1

theorem pick_perm {edits : List splice.Edit} {order : List Usize}
    (h : (order.map (·.val)).Perm (List.range edits.length)) :
    (pick edits order).Perm edits := by
  have : pick edits order = (order.map (·.val)).filterMap (fun j => edits[j]?) := by
    simp [pick, List.filterMap_map, Function.comp_def]
  rw [this]
  have := h.filterMap (fun j => edits[j]?)
  rwa [filterMap_range_getElem?] at this

theorem mem_of_mem_pick {edits : List splice.Edit} {order : List Usize} {e : splice.Edit}
    (h : e ∈ pick edits order) : e ∈ edits := by
  obtain ⟨i, _, hi⟩ := List.mem_filterMap.mp h
  exact List.mem_of_getElem? hi

theorem pick_sorted {edits : List splice.Edit} {order : List Usize}
    (h : order.Pairwise (ByStart edits)) :
    (pick edits order).Pairwise (fun a b : splice.Edit => a.target.lo.val ≤ b.target.lo.val) := by
  unfold pick
  rw [List.pairwise_filterMap]
  refine h.imp ?_
  intro i j hij a ha b hb
  try simp only [Option.mem_def] at ha hb
  simpa [ByStart, startAt, ha, hb] using hij

theorem admissible_perm {input : List U8} {text : Bool} {capture : List U8}
    {xs ys : List splice.Edit} (h : xs.Perm ys) :
    Admissible input text capture xs ↔ Admissible input text capture ys := by
  have hsym : ∀ {a b : splice.Edit}, Disjoint a.target b.target → Disjoint b.target a.target :=
    fun hab => hab.symm
  have hpay : payloadSize xs = payloadSize ys := by
    unfold payloadSize
    exact (h.map _).sum_eq
  unfold Admissible
  rw [h.length_eq, hpay, h.pairwise_iff hsym]
  constructor
  · rintro ⟨h1, h2, h3, h4⟩; exact ⟨h1, fun e he => h2 e (h.mem_iff.mpr he), h3, h4⟩
  · rintro ⟨h1, h2, h3, h4⟩; exact ⟨h1, fun e he => h2 e (h.mem_iff.mp he), h3, h4⟩

theorem payloadSize_take_succ (edits : List splice.Edit) (k : Nat) (h : k < edits.length) :
    payloadSize (edits.take (k + 1)) = payloadSize (edits.take k) + partsSize edits[k].parts.val := by
  unfold payloadSize
  rw [List.take_add_one, List.getElem?_eq_getElem h, Option.toList_some, List.map_append,
    List.sum_append]
  simp

theorem payloadSize_take_le (edits : List splice.Edit) (k : Nat) :
    payloadSize (edits.take k) ≤ payloadSize edits := by
  unfold payloadSize
  rw [List.map_take]
  have h := congrArg List.sum (List.take_append_drop k (edits.map (fun e => partsSize e.parts.val)))
  rw [List.sum_append] at h
  omega

theorem partsSize_le_payloadSize (edits : List splice.Edit) (k : Nat) (h : k < edits.length) :
    payloadSize (edits.take k) + partsSize edits[k].parts.val ≤ payloadSize edits := by
  rw [← payloadSize_take_succ edits k h]
  exact payloadSize_take_le edits (k + 1)

/-! ## The validation loop -/

theorem prepare_loop_spec (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (edits : Slice splice.Edit) (payload i : Usize)
    (hn : edits.val.length ≤ 4096) (hi : i.val ≤ edits.val.length)
    (hvalid : ∀ j (h : j < edits.val.length), j < i.val →
      EditValid input.val text capture.val edits.val[j])
    (hpay : payload.val = payloadSize (edits.val.take i.val)) (hcap : payload.val ≤ 16777216) :
    splice.prepare_loop input text capture edits payload i ⦃ r =>
      PrepareOutcome input.val text capture.val edits.val r ⦄ := by
  unfold splice.prepare_loop
  apply loop.spec_decr_nat
    (fun s : Usize × Usize => edits.val.length - s.2.val)
    (fun s : Usize × Usize => s.2.val ≤ edits.val.length ∧
      (∀ j (h : j < edits.val.length), j < s.2.val →
        EditValid input.val text capture.val edits.val[j]) ∧
      s.1.val = payloadSize (edits.val.take s.2.val) ∧ s.1.val ≤ 16777216)
  · rintro ⟨payload, i⟩ ⟨hi, hvalid, hpay, hcap⟩
    dsimp only at hi hvalid hpay hcap
    unfold splice.prepare_loop.body
    dsimp only
    split_ifs with hlt
    · have hlt' : i.val < edits.val.length := by scalar_tac
      step as ⟨ e, he ⟩
      apply WP.spec_bind (check_edit_spec _ _ _ _)
      intro r ⟨hok, herr⟩
      cases r with
      | Err refusal =>
        simp only [WP.spec_ok]
        have hbad : ¬ EditValid input.val text capture.val e := fun hv => by
          have := hok.mpr hv; simp at this
        have hmem : e ∈ edits.val := by rw [he]; exact List.getElem_mem _
        rcases herr refusal rfl with rfl | ⟨rfl, htext⟩
        · exact ⟨hn, e, hmem, hbad⟩
        · exact ⟨hn, htext, e, hmem, hbad⟩
      | Ok u =>
        cases u
        simp only
        have hv : EditValid input.val text capture.val e := hok.mp rfl
        apply WP.spec_bind (replacement_len_spec e)
        intro r hr
        have hr := hr hv.ordered
        have hle := partsSize_le_payloadSize edits.val i.val hlt'
        rw [← he] at hle
        by_cases hsize : partsSize e.parts.val ≤ 16777216
        · obtain ⟨amount, rfl, hamount⟩ := hr.1 hsize
          simp only
          apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
          intro room hroom
          simp only [max_payload_val] at hroom
          split_ifs with hover
          · simp only [WP.spec_ok, PrepareOutcome]
            refine ⟨hn, ?_⟩
            scalar_tac
          · apply WP.spec_bind (Usize.add_spec (by scalar_tac))
            intro payload' hpayload'
            apply WP.spec_bind (Usize.add_spec (by scalar_tac))
            intro i' hi'
            simp only [WP.spec_ok]
            refine ⟨by scalar_tac, fun j hj hji => ?_, ?_, by scalar_tac, by scalar_tac⟩
            · by_cases hji' : j < i.val
              · exact hvalid j hj hji'
              · have : j = i.val := by scalar_tac
                subst this
                rw [← he]; exact hv
            · have : i'.val = i.val + 1 := by scalar_tac
              rw [this, payloadSize_take_succ _ _ hlt', ← he, ← hpay, ← hamount]
              scalar_tac
        · rw [hr.2 (by omega)]
          simp only [WP.spec_ok, PrepareOutcome]
          exact ⟨hn, by omega⟩
    · have hiv : i.val = edits.val.length := by scalar_tac
      have hall : ∀ e ∈ edits.val, EditValid input.val text capture.val e := by
        intro e hmem
        obtain ⟨j, hj, rfl⟩ := List.getElem_of_mem hmem
        exact hvalid j hj (by omega)
      have hpayall : payloadSize edits.val ≤ 16777216 := by
        rw [hiv, List.take_length] at hpay; omega
      apply WP.spec_bind (sort_by_start_spec edits)
      intro order ⟨hperm, hsorted⟩
      have hord : InRangeOrder edits.val order.val := by
        intro j hj
        have := hperm.mem_iff.mp (List.mem_map_of_mem (f := (·.val)) hj)
        simpa using this
      have hpick := pick_perm hperm
      have hpickval : ∀ e ∈ pick edits.val order.val, Ordered e.target :=
        fun e he => (hall e (mem_of_mem_pick he)).1.1.2.1
      have hiff := isChain_iff_pairwise_disjoint hpickval (pick_sorted hsorted)
      have hsym : ∀ {a b : splice.Edit}, Disjoint a.target b.target → Disjoint b.target a.target :=
        fun hab => hab.symm
      rw [hpick.pairwise_iff hsym] at hiff
      apply WP.spec_bind (check_canonical_spec edits _ (by simpa using hord))
      intro r ⟨hok, herr⟩
      simp only [deref_val] at hok
      cases r with
      | Ok u =>
        cases u
        simp only [WP.spec_ok, PrepareOutcome]
        have hchain := hok.mp rfl
        exact ⟨⟨hn, hall, hpayall, hiff.mp hchain⟩, hperm, hchain⟩
      | Err refusal =>
        simp only
        rw [herr refusal rfl]
        simp only [WP.spec_ok, PrepareOutcome]
        refine ⟨hn, hall, hpayall, fun hpair => ?_⟩
        have := hok.mpr (hiff.mpr hpair)
        simp at this
  · exact ⟨hi, hvalid, hpay, hcap⟩

/-! ## The theorem -/

/-- `prepare` returns an admissible canonical order or a refusal whose reason
holds of the batch. -/
theorem prepare_spec (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (edits : Slice splice.Edit) :
    splice.prepare input text capture edits ⦃ r =>
      PrepareOutcome input.val text capture.val edits.val r ⦄ := by
  unfold splice.prepare
  dsimp only
  have hmax : splice.MAX_SPLICES.val = 4096 := max_splices_val
  split_ifs with h
  · simp only [WP.spec_ok, PrepareOutcome]
    rw [gt_iff_lt, UScalar.lt_equiv, hmax, Slice.len_val] at h
    simpa using h
  · apply prepare_loop_spec
    · rw [gt_iff_lt, UScalar.lt_equiv, hmax, Slice.len_val] at h
      simpa using h
    · simp
    · intro j _ hj; simp at hj
    · simp [payloadSize]
    · simp

/-- Exactly the admissible batches are accepted. -/
theorem prepare_ok_iff {input : List U8} {text : Bool} {capture : List U8}
    {edits : List splice.Edit} {r : core.result.Result (alloc.vec.Vec Usize) splice.Refusal}
    (h : PrepareOutcome input text capture edits r) :
    (∃ order, r = .Ok order) ↔ Admissible input text capture edits := by
  constructor
  · rintro ⟨order, rfl⟩
    exact h.1
  · intro ha
    obtain ⟨hn, hall, hpay, hpair⟩ := ha
    match r, h with
    | .Ok order, _ => exact ⟨order, rfl⟩
    | .Err .TooManySplices, h => exact absurd hn (Nat.not_le.mpr h)
    | .Err .OutOfBounds, h => obtain ⟨_, e, he, hbad⟩ := h; exact absurd (hall e he) hbad
    | .Err .InvalidUtf8Boundary, h => obtain ⟨_, _, e, he, hbad⟩ := h; exact absurd (hall e he) hbad
    | .Err .PayloadTooLarge, h => exact (Nat.lt_irrefl _ (Nat.lt_of_lt_of_le h.2 hpay)).elim
    | .Err .Overlap, h => exact absurd hpair h.2.2.2
    | .Err .CandidateTooLarge, h => exact h.elim

/-- An accepted order names every edit exactly once. -/
theorem prepare_order_perm {input : List U8} {text : Bool} {capture : List U8}
    {edits : List splice.Edit} {order : alloc.vec.Vec Usize}
    (h : PrepareOutcome input text capture edits (.Ok order)) :
    (pick edits order.val).Perm edits := pick_perm h.2.1

end Splicery.Kernel
