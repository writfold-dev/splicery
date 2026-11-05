import Splicery.Lowering

/-!
Executable bounded oracle for the primitive splice calculus.

Wire format (one operation per invocation):
  apply LO WIDTH INPUT_HEX REPLACEMENT_HEX

LO and WIDTH are exact natural numbers. INPUT_HEX and REPLACEMENT_HEX contain
only complete ASCII hexadecimal byte pairs. Inputs and replacements are limited
to 4096 bytes. Success prints the complete candidate as lowercase hexadecimal;
every parse, bound or profile refusal prints `REFUSAL`. This is a component
oracle, not the host protocol or a file-writing adapter.
-/

namespace Splicery.Oracle

private def hexNibble (c : Char) : Option Nat :=
  if '0' ≤ c && c ≤ '9' then some (c.toNat - '0'.toNat)
  else if 'a' ≤ c && c ≤ 'f' then some (10 + c.toNat - 'a'.toNat)
  else if 'A' ≤ c && c ≤ 'F' then some (10 + c.toNat - 'A'.toNat)
  else none

private def parseDigits : List Char → Option Splicery.Bytes
  | [] => some []
  | high :: low :: rest => do
    let hi ← hexNibble high
    let lo ← hexNibble low
    let tail ← parseDigits rest
    pure (UInt8.ofNat (16 * hi + lo) :: tail)
  | [_] => none

def parseBytes (text : String) : Option Splicery.Bytes :=
  parseDigits text.toList

private def hexDigit (n : Nat) : Char :=
  if n < 10 then Char.ofNat ('0'.toNat + n)
  else Char.ofNat ('a'.toNat + (n - 10))

def renderBytes (bytes : Splicery.Bytes) : String :=
  String.mk <| bytes.flatMap fun byte =>
    [hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)]

private def bounded (bytes : Splicery.Bytes) : Bool :=
  bytes.length ≤ 4096

def execute (lo width : Nat) (input replacement : Splicery.Bytes) :
    Except Unit Splicery.Bytes :=
  if bounded input && bounded replacement then
    match Splicery.applyAt input lo width replacement with
    | some output => .ok output
    | none => .error ()
  else
    .error ()

def run (args : List String) : Except Unit String :=
  match args with
  | [operation, loText, widthText, inputHex, replacementHex] =>
      if operation != "apply" then .error ()
      else
        match loText.toNat?, widthText.toNat?,
            parseBytes inputHex, parseBytes replacementHex with
        | some lo, some width, some input, some replacement =>
            match execute lo width input replacement with
            | .ok output => .ok (renderBytes output)
            | .error () => .error ()
        | _, _, _, _ => .error ()
  | _ => .error ()

end Splicery.Oracle

def main (args : List String) : IO UInt32 := do
  match Splicery.Oracle.run args with
  | .ok output =>
      IO.println output
      return 0
  | .error () =>
      IO.println "REFUSAL"
      return 1
