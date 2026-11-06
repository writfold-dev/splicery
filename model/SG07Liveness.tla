------------------------------- MODULE SG07Liveness -------------------------------
EXTENDS SG07

\* Liveness is deliberately not a conjunct of Spec or StrengthenedInv.
\* Progress needs old-process quiescence, working storage/fsync, responsive
\* owner and observers, and eventual authority; crashes may leave Unknown forever.
Availability(k) ==
  /\ k \in Keys
  /\ IF reqClient[k] \in Clients THEN
       /\ grant[reqClient[k]] /\ ~cancelled[reqClient[k]] /\ ~blocked[k]
       /\ epoch[reqClient[k]] = reqEpoch[k]
     ELSE FALSE

Fairness ==
  /\ \A k \in Keys : WF_vars(UnblockOwner(k))
  /\ \A k \in Keys, f \in Files : WF_vars(Write(k, f))
  /\ \A k \in Keys, f \in Files : WF_vars(Fsync(k, f))
  /\ \A k \in Keys, f \in Files : WF_vars(Observe(k, f))
  /\ \A k \in Keys, f \in Files : WF_vars(Confirm(k, f))
  /\ \A k \in Keys, f \in Files : WF_vars(ResolveNoEffect(k, f))
  /\ \A k \in Keys : WF_vars(Settle(k)) /\ WF_vars(Quiesce(k))

LivenessSpec == Spec /\ Fairness
EventuallyQuiescent == \A k \in Keys : live[k] ~> ~live[k]

\* Availability must eventually stabilize for a progress proof.  This module
\* only records that separate obligation; SG07Safety proves no liveness theorem.
ProgressAssumption == \A k \in Keys : phase[k] # "idle" => <>[]Availability(k)

====================================================================================
