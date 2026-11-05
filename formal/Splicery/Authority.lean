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

structure EffectRequest where
  input : PreparedInput
  identity : PreparedIdentity
  requestKey : Nat
  semanticIntent : Bytes
  deriving DecidableEq, Repr

inductive EffectPhase where
  | claimed
  | confirmed
  | unknown
  deriving DecidableEq, Repr

structure EffectState where
  request : EffectRequest
  phase : EffectPhase
  deriving DecidableEq, Repr

def claim (encoder : CanonicalEncoder) (grant : Grant)
    (request : EffectRequest) : Option EffectState :=
  if authorizes grant request.input &&
      request.identity = stamp encoder .editPreparation request.input then
    some ⟨request, .claimed⟩
  else
    none

theorem claim_refuses_read_only {encoder : CanonicalEncoder} {grant : Grant}
    {request : EffectRequest} (readOnly : grant.write = false) :
    claim encoder grant request = none := by
  simp [claim, authorizes, readOnly]

theorem claim_refuses_changed_identity {encoder : CanonicalEncoder} {grant : Grant}
    {request : EffectRequest}
    (changed : request.identity ≠ stamp encoder .editPreparation request.input) :
    claim encoder grant request = none := by
  simp [claim, changed]

theorem claim_accepts_live_exact_request {encoder : CanonicalEncoder} {grant : Grant}
    {request : EffectRequest} (live : authorizes grant request.input = true)
    (exact : request.identity = stamp encoder .editPreparation request.input) :
    claim encoder grant request = some ⟨request, .claimed⟩ := by
  simp [claim, live, exact]

def confirm (request : EffectRequest) (state : EffectState)
    (observedBefore observedAfter : Bytes) : Option EffectState :=
  if state.phase = .claimed && state.request = request &&
      observedBefore = request.input.ir.image.contents &&
      observedAfter = request.input.candidate then
    some ⟨request, .confirmed⟩
  else
    none

theorem confirm_refuses_wrong_preimage (request : EffectRequest) (state : EffectState)
    (observedBefore : Bytes) (wrong : observedBefore ≠ request.input.ir.image.contents) :
    confirm request state observedBefore request.input.candidate = none := by
  simp [confirm, wrong]

theorem confirm_accepts_exact_observation {request : EffectRequest} {state : EffectState}
    (claimed : state.phase = .claimed) (same : state.request = request) :
    confirm request state request.input.ir.image.contents request.input.candidate =
      some ⟨request, .confirmed⟩ := by
  simp [confirm, claimed, same]

def loseConfirmation (state : EffectState) : Option EffectState :=
  if state.phase = .claimed then some ⟨state.request, .unknown⟩ else none

def reconcile (state : EffectState) (observedBefore observedAfter : Bytes) : Option EffectState :=
  if state.phase = .unknown &&
      observedBefore = state.request.input.ir.image.contents &&
      observedAfter = state.request.input.candidate then
    some ⟨state.request, .confirmed⟩
  else
    none

theorem matching_after_alone_cannot_reconcile (state : EffectState)
    (observedBefore : Bytes)
    (wrong : observedBefore ≠ state.request.input.ir.image.contents) :
    reconcile state observedBefore state.request.input.candidate = none := by
  simp [reconcile, wrong]

theorem reconcile_accepts_exact_unknown (state : EffectState)
    (unknown : state.phase = .unknown) :
    reconcile state state.request.input.ir.image.contents state.request.input.candidate =
      some ⟨state.request, .confirmed⟩ := by
  simp [reconcile, unknown]

def replaySameKey (stored : EffectState) (request : EffectRequest) : Except Unit EffectState :=
  if stored.request.requestKey = request.requestKey && stored.request = request then
    .ok stored
  else
    .error ()

theorem identical_replay_retains_outcome (stored : EffectState) :
    replaySameKey stored stored.request = .ok stored := by
  simp [replaySameKey]

theorem changed_intent_same_key_conflicts (stored : EffectState) (request : EffectRequest)
    (sameKey : stored.request.requestKey = request.requestKey)
    (changed : stored.request ≠ request) :
    replaySameKey stored request = .error () := by
  simp [replaySameKey, sameKey, changed]

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
