import Kernel.Validate

/-!
# Canonical order

Theorems over `splice::insert_by_start`, `sort_by_start`, `compatible` and
`check_canonical`:

- `insert_by_start` is `List.orderedInsert` by target start, the step of
  formal/Batch.lean's `sortAuthored` (`insert_by_start_spec`);
- `sort_by_start` returns a permutation of the edit positions sorted by
  target start (`sort_by_start_spec`);
- `compatible` decides formal/Batch.lean's `AuthoredCompatible` restated over
  cuts (`compatible_spec`), and `check_canonical` accepts exactly the orders
  whose consecutive targets are compatible (`check_canonical_spec`);
- for valid cuts sorted by start, consecutive compatibility is equivalent to
  pairwise disjointness of the whole batch, in any order
  (`isChain_iff_pairwise_disjoint`).
-/

open Aeneas Aeneas.Std Result
open splicery_splice

namespace Splicery.Kernel

/-! ## Reference vocabulary -/

/-- formal/Batch.lean's `AuthoredCompatible` over cuts (`stop = hi`,
`width ≠ 0 ↔ lo ≠ hi`): `a` ends at or before `b` starts, and they touch only
when both are nonempty. -/
def Compatible (a b : splice.Cut) : Prop :=
  a.hi.val ≤ b.lo.val ∧ (a.hi.val = b.lo.val → a.lo.val ≠ a.hi.val ∧ b.lo.val ≠ b.hi.val)

/-- Two targets that may appear in one batch, in some order. -/
def Disjoint (a b : splice.Cut) : Prop := Compatible a b ∨ Compatible b a

/-- The target start of position `i`, or 0 past the end. -/
def startAt (edits : List splice.Edit) (i : Usize) : Nat :=
  match edits[i.val]? with
  | some e => e.target.lo.val
  | none => 0

/-- Order positions by target start. -/
def ByStart (edits : List splice.Edit) (i j : Usize) : Prop := startAt edits i ≤ startAt edits j

instance (edits : List splice.Edit) : DecidableRel (ByStart edits) := fun i j =>
  inferInstanceAs (Decidable (startAt edits i ≤ startAt edits j))

instance (edits : List splice.Edit) : Std.Total (ByStart edits) :=
  ⟨fun i j => Nat.le_total _ _⟩

instance (edits : List splice.Edit) : IsTrans Usize (ByStart edits) :=
  ⟨fun _ _ _ h1 h2 => Nat.le_trans h1 h2⟩

/-- The edits named by an order, in that order. -/
def pick (edits : List splice.Edit) (order : List Usize) : List splice.Edit :=
  order.filterMap (fun i => edits[i.val]?)

theorem startAt_of_lt {edits : List splice.Edit} {i : Usize} (h : i.val < edits.length) :
    startAt edits i = edits[i.val].target.lo.val := by
  simp [startAt, List.getElem?_eq_getElem h]

/-! ## Insertion -/

theorem insert_by_start_loop_spec (edits : Slice splice.Edit) (order : alloc.vec.Vec Usize)
    (i : Usize) (out : alloc.vec.Vec Usize) (placed : Bool) (k : Usize)
    (hi : i.val < edits.val.length) (hord : ∀ j ∈ order.val, j.val < edits.val.length)
    (hlen : order.val.length < Usize.max) (hk : k.val ≤ order.val.length)
    (hout : out.val ++ (if placed then order.val.drop k.val
        else (order.val.drop k.val).orderedInsert (ByStart edits.val) i) =
      order.val.orderedInsert (ByStart edits.val) i)
    (houtlen : out.val.length = k.val + (if placed then 1 else 0)) :
    splice.insert_by_start_loop edits order i out placed k ⦃ out' placed' =>
      out'.val ++ (if placed' then [] else [i]) = order.val.orderedInsert (ByStart edits.val) i ∧
      out'.val.length = order.val.length + (if placed' then 1 else 0) ⦄ := by
  unfold splice.insert_by_start_loop
  apply loop.spec_decr_nat
    (fun s : alloc.vec.Vec Usize × Bool × Usize => order.val.length - s.2.2.val)
    (fun s : alloc.vec.Vec Usize × Bool × Usize => s.2.2.val ≤ order.val.length ∧
      s.1.val ++ (if s.2.1 then order.val.drop s.2.2.val
        else (order.val.drop s.2.2.val).orderedInsert (ByStart edits.val) i) =
        order.val.orderedInsert (ByStart edits.val) i ∧
      s.1.val.length = s.2.2.val + (if s.2.1 then 1 else 0))
  · rintro ⟨out, placed, k⟩ ⟨hk, hout, houtlen⟩
    dsimp only at hk hout houtlen
    unfold splice.insert_by_start_loop.body
    simp only [alloc.vec.Vec.index_slice_index]
    by_cases hlt' : k.val < order.val.length
    · have hlt : k < alloc.vec.Vec.len order := by scalar_tac
      simp only [hlt, ↓reduceIte]
      have hdrop : order.val.drop k.val = order.val[k.val] :: order.val.drop (k.val + 1) :=
        List.drop_eq_getElem_cons hlt'
      have hb := hord _ (List.getElem_mem hlt')
      cases placed
      · simp only [Bool.false_eq_true, ↓reduceIte] at hout houtlen ⊢
        step as ⟨ e, he ⟩
        step as ⟨ b, hb' ⟩
        have hb'' : b.val < edits.val.length := by rw [hb']; exact hb
        apply WP.spec_bind (Slice.index_usize_spec edits b (by simpa using hb''))
        intro e1 he1
        have hrel : (e.target.lo ≤ e1.target.lo) ↔ ByStart edits.val i b := by
          simp only [ByStart, startAt_of_lt hi, startAt_of_lt hb'', he, he1, UScalar.le_equiv]
        split_ifs with hle
        · step with alloc.vec.Vec.push_spec as ⟨ out1, hout1 ⟩
          step as ⟨ b2, hb2 ⟩
          apply WP.spec_bind (alloc.vec.Vec.push_spec out1 b2 (by simp [hout1, houtlen]; scalar_tac))
          intro out2 hout2
          apply WP.spec_bind (Usize.add_spec (by scalar_tac))
          intro k' hk'
          simp only [↓reduceIte, WP.spec_ok]
          refine ⟨by scalar_tac, ?_, ?_, by scalar_tac⟩
          · rw [hout2, hout1, hb2, ← hout, hdrop]
            have hy : ByStart edits.val i order.val[k.val] := hb' ▸ hrel.mp hle
            have : k'.val = k.val + 1 := by scalar_tac
            (simp [List.orderedInsert_cons, hy, this]) <;> first | rfl | exact hdrop
          · simp [hout2, hout1, houtlen]; scalar_tac
        · step as ⟨ b2, hb2 ⟩
          step as ⟨ out2, hout2 ⟩
          step as ⟨ k', hk' ⟩
          simp only [Bool.false_eq_true, ↓reduceIte]
          refine ⟨by scalar_tac, ?_, ?_, by scalar_tac⟩
          · rw [hout2, hb2, ← hout, hdrop]
            have hn : ¬ ByStart edits.val i order.val[k.val] := fun h => hle (hrel.mpr (hb' ▸ h))
            have : k'.val = k.val + 1 := by scalar_tac
            (simp [List.orderedInsert_cons, hn, this]) <;> first | rfl | exact hdrop
          · simp [hout2, houtlen]; scalar_tac
      · simp only [↓reduceIte] at hout houtlen ⊢
        step as ⟨ b2, hb2 ⟩
        step as ⟨ out2, hout2 ⟩
        step as ⟨ k', hk' ⟩
        simp only [↓reduceIte]
        refine ⟨by scalar_tac, ?_, ?_, by scalar_tac⟩
        · rw [hout2, hb2, ← hout, hdrop]
          have : k'.val = k.val + 1 := by scalar_tac
          (simp [this]) <;> first | rfl | exact hdrop
        · simp [hout2, houtlen]; scalar_tac
    · have hlt : ¬ (k < alloc.vec.Vec.len order) := by scalar_tac
      simp only [hlt, ↓reduceIte, WP.spec_ok]
      have hkl : k.val = order.val.length := by omega
      rw [hkl, List.drop_length] at hout
      rw [hkl] at houtlen
      cases placed <;> simp_all
  · exact ⟨hk, hout, houtlen⟩

/-- `insert_by_start` is `List.orderedInsert` by target start. -/
theorem insert_by_start_spec (edits : Slice splice.Edit) (order : alloc.vec.Vec Usize)
    (i : Usize) (hi : i.val < edits.val.length)
    (hord : ∀ j ∈ order.val, j.val < edits.val.length)
    (hlen : order.val.length < Usize.max) :
    splice.insert_by_start edits order i ⦃ out =>
      out.val = order.val.orderedInsert (ByStart edits.val) i ⦄ := by
  unfold splice.insert_by_start
  apply WP.spec_bind (insert_by_start_loop_spec edits order i _ false 0#usize hi hord hlen
    (by simp) (by simp) (by simp))
  rintro ⟨out, placed⟩ ⟨hout, houtlen⟩
  cases placed
  · simp only [Bool.false_eq_true, ↓reduceIte] at hout houtlen ⊢
    apply WP.spec_mono (alloc.vec.Vec.push_spec out i (by scalar_tac))
    intro v hv
    rw [hv, hout]
  · simp_all

/-! ## Sorting -/

theorem sort_by_start_loop_spec (edits : Slice splice.Edit) (order : alloc.vec.Vec Usize)
    (i : Usize) (hi : i.val ≤ edits.val.length)
    (hperm : (order.val.map (·.val)).Perm (List.range' i.val (edits.val.length - i.val)))
    (hsorted : order.val.Pairwise (ByStart edits.val)) :
    splice.sort_by_start_loop edits order i ⦃ out =>
      (out.val.map (·.val)).Perm (List.range edits.val.length) ∧
      out.val.Pairwise (ByStart edits.val) ⦄ := by
  unfold splice.sort_by_start_loop
  have hmax := edits.property
  apply loop.spec_decr_nat
    (fun s : alloc.vec.Vec Usize × Usize => s.2.val)
    (fun s : alloc.vec.Vec Usize × Usize => s.2.val ≤ edits.val.length ∧
      (s.1.val.map (·.val)).Perm (List.range' s.2.val (edits.val.length - s.2.val)) ∧
      s.1.val.Pairwise (ByStart edits.val))
  · rintro ⟨order, i⟩ ⟨hi, hperm, hsorted⟩
    dsimp only at hi hperm hsorted
    unfold splice.sort_by_start_loop.body
    dsimp only
    split_ifs with hpos
    · step as ⟨ i', hi' ⟩
      have hlen := hperm.length_eq
      simp only [List.length_map, List.length_range'] at hlen
      have hord : ∀ j ∈ order.val, j.val < edits.val.length := by
        intro j hj
        have := hperm.mem_iff.mp (List.mem_map_of_mem (f := (·.val)) hj)
        simp [List.mem_range'] at this
        omega
      apply WP.spec_bind (insert_by_start_spec edits order i' (by scalar_tac) hord (by scalar_tac))
      intro out hout
      simp only [WP.spec_ok]
      refine ⟨by scalar_tac, ?_, ?_, by scalar_tac⟩
      · rw [hout]
        refine ((List.perm_orderedInsert (ByStart edits.val) i' order.val).map (·.val)).trans ?_
        have : edits.val.length - i'.val = (edits.val.length - i.val) + 1 := by scalar_tac
        rw [this, List.range'_succ]
        simp only [List.map_cons]
        have : i'.val + 1 = i.val := by scalar_tac
        rw [this]
        exact List.Perm.cons _ hperm
      · rw [hout]; exact hsorted.orderedInsert _ _
    · simp only [WP.spec_ok]
      have : i.val = 0 := by scalar_tac
      rw [this] at hperm
      refine ⟨?_, hsorted⟩
      rw [List.range_eq_range']
      simpa using hperm
  · exact ⟨hi, hperm, hsorted⟩

/-- `sort_by_start` returns every edit position exactly once, sorted by
target start. -/
theorem sort_by_start_spec (edits : Slice splice.Edit) :
    splice.sort_by_start edits ⦃ order =>
      (order.val.map (·.val)).Perm (List.range edits.val.length) ∧
      order.val.Pairwise (ByStart edits.val) ⦄ := by
  unfold splice.sort_by_start
  apply sort_by_start_loop_spec
  · simp
  · simp
  · simp

/-! ## Compatibility -/

/-- `compatible` decides `Compatible`. -/
theorem compatible_spec (a b : splice.Cut) :
    splice.compatible a b ⦃ r => r ↔ Compatible a b ⦄ := by
  unfold splice.compatible Compatible
  split_ifs <;> simp_all <;> scalar_tac

/-- Every index of an order is in range. -/
def InRangeOrder (edits : List splice.Edit) (order : List Usize) : Prop :=
  ∀ j ∈ order, j.val < edits.length

theorem pick_length {edits : List splice.Edit} {order : List Usize}
    (h : InRangeOrder edits order) : (pick edits order).length = order.length := by
  induction order with
  | nil => simp [pick]
  | cons j rest ih =>
    have hj := h j (by simp)
    have := ih (fun x hx => h x (by simp [hx]))
    simp only [pick] at this
    simp only [pick, List.filterMap_cons, List.getElem?_eq_getElem hj, List.length_cons]
    omega

theorem pick_getElem {edits : List splice.Edit} {order : List Usize}
    (h : InRangeOrder edits order) (k : Nat) (hk : k < order.length) :
    (pick edits order)[k]'(by rw [pick_length h]; exact hk) =
      edits[order[k].val]'(h _ (List.getElem_mem hk)) := by
  induction order generalizing k with
  | nil => simp at hk
  | cons j rest ih =>
    have hj := h j (by simp)
    have hrest : InRangeOrder edits rest := fun x hx => h x (by simp [hx])
    cases k with
    | zero => simp [pick, List.filterMap_cons, List.getElem?_eq_getElem hj]
    | succ k =>
      have := ih hrest k (by simpa using hk)
      simp only [pick, List.filterMap_cons, List.getElem?_eq_getElem hj] at this ⊢
      simpa using this

theorem check_canonical_loop_spec (edits : Slice splice.Edit) (order : Slice Usize) (k : Usize)
    (hord : InRangeOrder edits.val order.val) (hk : 1 ≤ k.val)
    (hprefix : ∀ j (hj : j + 1 < order.val.length), j + 1 < k.val →
      Compatible (edits.val[order.val[j].val]'(hord _ (List.getElem_mem (by omega)))).target
        (edits.val[order.val[j + 1].val]'(hord _ (List.getElem_mem hj))).target) :
    splice.check_canonical_loop edits order k ⦃ r =>
      (r = .Ok () ↔ List.IsChain (fun a b : splice.Edit => Compatible a.target b.target)
        (pick edits.val order.val)) ∧
      (∀ e, r = .Err e → e = .Overlap) ⦄ := by
  unfold splice.check_canonical_loop
  apply loop.spec_decr_nat (fun k : Usize => order.val.length + 1 - k.val)
    (fun k : Usize => 1 ≤ k.val ∧ ∀ j (hj : j + 1 < order.val.length), j + 1 < k.val →
      Compatible (edits.val[order.val[j].val]'(hord _ (List.getElem_mem (by omega)))).target
        (edits.val[order.val[j + 1].val]'(hord _ (List.getElem_mem hj))).target)
  · intro k ⟨hk, hprefix⟩
    unfold splice.check_canonical_loop.body
    dsimp only
    split_ifs with hlt
    · have hlt' : k.val < order.val.length := by scalar_tac
      step as ⟨ k1, hk1 ⟩
      step as ⟨ a, ha ⟩
      have ha' : a.val < edits.val.length := by rw [ha]; exact hord _ (List.getElem_mem _)
      step as ⟨ ea, hea ⟩
      step as ⟨ b, hb ⟩
      have hb' : b.val < edits.val.length := by rw [hb]; exact hord _ (List.getElem_mem _)
      step as ⟨ eb, heb ⟩
      apply WP.spec_bind (compatible_spec _ _)
      intro c hc
      have hidx : k1.val + 1 = k.val := by scalar_tac
      split_ifs with hcomp
      · apply WP.spec_bind (Usize.add_spec (by scalar_tac))
        intro k' hk'
        simp only [WP.spec_ok]
        refine ⟨by scalar_tac, fun j hj hjk => ?_, by scalar_tac⟩
        by_cases hjk' : j + 1 < k.val
        · exact hprefix j hj hjk'
        · have hjv : j = k1.val := by scalar_tac
          subst hjv
          have := hc.mp hcomp
          simp only [hea, heb, ha, hb] at this
          simpa [hidx] using this
      · simp only [WP.spec_ok, reduceCtorEq, false_iff, core.result.Result.Err.injEq]
        refine ⟨fun hchain => hcomp (hc.mpr ?_), fun e he => he.symm⟩
        have hpl := pick_length hord
        have := List.isChain_iff_getElem.mp hchain k1.val (by rw [hpl]; omega)
        rw [pick_getElem hord, pick_getElem hord] at this
        subst hea heb ha hb
        simp only [← hidx]
        exact this
    · simp only [WP.spec_ok, true_iff, reduceCtorEq, false_implies, implies_true, and_true]
      rw [List.isChain_iff_getElem]
      intro j hj
      have hpl := pick_length hord
      rw [pick_getElem hord, pick_getElem hord]
      exact hprefix j (by rw [hpl] at hj; exact hj) (by rw [hpl] at hj; scalar_tac)
  · exact ⟨hk, hprefix⟩

/-- `check_canonical` accepts exactly the orders whose consecutive targets
are compatible, and refuses with `Overlap` otherwise. -/
theorem check_canonical_spec (edits : Slice splice.Edit) (order : Slice Usize)
    (hord : InRangeOrder edits.val order.val) :
    splice.check_canonical edits order ⦃ r =>
      (r = .Ok () ↔ List.IsChain (fun a b : splice.Edit => Compatible a.target b.target)
        (pick edits.val order.val)) ∧
      (∀ e, r = .Err e → e = .Overlap) ⦄ := by
  unfold splice.check_canonical
  apply check_canonical_loop_spec edits order _ hord (by simp)
  intro j hj hjk
  simp at hjk

/-! ## Consecutive compatibility versus pairwise disjointness -/

/-- A cut is not reversed. -/
def Ordered (c : splice.Cut) : Prop := c.lo.val ≤ c.hi.val

theorem Compatible.trans {a b c : splice.Cut} (hb : Ordered b) (hab : Compatible a b)
    (hbc : Compatible b c) : Compatible a c := by
  unfold Compatible Ordered at *
  omega

theorem Compatible.not_rev {a b : splice.Cut} (ha : Ordered a) (hle : a.lo.val ≤ b.lo.val)
    (hba : Compatible b a) (hb : Ordered b) : False := by
  unfold Compatible Ordered at *
  omega

theorem pairwise_of_isChain {l : List splice.Edit} (hval : ∀ e ∈ l, Ordered e.target)
    (h : List.IsChain (fun a b : splice.Edit => Compatible a.target b.target) l) :
    l.Pairwise (fun a b : splice.Edit => Compatible a.target b.target) := by
  induction l with
  | nil => exact .nil
  | cons a rest ih =>
    have hrest : List.IsChain (fun a b : splice.Edit => Compatible a.target b.target) rest := by
      simpa using h.tail
    have ihr := ih (fun e he => hval e (by simp [he])) hrest
    refine List.pairwise_cons.mpr ⟨?_, ihr⟩
    cases rest with
    | nil => simp
    | cons b rest' =>
      have hab : Compatible a.target b.target := (List.isChain_cons_cons.mp h).1
      intro c hc
      rcases List.mem_cons.mp hc with rfl | hc'
      · exact hab
      · exact Compatible.trans (hval b (by simp)) hab (List.rel_of_pairwise_cons ihr hc')

/-- For valid cuts sorted by start, consecutive compatibility of the sorted
list is equivalent to pairwise disjointness. -/
theorem isChain_iff_pairwise_disjoint {l : List splice.Edit}
    (hval : ∀ e ∈ l, Ordered e.target)
    (hsorted : l.Pairwise (fun a b : splice.Edit => a.target.lo.val ≤ b.target.lo.val)) :
    List.IsChain (fun a b : splice.Edit => Compatible a.target b.target) l ↔
      l.Pairwise (fun a b : splice.Edit => Disjoint a.target b.target) := by
  constructor
  · intro h
    exact (pairwise_of_isChain hval h).imp (fun h => Or.inl h)
  · intro h
    have both := h.and hsorted
    have hc := List.Pairwise.isChain both
    refine List.IsChain.imp_of_mem_imp ?_ hc
    intro a b ha hb ⟨hdis, hle⟩
    rcases hdis with hab | hba
    · exact hab
    · exact (Compatible.not_rev (hval a ha) hle hba (hval b hb)).elim

end Splicery.Kernel
