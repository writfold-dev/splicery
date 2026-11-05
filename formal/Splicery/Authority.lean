import Splicery.Core

/-!
Authority and preparation model for L08–L09.

The identity encoder is injective by assumption. A cryptographic implementation
must separately justify that assumption for its domain-separated canonical
encoding and digest profile. Native path, grant and filesystem observations are
adapter inputs; this module proves no physical-disk theorem.
-/

namespace Splicery

inductive IdentityDomain where
  | readCapture
  | editPreparation
  | effectReceipt
  | citation
  deriving DecidableEq, Repr

structure WorktreeScope where
  realm : Nat
  run : Nat
  repository : Nat
  worktree : Nat
  bindingRevision : Nat
  ownerGeneration : Nat
  aliasEpoch : Nat
  deriving DecidableEq, Repr

structure ExactImage where
  scope : WorktreeScope
  profile : Nat
  contents : Bytes
  metadata : Nat
  deriving DecidableEq, Repr

structure ResolvedSplice where
  lo : Nat
  width : Nat
  replacement : Bytes
  origin : Nat
  deriving DecidableEq, Repr

structure ResolvedIR where
  image : ExactImage
  profile : Nat
  edits : List ResolvedSplice
  readSet : List (Nat × Nat)
  deriving DecidableEq, Repr

structure PreparedInput where
  schemaVersion : Nat
  compilerProfile : Nat
  materializerProfile : Nat
  recipe : Option Bytes
  ir : ResolvedIR
  candidate : Bytes
  deriving DecidableEq, Repr

structure PreparedIdentity where
  domain : IdentityDomain
  encoding : Bytes
  deriving DecidableEq, Repr

structure CanonicalEncoder where
  encode : PreparedInput → Bytes
  encode_injective : ∀ {left right : PreparedInput},
    encode left = encode right → left = right

def stamp (encoder : CanonicalEncoder) (domain : IdentityDomain)
    (input : PreparedInput) : PreparedIdentity :=
  ⟨domain, encoder.encode input⟩

theorem stamp_binds_input {encoder : CanonicalEncoder} {domain : IdentityDomain}
    {left right : PreparedInput}
    (h : stamp encoder domain left = stamp encoder domain right) : left = right := by
  apply encoder.encode_injective
  injection h with _ encoding

theorem changed_input_cannot_reuse_stamp {encoder : CanonicalEncoder}
    {domain : IdentityDomain} {left right : PreparedInput}
    (changed : left ≠ right)
    (h : stamp encoder domain left = stamp encoder domain right) : False :=
  changed (stamp_binds_input h)

theorem read_stamp_is_not_edit_stamp {encoder : CanonicalEncoder}
    {read edit : PreparedInput}
    (h : stamp encoder .readCapture read = stamp encoder .editPreparation edit) : False := by
  injection h with domain _
  contradiction

/-- A digest adapter must prove this collision-freedom premise; collision
resistance by itself is not a Lean injectivity proof. -/
structure DigestEncoder where
  canonical : CanonicalEncoder
  digest : Bytes → Bytes
  digest_collision_free : ∀ {left right : PreparedInput},
    digest (canonical.encode left) = digest (canonical.encode right) → left = right

def DigestEncoder.asCanonical (encoder : DigestEncoder) : CanonicalEncoder where
  encode input := encoder.digest (encoder.canonical.encode input)
  encode_injective := encoder.digest_collision_free

structure Grant where
  scope : WorktreeScope
  ownerGeneration : Nat
  write : Bool
  deriving DecidableEq, Repr

def authorizes (grant : Grant) (input : PreparedInput) : Bool :=
  grant.write && grant.scope = input.ir.image.scope &&
    grant.ownerGeneration = input.ir.image.scope.ownerGeneration

theorem read_only_grant_cannot_authorize (grant : Grant) (input : PreparedInput)
    (readOnly : grant.write = false) : authorizes grant input = false := by
  simp [authorizes, readOnly]

theorem wrong_generation_cannot_authorize (grant : Grant) (input : PreparedInput)
    (stale : grant.ownerGeneration ≠ input.ir.image.scope.ownerGeneration) :
    authorizes grant input = false := by
  simp [authorizes, stale]

/-- Receipts and citations are inputs only to attribution, never to the grant
decision. -/
def authorizesWithEvidence (grant : Grant) (input : PreparedInput)
    (_receipt _citation : PreparedIdentity) : Bool := authorizes grant input

theorem receipt_cannot_widen_authority (grant : Grant) (input : PreparedInput)
    (receipt citation : PreparedIdentity) :
    authorizesWithEvidence grant input receipt citation = authorizes grant input := rfl

inductive PreparationRefusal where
  | budgetExceeded
  | unsupportedProfile
  | staleTarget
  | denied
  deriving DecidableEq, Repr

structure PreparedCandidate where
  identity : PreparedIdentity
  contents : Bytes
  deriving DecidableEq, Repr

inductive PreparationResult where
  | success (candidate : PreparedCandidate)
  | refusal (reason : PreparationRefusal)
  deriving DecidableEq, Repr

inductive AdapterEffect where
  | fileWrite
  | ledgerAppend
  deriving DecidableEq, Repr

/-- The public pure-preparation result has no hidden effect channel. Work
accounting is supplied explicitly and checked before a candidate is returned. -/
def preparePure (encoder : CanonicalEncoder) (work : PreparedInput → Nat)
    (budget : Nat) (input : PreparedInput) : PreparationResult × List AdapterEffect :=
  if work input ≤ budget then
    (.success ⟨stamp encoder .editPreparation input, input.candidate⟩, [])
  else
    (.refusal .budgetExceeded, [])

theorem preparePure_no_effects (encoder : CanonicalEncoder)
    (work : PreparedInput → Nat) (budget : Nat) (input : PreparedInput) :
    (preparePure encoder work budget input).2 = [] := by
  unfold preparePure
  split <;> rfl

theorem preparePure_deterministic (encoder : CanonicalEncoder)
    (work : PreparedInput → Nat) (budget : Nat) (input : PreparedInput) :
    preparePure encoder work budget input = preparePure encoder work budget input := rfl

theorem preparePure_budget_refusal {encoder : CanonicalEncoder}
    {work : PreparedInput → Nat} {budget : Nat} {input : PreparedInput}
    (exceeded : ¬work input ≤ budget) :
    preparePure encoder work budget input = (.refusal .budgetExceeded, []) := by
  simp [preparePure, exceeded]

theorem preparePure_success_exact_candidate {encoder : CanonicalEncoder}
    {work : PreparedInput → Nat} {budget : Nat} {input : PreparedInput}
    (within : work input ≤ budget) :
    preparePure encoder work budget input =
      (.success ⟨stamp encoder .editPreparation input, input.candidate⟩, []) := by
  simp [preparePure, within]

inductive FileOperation where
  | create (sourceAbsent : Bool) (literalOnly : Bool)
  | rename (sourceRegular : Bool) (destinationAbsent : Bool) (sameFilesystem : Bool)
  | remove (sourceRegular : Bool)
  deriving DecidableEq, Repr

def fileOperationAdmitted : FileOperation → Bool
  | .create sourceAbsent literalOnly => sourceAbsent && literalOnly
  | .rename sourceRegular destinationAbsent sameFilesystem =>
      sourceRegular && destinationAbsent && sameFilesystem
  | .remove sourceRegular => sourceRegular

theorem create_requires_absence {sourceAbsent literalOnly : Bool}
    (admitted : fileOperationAdmitted (.create sourceAbsent literalOnly) = true) :
    sourceAbsent = true ∧ literalOnly = true := by
  simpa [fileOperationAdmitted] using admitted

theorem rename_requires_same_filesystem {sourceRegular destinationAbsent sameFilesystem : Bool}
    (admitted : fileOperationAdmitted
      (.rename sourceRegular destinationAbsent sameFilesystem) = true) :
    sourceRegular = true ∧ destinationAbsent = true ∧ sameFilesystem = true := by
  have checked : (sourceRegular = true ∧ destinationAbsent = true) ∧ sameFilesystem = true := by
    simpa [fileOperationAdmitted] using admitted
  exact ⟨checked.1.1, checked.1.2, checked.2⟩

theorem remove_requires_regular_source {sourceRegular : Bool}
    (admitted : fileOperationAdmitted (.remove sourceRegular) = true) :
    sourceRegular = true := by
  simpa [fileOperationAdmitted] using admitted

end Splicery
