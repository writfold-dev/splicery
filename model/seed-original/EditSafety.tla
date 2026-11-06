---------------------------- MODULE EditSafety ----------------------------
EXTENDS Edit, TLAPS
\* Starter TLAPS obligations for the one-effect seed, unexecuted.
CountPhase ==
  /\ (phase \in {"idle","ready","claimed","refused"} => nwrite = 0)
  /\ (phase \in {"wrote","confirmed"} => nwrite = 1)
  /\ (file = 2 <=> nwrite = 1)
Strong == TypeOK /\ AtMostOnce /\ Evidence /\ Owned /\ CountPhase

THEOREM StartOK == Init => Strong
BY SMT DEF Init, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase

THEOREM Preserve == Strong /\ [Next]_vars => Strong'
<1>1. Strong /\ Prepare => Strong'
  BY SMT DEF Prepare, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>2. Strong /\ Drift => Strong'
  BY SMT DEF Drift, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>3. Strong /\ Revoke => Strong'
  BY SMT DEF Revoke, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>4. Strong /\ Transfer => Strong'
  BY SMT DEF Transfer, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>5. Strong /\ Claim => Strong'
  BY SMT DEF Claim, Live, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>6. Strong /\ Write => Strong'
  BY SMT DEF Write, Live, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>7. Strong /\ Refuse => Strong'
  BY SMT DEF Refuse, Live, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>8. Strong /\ Lost => Strong'
  BY SMT DEF Lost, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>9. Strong /\ Confirm => Strong'
  BY SMT DEF Confirm, Strong, TypeOK, Phases, AtMostOnce, Evidence, Owned, CountPhase
<1>10. Strong /\ UNCHANGED vars => Strong'
  BY DEF vars, Strong, TypeOK, AtMostOnce, Evidence, Owned, CountPhase
<1> QED BY <1>1, <1>2, <1>3, <1>4, <1>5, <1>6, <1>7, <1>8, <1>9, <1>10 DEF Next

THEOREM SeedSafety == Spec => [](AtMostOnce /\ Evidence /\ Owned)
<1>1. Spec => []Strong BY StartOK, Preserve, PTL DEF Spec
<1> QED BY <1>1, PTL DEF Strong
=============================================================================
