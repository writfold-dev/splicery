import Splicery.Core

namespace Splicery

/-- Pieces have already been resolved against one immutable authorized capture.
    Establishing that precondition is a separate obligation. -/
inductive Piece where
  | lit (b : Bytes)
  | captured (b : Bytes)
  deriving Repr

def render : List Piece → Bytes
  | [] => []
  | .lit b :: xs => b ++ render xs
  | .captured b :: xs => b ++ render xs

inductive Command where
  | replace (parts : List Piece)
  | delete
  | before (parts : List Piece)
  | after (parts : List Piece)

/-- Direct surface interpretation on a previously checked decomposition. -/
def denote (c : Cut) : Command → Bytes
  | .replace ps => c.pre ++ (render ps ++ c.post)
  | .delete => c.pre ++ c.post
  | .before ps => c.pre ++ (render ps ++ (c.selected ++ c.post))
  | .after ps => (c.pre ++ c.selected) ++ (render ps ++ c.post)

structure Prim where
  lo : Nat
  width : Nat
  payload : Bytes

def lower (c : Cut) : Command → Prim
  | .replace ps => ⟨c.pre.length, c.selected.length, render ps⟩
  | .delete => ⟨c.pre.length, c.selected.length, []⟩
  | .before ps => ⟨c.pre.length, 0, render ps⟩
  | .after ps => ⟨(c.pre ++ c.selected).length, 0, render ps⟩

private theorem insert_at_cut (p q newBytes : Bytes) :
    applyAt (p ++ q) p.length 0 newBytes = some (p ++ newBytes ++ q) := by
  simpa using apply_local p [] q newBytes

theorem lower_correct (c : Cut) (cmd : Command) :
    let op := lower c cmd
    applyAt (c.pre ++ (c.selected ++ c.post)) op.lo op.width op.payload =
      some (denote c cmd) := by
  cases c with
  | mk p s q =>
    cases cmd with
    | replace ps => simp [lower, denote, apply_local, List.append_assoc]
    | delete => simp [lower, denote, apply_local]
    | before ps => simp [lower, denote, insert_at_cut, List.append_assoc]
    | after ps =>
      change applyAt (p ++ (s ++ q)) (p ++ s).length 0 (render ps) =
        some ((p ++ s) ++ (render ps ++ q))
      calc
        _ = applyAt ((p ++ s) ++ q) (p ++ s).length 0 (render ps) := by
            rw [List.append_assoc p s q]
        _ = _ := by simpa only [List.append_assoc] using insert_at_cut (p ++ s) q (render ps)

end Splicery
