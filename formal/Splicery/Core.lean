import Std

namespace Splicery

abbrev Bytes := List UInt8

/-- Taking too many bytes refuses; it never clamps an invalid offset. -/
def splitExact : Nat → Bytes → Option (Bytes × Bytes)
  | 0, xs => some ([], xs)
  | _ + 1, [] => none
  | n + 1, x :: xs => do
    let (l, r) ← splitExact n xs
    pure (x :: l, r)

theorem splitExact_sound {n : Nat} {xs l r : Bytes}
    (h : splitExact n xs = some (l, r)) : xs = l ++ r ∧ l.length = n := by
  induction n generalizing xs l r with
  | zero =>
    simp [splitExact] at h
    rcases h with ⟨hl, hr⟩
    subst l
    subst r
    simp
  | succ n ih =>
    cases xs with
    | nil => simp [splitExact] at h
    | cons x xs =>
      cases hs : splitExact n xs with
      | none => simp [splitExact, hs] at h
      | some p =>
        obtain ⟨a, b⟩ := p
        have hi := ih (xs := xs) (l := a) (r := b) hs
        simp [splitExact, hs] at h
        rcases h with ⟨hl, hr⟩
        subst l
        subst r
        simp [hi.1, hi.2]

theorem splitExact_append (l r : Bytes) :
    splitExact l.length (l ++ r) = some (l, r) := by
  induction l with
  | nil => rfl
  | cons x xs ih => simp [splitExact, ih]

theorem splitExact_of_le {n : Nat} {xs : Bytes} (h : n ≤ xs.length) :
    ∃ l r, splitExact n xs = some (l, r) := by
  induction n generalizing xs with
  | zero => exact ⟨[], xs, rfl⟩
  | succ n ih =>
    cases xs with
    | nil => simp at h
    | cons x xs =>
      have h' : n ≤ xs.length := by simpa using h
      obtain ⟨l, r, hr⟩ := ih h'
      refine ⟨x :: l, r, ?_⟩
      simp [splitExact, hr]

theorem splitExact_eq_some_iff {n : Nat} {xs l r : Bytes} :
    splitExact n xs = some (l, r) ↔ xs = l ++ r ∧ l.length = n := by
  constructor
  · exact splitExact_sound
  · rintro ⟨hxs, hl⟩
    subst xs
    subst n
    exact splitExact_append l r

theorem splitExact_some_iff {n : Nat} {xs : Bytes} :
    (∃ p, splitExact n xs = some p) ↔ n ≤ xs.length := by
  constructor
  · rintro ⟨⟨l, r⟩, h⟩
    have hs := splitExact_sound h
    rw [hs.1, List.length_append, hs.2]
    exact Nat.le_add_right n r.length
  · intro h
    obtain ⟨l, r, hs⟩ := splitExact_of_le h
    exact ⟨(l, r), hs⟩

theorem splitExact_eq_none_iff {n : Nat} {xs : Bytes} :
    splitExact n xs = none ↔ xs.length < n := by
  constructor
  · intro h
    by_cases hn : xs.length < n
    · exact hn
    · have hle : n ≤ xs.length := Nat.le_of_not_gt hn
      obtain ⟨l, r, hs⟩ := splitExact_of_le hle
      rw [hs] at h
      simp at h
  · intro hlt
    cases hs : splitExact n xs with
    | none => rfl
    | some p =>
      obtain ⟨l, r⟩ := p
      have hsound := splitExact_sound hs
      have hle : n ≤ xs.length := by
        rw [hsound.1, List.length_append, hsound.2]
        exact Nat.le_add_right n r.length
      exact ((Nat.not_le_of_gt hlt) hle).elim

structure Cut where
  pre : Bytes
  selected : Bytes
  post : Bytes
  deriving DecidableEq, Repr

def cut (xs : Bytes) (lo width : Nat) : Option Cut := do
  let (p, rest) ← splitExact lo xs
  let (s, q) ← splitExact width rest
  pure ⟨p, s, q⟩

theorem cut_sound {xs : Bytes} {lo width : Nat} {c : Cut}
    (h : cut xs lo width = some c) :
    xs = c.pre ++ (c.selected ++ c.post) ∧
    c.pre.length = lo ∧ c.selected.length = width := by
  cases hp : splitExact lo xs with
  | none => simp [cut, hp] at h
  | some p =>
    obtain ⟨a, b⟩ := p
    cases hs : splitExact width b with
    | none => simp [cut, hp, hs] at h
    | some s =>
      obtain ⟨d, e⟩ := s
      have h1 := splitExact_sound hp
      have h2 := splitExact_sound hs
      simp [cut, hp, hs] at h
      subst c
      exact ⟨by simpa only [h2.1] using h1.1, h1.2, h2.2⟩

theorem cut_of_parts (p s q : Bytes) :
    cut (p ++ (s ++ q)) p.length s.length = some ⟨p, s, q⟩ := by
  simp [cut, splitExact_append]

/-- A successful cut is exactly a decomposition at the requested lengths. -/
theorem cut_eq_some_iff {xs : Bytes} {lo width : Nat} {c : Cut} :
    cut xs lo width = some c ↔
      xs = c.pre ++ (c.selected ++ c.post) ∧
      c.pre.length = lo ∧ c.selected.length = width := by
  constructor
  · exact cut_sound
  · rintro ⟨hxs, hpre, hselected⟩
    subst xs
    subst lo
    subst width
    simpa using cut_of_parts c.pre c.selected c.post

/-- Validity uses a checked remainder, not a wrapping `lo + width` test. -/
theorem cut_some_iff {xs : Bytes} {lo width : Nat} :
    (∃ c, cut xs lo width = some c) ↔
      lo ≤ xs.length ∧ width ≤ xs.length - lo := by
  constructor
  · rintro ⟨c, hc⟩
    have hs := cut_sound hc
    have hlen : xs.length = lo + width + c.post.length := by
      simp [hs.1, hs.2.1, hs.2.2, Nat.add_assoc]
    have hsum : lo + width ≤ xs.length := by
      rw [hlen]
      exact Nat.le_add_right (lo + width) c.post.length
    have hlo : lo ≤ xs.length :=
      Nat.le_trans (Nat.le_add_right lo width) hsum
    exact ⟨hlo, Nat.le_sub_of_add_le (by simpa [Nat.add_comm] using hsum)⟩
  · rintro ⟨hlo, hwidth⟩
    obtain ⟨p, rest, hp⟩ := splitExact_of_le hlo
    have hps := splitExact_sound hp
    have hrest : rest.length = xs.length - lo := by
      have hlen : xs.length = lo + rest.length := by
        simp [hps.1, hps.2]
      calc
        rest.length = lo + rest.length - lo := by simp
        _ = xs.length - lo := by rw [hlen]
    have hwrest : width ≤ rest.length := by
      rw [hrest]
      exact hwidth
    obtain ⟨s, q, hs⟩ :=
      splitExact_of_le (n := width) (xs := rest) hwrest
    refine ⟨⟨p, s, q⟩, ?_⟩
    simp [cut, hp, hs]

theorem cut_eq_none_iff {xs : Bytes} {lo width : Nat} :
    cut xs lo width = none ↔ xs.length < lo ∨ xs.length - lo < width := by
  constructor
  · intro h
    by_cases hlo : xs.length < lo
    · exact Or.inl hlo
    · right
      have hlo' : lo ≤ xs.length := Nat.le_of_not_gt hlo
      by_cases hw : xs.length - lo < width
      · exact hw
      · have hw' : width ≤ xs.length - lo := Nat.le_of_not_gt hw
        obtain ⟨c, hc⟩ := (cut_some_iff).2 ⟨hlo', hw'⟩
        rw [hc] at h
        simp at h
  · intro hinv
    cases hc : cut xs lo width with
    | none => rfl
    | some c =>
      have hv := (cut_some_iff).1 ⟨c, hc⟩
      rcases hinv with hlo | hwidth
      · exact ((Nat.not_le_of_gt hlo) hv.1).elim
      · exact ((Nat.not_le_of_gt hwidth) hv.2).elim

theorem cut_eof (xs : Bytes) :
    cut xs xs.length 0 = some ⟨xs, [], []⟩ := by
  simpa using cut_of_parts xs [] []

/-- External half-open ranges are validated before Nat subtraction produces a
width. Thus reversed ranges refuse instead of acquiring a truncated width. -/
def cutRange (xs : Bytes) (lo hi : Nat) : Option Cut :=
  if hi < lo then none else cut xs lo (hi - lo)

theorem cutRange_sound {xs : Bytes} {lo hi : Nat} {c : Cut}
    (h : cutRange xs lo hi = some c) :
    xs = c.pre ++ (c.selected ++ c.post) ∧
    c.pre.length = lo ∧ c.selected.length = hi - lo := by
  by_cases reversed : hi < lo
  · simp [cutRange, reversed] at h
  · exact cut_sound (by simpa [cutRange, reversed] using h)

theorem cutRange_some_iff {xs : Bytes} {lo hi : Nat} :
    (∃ c, cutRange xs lo hi = some c) ↔ lo ≤ hi ∧ hi ≤ xs.length := by
  by_cases reversed : hi < lo
  · simp [cutRange, reversed, Nat.not_le_of_gt reversed]
  · have ordered : lo ≤ hi := Nat.le_of_not_gt reversed
    simp only [cutRange, reversed, ↓reduceIte]
    constructor
    · rintro ⟨c, hc⟩
      have valid := (cut_some_iff).1 ⟨c, hc⟩
      exact ⟨ordered, Nat.le_of_sub_le_sub_right valid.1 valid.2⟩
    · rintro ⟨_, contained⟩
      have start : lo ≤ xs.length := Nat.le_trans ordered contained
      exact (cut_some_iff).2 ⟨start, Nat.sub_le_sub_right contained lo⟩

theorem cutRange_eq_none_iff {xs : Bytes} {lo hi : Nat} :
    cutRange xs lo hi = none ↔ hi < lo ∨ xs.length < hi := by
  constructor
  · intro refused
    by_cases reversed : hi < lo
    · exact Or.inl reversed
    · right
      by_cases outside : xs.length < hi
      · exact outside
      · have ordered : lo ≤ hi := Nat.le_of_not_gt reversed
        have contained : hi ≤ xs.length := Nat.le_of_not_gt outside
        obtain ⟨c, success⟩ := (cutRange_some_iff).2 ⟨ordered, contained⟩
        rw [success] at refused
        simp at refused
  · intro invalid
    cases result : cutRange xs lo hi with
    | none => rfl
    | some c =>
      have valid := (cutRange_some_iff).1 ⟨c, result⟩
      cases invalid with
      | inl reversed => exact ((Nat.not_le_of_gt reversed) valid.1).elim
      | inr outside => exact ((Nat.not_le_of_gt outside) valid.2).elim

theorem cutRange_of_parts (p s q : Bytes) :
    cutRange (p ++ (s ++ q)) p.length (p.length + s.length) = some ⟨p, s, q⟩ := by
  simp [cutRange, cut_of_parts]

def applyAt (xs : Bytes) (lo width : Nat) (newBytes : Bytes) : Option Bytes := do
  let c ← cut xs lo width
  pure (c.pre ++ newBytes ++ c.post)

theorem apply_local (p old q newBytes : Bytes) :
    applyAt (p ++ (old ++ q)) p.length old.length newBytes =
      some (p ++ newBytes ++ q) := by simp [applyAt, cut_of_parts]

theorem keep_identity (p s q : Bytes) :
    applyAt (p ++ (s ++ q)) p.length s.length s = some (p ++ (s ++ q)) := by
  simp [apply_local, List.append_assoc]

/-- Success exposes the unchanged prefix and suffix around the exact replacement. -/
theorem applyAt_eq_some_iff {xs ys : Bytes} {lo width : Nat} {newBytes : Bytes} :
    applyAt xs lo width newBytes = some ys ↔
      ∃ pre old post,
        xs = pre ++ (old ++ post) ∧
        pre.length = lo ∧ old.length = width ∧
        ys = pre ++ newBytes ++ post := by
  constructor
  · intro h
    cases hc : cut xs lo width with
    | none => simp [applyAt, hc] at h
    | some c =>
      have hs := cut_sound hc
      simp [applyAt, hc] at h
      subst ys
      exact ⟨c.pre, c.selected, c.post, hs.1, hs.2.1, hs.2.2,
        by simp [List.append_assoc]⟩
  · rintro ⟨pre, old, post, hxs, hlo, hwidth, hys⟩
    subst xs
    subst lo
    subst width
    subst ys
    exact apply_local pre old post newBytes

theorem applyAt_length {xs ys : Bytes} {lo width : Nat} {newBytes : Bytes}
    (h : applyAt xs lo width newBytes = some ys) :
    ys.length = xs.length - width + newBytes.length := by
  obtain ⟨pre, old, post, hxs, hlo, hwidth, hys⟩ :=
    (applyAt_eq_some_iff).1 h
  have hxlen : xs.length = lo + width + post.length := by
    simp [hxs, hlo, hwidth, Nat.add_assoc]
  have hylen : ys.length = lo + newBytes.length + post.length := by
    simp [hys, hlo, Nat.add_assoc]
  have hreorder : lo + width + post.length = (lo + post.length) + width := by
    simp [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
  calc
    ys.length = lo + newBytes.length + post.length := hylen
    _ = (lo + post.length) + newBytes.length := by
      simp [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
    _ = (lo + width + post.length) - width + newBytes.length := by
      rw [hreorder, Nat.add_sub_cancel]
    _ = xs.length - width + newBytes.length := by rw [hxlen]

theorem applyAt_some_iff {xs : Bytes} {lo width : Nat} {newBytes : Bytes} :
    (∃ ys, applyAt xs lo width newBytes = some ys) ↔
      lo ≤ xs.length ∧ width ≤ xs.length - lo := by
  constructor
  · rintro ⟨ys, hys⟩
    cases hc : cut xs lo width with
    | none => simp [applyAt, hc] at hys
    | some c => exact (cut_some_iff).1 ⟨c, hc⟩
  · intro hv
    obtain ⟨c, hc⟩ := (cut_some_iff).2 hv
    refine ⟨c.pre ++ newBytes ++ c.post, ?_⟩
    simp [applyAt, hc]

/-- Refusal extends unchanged from the cut to the whole splice. -/
theorem applyAt_eq_none_iff {xs : Bytes} {lo width : Nat} {newBytes : Bytes} :
    applyAt xs lo width newBytes = none ↔
      xs.length < lo ∨ xs.length - lo < width := by
  constructor
  · intro h
    have hcut : cut xs lo width = none := by
      cases hc : cut xs lo width with
      | none => rfl
      | some c => simp [applyAt, hc] at h
    exact (cut_eq_none_iff).1 hcut
  · intro hinv
    have hc := (cut_eq_none_iff).2 hinv
    simp [applyAt, hc]

theorem applyAt_eof (xs newBytes : Bytes) :
    applyAt xs xs.length 0 newBytes = some (xs ++ newBytes) := by
  simpa using apply_local xs ([] : Bytes) [] newBytes

-- Empty input accepts insertion, while a nonempty selection is refused.
example : splitExact 0 ([] : Bytes) = some ([], []) := rfl
example : splitExact 1 ([] : Bytes) = none := rfl
example : cut ([] : Bytes) 0 0 = some ⟨[], [], []⟩ := rfl
example : cut ([] : Bytes) 0 1 = none := rfl
example : applyAt ([] : Bytes) 0 0 [1, 2] = some [1, 2] := rfl
example : applyAt ([] : Bytes) 0 1 [1] = none := rfl

-- EOF insertion succeeds without inventing a selected byte.
example : cut ([0] : Bytes) 1 0 = some ⟨[0], [], []⟩ := rfl
example : applyAt ([0] : Bytes) 1 0 [1] = some [0, 1] := rfl

-- Reversed and over-EOF intervals refuse; neither is clamped to a shorter cut.
example : splitExact 2 ([0] : Bytes) = none := rfl
example : cut ([0] : Bytes) 2 0 = none := rfl
example : cut ([0] : Bytes) 1 1 = none := rfl
example : applyAt ([0] : Bytes) 1 1 [2] = none := rfl
example : cutRange ([0] : Bytes) 1 0 = none := rfl
example : cutRange ([0] : Bytes) 0 2 = none := rfl
example : cutRange ([0] : Bytes) 0 1 = some ⟨[], [0], []⟩ := rfl

end Splicery
