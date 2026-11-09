import Lean
import Kernel

/-!
# Axiom audit

Every theorem under `Splicery.Kernel` is audited: its transitive axioms must
be a subset of Lean's standard three. Anything else — `sorryAx`, a project
axiom, an Aeneas `axiom` standing in for an unmodelled external — fails the
build. The headline claims are also required to exist, so a deleted or
renamed theorem cannot pass silently.
-/
open Lean Elab Command

/-- Lean's standard axioms. -/
def allowedAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

/-- The headline claims: cut and edit validation, canonical ordering, batch
admission, rendering and `apply_at`. -/
def claims : List Name := [
  ``Splicery.Kernel.same_capture_spec,
  ``Splicery.Kernel.char_boundary_spec,
  ``Splicery.Kernel.check_cut_spec,
  ``Splicery.Kernel.check_edit_spec,
  ``Splicery.Kernel.replacement_len_spec,
  ``Splicery.Kernel.insert_by_start_spec,
  ``Splicery.Kernel.sort_by_start_spec,
  ``Splicery.Kernel.compatible_spec,
  ``Splicery.Kernel.check_canonical_spec,
  ``Splicery.Kernel.isChain_iff_pairwise_disjoint,
  ``Splicery.Kernel.prepare_spec,
  ``Splicery.Kernel.prepare_ok_iff,
  ``Splicery.Kernel.prepare_order_perm,
  ``Splicery.Kernel.admissible_perm,
  ``Splicery.Kernel.prepare_order_independent,
  ``Splicery.Kernel.materialize_spec,
  ``Splicery.Kernel.same_bytes_spec,
  ``Splicery.Kernel.render_spec,
  ``Splicery.Kernel.prepared_ready,
  ``Splicery.Kernel.prepare_render,
  ``Splicery.Kernel.spliced_length,
  ``Splicery.Kernel.spliced_single,
  ``Splicery.Kernel.prepare_empty,
  ``Splicery.Kernel.render_empty,
  ``Splicery.Kernel.apply_at_spec,
]

/-- Every theorem under `Splicery.Kernel`, sorted by name. -/
def kernelTheorems (env : Environment) : Array Name :=
  let audited : Name := `Splicery.Kernel
  (env.constants.fold (init := (#[] : Array Name)) fun acc decl info =>
    if audited.isPrefixOf decl && decl.isInternal = false && info.isThm then acc.push decl
    else acc).qsort (fun a b => a.toString < b.toString)

-- `name` is a reserved token once the Aeneas library is imported (its attribute
-- syntax), so declarations are called `decl` here.
def auditAxioms : CommandElabM Unit := do
  let env ← getEnv
  let sorted := kernelTheorems env
  if sorted.isEmpty then
    throwError "axiom audit: no theorems under Splicery.Kernel"
  for decl in sorted do
    let axioms := (← collectAxioms decl).qsort (fun a b => a.toString < b.toString)
    logInfo m!"AXIOMS {decl}: {axioms.toList}"
    for ax in axioms do
      unless allowedAxioms.contains ax do
        throwError "axiom audit: {decl} depends on {ax}, which is not a standard Lean axiom"
  for claim in claims do
    unless sorted.contains claim do
      throwError "axiom audit: required claim {claim} is missing or is not a theorem"
  logInfo m!"AXIOM_AUDIT ok: {sorted.size} theorems, {claims.length} required claims, axioms ⊆ {allowedAxioms}"

run_cmd auditAxioms
