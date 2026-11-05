import Splicery.Lowering
import Lean.Elab.Tactic.Omega

namespace Splicery.Surface

/-!
# Independent /1 surface semantics

Coverage: L04 models original-image regions, named (including absent and present-empty)
groups, separate read/write capabilities, and same-image, stale-image and read-only-target
refusal.  L06 models source-order lexical environments, duplicate/forward/shadow refusal,
iteration-local binders, overlapping `one` search, `range`/`between`/`within`, and complete
frozen matcher inventories with `expect`/`one` cardinality and truncation/work-budget refusal.
L07 independently decodes quoted and hexadecimal literals, retains heredoc bodies byte for byte,
checks text boundaries, scans LF/CRLF logical lines without a synthetic final line, and defines
`lines like` and `indent`. Raw mode rejects text/line/regex/indent operations; text mode checks
both capture boundaries and the exact candidate. `denoteChange` is defined using `surfaceCut` and
`applySurface`, not `Core.applyAt`, `Lowering.lower`, or `Lowering.denote`; the final theorems
relate that direct meaning to all four lowered commands and payload forms.

Trusted boundaries are data, not axioms: `TextOracle` packages sound strict-UTF-8/image-boundary
decisions; a production oracle must instantiate its predicates with the named strict UTF-8 decoder.
Matcher evidence is untrusted until `freeze` checks its origin, exact scope, completeness,
non-truncation, cardinality and conservative work charge.  Completeness, leftmost-first regex
semantics, unique lexical group declarations supplied by the lexer, delimiter extraction for
HEREDOC, exact recipe-origin spans, principal/run/epoch authorization, and the claimed work charge
remain host/lexer/matcher obligations.  `heredoc` receives the exact content span after that
checked extraction; it does not parse delimiters.  Pure functions here neither authorize IO nor
claim physical resource usage.  Bounds use unbounded Lean naturals; a host must range-check before
allocation.  Invalid profile/boundary/origin/capability/cardinality/budget inputs return `none`,
so there is no partial candidate.
-/

/-- A text decoder is trusted only through its explicitly stated soundness interface. -/
structure TextOracle where
  UTF8 : Bytes → Prop
  Boundary : Bytes → Nat → Prop
  valid : Bytes → Bool
  boundary : Bytes → Nat → Bool
  valid_sound : ∀ b, valid b = true → UTF8 b
  boundary_sound : ∀ b n, boundary b n = true → Boundary b n

inductive Profile where
  | text
  | raw
  deriving DecidableEq, Repr

inductive LineInfo where
  | unterminated
  | lf
  | crlf
  deriving DecidableEq, Repr

def LineInfo.bytes : LineInfo → Option Bytes
  | .unterminated => none
  | .lf => some [10]
  | .crlf => some [13, 10]

structure Region where
  image : Nat
  lo : Nat
  hi : Nat
  readable : Bool
  writable : Bool
  deriving DecidableEq, Repr

structure NamedGroup where
  name : Bytes
  region : Option Region
  deriving DecidableEq, Repr

structure Capture where
  region : Region
  line : Option LineInfo := none
  groups : List NamedGroup := []
  deriving DecidableEq, Repr

def Capture.ofRegion (r : Region) : Capture := { region := r }

structure Ref where
  name : Bytes
  group : Option Bytes := none
  deriving DecidableEq, Repr

structure Image where
  id : Nat
  bytes : Bytes
  readable : Bool
  writable : Bool
  deriving DecidableEq, Repr

/-- An independent checked decomposition, deliberately not `Core.cut`. -/
structure SurfaceCut where
  pre : Bytes
  selected : Bytes
  post : Bytes
  deriving DecidableEq, Repr

def surfaceCut (xs : Bytes) (lo hi : Nat) : Option SurfaceCut :=
  if lo ≤ hi ∧ hi ≤ xs.length then
    some ⟨xs.take lo, (xs.drop lo).take (hi - lo), xs.drop hi⟩
  else none

theorem surfaceCut_sound {xs : Bytes} {lo hi : Nat} {c : SurfaceCut}
    (h : surfaceCut xs lo hi = some c) :
    xs = c.pre ++ (c.selected ++ c.post) ∧
    c.pre.length = lo ∧ c.selected.length = hi - lo := by
  by_cases hb : lo ≤ hi ∧ hi ≤ xs.length
  · simp [surfaceCut, hb] at h
    subst c
    change xs = xs.take lo ++ ((xs.drop lo).take (hi - lo) ++ xs.drop hi) ∧
      (xs.take lo).length = lo ∧
      ((xs.drop lo).take (hi - lo)).length = hi - lo
    have hlo : lo ≤ xs.length := by omega
    have hw : hi - lo ≤ (xs.drop lo).length := by
      simp only [List.length_drop]
      omega
    have hd : (xs.drop lo).drop (hi - lo) = xs.drop hi := by
      rw [List.drop_drop]
      congr 1
      omega
    refine ⟨?_, by simp [List.length_take, Nat.min_eq_left hlo], ?_⟩
    · symm
      calc
        xs.take lo ++ ((xs.drop lo).take (hi - lo) ++ xs.drop hi) =
            xs.take lo ++ ((xs.drop lo).take (hi - lo) ++
              (xs.drop lo).drop (hi - lo)) := by rw [← hd]
        _ = xs.take lo ++ xs.drop lo := by rw [List.take_append_drop]
        _ = xs := List.take_append_drop lo xs
    · simp only [List.length_drop] at hw
      simp [List.length_take, Nat.min_eq_left hw]
  · simp [surfaceCut, hb] at h

def sourceSlice (xs : Bytes) (lo hi : Nat) : Option Bytes := do
  let c ← surfaceCut xs lo hi
  pure c.selected

/-- Rejecting a bound is not clamping it. -/
theorem surfaceCut_rejects_reversed (xs : Bytes) {lo hi : Nat} (h : hi < lo) :
    surfaceCut xs lo hi = none := by
  simp [surfaceCut, Nat.not_le.mpr h]

theorem surfaceCut_rejects_overflow (xs : Bytes) {lo hi : Nat}
    (h : xs.length < hi) : surfaceCut xs lo hi = none := by
  simp [surfaceCut, Nat.not_le.mpr h]

private def byteMember (x : UInt8) : Bytes → Bool
  | [] => false
  | y :: ys => if x = y then true else byteMember x ys

def nameMember (x : Bytes) : List Bytes → Bool
  | [] => false
  | y :: ys => if x = y then true else nameMember x ys

private def namesUnique : List Bytes → Bool
  | [] => true
  | x :: xs => !nameMember x xs && namesUnique xs

def profileOK : Profile → TextOracle → Bytes → Nat → Nat → Bool
  | .raw, _, _, _, _ => true
  | .text, oracle, b, lo, hi =>
      oracle.valid b && oracle.boundary b lo && oracle.boundary b hi

theorem text_profile_boundaries {oracle : TextOracle} {b : Bytes} {lo hi : Nat}
    (h : profileOK .text oracle b lo hi = true) :
    oracle.UTF8 b ∧ oracle.Boundary b lo ∧ oracle.Boundary b hi := by
  simp only [profileOK, Bool.and_eq_true] at h
  exact ⟨oracle.valid_sound b h.1.1,
    oracle.boundary_sound b lo h.1.2, oracle.boundary_sound b hi h.2⟩

private def groupOriginsOK (parent : Region) : List NamedGroup → Bool
  | [] => true
  | g :: gs =>
      (match g.region with
       | none => true
       | some r => decide (r.image = parent.image ∧ parent.lo ≤ r.lo ∧
           r.lo ≤ r.hi ∧ r.hi ≤ parent.hi)) && groupOriginsOK parent gs

private def lineOriginOK (image : Image) (r : Region) : Option LineInfo → Bool
  | none => true
  | some info =>
      match info with
      | .unterminated => decide (r.hi = image.bytes.length)
      | .lf => decide (sourceSlice image.bytes r.hi (r.hi + 1) = some [10])
      | .crlf => decide (sourceSlice image.bytes r.hi (r.hi + 2) = some [13, 10])

private def originOK (image : Image) (profile : Profile) (oracle : TextOracle)
    (c : Capture) : Bool :=
  decide (c.region.image = image.id ∧ c.region.lo ≤ c.region.hi ∧
    c.region.hi ≤ image.bytes.length) &&
  profileOK profile oracle image.bytes c.region.lo c.region.hi &&
  lineOriginOK image c.region c.line &&
  groupOriginsOK c.region c.groups && namesUnique (c.groups.map NamedGroup.name)

def checkCapture (image : Image) (profile : Profile) (oracle : TextOracle)
    (c : Capture) : Option Unit :=
  if originOK image profile oracle c then some () else none

def readCapture (image : Image) (profile : Profile) (oracle : TextOracle)
    (c : Capture) : Option Bytes := do
  let _ ← checkCapture image profile oracle c
  if image.readable && c.region.readable then
    sourceSlice image.bytes c.region.lo c.region.hi
  else none

def checkTarget (image : Image) (profile : Profile) (oracle : TextOracle)
    (c : Capture) : Option Unit := do
  let _ ← checkCapture image profile oracle c
  if image.writable && c.region.writable then some () else none

theorem readCapture_original {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} {b : Bytes} (h : readCapture image profile oracle c = some b) :
    sourceSlice image.bytes c.region.lo c.region.hi = some b := by
  cases hc : checkCapture image profile oracle c with
  | none => simp [readCapture, hc] at h
  | some _ =>
    simp [readCapture, hc] at h
    exact h.2

/-- A stale or foreign capture never becomes readable by equal offsets alone. -/
theorem readCapture_stale {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} (h : c.region.image ≠ image.id) :
    readCapture image profile oracle c = none := by
  simp [readCapture, checkCapture, originOK, h]

theorem checkTarget_stale {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} (h : c.region.image ≠ image.id) :
    checkTarget image profile oracle c = none := by
  simp [checkTarget, checkCapture, originOK, h]

theorem checkTarget_read_only {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} (h : c.region.writable = false) :
    checkTarget image profile oracle c = none := by
  cases checkCapture image profile oracle c <;> simp [checkTarget, h]

theorem readCapture_image_read_only {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} (h : image.readable = false) :
    readCapture image profile oracle c = none := by
  cases checkCapture image profile oracle c <;> simp [readCapture, h]

theorem checkTarget_image_read_only {image : Image} {profile : Profile} {oracle : TextOracle}
    {c : Capture} (h : image.writable = false) :
    checkTarget image profile oracle c = none := by
  cases checkCapture image profile oracle c <;> simp [checkTarget, h]

/-- Lexical environments contain only earlier, immutable bindings. -/
abbrev Env := List (Bytes × Capture)

def lookup : Env → Bytes → Option Capture
  | [], _ => none
  | (n, c) :: env, name => if n = name then some c else lookup env name

def extend (env : Env) (name : Bytes) (c : Capture) : Option Env :=
  if nameMember name (env.map Prod.fst) then none else some ((name, c) :: env)

theorem extend_refuses_duplicate {env : Env} {name : Bytes} {c : Capture}
    (h : nameMember name (env.map Prod.fst) = true) : extend env name c = none := by
  simp [extend, h]

/-- The callback sees the binder, while the returned value contains no extended environment. -/
def withBinder (env : Env) (name : Bytes) (c : Capture)
    (body : Env → Option α) : Option α := do
  let scopedEnv ← extend env name c
  body scopedEnv

def findGroup : List NamedGroup → Bytes → Option NamedGroup
  | [], _ => none
  | g :: gs, name => if g.name = name then some g else findGroup gs name

def resolveRef (env : Env) (ref : Ref) : Option Capture := do
  let c ← lookup env ref.name
  match ref.group with
  | none => pure c
  | some name => do
    let g ← findGroup c.groups name
    let r ← g.region
    if r.image = c.region.image ∧ c.region.lo ≤ r.lo ∧ r.lo ≤ r.hi ∧
        r.hi ≤ c.region.hi then pure (Capture.ofRegion r) else none

/-- An unmatched optional group is refusal, not empty bytes. -/
theorem resolveRef_unmatched_group {env : Env} {name group : Bytes} {c : Capture}
    {g : NamedGroup} (hc : lookup env name = some c)
    (hg : findGroup c.groups group = some g) (hr : g.region = none) :
    resolveRef env ⟨name, some group⟩ = none := by
  simp [resolveRef, hc, hg, hr]

/-- Quoted decoding copies ordinary UTF-8 bytes and recognizes exactly the five /1 escapes. -/
private def plainQuotedByte (c : UInt8) : Bool :=
  decide (32 ≤ c.toNat) && decide (c ≠ 34) && decide (c ≠ 92)

def plainQuoted : Bytes → Bool
  | [] => true
  | c :: cs => plainQuotedByte c && plainQuoted cs

def decodeQuoted : Bytes → Option Bytes
  | [] => some []
  | c :: cs =>
      if plainQuotedByte c then do
        let tail ← decodeQuoted cs
        pure (c :: tail)
      else if c = 92 then
        match cs with
        | [] => none
        | e :: rest =>
            match e.toNat with
            | 34 => do let tail ← decodeQuoted rest; pure (34 :: tail)
            | 92 => do let tail ← decodeQuoted rest; pure (92 :: tail)
            | 110 => do let tail ← decodeQuoted rest; pure (10 :: tail)
            | 114 => do let tail ← decodeQuoted rest; pure (13 :: tail)
            | 116 => do let tail ← decodeQuoted rest; pure (9 :: tail)
            | _ => none
      else none

theorem decodeQuoted_plain {b : Bytes} (h : plainQuoted b = true) :
    decodeQuoted b = some b := by
  induction b with
  | nil => rfl
  | cons c cs ih =>
    simp only [plainQuoted, Bool.and_eq_true] at h
    rw [decodeQuoted.eq_def]
    simp [h.1, ih h.2]

/-- HEREDOC's trusted lexer supplies exactly the bytes between delimiter lines. -/
def decodeHeredoc (body : Bytes) : Option Bytes := some body

theorem heredoc_exact (body : Bytes) : decodeHeredoc body = some body := rfl

def hexNibble (c : UInt8) : Option Nat :=
  if 48 ≤ c.toNat ∧ c.toNat ≤ 57 then some (c.toNat - 48)
  else if 65 ≤ c.toNat ∧ c.toNat ≤ 70 then some (c.toNat - 65 + 10)
  else if 97 ≤ c.toNat ∧ c.toNat ≤ 102 then some (c.toNat - 97 + 10)
  else none

def decodeHex : Bytes → Option Bytes
  | [] => some []
  | [_] => none
  | a :: b :: rest => do
    let x ← hexNibble a
    let y ← hexNibble b
    let tail ← decodeHex rest
    pure (UInt8.ofNat (16 * x + y) :: tail)

theorem decodeHex_pair (a b : UInt8) (rest : Bytes) :
    decodeHex (a :: b :: rest) = (do
      let x ← hexNibble a
      let y ← hexNibble b
      let tail ← decodeHex rest
      pure (UInt8.ofNat (16 * x + y) :: tail)) := rfl

/-- `lineBreak` recognizes LF and CRLF; a lone CR is ordinary body content. -/
private def lineBreak : Bytes → Bytes × Option Bytes × Bytes
  | [] => ([], none, [])
  | 10 :: xs => ([], some [10], xs)
  | 13 :: 10 :: xs => ([], some [13, 10], xs)
  | [x] => ([x], none, [])
  | x :: xs =>
      let (body, eol, rest) := lineBreak xs
      (x :: body, eol, rest)

structure LogicalLine where
  body : Bytes
  eol : Option Bytes
  deriving DecidableEq, Repr

structure LocatedLine where
  lo : Nat
  line : LogicalLine
  deriving DecidableEq, Repr

private def scanLines : Nat → Nat → Bytes → List LocatedLine
  | 0, _, _ => []
  | _, _, [] => []
  | fuel + 1, pos, xs =>
      let (body, eol, rest) := lineBreak xs
      let used := body.length + (eol.map List.length).getD 0
      ⟨pos, ⟨body, eol⟩⟩ :: scanLines fuel (pos + used) rest

/-- The fuel is exactly input length plus one, so it is not a preview/truncation limit. -/
def locatedLines (b : Bytes) : List LocatedLine := scanLines (b.length + 1) 0 b

def logicalLines (b : Bytes) : List LogicalLine :=
  (locatedLines b).map LocatedLine.line

private def startsWith : Bytes → Bytes → Bool
  | [], _ => true
  | _ :: _, [] => false
  | x :: xs, y :: ys => if x = y then startsWith xs ys else false

/-- Every start is visited, including overlapping starts; no first-hit truncation. -/
private def occurrencesFrom (needle : Bytes) : Nat → Bytes → List Nat
  | _, [] => []
  | pos, _x :: xs =>
      let tail := occurrencesFrom needle (pos + 1) xs
      if startsWith needle (_x :: xs) then pos :: tail else tail

private def lineInfoOfEOL : Option Bytes → Option LineInfo
  | none => some .unterminated
  | some [10] => some .lf
  | some [13, 10] => some .crlf
  | _ => none

inductive Selector where
  | reference (ref : Ref)
  | range (lo hi : Nat)
  | oneText (quotedNeedle : Bytes) (within : Option Ref)
  | oneLine (quotedBody : Bytes) (within : Option Ref)
  | oneHex (hexNeedle : Bytes) (within : Option Ref)
  | between (first second : Ref)
  deriving Repr

private def rootCapture (image : Image) : Capture :=
  { region := ⟨image.id, 0, image.bytes.length, image.readable, image.writable⟩ }

private def selectionWindow (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) : Option Ref → Option Capture
  | none => some (rootCapture image)
  | some ref => do
    let c ← resolveRef env ref
    let _ ← checkCapture image profile oracle c
    pure c

private def selectOneBytes (image : Image) (profile : Profile) (oracle : TextOracle)
    (window : Capture) (needle : Bytes) : Option Capture := do
  if needle = [] then none else do
    let b ← sourceSlice image.bytes window.region.lo window.region.hi
    match occurrencesFrom needle window.region.lo b with
    | [lo] =>
        let c : Capture :=
          { region := ⟨image.id, lo, lo + needle.length,
            window.region.readable, window.region.writable⟩ }
        let _ ← checkCapture image profile oracle c
        pure c
    | _ => none

private def selectOneLine (image : Image) (profile : Profile) (oracle : TextOracle)
    (window : Capture) (needle : Bytes) : Option Capture := do
  let hits := (locatedLines image.bytes).filter (fun x =>
    decide (window.region.lo ≤ x.lo ∧
      x.lo + x.line.body.length ≤ window.region.hi) && x.line.body == needle)
  match hits with
  | [x] => do
      let info ← lineInfoOfEOL x.line.eol
      let c : Capture :=
        { region := ⟨image.id, x.lo, x.lo + needle.length,
          window.region.readable, window.region.writable⟩, line := some info }
      let _ ← checkCapture image profile oracle c
      pure c
  | _ => none

def select (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) : Selector → Option Capture
  | .reference ref => do
      let c ← resolveRef env ref
      let _ ← checkCapture image profile oracle c
      pure c
  | .range lo hi => do
      let c : Capture :=
        { region := ⟨image.id, lo, hi, image.readable, image.writable⟩ }
      let _ ← checkCapture image profile oracle c
      pure c
  | .oneText q within => do
      if profile = .text then do
        let needle ← decodeQuoted q
        let w ← selectionWindow image profile oracle env within
        selectOneBytes image profile oracle w needle
      else none
  | .oneHex h within => do
      let needle ← decodeHex h
      let w ← selectionWindow image profile oracle env within
      selectOneBytes image profile oracle w needle
  | .oneLine q within => do
      if profile = .text then do
        let needle ← decodeQuoted q
        let w ← selectionWindow image profile oracle env within
        selectOneLine image profile oracle w needle
      else none
  | .between firstRef secondRef => do
      let first ← resolveRef env firstRef
      let _ ← checkCapture image profile oracle first
      let second ← resolveRef env secondRef
      let _ ← checkCapture image profile oracle second
      if first.region = second.region then none else do
        let info ← first.line
        let _ ← second.line
        let eol ← info.bytes
        let fullEnd := first.region.hi + eol.length
        if fullEnd ≤ second.region.lo then do
          let c : Capture :=
            { region := ⟨image.id, fullEnd, second.region.lo,
              image.readable, image.writable⟩ }
          let _ ← checkCapture image profile oracle c
          pure c
        else none

theorem oneHex_empty_needle_refuses (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (within : Option Ref) :
    select image profile oracle env (.oneHex [] within) = none := by
  cases hw : selectionWindow image profile oracle env within with
  | none => simp [select, decodeHex, hw]
  | some w => simp [select, decodeHex, hw, selectOneBytes]

/-- Complete, bounded matcher evidence; no live matcher is consulted during apply. -/
structure MatcherEvidence where
  image : Nat
  matcherProfile : Nat
  scope : Option Capture
  members : List Capture
  complete : Bool
  truncated : Bool
  work : Nat
  limit : Nat
  deriving Repr

structure Frozen where
  image : Nat
  matcherProfile : Nat
  scope : Option Capture
  members : List Capture
  deriving DecidableEq, Repr

private def frozenScopeOK (image : Image) (profile : Profile) (oracle : TextOracle)
    : Option Capture → Bool
  | none => true
  | some c => originOK image profile oracle c

private def frozenMemberOK (image : Image) (profile : Profile) (oracle : TextOracle)
    (scope : Option Capture) (c : Capture) : Bool :=
  originOK image profile oracle c && decide (c.region.lo < c.region.hi) &&
  match scope with
  | none => true
  | some s => decide (c.region.image = s.region.image ∧
      s.region.lo ≤ c.region.lo ∧ c.region.hi ≤ s.region.hi)

private def frozenOrderOK : List Capture → Bool
  | [] => true
  | [_] => true
  | a :: b :: cs => decide (a.region.hi ≤ b.region.lo) && frozenOrderOK (b :: cs)

private def frozenInventoryOK (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) : Bool :=
  ev.complete && !ev.truncated && decide (ev.work ≤ ev.limit) &&
    decide (profile ≠ .raw) && decide (ev.image = image.id) &&
    frozenScopeOK image profile oracle ev.scope &&
    decide (ev.members.length = expected) && frozenOrderOK ev.members &&
    ev.members.all (frozenMemberOK image profile oracle ev.scope)

def freeze (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) : Option Frozen :=
  if frozenInventoryOK image profile oracle ev expected then
    some ⟨ev.image, ev.matcherProfile, ev.scope, ev.members⟩
  else none

theorem freeze_members {image : Image} {profile : Profile} {oracle : TextOracle}
    {ev : MatcherEvidence} {expected : Nat} {f : Frozen}
    (h : freeze image profile oracle ev expected = some f) :
    f.members = ev.members ∧ f.scope = ev.scope ∧ f.matcherProfile = ev.matcherProfile := by
  by_cases hb : frozenInventoryOK image profile oracle ev expected
  · simp [freeze, hb] at h
    subst f
    exact ⟨rfl, rfl, rfl⟩
  · simp [freeze, hb] at h

theorem frozen_complete_membership {image : Image} {profile : Profile} {oracle : TextOracle}
    {ev : MatcherEvidence} {expected : Nat} {f : Frozen} (c : Capture)
    (h : freeze image profile oracle ev expected = some f) :
    c ∈ f.members ↔ c ∈ ev.members := by rw [(freeze_members h).1]

theorem raw_no_frozen (image : Image) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) :
    freeze image .raw oracle ev expected = none := by simp [freeze, frozenInventoryOK]

theorem incomplete_no_frozen (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) (h : ev.complete = false) :
    freeze image profile oracle ev expected = none := by simp [freeze, frozenInventoryOK, h]

theorem truncated_no_frozen (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) (h : ev.truncated = true) :
    freeze image profile oracle ev expected = none := by simp [freeze, frozenInventoryOK, h]

theorem over_budget_no_frozen (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) (h : ev.limit < ev.work) :
    freeze image profile oracle ev expected = none := by
  simp [freeze, frozenInventoryOK, Nat.not_le.mpr h]

theorem wrong_expect_no_frozen (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) (expected : Nat) (h : ev.members.length ≠ expected) :
    freeze image profile oracle ev expected = none := by simp [freeze, frozenInventoryOK, h]

def oneFrozen (image : Image) (profile : Profile) (oracle : TextOracle)
    (ev : MatcherEvidence) : Option Capture := do
  let f ← freeze image profile oracle ev 1
  match f.members with
  | [c] => some c
  | _ => none

/-- Iteration cannot leak or shadow its binder, and one failed body refuses the whole expansion. -/
def eachFrozen (env : Env) (name : Bytes) (f : Frozen)
    (body : Env → Capture → Option α) : Option (List α) :=
  if nameMember name (env.map Prod.fst) then none else
    let rec go : List Capture → Option (List α)
      | [] => some []
      | c :: cs => do
        let x ← withBinder env name c (fun scopedEnv => body scopedEnv c)
        let xs ← go cs
        pure (x :: xs)
    go f.members

theorem eachFrozen_shadow_refusal {env : Env} {name : Bytes} {f : Frozen}
    {body : Env → Capture → Option α}
    (h : nameMember name (env.map Prod.fst) = true) :
    eachFrozen env name f body = none := by simp [eachFrozen, h]

private def mapOption (f : α → Option β) : List α → Option (List β)
  | [] => some []
  | x :: xs => do
    let y ← f x
    let ys ← mapOption f xs
    pure (y :: ys)

private def suppliedLineOK (b : Bytes) : Bool := !byteMember 10 b && !byteMember 13 b

private def decodeSuppliedLine (q : Bytes) : Option Bytes := do
  let b ← decodeQuoted q
  if suppliedLineOK b then some b else none

def renderLinesLike (eol : Bytes) : List Bytes → Bytes
  | [] => []
  | b :: bs => b ++ (eol ++ renderLinesLike eol bs)

/-- The final supplied line receives exactly the original line terminator too. -/
theorem renderLinesLike_last (eol : Bytes) (bs : List Bytes) (b : Bytes) :
    renderLinesLike eol (bs ++ [b]) = (renderLinesLike eol bs) ++ (b ++ eol) := by
  induction bs with
  | nil => simp [renderLinesLike]
  | cons x xs ih => simp [renderLinesLike, ih, List.append_assoc]

def denoteLinesLike (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (ref : Ref) (quoted : List Bytes) : Option Bytes :=
  if profile = .raw then none else do
    let c ← resolveRef env ref
    let _ ← readCapture image profile oracle c
    let info ← c.line
    let eol ← info.bytes
    let bodies ← mapOption decodeSuppliedLine quoted
    pure (renderLinesLike eol bodies)

def flattenLines : List LogicalLine → Bytes
  | [] => []
  | l :: ls => l.body ++ (l.eol.getD [] ++ flattenLines ls)

def indentLine (indentPrefix : Bytes) (l : LogicalLine) : LogicalLine :=
  ⟨(if l.body = [] then [] else indentPrefix ++ l.body), l.eol⟩

/-- Empty logical bodies, but not whitespace-only bodies, remain unprefixed. -/
def indentBytes (indentPrefix : Bytes) (b : Bytes) : Bytes :=
  flattenLines ((logicalLines b).map (indentLine indentPrefix))

theorem indent_preserves_recorded_eols (indentPrefix : Bytes) (lines : List LogicalLine) :
    ((lines.map (indentLine indentPrefix)).map LogicalLine.eol) = lines.map LogicalLine.eol := by
  induction lines with
  | nil => rfl
  | cons l ls ih => simp [indentLine, ih]

theorem indent_empty_body (indentPrefix : Bytes) (eol : Option Bytes) :
    indentLine indentPrefix ⟨[], eol⟩ = ⟨[], eol⟩ := rfl

private def prefixOK (b : Bytes) : Bool :=
  b.all (fun c => c == 32 || c == 9)

inductive SurfacePiece where
  | quoted (sourceBody : Bytes)
  | heredoc (exactBody : Bytes)
  | hex (sourceBody : Bytes)
  | keep (ref : Ref)
  | linesLike (ref : Ref) (quotedLines : List Bytes)
  | indent (quotedPrefix : Bytes) (payload : List SurfacePiece)
  deriving Repr

abbrev Payload := List SurfacePiece

mutual
  def denotePayload (image : Image) (profile : Profile) (oracle : TextOracle)
      (env : Env) : Payload → Option Bytes
    | [] => some []
    | p :: ps => do
      let b ← denotePiece image profile oracle env p
      let rest ← denotePayload image profile oracle env ps
      pure (b ++ rest)

  def denotePiece (image : Image) (profile : Profile) (oracle : TextOracle)
      (env : Env) : SurfacePiece → Option Bytes
    | .quoted q => decodeQuoted q
    | .heredoc b => decodeHeredoc b
    | .hex h => decodeHex h
    | .keep ref => do
        let c ← resolveRef env ref
        readCapture image profile oracle c
    | .linesLike ref qs => denoteLinesLike image profile oracle env ref qs
    | .indent q ps =>
        if profile = .raw then none else do
          let indentPrefix ← decodeQuoted q
          if prefixOK indentPrefix then do
            let b ← denotePayload image profile oracle env ps
            pure (indentBytes indentPrefix b)
          else none
end

mutual
  def lowerPayload (image : Image) (profile : Profile) (oracle : TextOracle)
      (env : Env) : Payload → Option (List Piece)
    | [] => some []
    | p :: ps => do
      let piece ← lowerPiece image profile oracle env p
      let pieces ← lowerPayload image profile oracle env ps
      pure (piece :: pieces)

  def lowerPiece (image : Image) (profile : Profile) (oracle : TextOracle)
      (env : Env) : SurfacePiece → Option Piece
    | .quoted q => do let b ← decodeQuoted q; pure (.lit b)
    | .heredoc b => pure (.lit b)
    | .hex h => do let b ← decodeHex h; pure (.lit b)
    | .keep ref => do
        let c ← resolveRef env ref
        let b ← readCapture image profile oracle c
        pure (.captured b)
    | .linesLike ref qs => do
        let b ← denoteLinesLike image profile oracle env ref qs
        pure (.lit b)
    | .indent q ps =>
        if profile = .raw then none else do
          let indentPrefix ← decodeQuoted q
          if prefixOK indentPrefix then do
            let pieces ← lowerPayload image profile oracle env ps
            pure (.lit (indentBytes indentPrefix (render pieces)))
          else none
end

mutual
  theorem lowerPayload_sound {image : Image} {profile : Profile} {oracle : TextOracle}
      {env : Env} {payload : Payload} {pieces : List Piece}
      (h : lowerPayload image profile oracle env payload = some pieces) :
      denotePayload image profile oracle env payload = some (render pieces) := by
    cases payload with
    | nil =>
      simp [lowerPayload] at h
      subst pieces
      rfl
    | cons p ps =>
      cases hp : lowerPiece image profile oracle env p with
      | none => simp [lowerPayload, hp] at h
      | some piece =>
        cases hs : lowerPayload image profile oracle env ps with
        | none => simp [lowerPayload, hp, hs] at h
        | some rest =>
          have h1 := lowerPiece_sound hp
          have h2 := lowerPayload_sound hs
          simp [lowerPayload, hp, hs] at h
          subst pieces
          cases piece <;> simp [denotePayload, h1, h2, render]

  theorem lowerPiece_sound {image : Image} {profile : Profile} {oracle : TextOracle}
      {env : Env} {piece : SurfacePiece} {lowered : Piece}
      (h : lowerPiece image profile oracle env piece = some lowered) :
      denotePiece image profile oracle env piece = some (render [lowered]) := by
    cases piece with
    | quoted q =>
      cases hq : decodeQuoted q with
      | none => simp [lowerPiece, hq] at h
      | some b => simp [lowerPiece, hq] at h; subst lowered; simp [denotePiece, hq, render]
    | heredoc b => simp [lowerPiece] at h; subst lowered; simp [denotePiece, decodeHeredoc, render]
    | hex x =>
      cases hx : decodeHex x with
      | none => simp [lowerPiece, hx] at h
      | some b => simp [lowerPiece, hx] at h; subst lowered; simp [denotePiece, hx, render]
    | keep ref =>
      cases hr : resolveRef env ref with
      | none => simp [lowerPiece, hr] at h
      | some c =>
        cases hb : readCapture image profile oracle c with
        | none => simp [lowerPiece, hr, hb] at h
        | some b =>
          simp [lowerPiece, hr, hb] at h
          subst lowered
          simp [denotePiece, hr, hb, render]
    | linesLike ref qs =>
      cases hb : denoteLinesLike image profile oracle env ref qs with
      | none => simp [lowerPiece, hb] at h
      | some b => simp [lowerPiece, hb] at h; subst lowered; simp [denotePiece, hb, render]
    | indent q ps =>
      cases profile with
      | raw => simp [lowerPiece] at h
      | text =>
        cases hq : decodeQuoted q with
        | none => simp [lowerPiece, hq] at h
        | some indentPrefix =>
          by_cases hf : prefixOK indentPrefix
          · cases hs : lowerPayload image .text oracle env ps with
            | none => simp [lowerPiece, hq, hf, hs] at h
            | some rest =>
              have hi := lowerPayload_sound hs
              simp [lowerPiece, hq, hf, hs] at h
              subst lowered
              simp [denotePiece, hq, hf, hi, render]
          · simp [lowerPiece, hq, hf] at h
end

inductive Change where
  | replace (target : Selector) (payload : Payload)
  | delete (target : Selector)
  | before (target : Selector) (payload : Payload)
  | after (target : Selector) (payload : Payload)
  deriving Repr

inductive ChangeKind where
  | replace | delete | before | after
  deriving Repr

def Change.target : Change → Selector
  | .replace t _ => t
  | .delete t => t
  | .before t _ => t
  | .after t _ => t

def Change.payload : Change → Payload
  | .replace _ p => p
  | .delete _ => []
  | .before _ p => p
  | .after _ p => p

def Change.kind : Change → ChangeKind
  | .replace _ _ => .replace
  | .delete _ => .delete
  | .before _ _ => .before
  | .after _ _ => .after

def selectedCut (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (target : Selector) : Option SurfaceCut := do
  let c ← select image profile oracle env target
  let _ ← checkTarget image profile oracle c
  surfaceCut image.bytes c.region.lo c.region.hi

/-- Direct command meaning; no primitive lowering or application is used here. -/
def applySurface (c : SurfaceCut) : ChangeKind → Bytes → Bytes
  | .replace, b => c.pre ++ (b ++ c.post)
  | .delete, _ => c.pre ++ c.post
  | .before, b => c.pre ++ (b ++ (c.selected ++ c.post))
  | .after, b => (c.pre ++ c.selected) ++ (b ++ c.post)

def candidateOK : Profile → TextOracle → Bytes → Bool
  | .raw, _, _ => true
  | .text, oracle, b => oracle.valid b

def denoteChange (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (change : Change) : Option Bytes := do
  let c ← selectedCut image profile oracle env change.target
  let b ← denotePayload image profile oracle env change.payload
  let out := applySurface c change.kind b
  if candidateOK profile oracle out then some out else none

def toCoreCut (c : SurfaceCut) : Cut := ⟨c.pre, c.selected, c.post⟩

def toCommand (kind : ChangeKind) (parts : List Piece) : Command :=
  match kind with
  | .replace => .replace parts
  | .delete => .delete
  | .before => .before parts
  | .after => .after parts

def lowerChange (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (change : Change) : Option (Cut × Command) := do
  let c ← selectedCut image profile oracle env change.target
  let ps ← lowerPayload image profile oracle env change.payload
  let out := applySurface c change.kind (render ps)
  if candidateOK profile oracle out then pure (toCoreCut c, toCommand change.kind ps) else none

theorem selectedCut_sound {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {target : Selector} {c : SurfaceCut}
    (h : selectedCut image profile oracle env target = some c) :
    image.bytes = c.pre ++ (c.selected ++ c.post) := by
  cases hs : select image profile oracle env target with
  | none => simp [selectedCut, hs] at h
  | some cap =>
    cases ht : checkTarget image profile oracle cap with
    | none => simp [selectedCut, hs, ht] at h
    | some _ =>
      cases hc : surfaceCut image.bytes cap.region.lo cap.region.hi with
      | none => simp [selectedCut, hs, ht, hc] at h
      | some cut =>
        simp [selectedCut, hs, ht, hc] at h
        subst c
        exact (surfaceCut_sound hc).1

/-- Surface-to-IR refinement for every command and every payload production. -/
theorem lowerChange_refines {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {change : Change} {compiled : Cut × Command}
    (h : lowerChange image profile oracle env change = some compiled) :
    denoteChange image profile oracle env change =
      some (denote compiled.1 compiled.2) := by
  cases hs : selectedCut image profile oracle env change.target with
  | none => simp [lowerChange, hs] at h
  | some c =>
    cases hp : lowerPayload image profile oracle env change.payload with
    | none => simp [lowerChange, hs, hp] at h
    | some ps =>
      have hd := lowerPayload_sound hp
      by_cases hb : candidateOK profile oracle (applySurface c change.kind (render ps))
      · cases compiled with
        | mk core command =>
          simp [lowerChange, hs, hp, hb] at h
          rcases h with ⟨rfl, rfl⟩
          cases change <;>
            simp only [Change.target, Change.payload, Change.kind, applySurface,
              List.append_assoc] at hs hp hd hb <;>
            simp [denoteChange, Change.target, Change.payload, Change.kind,
              hs, hd, hb, applySurface, toCoreCut, toCommand,
              denote, List.append_assoc]
      · simp [lowerChange, hs, hp, hb] at h

theorem lowerChange_components {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {change : Change} {compiled : Cut × Command}
    (h : lowerChange image profile oracle env change = some compiled) :
    ∃ c ps, selectedCut image profile oracle env change.target = some c ∧
      lowerPayload image profile oracle env change.payload = some ps ∧
      compiled = (toCoreCut c, toCommand change.kind ps) := by
  cases hs : selectedCut image profile oracle env change.target with
  | none => simp [lowerChange, hs] at h
  | some c =>
    cases hp : lowerPayload image profile oracle env change.payload with
    | none => simp [lowerChange, hs, hp] at h
    | some ps =>
      by_cases hb : candidateOK profile oracle (applySurface c change.kind (render ps))
      · have compiledExact : compiled = (toCoreCut c, toCommand change.kind ps) := by
          simpa [lowerChange, hs, hp, hb] using h.symm
        exact ⟨c, ps, by simp, by simp, compiledExact⟩
      · simp [lowerChange, hs, hp, hb] at h

/-- The independently denoted change executes as the primitive over the same original bytes. -/
theorem lowerChange_executes {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {change : Change} {compiled : Cut × Command}
    (h : lowerChange image profile oracle env change = some compiled) :
    let op := lower compiled.1 compiled.2
    applyAt image.bytes op.lo op.width op.payload = some (denote compiled.1 compiled.2) := by
  rcases lowerChange_components h with ⟨c, ps, hs, _hp, rfl⟩
  have hbytes := selectedCut_sound hs
  change image.bytes = (toCoreCut c).pre ++
    ((toCoreCut c).selected ++ (toCoreCut c).post) at hbytes
  rw [hbytes]
  exact lower_correct (toCoreCut c) (toCommand change.kind ps)

theorem lowerChange_end_to_end {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {change : Change} {compiled : Cut × Command}
    (h : lowerChange image profile oracle env change = some compiled) :
    ∃ out, denoteChange image profile oracle env change = some out ∧
      let op := lower compiled.1 compiled.2
      applyAt image.bytes op.lo op.width op.payload = some out := by
  exact ⟨denote compiled.1 compiled.2, lowerChange_refines h, lowerChange_executes h⟩

/-- A refused writable/origin check cannot expose either a direct or lowered candidate. -/
theorem target_refusal_no_candidate {image : Image} {profile : Profile} {oracle : TextOracle}
    {env : Env} {change : Change}
    (h : selectedCut image profile oracle env change.target = none) :
    denoteChange image profile oracle env change = none ∧
    lowerChange image profile oracle env change = none := by
  simp [denoteChange, lowerChange, h]

theorem invalid_text_candidate_refuses {image : Image} {oracle : TextOracle}
    {env : Env} {change : Change} {c : SurfaceCut} {b : Bytes}
    (hs : selectedCut image .text oracle env change.target = some c)
    (hd : denotePayload image .text oracle env change.payload = some b)
    (hv : oracle.valid (applySurface c change.kind b) = false) :
    denoteChange image .text oracle env change = none := by
  simp [denoteChange, hs, hd, candidateOK, hv]

/-! Concrete trusted oracles and captures used only by the nonvacuous examples below. -/
private def testOracle : TextOracle where
  UTF8 := fun _ => True
  Boundary := fun _ _ => True
  valid := fun _ => true
  boundary := fun _ _ => true
  valid_sound := by intros; trivial
  boundary_sound := by intros; trivial

private def noMiddleBoundary : TextOracle where
  UTF8 := fun _ => True
  Boundary := fun b n => n ≠ 1 ∧ n ≤ b.length
  valid := fun _ => true
  boundary := fun b n => decide (n ≠ 1 ∧ n ≤ b.length)
  valid_sound := by intros; trivial
  boundary_sound := by intros b n h; exact of_decide_eq_true h

private def rejectFFOracle : TextOracle where
  UTF8 := fun b => b ≠ [255]
  Boundary := fun _ _ => True
  valid := fun b => decide (b ≠ [255])
  boundary := fun _ _ => true
  valid_sound := by intro b h; exact of_decide_eq_true h
  boundary_sound := by intros; trivial

private def testImage : Image := ⟨7, [97, 97, 13, 10, 32, 10, 98], true, true⟩
private def firstByte : Capture := { region := ⟨7, 0, 1, true, true⟩ }
private def secondByte : Capture := { region := ⟨7, 1, 2, true, true⟩ }
private def readOnlyByte : Capture := { region := ⟨7, 0, 1, true, false⟩ }
private def foreignByte : Capture := { region := ⟨8, 0, 1, true, true⟩ }
private def grouped : Capture :=
  ⟨⟨7, 0, 2, true, true⟩, none,
    [⟨[103], some ⟨7, 1, 2, true, false⟩⟩, ⟨[117], none⟩, ⟨[122], some ⟨7, 0, 0, true, false⟩⟩]⟩

-- L04: reads and named groups come from [97,97], even if a candidate is different.
example : denotePayload testImage .raw testOracle [([120], grouped)]
    [.keep ⟨[120], some [103]⟩] = some [97] := by decide
example : readCapture testImage .raw testOracle foreignByte = none := by decide
example : checkTarget testImage .raw testOracle readOnlyByte = none := by decide
example : readCapture ⟨7, [97], false, true⟩ .raw testOracle firstByte = none := by decide
example : checkTarget ⟨7, [97], true, false⟩ .raw testOracle firstByte = none := by decide
example : denotePayload testImage .raw testOracle [([120], grouped)]
    [.keep ⟨[120], some [117]⟩] = none := by decide
example : denotePayload testImage .raw testOracle [([120], grouped)]
    [.quoted [88], .keep ⟨[120], some [103]⟩, .hex [53, 65]] =
      some [88, 97, 90] := by decide
-- Present-but-empty is not absent.
example : denotePayload testImage .raw testOracle [([120], grouped)]
    [.keep ⟨[120], some [122]⟩] = some [] := by decide
example : let candidate : Bytes := [255, 255]
    candidate ≠ testImage.bytes ∧
    denotePayload testImage .raw testOracle [([120], grouped)]
      [.keep ⟨[120], some [103]⟩] = some [97] := by decide

-- L06: forward, duplicate and iteration-shadow refusal; outer lookup is unchanged.
example : lookup [([97], firstByte)] [98] = none := by decide
example : extend [([97], firstByte)] [97] secondByte = none := by decide
example : withBinder [([97], firstByte)] [97] secondByte
    (fun _ => some ()) = none := by decide
example : withBinder [([97], firstByte)] [98] secondByte
    (fun scopedEnv => lookup scopedEnv [98]) = some secondByte := by decide
example : lookup [([97], firstByte)] [98] = none := by decide

private def completeEvidence : MatcherEvidence :=
  ⟨7, 11, none, [firstByte, secondByte], true, false, 2, 2⟩
private def truncatedEvidence : MatcherEvidence :=
  ⟨7, 11, none, [firstByte], true, true, 1, 2⟩
private def overBudgetEvidence : MatcherEvidence :=
  ⟨7, 11, none, [firstByte], true, false, 3, 2⟩
private def emptyEvidence : MatcherEvidence :=
  ⟨7, 11, none, [], true, false, 0, 0⟩
private def zeroWidthEvidence : MatcherEvidence :=
  ⟨7, 11, none, [{ region := ⟨7, 0, 0, true, true⟩ }], true, false, 1, 1⟩
private def unsortedEvidence : MatcherEvidence :=
  ⟨7, 11, none, [secondByte, firstByte], true, false, 2, 2⟩

example : freeze testImage .text testOracle completeEvidence 2 =
    some ⟨7, 11, none, [firstByte, secondByte]⟩ := by decide
example : freeze testImage .text testOracle completeEvidence 1 = none := by decide
example : freeze testImage .text testOracle truncatedEvidence 1 = none := by decide
example : freeze testImage .text testOracle overBudgetEvidence 1 = none := by decide
example : oneFrozen testImage .text testOracle completeEvidence = none := by decide
example : freeze testImage .raw testOracle completeEvidence 2 = none := by decide
example : freeze testImage .text testOracle emptyEvidence 0 =
    some ⟨7, 11, none, []⟩ := by decide
example : freeze testImage .text testOracle zeroWidthEvidence 1 = none := by decide
example : freeze testImage .text testOracle unsortedEvidence 2 = none := by decide
example : eachFrozen [] [110] ⟨7, 11, none, [firstByte, secondByte]⟩
    (fun scopedEnv _ => lookup scopedEnv [110]) = some [firstByte, secondByte] := by decide

-- `one text` sees both overlapping starts and refuses instead of choosing the first.
example : select testImage .text testOracle [] (.oneText [97] none) = none := by decide
example : select testImage .raw testOracle [] (.oneText [97] none) = none := by decide
example : select testImage .text testOracle [] (.oneHex [57, 55] none) = none := by decide
example : select testImage .text testOracle [([115], secondByte)]
    (.oneHex [54, 49] (some { name := [115] })) = some secondByte := by decide
example : select testImage .text testOracle [] (.oneHex [54, 50] none) =
    some { region := ⟨7, 6, 7, true, true⟩ } := by decide
example : select testImage .text noMiddleBoundary [] (.range 1 2) = none := by decide
example : readCapture ⟨7, [255], true, true⟩ .raw testOracle
    { region := ⟨7, 0, 1, true, true⟩ } = some [255] := by decide

private def lineA : Capture := { region := ⟨7, 0, 2, true, true⟩, line := some .crlf }
private def lineB : Capture := { region := ⟨7, 4, 5, true, true⟩, line := some .lf }
private def lineLast : Capture := { region := ⟨7, 6, 7, true, true⟩, line := some .unterminated }
private def noEOL : Capture := { region := ⟨7, 0, 2, true, true⟩, line := some .unterminated }

example : select testImage .text testOracle [] (.oneLine [98] none) = some lineLast := by decide
example : select testImage .raw testOracle [] (.oneLine [98] none) = none := by decide
example : select testImage .text testOracle [([108], lineA)]
    (.oneLine [97, 97] (some { name := [108] })) = some lineA := by decide
example : select testImage .text testOracle [([115], secondByte)]
    (.oneLine [97] (some { name := [115] })) = none := by decide
example : select testImage .text testOracle [([97], lineA), ([98], lineB)]
    (.between { name := [97] } { name := [98] }) = some { region := ⟨7, 4, 4, true, true⟩ } := by decide
example : select testImage .text testOracle [([97], noEOL), ([98], lineB)]
    (.between { name := [97] } { name := [98] }) = none := by decide
example : select testImage .text testOracle [([97], lineA), ([98], secondByte)]
    (.between { name := [97] } { name := [98] }) = none := by decide
example : select testImage .text testOracle [([97], lineB), ([98], lineA)]
    (.between { name := [97] } { name := [98] }) = none := by decide

-- L07: escapes, exact Unicode bytes, heredoc EOLs, raw hex and malformed inputs.
example : decodeQuoted [65, 92, 110, 66] = some [65, 10, 66] := by decide
example : decodeQuoted [240, 159, 152, 128] = some [240, 159, 152, 128] := by decide
example : decodeQuoted [92, 113] = none := by decide
example : decodeQuoted [10] = none := by decide
example : decodeHeredoc [35, 32, 120, 13, 10, 121, 10] =
    some [35, 32, 120, 13, 10, 121, 10] := by decide
example : decodeHeredoc [] = some [] := by decide
example : decodeHex [48, 48, 102, 70] = some [0, 255] := by decide
example : decodeHex [48] = none := by decide
example : decodeHex [48, 103] = none := by decide

-- Two physical lines plus an unterminated final body; no line follows the last LF by fiat.
example : logicalLines [97, 13, 10, 32, 10, 98] =
    [⟨[97], some [13, 10]⟩, ⟨[32], some [10]⟩, ⟨[98], none⟩] := by decide
example : logicalLines [97, 10] = [⟨[97], some [10]⟩] := by decide
example : logicalLines [13] = [⟨[13], none⟩] := by decide
example : denoteLinesLike testImage .text testOracle [([108], lineA)] { name := [108] }
    [[120], [121]] = some [120, 13, 10, 121, 13, 10] := by decide
example : denoteLinesLike testImage .text testOracle [([108], lineLast)] { name := [108] }
    [[120]] = none := by decide
example : denoteLinesLike testImage .text testOracle [([108], lineA)] { name := [108] }
    [[92, 110]] = none := by decide
-- Empty body stays empty, whitespace-only body is indented, and final EOF gets no synthetic EOL.
example : indentBytes [32, 32] [10, 32, 10, 98] =
    [10, 32, 32, 32, 10, 32, 32, 98] := by decide
example : indentBytes [32] [] = [] := by decide
example : denotePayload testImage .text testOracle []
    [.indent [32] [.heredoc [97, 10, 98]]] = some [32, 97, 10, 32, 98] := by decide
example : denotePayload testImage .text testOracle []
    [.indent [120] [.quoted [97]]] = none := by decide
example : denotePayload testImage .raw testOracle []
    [.indent [] [.quoted [97]]] = none := by decide
example : denoteLinesLike testImage .raw testOracle [([108], lineA)] { name := [108] }
    [[120]] = none := by decide
example : denoteChange testImage .text rejectFFOracle []
    (.replace (.range 0 7) [.hex [102, 102]]) = none := by decide
example : lowerChange testImage .text rejectFFOracle []
    (.replace (.range 0 7) [.hex [102, 102]]) = none := by rfl

-- Four nonvacuous command forms, including an insertion at EOF and an empty replacement.
example : denoteChange testImage .raw testOracle []
    (.replace (.range 0 1) [.hex [55, 56]]) = some [120, 97, 13, 10, 32, 10, 98] := by decide
example : denoteChange testImage .raw testOracle []
    (.delete (.range 0 1)) = some [97, 13, 10, 32, 10, 98] := by decide
example : denoteChange testImage .raw testOracle []
    (.before (.range 0 0) [.quoted [120]]) =
      some [120, 97, 97, 13, 10, 32, 10, 98] := by decide
example : denoteChange testImage .raw testOracle []
    (.after (.range 7 7) [.quoted [120]]) =
      some [97, 97, 13, 10, 32, 10, 98, 120] := by decide
example : denoteChange testImage .raw testOracle []
    (.replace (.range 8 8) []) = none := by decide

end Splicery.Surface
