import Splicery.Core
import Lean.Elab.Tactic.Omega

namespace Splicery

/-- A normalized batch stores untouched gaps and resolved disjoint edits in
    original order. Numeric validation and order normalization are separate. -/
inductive Batch where
  | tail (rest : Bytes)
  | step (gap old replacement : Bytes) (more : Batch)
  deriving DecidableEq, Repr

def Batch.apply : Batch → Bytes
  | .tail b => b
  | .step gap _ newBytes rest => gap ++ newBytes ++ rest.apply

def Batch.input : Batch → Bytes
  | .tail b => b
  | .step gap old _ rest => gap ++ old ++ rest.input

/-- An independent relation tracks the same exact gap on both sides. -/
inductive KeepsGaps : Bytes → Bytes → Prop where
  | tail (b : Bytes) : KeepsGaps b b
  | step (gap old newBytes : Bytes) {x y : Bytes} (rest : KeepsGaps x y) :
      KeepsGaps (gap ++ old ++ x) (gap ++ newBytes ++ y)

theorem batch_preserves_gaps (b : Batch) : KeepsGaps b.input b.apply := by
  induction b with
  | tail rest => exact KeepsGaps.tail rest
  | step gap old newBytes more ih => exact KeepsGaps.step gap old newBytes ih

theorem swap_from_original (p a gap b q : Bytes) :
    (Batch.step p a b (.step gap b a (.tail q))).apply =
      p ++ b ++ (gap ++ a ++ q) := rfl

/-- An authored edit uses positions in the one immutable input. `width` bytes
    are replaced by the explicit `replacement` payload. -/
structure AuthoredSplice where
  start : Nat
  width : Nat
  replacement : Bytes
  deriving DecidableEq, Repr

/-- The exclusive end of an authored edit in the immutable input. -/
def AuthoredSplice.stop (s : AuthoredSplice) : Nat := s.start + s.width

/-- Consecutive canonical edits must have disjoint ranges. Touching is allowed
    only for two nonempty replacements. Thus coincident insertions and an
    insertion at either boundary of a replacement are not canonical. -/
def AuthoredCompatible (a b : AuthoredSplice) : Prop :=
  a.stop ≤ b.start ∧ (a.stop = b.start → a.width ≠ 0 ∧ b.width ≠ 0)

/-- The adjacency invariant of a positionally canonical edit list. -/
inductive CanonicalPositions : List AuthoredSplice → Prop where
  | nil : CanonicalPositions []
  | single (a : AuthoredSplice) : CanonicalPositions [a]
  | cons {a b : AuthoredSplice} {rest : List AuthoredSplice}
      (head : AuthoredCompatible a b) (tail : CanonicalPositions (b :: rest)) :
      CanonicalPositions (a :: b :: rest)

def HeadCompatible (a : AuthoredSplice) (rest : List AuthoredSplice) : Prop :=
  match rest with
  | [] => True
  | b :: _ => AuthoredCompatible a b

instance (a b : AuthoredSplice) : Decidable (AuthoredCompatible a b) := by
  unfold AuthoredCompatible AuthoredSplice.stop
  infer_instance

instance (a : AuthoredSplice) (rest : List AuthoredSplice) :
    Decidable (HeadCompatible a rest) := by
  cases rest <;> simp [HeadCompatible] <;> infer_instance

/-- Stable insertion ordering is finite and executable; validation below turns
    its non-strict start order into the strict canonical order. -/
private def insertAuthored (a : AuthoredSplice) : List AuthoredSplice → List AuthoredSplice
  | [] => [a]
  | b :: bs =>
      if a.start ≤ b.start then a :: b :: bs else b :: insertAuthored a bs

def sortAuthored : List AuthoredSplice → List AuthoredSplice
  | [] => []
  | a :: as => insertAuthored a (sortAuthored as)

/-- Convert already ordered edits to exact gaps. Every range is checked with
    `cut` before any result is exposed. The suffix and offset always describe
    the unconsumed part of the same immutable input. -/
def buildCanonical (suffix : Bytes) (offset : Nat) :
    List AuthoredSplice → Option Batch
  | [] => some (.tail suffix)
  | a :: rest =>
      if offset ≤ a.start ∧ HeadCompatible a rest then
        match cut suffix (a.start - offset) a.width with
        | none => none
        | some c =>
            match buildCanonical c.post a.stop rest with
            | none => none
            | some more => some (.step c.pre c.selected a.replacement more)
      else none

/-- Validate, canonicalize, and resolve all authored edits against `input`.
    Failure of any position or range returns no batch at all. -/
def normalizeAuthored (input : Bytes) (edits : List AuthoredSplice) : Option Batch :=
  buildCanonical input 0 (sortAuthored edits)

/-- Validation precedes application, so application itself is total and cannot
    return a successfully rewritten prefix of an invalid batch. -/
def applyAuthored (input : Bytes) (edits : List AuthoredSplice) : Option Bytes :=
  (normalizeAuthored input edits).map Batch.apply

private theorem insertAuthored_perm (a : AuthoredSplice) (as : List AuthoredSplice) :
    List.Perm (a :: as) (insertAuthored a as) := by
  induction as with
  | nil => exact List.Perm.refl _
  | cons b bs ih =>
      by_cases h : a.start ≤ b.start
      · simp [insertAuthored, h]
      · simp only [insertAuthored, h, ↓reduceIte]
        exact (List.Perm.swap b a bs).trans (List.Perm.cons b ih)

private theorem sortAuthored_perm (as : List AuthoredSplice) :
    List.Perm as (sortAuthored as) := by
  induction as with
  | nil => exact List.Perm.refl _
  | cons a as ih =>
      exact (List.Perm.cons a ih).trans (insertAuthored_perm a (sortAuthored as))

private theorem compatible_start_lt {a b : AuthoredSplice}
    (h : AuthoredCompatible a b) : a.start < b.start := by
  rcases h with ⟨hle, heq⟩
  by_cases hstop : a.stop = b.start
  · have hnz := heq hstop
    simp only [AuthoredSplice.stop] at hstop
    omega
  · simp only [AuthoredSplice.stop] at hle hstop
    omega

private theorem buildCanonical_input {suffix : Bytes} {offset : Nat}
    {edits : List AuthoredSplice} {b : Batch}
    (h : buildCanonical suffix offset edits = some b) : b.input = suffix := by
  induction edits generalizing suffix offset b with
  | nil =>
      simp only [buildCanonical, Option.some.injEq] at h
      subst b
      rfl
  | cons a rest ih =>
      by_cases hv : offset ≤ a.start ∧ HeadCompatible a rest
      · cases hc : cut suffix (a.start - offset) a.width with
        | none => simp [buildCanonical, hv, hc] at h
        | some c =>
            cases hr : buildCanonical c.post a.stop rest with
            | none =>
                rw [buildCanonical] at h
                simp only [hv, ↓reduceIte, hc] at h
                rw [hr] at h
                contradiction
            | some more =>
                simp only [buildCanonical, hv, ↓reduceIte, hc, hr] at h
                injection h with hb
                subst b
                have htail := ih hr
                simpa only [Batch.input, htail, List.append_assoc] using
                  (cut_sound hc).1.symm
      · simp [buildCanonical, hv] at h

private theorem buildCanonical_positions {suffix : Bytes} {offset : Nat}
    {edits : List AuthoredSplice} {b : Batch}
    (h : buildCanonical suffix offset edits = some b) :
    CanonicalPositions edits := by
  induction edits generalizing suffix offset b with
  | nil => exact CanonicalPositions.nil
  | cons a rest ih =>
      cases rest with
      | nil => exact CanonicalPositions.single a
      | cons c cs =>
          by_cases hv : offset ≤ a.start ∧ HeadCompatible a (c :: cs)
          · have hab : AuthoredCompatible a c := hv.2
            cases hc : cut suffix (a.start - offset) a.width with
            | none => simp [buildCanonical, hv, hc] at h
            | some cutResult =>
                cases hr : buildCanonical cutResult.post a.stop (c :: cs) with
                | none =>
                    rw [buildCanonical] at h
                    simp only [hv, ↓reduceIte, hc] at h
                    rw [hr] at h
                    contradiction
                | some more => exact CanonicalPositions.cons hab (ih hr)
          · simp [buildCanonical, hv] at h

private theorem buildCanonical_strict {suffix : Bytes} {offset : Nat}
    {edits : List AuthoredSplice} {b : Batch}
    (h : buildCanonical suffix offset edits = some b) :
    List.Pairwise (fun a b => a.start < b.start) edits := by
  induction edits generalizing suffix offset b with
  | nil => simp
  | cons a rest ih =>
      cases rest with
      | nil => simp
      | cons c cs =>
          by_cases hv : offset ≤ a.start ∧ HeadCompatible a (c :: cs)
          · have hab : AuthoredCompatible a c := hv.2
            cases hc : cut suffix (a.start - offset) a.width with
            | none => simp [buildCanonical, hv, hc] at h
            | some cutResult =>
                cases hr : buildCanonical cutResult.post a.stop (c :: cs) with
                | none =>
                    rw [buildCanonical] at h
                    simp only [hv, ↓reduceIte, hc] at h
                    rw [hr] at h
                    contradiction
                | some more =>
                    have htail := ih hr
                    have hac := compatible_start_lt hab
                    apply List.pairwise_cons.mpr
                    constructor
                    · intro x hx
                      rcases List.mem_cons.mp hx with rfl | hx
                      · exact hac
                      · exact Nat.lt_trans hac ((List.pairwise_cons.mp htail).1 x hx)
                    · exact htail
          · simp [buildCanonical, hv] at h

private theorem perm_eq_of_strict_starts {xs ys : List AuthoredSplice}
    (hp : List.Perm xs ys)
    (hx : List.Pairwise (fun a b => a.start < b.start) xs)
    (hy : List.Pairwise (fun a b => a.start < b.start) ys) : xs = ys := by
  induction xs generalizing ys with
  | nil => exact (List.perm_nil.mp hp.symm).symm
  | cons x xs ih =>
      cases ys with
      | nil => simp at hp
      | cons y ys =>
          have hxmem : x ∈ y :: ys := hp.mem_iff.mp (by simp)
          have hymem : y ∈ x :: xs := hp.symm.mem_iff.mp (by simp)
          have hxy : x = y := by
            by_cases h : x = y
            · exact h
            · have hxin : x ∈ ys := (List.mem_cons.mp hxmem).resolve_left h
              have hyin : y ∈ xs := (List.mem_cons.mp hymem).resolve_left (Ne.symm h)
              have hyx := (List.pairwise_cons.mp hy).1 x hxin
              have hxy := (List.pairwise_cons.mp hx).1 y hyin
              omega
          subst y
          exact congrArg (List.cons x)
            (ih (List.Perm.cons_inv hp) (List.pairwise_cons.mp hx).2
              (List.pairwise_cons.mp hy).2)

/-- Successful normalization reconstructs every untouched gap and every old
    payload from exactly the immutable input. -/
theorem normalizeAuthored_input {input : Bytes} {edits : List AuthoredSplice} {b : Batch}
    (h : normalizeAuthored input edits = some b) : b.input = input :=
  buildCanonical_input h

/-- The normalized result retains the exact input gaps on both sides. -/
theorem normalizeAuthored_preserves_gaps {input : Bytes} {edits : List AuthoredSplice}
    {b : Batch} (h : normalizeAuthored input edits = some b) :
    KeepsGaps input b.apply := by
  rw [← normalizeAuthored_input h]
  exact batch_preserves_gaps b

/-- Successful canonical validation proves every consecutive positional check. -/
theorem normalizeAuthored_positions {input : Bytes} {edits : List AuthoredSplice}
    {b : Batch} (h : normalizeAuthored input edits = some b) :
    CanonicalPositions (sortAuthored edits) :=
  buildCanonical_positions h

/-- Canonical starts are strictly increasing, including for adjacent nonempty
    replacements and for distinct insertions. -/
theorem normalizeAuthored_strict_starts {input : Bytes} {edits : List AuthoredSplice}
    {b : Batch} (h : normalizeAuthored input edits = some b) :
    List.Pairwise (fun a b => a.start < b.start) (sortAuthored edits) :=
  buildCanonical_strict h

/-- The first failing adjacency check refuses the entire canonical batch. -/
theorem canonical_rejects_incompatible {suffix : Bytes} {offset : Nat}
    {a b : AuthoredSplice} {rest : List AuthoredSplice}
    (h : ¬ AuthoredCompatible a b) :
    buildCanonical suffix offset (a :: b :: rest) = none := by
  simp [buildCanonical, HeadCompatible, h]

/-- A failed exact range check refuses rather than clamping the range. -/
theorem canonical_rejects_invalid_range {suffix : Bytes} {offset : Nat}
    {a : AuthoredSplice} {rest : List AuthoredSplice}
    (hv : offset ≤ a.start ∧ HeadCompatible a rest)
    (hc : cut suffix (a.start - offset) a.width = none) :
    buildCanonical suffix offset (a :: rest) = none := by
  simp [buildCanonical, hv, hc]

/-- Failure anywhere in the unresolved suffix discards already resolved steps. -/
theorem canonical_no_partial_result {suffix : Bytes} {offset : Nat}
    {a : AuthoredSplice} {rest : List AuthoredSplice} {c : Cut}
    (hv : offset ≤ a.start ∧ HeadCompatible a rest)
    (hc : cut suffix (a.start - offset) a.width = some c)
    (hr : buildCanonical c.post a.stop rest = none) :
    buildCanonical suffix offset (a :: rest) = none := by
  simp [buildCanonical, hv, hc, hr]

/-- Nonempty intersection is rejected, irrespective of either payload. -/
theorem not_compatible_of_intersection {a b : AuthoredSplice}
    (h : b.start < a.stop) : ¬ AuthoredCompatible a b := by
  intro hab
  exact (Nat.not_lt_of_ge hab.1) h

/-- Two insertions cannot occupy the same position. -/
theorem not_compatible_of_coincident_insertions {a b : AuthoredSplice}
    (ha : a.width = 0) (_hb : b.width = 0) (hstart : a.start = b.start) :
    ¬ AuthoredCompatible a b := by
  intro hab
  have hstop : a.stop = b.start := by
    simp [AuthoredSplice.stop, ha, hstart]
  exact (hab.2 hstop).1 ha

/-- An insertion at the start of a nonempty replacement is rejected. -/
theorem not_compatible_of_insertion_at_replacement_start {a b : AuthoredSplice}
    (ha : a.width = 0) (_hb : b.width ≠ 0) (hstart : a.start = b.start) :
    ¬ AuthoredCompatible a b := by
  intro hab
  have hstop : a.stop = b.start := by
    simp [AuthoredSplice.stop, ha, hstart]
  exact (hab.2 hstop).1 ha

/-- An insertion inside a replacement or at its exclusive end is rejected. -/
theorem not_compatible_of_insertion_at_or_inside_replacement {a b : AuthoredSplice}
    (_ha : a.width ≠ 0) (hb : b.width = 0) (hinside : b.start ≤ a.stop) :
    ¬ AuthoredCompatible a b := by
  intro hab
  have hstop : a.stop = b.start := Nat.le_antisymm hab.1 hinside
  exact (hab.2 hstop).2 hb

/-- Valid authored permutations have one canonical normalized representation. -/
theorem normalizeAuthored_eq_of_perm {input : Bytes} {xs ys : List AuthoredSplice}
    {bx batchY : Batch} (hp : List.Perm xs ys)
    (hx : normalizeAuthored input xs = some bx)
    (hy : normalizeAuthored input ys = some batchY) : bx = batchY := by
  have hsp : List.Perm (sortAuthored xs) (sortAuthored ys) :=
    (sortAuthored_perm xs).symm.trans (hp.trans (sortAuthored_perm ys))
  have hsx := buildCanonical_strict hx
  have hsy := buildCanonical_strict hy
  have hs : sortAuthored xs = sortAuthored ys :=
    perm_eq_of_strict_starts hsp hsx hsy
  unfold normalizeAuthored at hx hy
  rw [hs] at hx
  exact Option.some.inj (hx.symm.trans hy)

/-- Consequently, successful application is independent of authored order. -/
theorem applyAuthored_eq_of_perm {input : Bytes} {xs ys : List AuthoredSplice}
    {x y : Bytes} (hp : List.Perm xs ys)
    (hx : applyAuthored input xs = some x)
    (hy : applyAuthored input ys = some y) : x = y := by
  cases hnx : normalizeAuthored input xs with
  | none => simp [applyAuthored, hnx] at hx
  | some bx =>
      cases hny : normalizeAuthored input ys with
      | none => simp [applyAuthored, hny] at hy
      | some batchY =>
          simp only [applyAuthored, hnx, hny, Option.map_some, Option.some.injEq] at hx hy
          subst x
          subst y
          rw [normalizeAuthored_eq_of_perm hp hnx hny]

/-- Option-level refusal exposes no partial bytes. -/
theorem applyAuthored_none_iff (input : Bytes) (edits : List AuthoredSplice) :
    applyAuthored input edits = none ↔ normalizeAuthored input edits = none := by
  cases h : normalizeAuthored input edits <;> simp [applyAuthored, h]

/-- Two disjoint nonempty spans can be exchanged numerically. All offsets refer
    to the original input, and the second span may be separated by any gap. -/
theorem swap_authored_from_original (p a gap b q : Bytes)
    (ha : 0 < a.length) (hb : 0 < b.length) :
    normalizeAuthored (p ++ (a ++ (gap ++ (b ++ q))))
      [⟨p.length, a.length, b⟩, ⟨(p ++ a ++ gap).length, b.length, a⟩] =
      some (Batch.step p a b (.step gap b a (.tail q))) := by
  have hstart : p.length ≤ (p ++ a ++ gap).length := by
    simp only [List.length_append]
    omega
  have hcompatible : AuthoredCompatible
      ⟨p.length, a.length, b⟩ ⟨(p ++ a ++ gap).length, b.length, a⟩ := by
    constructor
    · simp only [AuthoredSplice.stop, List.length_append]
      omega
    · intro _
      exact ⟨Nat.ne_of_gt ha, Nat.ne_of_gt hb⟩
  have hstop : p.length + a.length ≤ (p ++ a ++ gap).length := hcompatible.1
  have hgap : (p ++ a ++ gap).length - (p.length + a.length) = gap.length := by
    simp only [List.length_append]
    omega
  simp only [List.length_append, Nat.add_assoc] at hstart hcompatible hstop hgap
  simp [normalizeAuthored, sortAuthored, insertAuthored, hstart, buildCanonical,
    HeadCompatible, AuthoredSplice.stop, hcompatible, hstop, hgap, cut_of_parts]

/-- A single edit resolves its exact gap, old payload, and new payload. -/
example :
    normalizeAuthored [1, 2, 3, 4] [⟨1, 2, [9]⟩] =
      some (.step [1] [2, 3] [9] (.tail [4])) := by
  decide

/-- Adjacent nonempty replacements are valid, even when authored in reverse. -/
example :
    normalizeAuthored [0, 1, 2, 3] [⟨1, 2, [8, 8]⟩, ⟨0, 1, [9]⟩] =
      some (.step [] [0] [9] (.step [] [1, 2] [8, 8] (.tail [3]))) := by
  decide

/-- An insertion at EOF has the whole immutable input as its exact gap prefix. -/
example :
    normalizeAuthored [1, 2] [⟨2, 0, [9]⟩] =
      some (.step [1, 2] [] [9] (.tail [])) := by
  decide

/-- Nonempty intersection is refused. -/
example : normalizeAuthored [0, 1, 2] [⟨0, 2, [9]⟩, ⟨1, 1, [8]⟩] = none := by
  decide

/-- Coincident insertions are refused. -/
example : normalizeAuthored [0, 1] [⟨1, 0, [9]⟩, ⟨1, 0, [8]⟩] = none := by
  decide

/-- Insertion at a replacement's start is refused. -/
example : normalizeAuthored [0, 1, 2] [⟨1, 1, [9]⟩, ⟨1, 0, [8]⟩] = none := by
  decide

/-- Insertion strictly inside a replacement is refused. -/
example : normalizeAuthored [0, 1, 2, 3] [⟨1, 3, [9]⟩, ⟨2, 0, [8]⟩] = none := by
  decide

/-- Insertion at a replacement's exclusive end is refused. -/
example : normalizeAuthored [0, 1, 2] [⟨1, 2, [9]⟩, ⟨3, 0, [8]⟩] = none := by
  decide

/-- A later out-of-range edit refuses the whole batch, including a valid prefix. -/
example : normalizeAuthored [0, 1] [⟨0, 1, [9]⟩, ⟨2, 1, [8]⟩] = none := by
  decide

end Splicery
