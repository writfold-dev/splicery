import Kernel.Prepare

/-!
# Rendering

Theorems over `splice::materialize`, `same_bytes`, `render` and `apply_at`:

- a replacement is its parts in authored order, every copy read from the
  original image (`materialize_spec`);
- `render` of a prepared batch is the original with each target replaced by
  its replacement, in canonical order — every untouched gap copied from the
  original — and its reported replacements are exactly those bytes; it
  refuses, with `CandidateTooLarge` only, exactly when that output exceeds
  `MAX_CANDIDATE_BYTES`; `no_change` holds exactly when the output equals the
  input (`render_spec`, `prepare_render`);
- an empty batch renders the original unchanged (`render_empty`);
- the output length is the input length minus every target width plus every
  replacement length (`spliced_length`);
- `apply_at` replaces exactly `width` bytes at `lo` or refuses, and refuses
  exactly the invalid cuts and oversized outputs (`apply_at_spec`); a
  one-edit batch renders what `apply_at` produces (`spliced_single`).
-/

open Aeneas Aeneas.Std Result
open splicery_splice

namespace Splicery.Kernel

/-! ## Reference denotation -/

/-- The bytes a piece denotes: a literal, or a range of the original. -/
def pieceBytes (input : List U8) : splice.Piece → List U8
  | .Literal b => b.val
  | .Copy c => input.slice c.lo.val c.hi.val

/-- A replacement: its pieces' bytes in authored order. -/
def partsBytes (input : List U8) (ps : List splice.Piece) : List U8 :=
  (ps.map (pieceBytes input)).flatten

/-- The original from `cursor` with each edit's target replaced by its
replacement, in list order: the gap before each target, then its
replacement, then the rest (formal/Batch.lean's `Batch.apply` over gaps). -/
def emit (input : List U8) : Nat → List splice.Edit → List U8
  | c, [] => input.drop c
  | c, e :: rest =>
      input.slice c e.target.lo.val ++ partsBytes input e.parts.val ++ emit input e.target.hi.val rest

/-- The candidate a canonical batch denotes. -/
def spliced (input : List U8) (sorted : List splice.Edit) : List U8 := emit input 0 sorted

/-- A copy inside the image. -/
def CopyIn (input : List U8) : splice.Piece → Prop
  | .Literal _ => True
  | .Copy c => c.lo.val ≤ c.hi.val ∧ c.hi.val ≤ input.length

/-- What `render` needs of the order it is handed; `prepare` establishes it
(`prepared_ready`). -/
def RenderReady (input : List U8) (edits : List splice.Edit) (order : List Usize) : Prop :=
  InRangeOrder edits order ∧
  (∀ e ∈ pick edits order, e.target.lo.val ≤ e.target.hi.val ∧ e.target.hi.val ≤ input.length ∧
    (∀ p ∈ e.parts.val, CopyIn input p) ∧ partsSize e.parts.val ≤ 16777216) ∧
  List.IsChain (fun a b : splice.Edit => a.target.hi.val ≤ b.target.lo.val) (pick edits order)

theorem pieceBytes_length {input : List U8} {p : splice.Piece} (h : CopyIn input p) :
    (pieceBytes input p).length = pieceSize p := by
  cases p with
  | Literal b => simp [pieceBytes, pieceSize]
  | Copy c =>
    simp only [CopyIn] at h
    simp [pieceBytes, pieceSize, List.slice]
    omega

theorem partsBytes_length {input : List U8} {ps : List splice.Piece}
    (h : ∀ p ∈ ps, CopyIn input p) : (partsBytes input ps).length = partsSize ps := by
  induction ps with
  | nil => simp [partsBytes, partsSize]
  | cons p rest ih =>
    have hp := pieceBytes_length (h p (by simp))
    have := ih (fun q hq => h q (by simp [hq]))
    simp only [partsBytes, partsSize, List.map_cons, List.flatten_cons, List.length_append,
      List.sum_cons] at this ⊢
    omega

theorem partsBytes_take_succ (input : List U8) (ps : List splice.Piece) (k : Nat)
    (h : k < ps.length) :
    partsBytes input (ps.take (k + 1)) = partsBytes input (ps.take k) ++ pieceBytes input ps[k] := by
  unfold partsBytes
  rw [List.take_add_one, List.getElem?_eq_getElem h, Option.toList_some, List.map_append,
    List.flatten_append]
  simp

/-! ## Materialization -/

theorem materialize_loop_spec (input : Slice U8) (edit : splice.Edit) (out : alloc.vec.Vec U8)
    (k : Usize) (hcopy : ∀ p ∈ edit.parts.val, CopyIn input.val p)
    (hsize : partsSize edit.parts.val ≤ 16777216) (hk : k.val ≤ edit.parts.val.length)
    (hout : out.val = partsBytes input.val (edit.parts.val.take k.val)) :
    splice.materialize_loop input edit out k ⦃ r =>
      ∃ v, r = .Ok v ∧ v.val = partsBytes input.val edit.parts.val ⦄ := by
  unfold splice.materialize_loop
  have hlenof : ∀ k, (partsBytes input.val (edit.parts.val.take k)).length ≤ 16777216 := by
    intro k
    rw [partsBytes_length (fun p hp => hcopy p (List.mem_of_mem_take hp))]
    exact Nat.le_trans (partsSize_take_le _ _) hsize
  apply loop.spec_decr_nat
    (fun s : alloc.vec.Vec U8 × Usize => edit.parts.val.length - s.2.val)
    (fun s : alloc.vec.Vec U8 × Usize => s.2.val ≤ edit.parts.val.length ∧
      s.1.val = partsBytes input.val (edit.parts.val.take s.2.val))
  · rintro ⟨out, k⟩ ⟨hk, hout⟩
    dsimp only at hk hout
    unfold splice.materialize_loop.body
    simp only [alloc.vec.Vec.index_slice_index]
    try dsimp only
    split_ifs with hlt
    · have hlt' : k.val < edit.parts.val.length := by scalar_tac
      step as ⟨ p, hp ⟩
      have hnext := partsBytes_take_succ input.val edit.parts.val k.val hlt'
      rw [← hp] at hnext
      have hfit := hlenof (k.val + 1)
      rw [hnext, List.length_append, ← hout] at hfit
      have hpin : CopyIn input.val p := hcopy p (by rw [hp]; exact List.getElem_mem _)
      cases hpc : p with
      | Literal b =>
        simp only [hpc, pieceBytes] at hfit hnext
        try simp only [max_payload_val]
        apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
        intro room hroom
        simp only [max_payload_val] at hroom
        split_ifs with hover
        · exfalso; scalar_tac
        · apply WP.spec_bind (extend_spec out (alloc.vec.Vec.deref b) (by simp; scalar_tac))
          intro out1 hout1
          apply WP.spec_bind (Usize.add_spec (by scalar_tac))
          intro k' hk'
          simp only [WP.spec_ok]
          refine ⟨by scalar_tac, ?_, by scalar_tac⟩
          have : k'.val = k.val + 1 := by scalar_tac
          rw [this, hnext, hout1, hout]
          simp
      | Copy c =>
        rw [hpc] at hpin
        simp only [CopyIn] at hpin
        simp only [hpc, pieceBytes] at hfit hnext
        try simp only [max_payload_val]
        split_ifs with h1 h2
        · exfalso; scalar_tac
        · exfalso; scalar_tac
        · apply WP.spec_bind (Usize.sub_spec (by scalar_tac))
          intro w hw
          apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
          intro room hroom
          simp only [max_payload_val] at hroom
          have hslen := slice_length input.val c.lo.val c.hi.val hpin.2
          split_ifs with hover
          · exfalso; scalar_tac
          · step as ⟨ s, hs, hslen' ⟩
            apply WP.spec_bind (extend_spec out s (by scalar_tac))
            intro out1 hout1
            apply WP.spec_bind (Usize.add_spec (by scalar_tac))
            intro k' hk'
            simp only [WP.spec_ok]
            refine ⟨by scalar_tac, ?_, by scalar_tac⟩
            have : k'.val = k.val + 1 := by scalar_tac
            rw [this, hnext, hout1, hout, hs]
    · simp only [WP.spec_ok]
      have : k.val = edit.parts.val.length := by scalar_tac
      rw [this, List.take_length] at hout
      exact ⟨out, rfl, hout⟩
  · exact ⟨hk, hout⟩

/-- A replacement is its parts in authored order, copies read from the
original image. -/
theorem materialize_spec (input : Slice U8) (edit : splice.Edit)
    (hcopy : ∀ p ∈ edit.parts.val, CopyIn input.val p)
    (hsize : partsSize edit.parts.val ≤ 16777216) :
    splice.materialize input edit ⦃ r =>
      ∃ v, r = .Ok v ∧ v.val = partsBytes input.val edit.parts.val ⦄ := by
  unfold splice.materialize
  apply materialize_loop_spec _ _ _ _ hcopy hsize (by simp)
  simp [partsBytes]

/-! ## Byte equality -/

theorem same_bytes_loop_spec (a b : Slice U8) (i : Usize)
    (hlen : a.val.length = b.val.length) (hi : i.val ≤ a.val.length)
    (hprefix : ∀ j, j < i.val → a.val[j]? = b.val[j]?) :
    splice.same_bytes_loop a b i ⦃ r => r ↔ a.val = b.val ⦄ := by
  unfold splice.same_bytes_loop
  apply loop.spec_decr_nat (fun i : Usize => a.val.length - i.val)
    (fun i : Usize => i.val ≤ a.val.length ∧ ∀ j, j < i.val → a.val[j]? = b.val[j]?)
  · intro i ⟨hi, hprefix⟩
    unfold splice.same_bytes_loop.body
    try dsimp only
    split_ifs with hlt
    · have hlt' : i.val < a.val.length := by scalar_tac
      step as ⟨ x, hx ⟩
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
      have h : i.val = a.val.length := by scalar_tac
      apply List.ext_getElem?
      intro j
      by_cases hj : j < a.val.length
      · exact hprefix j (by omega)
      · rw [List.getElem?_eq_none (by scalar_tac), List.getElem?_eq_none (by scalar_tac)]
  · exact ⟨hi, hprefix⟩

/-- `same_bytes` is byte equality. -/
theorem same_bytes_spec (a b : Slice U8) :
    splice.same_bytes a b ⦃ r => r ↔ a.val = b.val ⦄ := by
  unfold splice.same_bytes
  try dsimp only
  split_ifs with hne
  · simp only [WP.spec_ok, Bool.false_eq_true, false_iff]
    intro h
    rw [bne_iff_ne] at hne
    apply hne
    apply UScalar.eq_of_val_eq
    simp [h]
  · apply same_bytes_loop_spec
    · rw [bne_iff_ne, not_not] at hne
      have := congrArg UScalar.val hne
      simpa [Slice.len_val] using this
    · simp
    · intro j hj; simp at hj

/-! ## Rendering -/

theorem emit_cons_length_ge (input : List U8) (c : Nat) (e : splice.Edit) (rest : List splice.Edit)
    (hlo : e.target.lo.val ≤ input.length) :
    c ≤ e.target.lo.val →
      (e.target.lo.val - c) + (partsBytes input e.parts.val).length ≤
        (emit input c (e :: rest)).length := by
  intro hc
  simp only [emit, List.length_append, List.slice, List.length_take, List.length_drop]
  omega

/-- What `render` returns from a prepared order. -/
def RenderOutcome (input : List U8) (edits : List splice.Edit) (order : List Usize) :
    core.result.Result splice.Rendered splice.Refusal → Prop
  | .Ok out => out.bytes.val = spliced input (pick edits order) ∧
      (spliced input (pick edits order)).length ≤ 16777216 ∧
      out.replacements.val.map (·.val) =
        (pick edits order).map (fun e => partsBytes input e.parts.val) ∧
      (out.no_change ↔ out.bytes.val = input)
  | .Err e => e = .CandidateTooLarge ∧ 16777216 < (spliced input (pick edits order)).length

theorem render_loop_spec (input : Slice U8) (edits : Slice splice.Edit) (order : Slice Usize)
    (bytes : alloc.vec.Vec U8) (reps : alloc.vec.Vec (alloc.vec.Vec U8)) (cursor k : Usize)
    (hready : RenderReady input.val edits.val order.val)
    (hk : k.val ≤ order.val.length) (hcur : cursor.val ≤ input.val.length)
    (hnext : ∀ h : k.val < (pick edits.val order.val).length,
      cursor.val ≤ ((pick edits.val order.val)[k.val]).target.lo.val)
    (hbytes : bytes.val ++ emit input.val cursor.val ((pick edits.val order.val).drop k.val) =
      spliced input.val (pick edits.val order.val))
    (hbcap : bytes.val.length ≤ 16777216)
    (hreps : reps.val.map (·.val) =
      ((pick edits.val order.val).take k.val).map (fun e => partsBytes input.val e.parts.val))
    (hrlen : reps.val.length = k.val) :
    splice.render_loop input edits order bytes reps cursor k ⦃ r =>
      RenderOutcome input.val edits.val order.val r ⦄ := by
  obtain ⟨hord, hedits, hchain⟩ := hready
  have hpl := pick_length hord
  have hmax := order.property
  unfold splice.render_loop
  apply loop.spec_decr_nat
    (fun s : alloc.vec.Vec U8 × alloc.vec.Vec (alloc.vec.Vec U8) × Usize × Usize =>
      order.val.length - s.2.2.2.val)
    (fun s : alloc.vec.Vec U8 × alloc.vec.Vec (alloc.vec.Vec U8) × Usize × Usize =>
      s.2.2.2.val ≤ order.val.length ∧ s.2.2.1.val ≤ input.val.length ∧
      (∀ h : s.2.2.2.val < (pick edits.val order.val).length,
        s.2.2.1.val ≤ ((pick edits.val order.val)[s.2.2.2.val]).target.lo.val) ∧
      s.1.val ++ emit input.val s.2.2.1.val ((pick edits.val order.val).drop s.2.2.2.val) =
        spliced input.val (pick edits.val order.val) ∧
      s.1.val.length ≤ 16777216 ∧
      s.2.1.val.map (·.val) =
        ((pick edits.val order.val).take s.2.2.2.val).map (fun e => partsBytes input.val e.parts.val) ∧
      s.2.1.val.length = s.2.2.2.val)
  · rintro ⟨bytes, reps, cursor, k⟩ ⟨hk, hcur, hnext, hbytes, hbcap, hreps, hrlen⟩
    dsimp only at hk hcur hnext hbytes hbcap hreps hrlen
    unfold splice.render_loop.body
    try dsimp only
    split_ifs with hlt
    · have hlt' : k.val < order.val.length := by scalar_tac
      have hkp : k.val < (pick edits.val order.val).length := by rw [hpl]; exact hlt'
      step as ⟨ j, hj ⟩
      have hjin : j.val < edits.val.length := by rw [hj]; exact hord _ (List.getElem_mem _)
      have hpick : (pick edits.val order.val)[k.val] = edits.val[j.val] := by
        rw [pick_getElem hord _ hlt']; simp [hj]
      split_ifs with hjge
      · exfalso; scalar_tac
      step as ⟨ edit, hedit ⟩
      rw [← hpick] at hedit
      have hmem : edit ∈ pick edits.val order.val := by rw [hedit]; exact List.getElem_mem _
      obtain ⟨hlohi, hhi, hcopy, hsize⟩ := hedits edit hmem
      have hc := hnext hkp
      rw [← hedit] at hc
      have hdrop : (pick edits.val order.val).drop k.val =
          edit :: (pick edits.val order.val).drop (k.val + 1) := by
        rw [List.drop_eq_getElem_cons hkp, hedit]
      rw [hdrop] at hbytes
      have hge := emit_cons_length_ge input.val cursor.val edit
        ((pick edits.val order.val).drop (k.val + 1)) (by omega) hc
      have htot := congrArg List.length hbytes
      rw [List.length_append] at htot
      split_ifs with h1 h2 h3
      · exfalso; scalar_tac
      · exfalso; scalar_tac
      · exfalso; scalar_tac
      step as ⟨ gap, hgap ⟩
      try simp only [max_candidate_val]
      apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
      intro room hroom
      simp only [max_candidate_val] at hroom
      split_ifs with hover
      · simp only [WP.spec_ok, RenderOutcome, true_and]
        scalar_tac
      step as ⟨ s, hs, hslen ⟩
      apply WP.spec_bind (extend_spec bytes s (by scalar_tac))
      intro bytes1 hbytes1
      apply WP.spec_bind (materialize_spec input edit hcopy hsize)
      rintro r ⟨rep, rfl, hrep⟩
      simp only
      apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
      intro room1 hroom1
      simp only [max_candidate_val] at hroom1
      have hb1 : bytes1.val.length = bytes.val.length + (edit.target.lo.val - cursor.val) := by
        have : s.val.length = edit.target.lo.val - cursor.val := hslen
        rw [hbytes1, List.length_append]; omega
      have hr1 : rep.val.length = (partsBytes input.val edit.parts.val).length := by rw [hrep]
      split_ifs with hover1
      · simp only [WP.spec_ok, RenderOutcome, true_and]
        scalar_tac
      apply WP.spec_bind (extend_spec bytes1 (alloc.vec.Vec.deref rep) (by simp [hbytes1]; scalar_tac))
      intro bytes2 hbytes2
      apply WP.spec_bind (alloc.vec.Vec.push_spec reps rep (by scalar_tac))
      intro reps1 hreps1
      apply WP.spec_bind (Usize.add_spec (by scalar_tac))
      intro k' hk'
      have hk'v : k'.val = k.val + 1 := by scalar_tac
      simp only [WP.spec_ok]
      refine ⟨by scalar_tac, by scalar_tac, fun h => ?_, ?_, ?_, ?_, ?_, by scalar_tac⟩
      · have hk1 : k.val + 1 < (pick edits.val order.val).length := by rw [← hk'v]; exact h
        have := List.isChain_iff_getElem.mp hchain k.val hk1
        rw [← hedit] at this
        simpa [hk'v] using this
      · rw [hk'v, hbytes2, hbytes1, hs, ← hbytes]
        simp [emit, hrep]
      · have : bytes2.val.length = bytes1.val.length + rep.val.length := by
          rw [hbytes2]; simp
        scalar_tac
      · rw [hreps1, hk'v, List.take_add_one, List.getElem?_eq_getElem hkp, ← hedit]
        simp [hreps, hrep]
      · simp [hreps1, hrlen, hk'v]
    · have hkv : k.val = order.val.length := by scalar_tac
      have hnil : (pick edits.val order.val).drop k.val = [] := by
        rw [hkv, ← hpl, List.drop_length]
      rw [hnil] at hbytes
      simp only [emit] at hbytes
      have htot := congrArg List.length hbytes
      simp only [List.length_append, List.length_drop] at htot
      step as ⟨ tail, htail ⟩
      try simp only [max_candidate_val]
      apply WP.spec_bind (Usize.sub_spec (by simp; scalar_tac))
      intro room hroom
      simp only [max_candidate_val] at hroom
      split_ifs with hover
      · simp only [WP.spec_ok, RenderOutcome, true_and]
        scalar_tac
      step as ⟨ s, hs, hslen ⟩
      apply WP.spec_bind (extend_spec bytes s (by scalar_tac))
      intro bytes1 hbytes1
      apply WP.spec_bind (same_bytes_spec _ _)
      intro nc hnc
      simp only [WP.spec_ok, RenderOutcome, deref_val] at hnc ⊢
      have hout : bytes1.val = spliced input.val (pick edits.val order.val) := by
        rw [hbytes1, hs, ← hbytes]
      refine ⟨hout, ?_, ?_, hnc⟩
      · rw [← hout, hbytes1, List.length_append]; scalar_tac
      · rw [hreps, hkv, ← hpl, List.take_length]
  · exact ⟨hk, hcur, hnext, hbytes, hbcap, hreps, hrlen⟩

/-- `render` of a prepared order is the reference splice of the original, or
refuses with `CandidateTooLarge` exactly when that is too large. -/
theorem render_spec (input : Slice U8) (edits : Slice splice.Edit) (order : Slice Usize)
    (hready : RenderReady input.val edits.val order.val) :
    splice.render input edits order ⦃ r => RenderOutcome input.val edits.val order.val r ⦄ := by
  unfold splice.render
  apply render_loop_spec _ _ _ _ _ _ _ hready (by simp) (by simp)
  · intro _
    simp
  · simp [spliced]
  · simp
  · simp
  · simp

/-! ## Prepared batches are ready to render -/

/-- Everything `render` needs follows from `prepare` accepting the batch. -/
theorem prepared_ready {input : List U8} {text : Bool} {capture : List U8}
    {edits : List splice.Edit} {order : alloc.vec.Vec Usize}
    (h : PrepareOutcome input text capture edits (.Ok order)) :
    RenderReady input edits order.val := by
  obtain ⟨⟨_, hall, hpay, _⟩, hperm, hchain⟩ := h
  have hord : InRangeOrder edits order.val := by
    intro j hj
    have := hperm.mem_iff.mp (List.mem_map_of_mem (f := (·.val)) hj)
    simpa using this
  refine ⟨hord, fun e he => ?_, hchain.imp (fun _ _ hab => hab.1)⟩
  have hmem := mem_of_mem_pick he
  obtain ⟨⟨⟨_, hlohi, hhi⟩, _⟩, hparts⟩ := hall e hmem
  refine ⟨hlohi, hhi, fun p hp => ?_, ?_⟩
  · have := hparts p hp
    cases p with
    | Literal _ => trivial
    | Copy c => exact ⟨this.1.2.1, this.1.2.2⟩
  · have hle : partsSize e.parts.val ≤ payloadSize edits := by
      obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hmem
      have := partsSize_le_payloadSize edits i hi
      omega
    omega

/-- The pipeline production runs: `prepare`, then `render` of the order it
returned. -/
theorem prepare_render (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (edits : Slice splice.Edit) (order : alloc.vec.Vec Usize)
    (h : PrepareOutcome input.val text capture.val edits.val (.Ok order)) :
    splice.render input edits (alloc.vec.Vec.deref order) ⦃ r =>
      RenderOutcome input.val edits.val order.val r ⦄ := by
  have := render_spec input edits (alloc.vec.Vec.deref order) (by simpa using prepared_ready h)
  simpa using this

/-! ## Consequences -/

/-- Total target width of a list of edits. -/
def widths (sorted : List splice.Edit) : Nat :=
  (sorted.map (fun e => e.target.hi.val - e.target.lo.val)).sum

/-- Total replacement bytes of a list of edits. -/
def replacementBytes (input : List U8) (sorted : List splice.Edit) : Nat :=
  (sorted.map (fun e => (partsBytes input e.parts.val).length)).sum

theorem emit_length (input : List U8) (c : Nat) (sorted : List splice.Edit)
    (hc : c ≤ input.length)
    (hvalid : ∀ e ∈ sorted, e.target.lo.val ≤ e.target.hi.val ∧ e.target.hi.val ≤ input.length)
    (hfirst : ∀ e ∈ sorted.head?, c ≤ e.target.lo.val)
    (hchain : List.IsChain (fun a b : splice.Edit => a.target.hi.val ≤ b.target.lo.val) sorted) :
    (emit input c sorted).length + widths sorted =
      (input.length - c) + replacementBytes input sorted := by
  induction sorted generalizing c with
  | nil => simp [emit, widths, replacementBytes]
  | cons e rest ih =>
    have he := hvalid e (by simp)
    have hce : c ≤ e.target.lo.val := hfirst e (by simp)
    have hrest := ih e.target.hi.val he.2 (fun x hx => hvalid x (by simp [hx]))
      (by
        intro x hx
        cases rest with
        | nil => simp at hx
        | cons b rest' =>
          simp only [List.head?_cons, Option.mem_def, Option.some.injEq] at hx
          subst hx
          exact (List.isChain_cons_cons.mp hchain).1)
      (by simpa using hchain.tail)
    simp only [emit, widths, replacementBytes, List.map_cons, List.sum_cons, List.length_append,
      List.slice, List.length_take, List.length_drop] at hrest ⊢
    omega

/-- Byte counts add up: the candidate is the input minus every target width
plus every replacement length. -/
theorem spliced_length {input : List U8} {edits : List splice.Edit} {order : List Usize}
    (h : RenderReady input edits order) :
    (spliced input (pick edits order)).length + widths (pick edits order) =
      input.length + replacementBytes input (pick edits order) := by
  obtain ⟨_, hedits, hchain⟩ := h
  have := emit_length input 0 (pick edits order) (Nat.zero_le _)
    (fun e he => ⟨(hedits e he).1, (hedits e he).2.1⟩) (by intro e _; exact Nat.zero_le _) hchain
  simpa [spliced] using this

/-- A one-edit batch: the prefix, the replacement, the suffix. -/
theorem spliced_single (input : List U8) (e : splice.Edit) :
    spliced input [e] =
      input.take e.target.lo.val ++ partsBytes input e.parts.val ++ input.drop e.target.hi.val := by
  simp [spliced, emit, List.slice]

/-- An empty batch is accepted and renders the original unchanged (inputs
within the candidate cap; production inputs are at most 8 MiB). -/
theorem render_empty (input : Slice U8) (edits : Slice splice.Edit) (order : Slice Usize)
    (hempty : order.val = []) (hcap : input.val.length ≤ 16777216) :
    splice.render input edits order ⦃ r =>
      ∃ out, r = .Ok out ∧ out.bytes.val = input.val ∧ out.no_change = true ∧
        out.replacements.val = [] ⦄ := by
  have hready : RenderReady input.val edits.val order.val := by
    refine ⟨by simp [InRangeOrder, hempty], by simp [pick, hempty], by simp [pick, hempty]⟩
  apply WP.spec_mono (render_spec input edits order hready)
  intro r hr
  have hsp : spliced input.val (pick edits.val order.val) = input.val := by
    simp [spliced, pick, hempty, emit]
  cases r with
  | Ok out =>
    obtain ⟨hb, _, hreps, hnc⟩ := hr
    rw [hsp] at hb
    refine ⟨out, rfl, hb, hnc.mpr hb, ?_⟩
    simpa [pick, hempty] using hreps
  | Err e =>
    obtain ⟨_, hlen⟩ := hr
    rw [hsp] at hlen
    omega

/-- `prepare` accepts an empty batch with the empty order. -/
theorem prepare_empty (input : Slice U8) (text : Bool) (capture : Array U8 32#usize)
    (edits : Slice splice.Edit) (hempty : edits.val = []) :
    splice.prepare input text capture edits ⦃ r => ∃ order, r = .Ok order ∧ order.val = [] ⦄ := by
  apply WP.spec_mono (prepare_spec input text capture edits)
  intro r hr
  have hadm : Admissible input.val text capture.val edits.val := by
    simp [Admissible, hempty, payloadSize]
  obtain ⟨order, rfl⟩ := (prepare_ok_iff hr).mpr hadm
  refine ⟨order, rfl, ?_⟩
  have := hr.2.1.length_eq
  simp [hempty] at this
  exact this

/-! ## Independence of authored order -/

/-- Strictly earlier start. -/
def StartsBefore (a b : splice.Edit) : Prop := a.target.lo.val < b.target.lo.val

instance : Std.Antisymm StartsBefore :=
  ⟨fun _ _ h1 h2 => absurd (Nat.lt_trans h1 h2) (Nat.lt_irrefl _)⟩

/-- Canonical valid targets start strictly in order. -/
theorem canonical_strict {l : List splice.Edit} (hval : ∀ e ∈ l, Ordered e.target)
    (h : List.IsChain (fun a b : splice.Edit => Compatible a.target b.target) l) :
    l.Pairwise StartsBefore := by
  refine List.Pairwise.imp_of_mem ?_ (pairwise_of_isChain hval h)
  intro a b ha _ hab
  have := hval a ha
  unfold Compatible Ordered at *
  unfold StartsBefore
  omega

/-- formal/Batch.lean's `normalizeAuthored_eq_of_perm` over the kernel: two
authored orders of the same edits, both accepted, yield the same canonical
list, hence (`RenderOutcome` depends on nothing else) the same candidate and
reports. -/
theorem prepare_order_independent {input : List U8} {text : Bool} {capture : List U8}
    {xs ys : List splice.Edit} {ox oy : alloc.vec.Vec Usize} (hperm : xs.Perm ys)
    (hx : PrepareOutcome input text capture xs (.Ok ox))
    (hy : PrepareOutcome input text capture ys (.Ok oy)) :
    pick xs ox.val = pick ys oy.val := by
  have hpx := prepare_order_perm hx
  have hpy := prepare_order_perm hy
  have hvx : ∀ e ∈ pick xs ox.val, Ordered e.target :=
    fun e he => (hx.1.2.1 e (mem_of_mem_pick he)).1.1.2.1
  have hvy : ∀ e ∈ pick ys oy.val, Ordered e.target :=
    fun e he => (hy.1.2.1 e (mem_of_mem_pick he)).1.1.2.1
  exact List.Perm.eq_of_pairwise' (canonical_strict hvx hx.2.2) (canonical_strict hvy hy.2.2)
    ((hpx.trans hperm).trans hpy.symm)

/-! ## apply_at -/

/-- A cut `apply_at` accepts: inside the input (checked remainder, never a
wrapped `lo + width`) with an output within `MAX_CANDIDATE_BYTES`. -/
def ApplyFits (input : List U8) (lo width : Nat) (replacement : List U8) : Prop :=
  lo ≤ input.length ∧ width ≤ input.length - lo ∧
    input.length - width + replacement.length ≤ 16777216

/-- `apply_at` replaces exactly `width` bytes at `lo` (formal/Core.lean's
`applyAt`), and refuses exactly a cut outside the input or an output beyond
`MAX_CANDIDATE_BYTES`. -/
theorem apply_at_spec (input : Slice U8) (lo width : Usize) (replacement : Slice U8) :
    splice.apply_at input lo width replacement ⦃ r =>
      (ApplyFits input.val lo.val width.val replacement.val →
        ∃ out, r = some out ∧
          out.val = input.val.take lo.val ++ replacement.val ++ input.val.drop (lo.val + width.val)) ∧
      (¬ ApplyFits input.val lo.val width.val replacement.val → r = none) ⦄ := by
  unfold splice.apply_at
  try dsimp only
  have hmax : splice.MAX_CANDIDATE_BYTES.val = 16777216 := max_candidate_val
  by_cases h1 : lo.val ≤ input.val.length
  swap
  · have h1' : lo > Slice.len input := by scalar_tac
    rw [if_pos h1']
    apply (WP.spec_ok _).mpr
    exact ⟨fun h => absurd h.1 h1, fun _ => rfl⟩
  have h1' : ¬ (lo > Slice.len input) := by scalar_tac
  rw [if_neg h1']
  step as ⟨ rem, hrem ⟩
  by_cases h2 : width.val ≤ input.val.length - lo.val
  swap
  · have h2' : width > rem := by scalar_tac
    rw [if_pos h2']
    apply (WP.spec_ok _).mpr
    exact ⟨fun h => absurd h.2.1 h2, fun _ => rfl⟩
  have h2' : ¬ (width > rem) := by scalar_tac
  rw [if_neg h2']
  by_cases h3 : replacement.val.length ≤ 16777216
  swap
  · have h3' : Slice.len replacement > splice.MAX_CANDIDATE_BYTES := by
      rw [gt_iff_lt, UScalar.lt_equiv, hmax, Slice.len_val]; simpa using h3
    rw [if_pos h3']
    apply (WP.spec_ok _).mpr
    exact ⟨fun h => absurd (by unfold ApplyFits at h; omega) h3, fun _ => rfl⟩
  have h3' : ¬ (Slice.len replacement > splice.MAX_CANDIDATE_BYTES) := by
    rw [gt_iff_lt, UScalar.lt_equiv, hmax, Slice.len_val]; simpa using h3
  rw [if_neg h3']
  step as ⟨ kept, hkept ⟩
  apply WP.spec_bind (Usize.sub_spec (by rw [hmax]; scalar_tac))
  intro room hroom
  rw [hmax] at hroom
  by_cases h4 : input.val.length - width.val + replacement.val.length ≤ 16777216
  swap
  · have h4' : kept > room := by scalar_tac
    rw [if_pos h4']
    apply (WP.spec_ok _).mpr
    exact ⟨fun h => absurd h.2.2 h4, fun _ => rfl⟩
  have h4' : ¬ (kept > room) := by scalar_tac
  rw [if_neg h4']
  step as ⟨ pre, hpre, hprelen ⟩
  apply WP.spec_bind (extend_spec _ pre (by simp))
  intro o1 ho1
  apply WP.spec_bind (extend_spec o1 replacement (by simp [ho1]; scalar_tac))
  intro o2 ho2
  step as ⟨ stop, hstop ⟩
  step as ⟨ post, hpost, hpostlen ⟩
  apply WP.spec_bind (extend_spec o2 post (by simp [ho2, ho1]; scalar_tac))
  intro o3 ho3
  apply (WP.spec_ok _).mpr
  refine ⟨fun _ => ⟨o3, rfl, ?_⟩, fun h => absurd (show ApplyFits _ _ _ _ from ⟨h1, h2, h4⟩) h⟩
  rw [ho3, ho2, ho1, hpre, hpost, hstop]
  simp [List.slice]

end Splicery.Kernel
