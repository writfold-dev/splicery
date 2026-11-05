import Splicery.Core

namespace Splicery

/-- A normalized batch stores untouched gaps and resolved disjoint edits in
    original order. Numeric validation and order normalization are separate. -/
inductive Batch where
  | tail (rest : Bytes)
  | step (gap old replacement : Bytes) (more : Batch)

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

end Splicery
