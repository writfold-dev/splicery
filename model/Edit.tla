------------------------------ MODULE Edit ------------------------------
EXTENDS Naturals
\* Qualification seed: one logical edit, two content versions and one outsider
\* update. Inputs/grants are abstract values; filesystem assumptions in protocol/.
VARIABLES phase, file, base, owner, stamp, grant, nwrite, observed
vars == <<phase, file, base, owner, stamp, grant, nwrite, observed>>
Phases == {"idle","ready","claimed","wrote","unknown","confirmed","refused"}
Init == /\ phase = "idle" /\ file = 0 /\ base = 0 /\ owner = 0
        /\ stamp = 0 /\ grant = TRUE /\ nwrite = 0 /\ ~observed
Prepare == /\ phase = "idle" /\ grant /\ file # 2
           /\ phase' = "ready" /\ base' = file /\ stamp' = owner
           /\ UNCHANGED <<file,owner,grant,nwrite,observed>>
Drift == /\ phase \in {"idle","ready"} /\ file = 0 /\ file' = 1
         /\ UNCHANGED <<phase,base,owner,stamp,grant,nwrite,observed>>
Revoke == /\ grant /\ grant' = FALSE
          /\ UNCHANGED <<phase,file,base,owner,stamp,nwrite,observed>>
Transfer == /\ phase \in {"idle","ready","confirmed","refused"} /\ owner = 0
            /\ owner' = 1 /\ UNCHANGED <<phase,file,base,stamp,grant,nwrite,observed>>
Live == grant /\ stamp = owner /\ file = base
Claim == /\ phase = "ready" /\ Live /\ phase' = "claimed"
         /\ UNCHANGED <<file,base,owner,stamp,grant,nwrite,observed>>
Write == /\ phase = "claimed" /\ Live
         /\ phase' = "wrote" /\ file' = 2 /\ nwrite' = nwrite + 1
         /\ UNCHANGED <<base,owner,stamp,grant,observed>>
Refuse == /\ phase \in {"ready","claimed"} /\ ~Live /\ phase' = "refused"
          /\ UNCHANGED <<file,base,owner,stamp,grant,nwrite,observed>>
Lost == /\ phase \in {"claimed","wrote"} /\ phase' = "unknown"
        /\ UNCHANGED <<file,base,owner,stamp,grant,nwrite,observed>>
\* Exact observation assumes exclusive fenced resource, distinct after-image,
\* no other writers since dispatch, no ABA, and a quiescent old process.
Confirm == /\ phase \in {"wrote","unknown"} /\ file = 2
           /\ phase' = "confirmed" /\ observed' = TRUE
           /\ UNCHANGED <<file,base,owner,stamp,grant,nwrite>>
Next == Prepare \/ Drift \/ Revoke \/ Transfer \/ Claim \/ Write \/ Refuse \/ Lost \/ Confirm
Spec == Init /\ [][Next]_vars
TypeOK == /\ phase \in Phases /\ file \in 0..2 /\ base \in 0..1
          /\ owner \in 0..1 /\ stamp \in 0..1 /\ grant \in BOOLEAN
          /\ nwrite \in Nat /\ observed \in BOOLEAN
AtMostOnce == nwrite <= 1
Evidence == (phase = "confirmed") => (observed /\ nwrite = 1 /\ file = 2)
Owned == phase \in {"claimed","wrote","unknown"} => stamp = owner
=============================================================================
