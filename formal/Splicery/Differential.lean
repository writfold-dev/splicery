import Splicery.Surface
import Splicery.Batch

/-!
Direct executable surface differential vectors.

Every successful case is evaluated twice from the same immutable original:
`Surface.denoteChange` supplies the independent direct meaning, while
`Surface.lowerChange`, `Lowering.lower`, and the bounded `applyAt` wrapper below
supply the lowered primitive meaning.  A multi-change result is normalized with
`applyAuthored`; all captures and payloads are still resolved against the
original image.

The two `line_matches` cases use explicit matcher evidence, checked by `freeze`
and expanded by `eachFrozen`.  The regular-expression matcher itself remains a
trusted evidence producer, exactly as in `Surface`; no Python or Rust regex
result is used as the Lean meaning.  Recipe origins below are byte spans in the
corresponding committed `examples/*.spl` files.

`strictTextOracle` is a concrete strict UTF-8 decoder and code-point-boundary
predicate.  Its soundness fields relate the executable functions to their
explicitly named propositions; there are no decoder axioms or host IO claims.
The output is one JSON document consumed by `tools/check_surface_differential.py`.
-/

namespace Splicery.Differential

open Splicery.Surface

private def hexNibble (c : Char) : Option Nat :=
  if '0' ≤ c && c ≤ '9' then some (c.toNat - '0'.toNat)
  else if 'a' ≤ c && c ≤ 'f' then some (10 + c.toNat - 'a'.toNat)
  else if 'A' ≤ c && c ≤ 'F' then some (10 + c.toNat - 'A'.toNat)
  else none

private def parseDigits : List Char → Option Bytes
  | [] => some []
  | high :: low :: rest => do
    let hi ← hexNibble high
    let lo ← hexNibble low
    let tail ← parseDigits rest
    pure (UInt8.ofNat (16 * hi + lo) :: tail)
  | [_] => none

private def parseBytes (text : String) : Option Bytes := parseDigits text.toList

private def hexDigit (n : Nat) : Char :=
  if n < 10 then Char.ofNat ('0'.toNat + n)
  else Char.ofNat ('a'.toNat + (n - 10))

private def renderBytes (bytes : Bytes) : String :=
  String.mk <| bytes.flatMap fun byte =>
    [hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)]

private def bounded (bytes : Bytes) : Bool := bytes.length ≤ 4096

private def continuation (c : UInt8) : Bool :=
  128 ≤ c.toNat && c.toNat ≤ 191

private def secondOfThree (lead second : UInt8) : Bool :=
  if lead.toNat = 224 then 160 ≤ second.toNat && second.toNat ≤ 191
  else if lead.toNat = 237 then 128 ≤ second.toNat && second.toNat ≤ 159
  else continuation second

private def secondOfFour (lead second : UInt8) : Bool :=
  if lead.toNat = 240 then 144 ≤ second.toNat && second.toNat ≤ 191
  else if lead.toNat = 244 then 128 ≤ second.toNat && second.toNat ≤ 143
  else continuation second

/-- Strict RFC 3629 UTF-8: no overlong forms, surrogates, or values above U+10FFFF. -/
def validUTF8 : Bytes → Bool
  | [] => true
  | c :: cs =>
      if c.toNat ≤ 127 then validUTF8 cs
      else if 194 ≤ c.toNat && c.toNat ≤ 223 then
        match cs with
        | c2 :: rest => continuation c2 && validUTF8 rest
        | [] => false
      else if 224 ≤ c.toNat && c.toNat ≤ 239 then
        match cs with
        | c2 :: c3 :: rest =>
            secondOfThree c c2 && continuation c3 && validUTF8 rest
        | _ => false
      else if 240 ≤ c.toNat && c.toNat ≤ 244 then
        match cs with
        | c2 :: c3 :: c4 :: rest =>
            secondOfFour c c2 && continuation c3 && continuation c4 && validUTF8 rest
        | _ => false
      else false

/-- A boundary is in a valid image and does not split a multi-byte scalar. -/
def utf8Boundary (b : Bytes) (n : Nat) : Bool :=
  validUTF8 b && decide (n ≤ b.length) &&
    match b.drop n with
    | [] => true
    | c :: _ => !continuation c

def ValidUTF8 (b : Bytes) : Prop := validUTF8 b = true
def UTF8Boundary (b : Bytes) (n : Nat) : Prop := utf8Boundary b n = true

def strictTextOracle : TextOracle where
  UTF8 := ValidUTF8
  Boundary := UTF8Boundary
  valid := validUTF8
  boundary := utf8Boundary
  valid_sound := by intro b h; exact h
  boundary_sound := by intro b n h; exact h

/-- Vector notation is ASCII-only; Unicode input images use hexadecimal above. -/
private def a (s : String) : Bytes :=
  s.toList.map fun c => UInt8.ofNat c.toNat

private def ref (name : String) : Ref := { name := a name }
private def groupRef (name group : String) : Ref := ⟨a name, some (a group)⟩

private def bind (image : Image) (profile : Profile) (oracle : TextOracle)
    (env : Env) (name : String) (selector : Selector) : Option Env := do
  let capture ← select image profile oracle env selector
  extend env (a name) capture

structure ScopedChange where
  env : Env
  change : Change

structure Vector where
  name : String
  inputHex : String
  profile : Profile
  refusal : Option String
  origins : List (Nat × Nat)
  prepare : Image → TextOracle → Option (List ScopedChange)

private def one (env : Env) (change : Change) : List ScopedChange := [⟨env, change⟩]

private def prepare01 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env ← bind image .text oracle [] "call"
    (.oneText (a "transport.send(request)") none)
  pure (one env (.after (.reference (ref "call")) [.quoted (a ".await")]))

private def prepare02 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env ← bind image .text oracle [] "expr" (.range 8 34)
  pure (one env (.replace (.reference (ref "expr"))
    [.quoted (a "trace_send("), .keep (ref "expr"), .quoted (a ")")]))

private def prepare03 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env₁ ← bind image .text oracle [] "begin"
    (.oneLine (a "<!-- policy:begin -->") none)
  let env₂ ← bind image .text oracle env₁ "end_marker"
    (.oneLine (a "<!-- policy:end -->") none)
  pure (one env₂ (.replace (.between (ref "begin") (ref "end_marker"))
    [.heredoc (a "Attempts: 3\nBackoff: exponential\n")]))

private def region (image : Image) (lo hi : Nat) : Region :=
  ⟨image.id, lo, hi, true, true⟩

private def namedRegion (image : Image) (name : String) (lo hi : Nat) : NamedGroup :=
  ⟨a name, some (region image lo hi)⟩

private def routeCapture (image : Image) (lo hi : Nat) (line : LineInfo)
    (verbLo verbHi pathLo pathHi : Nat) : Capture :=
  { region := region image lo hi
    line := some line
    groups := [namedRegion image "verb" verbLo verbHi,
      namedRegion image "path" pathLo pathHi] }

private def routeEvidence (image : Image) : MatcherEvidence :=
  { image := image.id
    matcherProfile := 1
    scope := none
    members := [routeCapture image 10 28 .crlf 17 20 21 28,
      routeCapture image 30 46 .lf 36 40 41 46,
      routeCapture image 47 65 .unterminated 53 56 57 65]
    complete := true
    truncated := false
    work := 0
    limit := 0 }

private def routeChange (env : Env) : ScopedChange :=
  ⟨env, .replace (.reference (ref "r"))
    [.keep (groupRef "r" "verb"), .quoted (a " "), .keep (groupRef "r" "path")]⟩

private def prepare04 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let frozen ← freeze image .text oracle (routeEvidence image) 3
  eachFrozen [] (a "r") frozen fun env _ => pure (routeChange env)

private def prepare05 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env₁ ← bind image .text oracle [] "left" (.range 0 5)
  let env₂ ← bind image .text oracle env₁ "right" (.range 6 10)
  pure [⟨env₂, .replace (.reference (ref "left")) [.keep (ref "right")]⟩,
    ⟨env₂, .replace (.reference (ref "right")) [.keep (ref "left")]⟩]

private def prepare06 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env ← bind image .raw oracle [] "slot" (.oneHex (a "00ff") none)
  pure (one env (.replace (.reference (ref "slot")) [.hex (a "80fe")]))

private def prepare07 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env₁ ← bind image .text oracle [] "banner" (.oneLine (a "[capture]") none)
  let env₂ ← bind image .text oracle env₁ "here" (.oneLine (a "obsolete") none)
  pure (one env₂ (.replace (.reference (ref "here"))
    [.linesLike (ref "banner") [a "A", a "B"]]))

private def prepare08 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env ← bind image .text oracle [] "body" (.range 0 6)
  pure (one env (.replace (.reference (ref "body"))
    [.indent (a "  ") [.keep (ref "body")]]))

private def prepare09 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env₁ ← bind image .text oracle [] "a" (.oneLine (a "// imports begin") none)
  let env₂ ← bind image .text oracle env₁ "b" (.oneLine (a "// imports end") none)
  let env₃ ← bind image .text oracle env₂ "window" (.between (ref "a") (ref "b"))
  let env₄ ← bind image .text oracle env₃ "anchor"
    (.oneLine (a "use lib::Config;") (some (ref "window")))
  pure (one env₄ (.before (.reference (ref "anchor"))
    [.linesLike (ref "anchor") [a "use lib::Policy;"]]))

private def prepare10 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let env₁ ← bind image .text oracle [] "old"
    (.oneText (a "# pinned\\noption=old\\n") none)
  let env₂ ← bind image .text oracle env₁ "dest" (.oneText (a "TAIL") none)
  pure [⟨env₂, .delete (.reference (ref "old"))⟩,
    ⟨env₂, .before (.reference (ref "dest")) [.keep (ref "old")]⟩]

private def absenceCapture (image : Image) : Capture :=
  { region := region image 0 1
    line := some .unterminated
    groups := [{ name := a "first", region := none }] }

private def absenceEvidence (image : Image) : MatcherEvidence :=
  { image := image.id
    matcherProfile := 1
    scope := none
    members := [absenceCapture image]
    complete := true
    truncated := false
    work := 0
    limit := 0 }

private def prepare11 (image : Image) (oracle : TextOracle) : Option (List ScopedChange) := do
  let frozen ← freeze image .text oracle (absenceEvidence image) 1
  eachFrozen [] (a "r") frozen fun env _ =>
    pure ⟨env, .replace (.reference (ref "r")) [.keep (groupRef "r" "first")]⟩

private def vectors : List Vector :=
  [⟨"01-await", "6c6574207265706c79203d207472616e73706f72742e73656e642872657175657374293f3b0d0a",
      .text, none, [(75, 102)], prepare01⟩,
    ⟨"02-wrap", "6c65742078203d2073656e64280d0a202020207265712c202f2f20636166c3a90a293f3b",
      .text, none, [(53, 105)], prepare02⟩,
    ⟨"03-policy", "6b6565700a3c212d2d20706f6c6963793a626567696e202d2d3e0a6f62736f6c6574650a3c212d2d20706f6c6963793a656e64202d2d3e0a656e64",
      .text, none, [(123, 218)], prepare03⟩,
    ⟨"04-routes", "2320726f757465730d0a726f7574652020474554202f6865616c74680d0a726f75746520504f5354202f6a6f62730a726f75746520474554202f76322f6a6f6273",
      .text, none, [(129, 180), (129, 180), (129, 180)], prepare04⟩,
    ⟨"05-swap", "616c7068612062657461", .text, none,
      [(82, 113), (116, 147)], prepare05⟩,
    ⟨"06-raw", "aa00ffbb", .raw, none, [(64, 92)], prepare06⟩,
    ⟨"07-lines", "5b636170747572655d0d0a6f62736f6c657465",
      .text, none, [(92, 140)], prepare07⟩,
    ⟨"08-indent", "610d0a0a6263", .text, none, [(51, 95)], prepare08⟩,
    ⟨"09-import", "2f2f20696d706f72747320626567696e0a757365206c69623a3a436f6e6669673b0a2f2f20696d706f72747320656e640a757365206c69623a3a436f6e6669673b0a",
      .text, none, [(183, 244)], prepare09⟩,
    ⟨"10-move", "232070696e6e65640a6f7074696f6e3d6f6c640a666f6f3d310a5441494c",
      .text, none, [(99, 110), (113, 143)], prepare10⟩,
    ⟨"11-absence", "79", .text, some "AbsentGroup", [(81, 111)], prepare11⟩]

private def mapOption (f : α → Option β) : List α → Option (List β)
  | [] => some []
  | x :: xs => do
    let y ← f x
    let ys ← mapOption f xs
    pure (y :: ys)

structure NormalizedSplice where
  lo : Nat
  hi : Nat
  replacement : Bytes
  origin : Nat × Nat

structure Successful where
  directOutput : Bytes
  primitiveOutput : Bytes
  directChanges : List Bytes
  primitiveChanges : List Bytes
  splices : List NormalizedSplice

inductive Observation where
  | success (result : Successful)
  | refusal (kind : String)

/-- Direct target calculation, intentionally independent of `Lowering.lower`. -/
private def directSplice (c : SurfaceCut) (kind : ChangeKind) (replacement : Bytes) :
    AuthoredSplice :=
  match kind with
  | .replace | .delete => ⟨c.pre.length, c.selected.length, replacement⟩
  | .before => ⟨c.pre.length, 0, replacement⟩
  | .after => ⟨(c.pre ++ c.selected).length, 0, replacement⟩

private def directOne (image : Image) (profile : Profile) (oracle : TextOracle)
    (scopedChange : ScopedChange) : Option (Bytes × AuthoredSplice) := do
  let output ← denoteChange image profile oracle scopedChange.env scopedChange.change
  let cut ← selectedCut image profile oracle scopedChange.env scopedChange.change.target
  let replacement ← denotePayload image profile oracle scopedChange.env scopedChange.change.payload
  pure (output, directSplice cut scopedChange.change.kind replacement)

private def primitiveOne (image : Image) (p : Prim) : Option Bytes :=
  if bounded image.bytes && bounded p.payload then Splicery.applyAt image.bytes p.lo p.width p.payload
  else none

private def normalized (input : Bytes) (edits : List AuthoredSplice) : Option Bytes :=
  if bounded input && edits.length ≤ 4096 &&
      edits.all (fun edit => bounded edit.replacement) then Splicery.applyAuthored input edits
  else none

private def succeed (v : Vector) (image : Image) (scopedChange : List ScopedChange) :
    Option Successful := do
  let direct ← mapOption (directOne image v.profile strictTextOracle) scopedChange
  let directChanges := direct.map Prod.fst
  let directEdits := direct.map Prod.snd
  let directOutput ← normalized image.bytes directEdits
  let lowered ← mapOption (fun item =>
    lowerChange image v.profile strictTextOracle item.env item.change) scopedChange
  let primitives := lowered.map fun item => Splicery.lower item.1 item.2
  let primitiveChanges ← mapOption (primitiveOne image) primitives
  if directChanges != primitiveChanges then none else
    let primitiveEdits : List AuthoredSplice := primitives.map fun p => ⟨p.lo, p.width, p.payload⟩
    let primitiveOutput ← normalized image.bytes primitiveEdits
    if directOutput != primitiveOutput || v.origins.length != directEdits.length then none else
      let splices : List NormalizedSplice := (directEdits.zip v.origins).map fun item =>
        ⟨item.1.start, item.1.stop, item.1.replacement, item.2⟩
      pure ⟨directOutput, primitiveOutput, directChanges, primitiveChanges, splices⟩

private def observe (v : Vector) : Observation :=
  match parseBytes v.inputHex with
  | none => .refusal "VectorEncoding"
  | some input =>
      let image : Image := ⟨0, input, true, true⟩
      match v.prepare image strictTextOracle with
      | none => .refusal (v.refusal.getD "Preparation")
      | some scopedChange =>
          match succeed v image scopedChange with
          | some result => .success result
          | none => .refusal (v.refusal.getD "UnexpectedRefusal")

private def jsonString (s : String) : String :=
  let escaped := s.toList.flatMap fun c =>
    if c = '"' then ['\\', '"']
    else if c = '\\' then ['\\', '\\']
    else if c = '\n' then ['\\', 'n']
    else if c = '\r' then ['\\', 'r']
    else if c = '\t' then ['\\', 't']
    else [c]
  "\"" ++ String.mk escaped ++ "\""

private def jsonList (items : List String) : String :=
  "[" ++ String.intercalate "," items ++ "]"

private def jsonBytes (b : Bytes) : String := jsonString (renderBytes b)
private def jsonBytesList (bs : List Bytes) : String := jsonList (bs.map jsonBytes)
private def jsonBool (b : Bool) : String := if b then "true" else "false"

private def jsonSplice (s : NormalizedSplice) : String :=
  "{\"target\":[" ++ toString s.lo ++ "," ++ toString s.hi ++
    "],\"replacement_hex\":" ++ jsonBytes s.replacement ++
    ",\"origin\":[" ++ toString s.origin.1 ++ "," ++ toString s.origin.2 ++ "]}"

private def jsonCase (v : Vector) : String :=
  let casePrefix := "{\"name\":" ++ jsonString v.name ++
    ",\"input_hex\":" ++ jsonString v.inputHex
  match observe v with
  | .success s =>
      casePrefix ++ ",\"status\":\"success\",\"direct_output_hex\":" ++
        jsonBytes s.directOutput ++ ",\"primitive_output_hex\":" ++
        jsonBytes s.primitiveOutput ++ ",\"direct_change_outputs_hex\":" ++
        jsonBytesList s.directChanges ++ ",\"primitive_change_outputs_hex\":" ++
        jsonBytesList s.primitiveChanges ++ ",\"normalized_splices\":" ++
        jsonList (s.splices.map jsonSplice) ++ ",\"refusal\":null}"
  | .refusal kind =>
      casePrefix ++ ",\"status\":\"refusal\",\"direct_output_hex\":null," ++
        "\"primitive_output_hex\":null,\"direct_change_outputs_hex\":null," ++
        "\"primitive_change_outputs_hex\":null,\"normalized_splices\":null," ++
        "\"refusal\":" ++ jsonString kind ++ "}"

private def normalizeCRLF : Bytes → Bytes
  | [] => []
  | 13 :: 10 :: rest => 10 :: normalizeCRLF rest
  | c :: rest => c :: normalizeCRLF rest

/-- Wrong sequential capture: the second swap reads @left from the first
candidate, yielding `beta beta` instead of reading the original snapshot. -/
private def sequentialCaptureMutant : Option (Bytes × Bytes) := do
  let input ← parseBytes "616c7068612062657461"
  let image : Image := ⟨0, input, true, true⟩
  let changes ← prepare05 image strictTextOracle
  match changes with
  | [first, second] =>
      let reference := a "beta alpha"
      let intermediate ← denoteChange image .text strictTextOracle first.env first.change
      let changedImage := { image with bytes := intermediate ++ a " " }
      let mutated ← denoteChange changedImage .text strictTextOracle second.env second.change
      pure (reference, mutated)
  | _ => none

private def crlfMutant : Option (Bytes × Bytes) := do
  let input ← parseBytes "610d0a0a6263"
  let image : Image := ⟨0, input, true, true⟩
  let changes ← prepare08 image strictTextOracle
  match changes with
  | [change] =>
      let reference ← denoteChange image .text strictTextOracle change.env change.change
      pure (reference, normalizeCRLF reference)
  | _ => none

private def jsonMutant (name : String) (result : Option (Bytes × Bytes)) : String :=
  let casePrefix := "{\"name\":" ++ jsonString name
  match result with
  | none => casePrefix ++ ",\"caught\":false,\"reference_hex\":null,\"mutant_hex\":null}"
  | some (reference, mutated) =>
      casePrefix ++ ",\"caught\":" ++ jsonBool (reference != mutated) ++
        ",\"reference_hex\":" ++ jsonBytes reference ++
        ",\"mutant_hex\":" ++ jsonBytes mutated ++ "}"

def document : String :=
  "{\"schema_version\":1,\"scope\":\"direct-lean-surface-differential\"," ++
    "\"text_oracle\":\"strict-rfc3629-utf8-codepoint-boundaries-v1\",\"cases\":" ++
    jsonList (vectors.map jsonCase) ++ ",\"mutants\":[" ++
    jsonMutant "sequential-capture" sequentialCaptureMutant ++ "," ++
    jsonMutant "crlf-normalization" crlfMutant ++ "]}"

end Splicery.Differential

def main : IO UInt32 := do
  IO.println Splicery.Differential.document
  return 0
