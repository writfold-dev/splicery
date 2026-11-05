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

def applyAt (xs : Bytes) (lo width : Nat) (newBytes : Bytes) : Option Bytes := do
  let c ← cut xs lo width
  pure (c.pre ++ newBytes ++ c.post)

theorem apply_local (p old q newBytes : Bytes) :
    applyAt (p ++ (old ++ q)) p.length old.length newBytes =
      some (p ++ newBytes ++ q) := by simp [applyAt, cut_of_parts]

theorem keep_identity (p s q : Bytes) :
    applyAt (p ++ (s ++ q)) p.length s.length s = some (p ++ (s ++ q)) := by
  simp [apply_local, List.append_assoc]

end Splicery
