-------------------------------- MODULE SG07 --------------------------------
EXTENDS Naturals

\* SG07 is a bounded safety model of the owner ledger and native adapter.
\* A request key owns at most one claim.  Each prepared edit addresses both
\* files, so every request exposes per-file rather than global outcomes.
Keys == {"k0", "k1"}
Clients == {"c0", "c1"}
Preps == {"p0", "p1"}
Files == {"f0", "f1"}
Digests == 0..1
Intents == 1..2
Phases == {"idle", "ready", "claimed", "unknown", "confirmed", "refused", "partial"}
Results == {"pending", "renamed", "durable", "unknown", "confirmed", "refused"}
Tokens == {"initial", "external", "rival", "k0", "k1"}
NoKey == "none"
NoPrep == "none"

PrepKey(p) == IF p = "p0" THEN "k0" ELSE "k1"
PrepClient(p) == IF p = "p0" THEN "c0" ELSE "c1"
PrepIntent(p) == IF p = "p0" THEN 1 ELSE 2
PrepBase(p, f) == 0
PrepAfter(p, f) == 1

VARIABLES
  phase_k0, phase_k1,
  intent_k0, intent_k1,
  reqPrep_k0, reqPrep_k1,
  reqClient_k0, reqClient_k1,
  reqEpoch_k0, reqEpoch_k1,
  base_k0_f0, base_k0_f1, base_k1_f0, base_k1_f1,
  baseWriter_k0_f0, baseWriter_k0_f1, baseWriter_k1_f0, baseWriter_k1_f1,
  after_k0_f0, after_k0_f1, after_k1_f0, after_k1_f1,
  prepLive_p0, prepLive_p1,
  grant_c0, grant_c1,
  cancelled_c0, cancelled_c1,
  epoch_c0, epoch_c1,
  fileState_f0, fileState_f1,
  fileWriter_f0, fileWriter_f1,
  captured_f0, captured_f1,
  live_k0, live_k1,
  journal_k0, journal_k1,
  blocked_k0, blocked_k1,
  result_k0_f0, result_k0_f1, result_k1_f0, result_k1_f1,
  writes_k0_f0, writes_k0_f1, writes_k1_f0, writes_k1_f1,
  authorityAtWrite_k0_f0, authorityAtWrite_k0_f1, authorityAtWrite_k1_f0, authorityAtWrite_k1_f1,
  preimageAtWrite_k0_f0, preimageAtWrite_k0_f1, preimageAtWrite_k1_f0, preimageAtWrite_k1_f1,
  obsKey_k0_f0, obsKey_k0_f1, obsKey_k1_f0, obsKey_k1_f1,
  obsDigest_k0_f0, obsDigest_k0_f1, obsDigest_k1_f0, obsDigest_k1_f1,
  obsWriter_k0_f0, obsWriter_k0_f1, obsWriter_k1_f0, obsWriter_k1_f1,
  confirmKey_k0_f0, confirmKey_k0_f1, confirmKey_k1_f0, confirmKey_k1_f1,
  confirmDigest_k0_f0, confirmDigest_k0_f1, confirmDigest_k1_f0, confirmDigest_k1_f1,
  confirmWriter_k0_f0, confirmWriter_k0_f1, confirmWriter_k1_f0, confirmWriter_k1_f1,
  everUnknown_k0, everUnknown_k1, resources, step

phaseVars == <<phase_k0, phase_k1>>
intentVars == <<intent_k0, intent_k1>>
reqPrepVars == <<reqPrep_k0, reqPrep_k1>>
reqClientVars == <<reqClient_k0, reqClient_k1>>
reqEpochVars == <<reqEpoch_k0, reqEpoch_k1>>
baseVars == <<base_k0_f0, base_k0_f1, base_k1_f0, base_k1_f1>>
baseWriterVars == <<baseWriter_k0_f0, baseWriter_k0_f1, baseWriter_k1_f0, baseWriter_k1_f1>>
afterVars == <<after_k0_f0, after_k0_f1, after_k1_f0, after_k1_f1>>
prepLiveVars == <<prepLive_p0, prepLive_p1>>
grantVars == <<grant_c0, grant_c1>>
cancelledVars == <<cancelled_c0, cancelled_c1>>
epochVars == <<epoch_c0, epoch_c1>>
fileStateVars == <<fileState_f0, fileState_f1>>
fileWriterVars == <<fileWriter_f0, fileWriter_f1>>
capturedVars == <<captured_f0, captured_f1>>
liveVars == <<live_k0, live_k1>>
journalVars == <<journal_k0, journal_k1>>
blockedVars == <<blocked_k0, blocked_k1>>
resultVars == <<result_k0_f0, result_k0_f1, result_k1_f0, result_k1_f1>>
writesVars == <<writes_k0_f0, writes_k0_f1, writes_k1_f0, writes_k1_f1>>
authorityAtWriteVars == <<authorityAtWrite_k0_f0, authorityAtWrite_k0_f1, authorityAtWrite_k1_f0, authorityAtWrite_k1_f1>>
preimageAtWriteVars == <<preimageAtWrite_k0_f0, preimageAtWrite_k0_f1, preimageAtWrite_k1_f0, preimageAtWrite_k1_f1>>
obsKeyVars == <<obsKey_k0_f0, obsKey_k0_f1, obsKey_k1_f0, obsKey_k1_f1>>
obsDigestVars == <<obsDigest_k0_f0, obsDigest_k0_f1, obsDigest_k1_f0, obsDigest_k1_f1>>
obsWriterVars == <<obsWriter_k0_f0, obsWriter_k0_f1, obsWriter_k1_f0, obsWriter_k1_f1>>
confirmKeyVars == <<confirmKey_k0_f0, confirmKey_k0_f1, confirmKey_k1_f0, confirmKey_k1_f1>>
confirmDigestVars == <<confirmDigest_k0_f0, confirmDigest_k0_f1, confirmDigest_k1_f0, confirmDigest_k1_f1>>
confirmWriterVars == <<confirmWriter_k0_f0, confirmWriter_k0_f1, confirmWriter_k1_f0, confirmWriter_k1_f1>>
everUnknownVars == <<everUnknown_k0, everUnknown_k1>>

vars == <<
          phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars,
          baseVars, baseWriterVars, afterVars, prepLiveVars, grantVars,
          cancelledVars, epochVars, fileStateVars, fileWriterVars, capturedVars,
          liveVars, journalVars, blockedVars, resultVars, writesVars,
          authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars, obsWriterVars,
          confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars, resources,
          step>>

EffectVars == <<
                phaseVars, fileStateVars, fileWriterVars, liveVars, journalVars,
                resources, resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars,
                obsKeyVars, obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars,
                confirmWriterVars, everUnknownVars, step>>

Init ==
  /\ phase_k0 = "idle"
  /\ phase_k1 = "idle"
  /\ intent_k0 = 0
  /\ intent_k1 = 0
  /\ reqPrep_k0 = NoPrep
  /\ reqPrep_k1 = NoPrep
  /\ reqClient_k0 = NoKey
  /\ reqClient_k1 = NoKey
  /\ reqEpoch_k0 = 0
  /\ reqEpoch_k1 = 0
  /\ base_k0_f0 = 0
  /\ base_k0_f1 = 0
  /\ base_k1_f0 = 0
  /\ base_k1_f1 = 0
  /\ baseWriter_k0_f0 = "initial"
  /\ baseWriter_k0_f1 = "initial"
  /\ baseWriter_k1_f0 = "initial"
  /\ baseWriter_k1_f1 = "initial"
  /\ after_k0_f0 = 0
  /\ after_k0_f1 = 0
  /\ after_k1_f0 = 0
  /\ after_k1_f1 = 0
  /\ prepLive_p0 = TRUE
  /\ prepLive_p1 = TRUE
  /\ grant_c0 = TRUE
  /\ grant_c1 = TRUE
  /\ cancelled_c0 = FALSE
  /\ cancelled_c1 = FALSE
  /\ epoch_c0 = 0
  /\ epoch_c1 = 0
  /\ fileState_f0 = 0
  /\ fileState_f1 = 0
  /\ fileWriter_f0 = "initial"
  /\ fileWriter_f1 = "initial"
  /\ captured_f0 = FALSE
  /\ captured_f1 = FALSE
  /\ live_k0 = FALSE
  /\ live_k1 = FALSE
  /\ journal_k0 = FALSE
  /\ journal_k1 = FALSE
  /\ resources = 0
  /\ blocked_k0 = FALSE
  /\ blocked_k1 = FALSE
  /\ result_k0_f0 = "pending"
  /\ result_k0_f1 = "pending"
  /\ result_k1_f0 = "pending"
  /\ result_k1_f1 = "pending"
  /\ writes_k0_f0 = 0
  /\ writes_k0_f1 = 0
  /\ writes_k1_f0 = 0
  /\ writes_k1_f1 = 0
  /\ authorityAtWrite_k0_f0 = FALSE
  /\ authorityAtWrite_k0_f1 = FALSE
  /\ authorityAtWrite_k1_f0 = FALSE
  /\ authorityAtWrite_k1_f1 = FALSE
  /\ preimageAtWrite_k0_f0 = 0
  /\ preimageAtWrite_k0_f1 = 0
  /\ preimageAtWrite_k1_f0 = 0
  /\ preimageAtWrite_k1_f1 = 0
  /\ obsKey_k0_f0 = NoKey
  /\ obsKey_k0_f1 = NoKey
  /\ obsKey_k1_f0 = NoKey
  /\ obsKey_k1_f1 = NoKey
  /\ obsDigest_k0_f0 = 0
  /\ obsDigest_k0_f1 = 0
  /\ obsDigest_k1_f0 = 0
  /\ obsDigest_k1_f1 = 0
  /\ obsWriter_k0_f0 = "initial"
  /\ obsWriter_k0_f1 = "initial"
  /\ obsWriter_k1_f0 = "initial"
  /\ obsWriter_k1_f1 = "initial"
  /\ confirmKey_k0_f0 = NoKey
  /\ confirmKey_k0_f1 = NoKey
  /\ confirmKey_k1_f0 = NoKey
  /\ confirmKey_k1_f1 = NoKey
  /\ confirmDigest_k0_f0 = 0
  /\ confirmDigest_k0_f1 = 0
  /\ confirmDigest_k1_f0 = 0
  /\ confirmDigest_k1_f1 = 0
  /\ confirmWriter_k0_f0 = "initial"
  /\ confirmWriter_k0_f1 = "initial"
  /\ confirmWriter_k1_f0 = "initial"
  /\ confirmWriter_k1_f1 = "initial"
  /\ everUnknown_k0 = FALSE
  /\ everUnknown_k1 = FALSE
  /\ step = 0
FileReserved(f) == live_k0 \/ live_k1

Predecessor(k, f) ==
  /\ (IF f = "f0" THEN fileState_f0 ELSE fileState_f1) =
       (IF k = "k0" THEN (IF f = "f0" THEN base_k0_f0 ELSE base_k0_f1)
        ELSE (IF f = "f0" THEN base_k1_f0 ELSE base_k1_f1))
  /\ (IF f = "f0" THEN fileWriter_f0 ELSE fileWriter_f1) =
       (IF k = "k0" THEN (IF f = "f0" THEN baseWriter_k0_f0 ELSE baseWriter_k0_f1)
        ELSE (IF f = "f0" THEN baseWriter_k1_f0 ELSE baseWriter_k1_f1))

Authority(k) ==
  /\ (IF k = "k0" THEN reqClient_k0 ELSE reqClient_k1) \in Clients
  /\ (IF (IF k = "k0" THEN reqClient_k0 ELSE reqClient_k1) = "c0" THEN grant_c0 ELSE grant_c1)
  /\ ~(IF (IF k = "k0" THEN reqClient_k0 ELSE reqClient_k1) = "c0" THEN cancelled_c0 ELSE cancelled_c1)
  /\ ~(IF k = "k0" THEN blocked_k0 ELSE blocked_k1)
  /\ (IF (IF k = "k0" THEN reqClient_k0 ELSE reqClient_k1) = "c0" THEN epoch_c0 ELSE epoch_c1) =
       (IF k = "k0" THEN reqEpoch_k0 ELSE reqEpoch_k1)

LiveAtWrite(k, f) ==
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed"
  /\ (IF k = "k0" THEN live_k0 ELSE live_k1)
  /\ (IF k = "k0" THEN journal_k0 ELSE journal_k1)
  /\ Authority(k)
  /\ Predecessor(k, f)

ReadyToClaim(k) ==
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "ready"
  /\ Authority(k)
  /\ Predecessor(k, "f0") /\ Predecessor(k, "f1")

\* Intake is keyed before any effect.  A same-key/different-intent request is
\* handled by Conflict below, never by replacing this ledger entry.
Dispatch(k, p, c) ==
  /\ k \in Keys /\ p \in Preps /\ c \in Clients
  /\ ((step = 0 /\ k = "k0") \/ (step = 1 /\ k = "k1") \/ (step = 12 /\ k = "k0"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "idle"
  /\ PrepKey(p) = k /\ PrepClient(p) = c
  /\ (IF p = "p0" THEN prepLive_p0 ELSE prepLive_p1) /\ (IF c = "c0" THEN grant_c0 ELSE grant_c1) /\ ~(IF c = "c0" THEN cancelled_c0 ELSE cancelled_c1) /\ ~(IF k = "k0" THEN blocked_k0 ELSE blocked_k1)
  /\ fileState_f0 = PrepBase(p, "f0") /\ fileWriter_f0 = "initial"
  /\ fileState_f1 = PrepBase(p, "f1") /\ fileWriter_f1 = "initial"
  /\ phase_k0' = IF k = "k0" THEN "ready" ELSE phase_k0
  /\ phase_k1' = IF k = "k1" THEN "ready" ELSE phase_k1
  /\ intent_k0' = IF k = "k0" THEN PrepIntent(p) ELSE intent_k0
  /\ intent_k1' = IF k = "k1" THEN PrepIntent(p) ELSE intent_k1
  /\ reqPrep_k0' = IF k = "k0" THEN p ELSE reqPrep_k0
  /\ reqPrep_k1' = IF k = "k1" THEN p ELSE reqPrep_k1
  /\ reqClient_k0' = IF k = "k0" THEN c ELSE reqClient_k0
  /\ reqClient_k1' = IF k = "k1" THEN c ELSE reqClient_k1
  /\ reqEpoch_k0' = IF k = "k0" THEN (IF c = "c0" THEN epoch_c0 ELSE epoch_c1) ELSE reqEpoch_k0
  /\ reqEpoch_k1' = IF k = "k1" THEN (IF c = "c0" THEN epoch_c0 ELSE epoch_c1) ELSE reqEpoch_k1
  /\ base_k0_f0' = IF k = "k0" THEN PrepBase(p, "f0") ELSE base_k0_f0
  /\ base_k0_f1' = IF k = "k0" THEN PrepBase(p, "f1") ELSE base_k0_f1
  /\ base_k1_f0' = IF k = "k1" THEN PrepBase(p, "f0") ELSE base_k1_f0
  /\ base_k1_f1' = IF k = "k1" THEN PrepBase(p, "f1") ELSE base_k1_f1
  /\ baseWriter_k0_f0' = IF k = "k0" THEN "initial" ELSE baseWriter_k0_f0
  /\ baseWriter_k0_f1' = IF k = "k0" THEN "initial" ELSE baseWriter_k0_f1
  /\ baseWriter_k1_f0' = IF k = "k1" THEN "initial" ELSE baseWriter_k1_f0
  /\ baseWriter_k1_f1' = IF k = "k1" THEN "initial" ELSE baseWriter_k1_f1
  /\ after_k0_f0' = IF k = "k0" THEN PrepAfter(p, "f0") ELSE after_k0_f0
  /\ after_k0_f1' = IF k = "k0" THEN PrepAfter(p, "f1") ELSE after_k0_f1
  /\ after_k1_f0' = IF k = "k1" THEN PrepAfter(p, "f0") ELSE after_k1_f0
  /\ after_k1_f1' = IF k = "k1" THEN PrepAfter(p, "f1") ELSE after_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

\* An acknowledged identical retry reads the ledger even after revocation.
Replay(k, p, d) ==
  /\ k \in Keys /\ p \in Preps /\ d \in Intents
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle"
  /\ (IF k = "k0" THEN reqPrep_k0 ELSE reqPrep_k1) = p /\ (IF k = "k0" THEN intent_k0 ELSE intent_k1) = d
  /\ UNCHANGED vars

Conflict(k, d) ==
  /\ k \in Keys /\ d \in Intents
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle" /\ d # (IF k = "k0" THEN intent_k0 ELSE intent_k1)
  /\ UNCHANGED vars

HistoricalLookup(k, c) ==
  /\ k \in Keys /\ c \in Clients
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle" /\ (IF k = "k0" THEN reqClient_k0 ELSE reqClient_k1) = c
  /\ UNCHANGED vars

RejectEvicted(k, p) ==
  /\ k \in Keys /\ p \in Preps
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "idle" /\ PrepKey(p) = k /\ ~(IF p = "p0" THEN prepLive_p0 ELSE prepLive_p1)
  /\ UNCHANGED vars

Evict(p) ==
  /\ p \in Preps /\ (IF p = "p0" THEN prepLive_p0 ELSE prepLive_p1) /\ step = 0 /\ p = "p1"
  /\ reqPrep_k0 # p /\ reqPrep_k1 # p
  /\ prepLive_p0' = prepLive_p0 /\ prepLive_p1' = FALSE
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars, capturedVars,
                 liveVars, journalVars, resources, blockedVars, resultVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

Capture(f) ==
  /\ f \in Files /\ ~(IF f = "f0" THEN captured_f0 ELSE captured_f1) /\ step = 0
  /\ captured_f0' = IF f = "f0" THEN TRUE ELSE captured_f0
  /\ captured_f1' = IF f = "f1" THEN TRUE ELSE captured_f1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 liveVars, journalVars, resources, blockedVars, resultVars, writesVars,
                 authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars, obsWriterVars,
                 confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars, step>>

Revoke(c) ==
  /\ c \in Clients /\ (IF c = "c0" THEN grant_c0 ELSE grant_c1) /\ c = "c0" /\ step \in {2, 3, 6}
  /\ grant_c0' = FALSE /\ grant_c1' = grant_c1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

Cancel(c) ==
  /\ c \in Clients /\ ~(IF c = "c0" THEN cancelled_c0 ELSE cancelled_c1) /\ c = "c0" /\ step \in {2, 3}
  /\ cancelled_c0' = TRUE /\ cancelled_c1' = cancelled_c1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, epochVars, fileStateVars, fileWriterVars, capturedVars,
                 liveVars, journalVars, resources, blockedVars, resultVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

RotateAlias(c) ==
  /\ c \in Clients /\ (IF c = "c0" THEN epoch_c0 ELSE epoch_c1) = 0 /\ c = "c0" /\ step \in {2, 3}
  /\ epoch_c0' = 1 /\ epoch_c1' = epoch_c1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

\* External predecessor changes and rivals are admitted only outside a held
\* reservation: this is the cooperating-writer assumption made executable.
Drift(f) ==
  /\ f \in Files /\ ~FileReserved(f) /\ step = 2
  /\ fileState_f0' = IF f = "f0" THEN (IF fileState_f0 = 0 THEN 1 ELSE 0) ELSE fileState_f0
  /\ fileState_f1' = IF f = "f1" THEN (IF fileState_f1 = 0 THEN 1 ELSE 0) ELSE fileState_f1
  /\ fileWriter_f0' = IF f = "f0" THEN "external" ELSE fileWriter_f0
  /\ fileWriter_f1' = IF f = "f1" THEN "external" ELSE fileWriter_f1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, capturedVars,
                 liveVars, journalVars, resources, blockedVars, resultVars, writesVars,
                 authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars, obsWriterVars,
                 confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars, step>>

RivalWrite(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) # "idle" /\ ~FileReserved(f) /\ step \in {2, 5} /\ k = "k0"
  /\ fileState_f0' = IF f = "f0" THEN (IF k = "k0" THEN after_k0_f0 ELSE after_k1_f0) ELSE fileState_f0
  /\ fileState_f1' = IF f = "f1" THEN (IF k = "k0" THEN after_k0_f1 ELSE after_k1_f1) ELSE fileState_f1
  /\ fileWriter_f0' = IF f = "f0" THEN "rival" ELSE fileWriter_f0
  /\ fileWriter_f1' = IF f = "f1" THEN "rival" ELSE fileWriter_f1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, capturedVars,
                 liveVars, journalVars, resources, blockedVars, resultVars, writesVars,
                 authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars, obsWriterVars,
                 confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars, step>>

Claim(k) ==
  /\ k \in Keys /\ ReadyToClaim(k) /\ step = 2
  /\ resources = 0 /\ ~(IF k = "k0" THEN live_k0 ELSE live_k1) /\ ~(IF k = "k0" THEN journal_k0 ELSE journal_k1)
  /\ phase_k0' = IF k = "k0" THEN "claimed" ELSE phase_k0
  /\ phase_k1' = IF k = "k1" THEN "claimed" ELSE phase_k1
  /\ live_k0' = IF k = "k0" THEN TRUE ELSE live_k0
  /\ live_k1' = IF k = "k1" THEN TRUE ELSE live_k1
  /\ journal_k0' = IF k = "k0" THEN TRUE ELSE journal_k0
  /\ journal_k1' = IF k = "k1" THEN TRUE ELSE journal_k1
  /\ resources' = resources + 1
  /\ step' = step + 1
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, blockedVars, resultVars, writesVars,
                 authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars, obsWriterVars,
                 confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars>>

Exhaust(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "ready" /\ resources = 1
  /\ UNCHANGED vars

BlockOwner(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"ready", "claimed", "unknown"} /\ ~(IF k = "k0" THEN blocked_k0 ELSE blocked_k1)
  /\ step = 3 /\ k = "k0"
  /\ blocked_k0' = TRUE /\ blocked_k1' = blocked_k1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, resultVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

UnblockOwner(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN blocked_k0 ELSE blocked_k1) /\ step = 3 /\ k = "k0"
  /\ blocked_k0' = FALSE /\ blocked_k1' = blocked_k1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, resultVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars, step>>

\* The durable claim already exists.  Rename is the first target-visible IO;
\* authority and exact predecessor are journaled with the effect boundary.
Write(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 3 /\ f = "f0") \/ (step = 6 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed" /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) = "pending" /\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 0
  /\ LiveAtWrite(k, f)
  /\ authorityAtWrite_k0_f0' = IF f = "f0" THEN LiveAtWrite(k, f) ELSE authorityAtWrite_k0_f0
  /\ authorityAtWrite_k0_f1' = IF f = "f1" THEN LiveAtWrite(k, f) ELSE authorityAtWrite_k0_f1
  /\ authorityAtWrite_k1_f0' = authorityAtWrite_k1_f0
  /\ authorityAtWrite_k1_f1' = authorityAtWrite_k1_f1
  /\ preimageAtWrite_k0_f0' = IF f = "f0" THEN fileState_f0 ELSE preimageAtWrite_k0_f0
  /\ preimageAtWrite_k0_f1' = IF f = "f1" THEN fileState_f1 ELSE preimageAtWrite_k0_f1
  /\ preimageAtWrite_k1_f0' = preimageAtWrite_k1_f0
  /\ preimageAtWrite_k1_f1' = preimageAtWrite_k1_f1
  /\ writes_k0_f0' = IF f = "f0" THEN 1 ELSE writes_k0_f0
  /\ writes_k0_f1' = IF f = "f1" THEN 1 ELSE writes_k0_f1
  /\ writes_k1_f0' = writes_k1_f0
  /\ writes_k1_f1' = writes_k1_f1
  /\ result_k0_f0' = IF f = "f0" THEN "renamed" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "renamed" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ obsKey_k0_f0' = IF f = "f0" THEN NoKey ELSE obsKey_k0_f0
  /\ obsKey_k0_f1' = IF f = "f1" THEN NoKey ELSE obsKey_k0_f1
  /\ obsKey_k1_f0' = obsKey_k1_f0
  /\ obsKey_k1_f1' = obsKey_k1_f1
  /\ fileState_f0' = IF f = "f0" THEN (IF k = "k0" THEN after_k0_f0 ELSE after_k1_f0) ELSE fileState_f0
  /\ fileState_f1' = IF f = "f1" THEN (IF k = "k0" THEN after_k0_f1 ELSE after_k1_f1) ELSE fileState_f1
  /\ fileWriter_f0' = IF f = "f0" THEN k ELSE fileWriter_f0
  /\ fileWriter_f1' = IF f = "f1" THEN k ELSE fileWriter_f1
  /\ step' = step + 1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, capturedVars,
                 liveVars, journalVars, resources, blockedVars, obsDigestVars, obsWriterVars,
                 confirmKeyVars, confirmDigestVars, confirmWriterVars, everUnknownVars>>

RefuseReady(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "ready" /\ ~ReadyToClaim(k) /\ step = 2
  /\ phase_k0' = IF k = "k0" THEN "refused" ELSE phase_k0
  /\ phase_k1' = IF k = "k1" THEN "refused" ELSE phase_k1
  /\ result_k0_f0' = IF k = "k0" THEN "refused" ELSE result_k0_f0
  /\ result_k0_f1' = IF k = "k0" THEN "refused" ELSE result_k0_f1
  /\ result_k1_f0' = IF k = "k1" THEN "refused" ELSE result_k1_f0
  /\ result_k1_f1' = IF k = "k1" THEN "refused" ELSE result_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

Refuse(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 3 /\ f = "f0") \/ (step \in {4, 6} /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed" /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) = "pending" /\ ~LiveAtWrite(k, f)
  /\ result_k0_f0' = IF f = "f0" THEN "refused" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "refused" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

Fsync(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 4 /\ f = "f0") \/ (step = 7 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"claimed", "unknown"} /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) = "renamed"
  /\ result_k0_f0' = IF f = "f0" THEN "durable" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "durable" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

PowerLoss(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 4 /\ f = "f0") \/ (step = 7 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed" /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) = "renamed"
  /\ phase_k0' = "unknown" /\ phase_k1' = phase_k1
  /\ result_k0_f0' = IF f = "f0" THEN "unknown" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "unknown" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ everUnknown_k0' = TRUE /\ everUnknown_k1' = everUnknown_k1
  /\ step' = step + 1
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars>>

Timeout(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed" /\ k = "k0" /\ step = 3
  /\ (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) = "pending" /\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) = "pending"
  /\ phase_k0' = "unknown" /\ phase_k1' = phase_k1
  /\ everUnknown_k0' = TRUE /\ everUnknown_k1' = everUnknown_k1
  /\ step' = 12
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars>>

InterruptBeforeClaim(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "ready"
  /\ UNCHANGED vars

InterruptActive(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "claimed" /\ k = "k0" /\ step = 3
  /\ phase_k0' = "unknown" /\ phase_k1' = phase_k1
  /\ everUnknown_k0' = TRUE /\ everUnknown_k1' = everUnknown_k1
  /\ step' = 12
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars>>

Observe(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 5 /\ f = "f0") \/ (step = 8 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"claimed", "unknown"}
  /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) \in {"durable", "unknown"}
  /\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 1
  /\ (IF f = "f0" THEN fileState_f0 ELSE fileState_f1) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1)) /\ (IF f = "f0" THEN fileWriter_f0 ELSE fileWriter_f1) = k
  /\ obsKey_k0_f0' = IF f = "f0" THEN "k0" ELSE obsKey_k0_f0
  /\ obsKey_k0_f1' = IF f = "f1" THEN "k0" ELSE obsKey_k0_f1
  /\ obsKey_k1_f0' = obsKey_k1_f0
  /\ obsKey_k1_f1' = obsKey_k1_f1
  /\ obsDigest_k0_f0' = IF f = "f0" THEN fileState_f0 ELSE obsDigest_k0_f0
  /\ obsDigest_k0_f1' = IF f = "f1" THEN fileState_f1 ELSE obsDigest_k0_f1
  /\ obsDigest_k1_f0' = obsDigest_k1_f0
  /\ obsDigest_k1_f1' = obsDigest_k1_f1
  /\ obsWriter_k0_f0' = IF f = "f0" THEN fileWriter_f0 ELSE obsWriter_k0_f0
  /\ obsWriter_k0_f1' = IF f = "f1" THEN fileWriter_f1 ELSE obsWriter_k0_f1
  /\ obsWriter_k1_f0' = obsWriter_k1_f0
  /\ obsWriter_k1_f1' = obsWriter_k1_f1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, confirmKeyVars,
                 confirmDigestVars, confirmWriterVars, everUnknownVars, step>>

CompetingObserve(k, f, other) ==
  /\ k \in Keys /\ f \in Files /\ other \in Keys /\ other # k
  /\ k = "k0" /\ other = "k1" /\ ((step = 5 /\ f = "f0") \/ (step = 8 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"claimed", "unknown"}
  /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) \in {"durable", "unknown"}
  /\ obsKey_k0_f0' = IF f = "f0" THEN other ELSE obsKey_k0_f0
  /\ obsKey_k0_f1' = IF f = "f1" THEN other ELSE obsKey_k0_f1
  /\ obsKey_k1_f0' = obsKey_k1_f0
  /\ obsKey_k1_f1' = obsKey_k1_f1
  /\ obsDigest_k0_f0' = IF f = "f0" THEN fileState_f0 ELSE obsDigest_k0_f0
  /\ obsDigest_k0_f1' = IF f = "f1" THEN fileState_f1 ELSE obsDigest_k0_f1
  /\ obsDigest_k1_f0' = obsDigest_k1_f0
  /\ obsDigest_k1_f1' = obsDigest_k1_f1
  /\ obsWriter_k0_f0' = IF f = "f0" THEN fileWriter_f0 ELSE obsWriter_k0_f0
  /\ obsWriter_k0_f1' = IF f = "f1" THEN fileWriter_f1 ELSE obsWriter_k0_f1
  /\ obsWriter_k1_f0' = obsWriter_k1_f0
  /\ obsWriter_k1_f1' = obsWriter_k1_f1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, confirmKeyVars,
                 confirmDigestVars, confirmWriterVars, everUnknownVars, step>>

\* Confirm abstracts reconciliation plus any still-required directory fsync.
\* The discriminating evidence is key + effect token + digest, not bytes alone.
Confirm(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 5 /\ f = "f0") \/ (step = 8 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"claimed", "unknown"}
  /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) \in {"durable", "unknown"}
  /\ (IF k = "k0" THEN journal_k0 ELSE journal_k1) /\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 1
  /\ (IF f = "f0" THEN fileState_f0 ELSE fileState_f1) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1)) /\ (IF f = "f0" THEN fileWriter_f0 ELSE fileWriter_f1) = k
  /\ (IF k = "k0" THEN (IF f = "f0" THEN obsKey_k0_f0 ELSE obsKey_k0_f1) ELSE (IF f = "f0" THEN obsKey_k1_f0 ELSE obsKey_k1_f1)) = k /\ (IF k = "k0" THEN (IF f = "f0" THEN obsWriter_k0_f0 ELSE obsWriter_k0_f1) ELSE (IF f = "f0" THEN obsWriter_k1_f0 ELSE obsWriter_k1_f1)) = k
  /\ (IF k = "k0" THEN (IF f = "f0" THEN obsDigest_k0_f0 ELSE obsDigest_k0_f1) ELSE (IF f = "f0" THEN obsDigest_k1_f0 ELSE obsDigest_k1_f1)) = (IF k = "k0" THEN (IF f = "f0" THEN after_k0_f0 ELSE after_k0_f1) ELSE (IF f = "f0" THEN after_k1_f0 ELSE after_k1_f1))
  /\ result_k0_f0' = IF f = "f0" THEN "confirmed" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "confirmed" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ confirmKey_k0_f0' = IF f = "f0" THEN obsKey_k0_f0 ELSE confirmKey_k0_f0
  /\ confirmKey_k0_f1' = IF f = "f1" THEN obsKey_k0_f1 ELSE confirmKey_k0_f1
  /\ confirmKey_k1_f0' = confirmKey_k1_f0
  /\ confirmKey_k1_f1' = confirmKey_k1_f1
  /\ confirmDigest_k0_f0' = IF f = "f0" THEN obsDigest_k0_f0 ELSE confirmDigest_k0_f0
  /\ confirmDigest_k0_f1' = IF f = "f1" THEN obsDigest_k0_f1 ELSE confirmDigest_k0_f1
  /\ confirmDigest_k1_f0' = confirmDigest_k1_f0
  /\ confirmDigest_k1_f1' = confirmDigest_k1_f1
  /\ confirmWriter_k0_f0' = IF f = "f0" THEN fileWriter_f0 ELSE confirmWriter_k0_f0
  /\ confirmWriter_k0_f1' = IF f = "f1" THEN fileWriter_f1 ELSE confirmWriter_k0_f1
  /\ confirmWriter_k1_f0' = confirmWriter_k1_f0
  /\ confirmWriter_k1_f1' = confirmWriter_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, everUnknownVars>>

ResolveNoEffect(k, f) ==
  /\ k \in Keys /\ f \in Files
  /\ k = "k0" /\ ((step = 12 /\ f = "f0") \/ (step = 13 /\ f = "f1"))
  /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) = "unknown" /\ (IF k = "k0" THEN (IF f = "f0" THEN result_k0_f0 ELSE result_k0_f1) ELSE (IF f = "f0" THEN result_k1_f0 ELSE result_k1_f1)) = "pending" /\ (IF k = "k0" THEN (IF f = "f0" THEN writes_k0_f0 ELSE writes_k0_f1) ELSE (IF f = "f0" THEN writes_k1_f0 ELSE writes_k1_f1)) = 0
  /\ Predecessor(k, f)
  /\ result_k0_f0' = IF f = "f0" THEN "refused" ELSE result_k0_f0
  /\ result_k0_f1' = IF f = "f1" THEN "refused" ELSE result_k0_f1
  /\ result_k1_f0' = result_k1_f0
  /\ result_k1_f1' = result_k1_f1
  /\ step' = step + 1
  /\ UNCHANGED <<phaseVars, intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars,
                 afterVars, prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars, obsDigestVars,
                 obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

Settle(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"claimed", "unknown"}
  /\ k = "k0" /\ step \in {5, 7, 9, 14}
  /\ (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) \in {"confirmed", "refused"}
  /\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) \in {"confirmed", "refused"}
  /\ phase_k0' =
       IF (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) = "confirmed" /\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) = "confirmed" THEN "confirmed"
       ELSE IF (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) = "refused" /\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) = "refused" THEN "refused"
       ELSE "partial"
  /\ phase_k1' = phase_k1
  /\ step' = step + 1
  /\ UNCHANGED <<intentVars, reqPrepVars, reqClientVars, reqEpochVars, baseVars, baseWriterVars, afterVars,
                 prepLiveVars, grantVars, cancelledVars, epochVars, fileStateVars, fileWriterVars,
                 capturedVars, liveVars, journalVars, resources, blockedVars,
                 resultVars, writesVars, authorityAtWriteVars, preimageAtWriteVars, obsKeyVars,
                 obsDigestVars, obsWriterVars, confirmKeyVars, confirmDigestVars, confirmWriterVars,
                 everUnknownVars>>

Quiesce(k) ==
  /\ k \in Keys /\ (IF k = "k0" THEN phase_k0 ELSE phase_k1) \in {"confirmed", "refused", "partial"}
  /\ k = "k0" /\ step \in {6, 8, 9, 10, 15}
  /\ (IF k = "k0" THEN live_k0 ELSE live_k1) /\ resources = 1
  /\ (IF k = "k0" THEN result_k0_f0 ELSE result_k1_f0) \in {"confirmed", "refused"}
  /\ (IF k = "k0" THEN result_k0_f1 ELSE result_k1_f1) \in {"confirmed", "refused"}
  /\ live_k0' = FALSE /\ live_k1' = live_k1
  /\ resources' = resources - 1
  /\ step' = step + 1
  /\ UNCHANGED phaseVars
  /\ UNCHANGED intentVars
  /\ UNCHANGED reqPrepVars
  /\ UNCHANGED reqClientVars
  /\ UNCHANGED reqEpochVars
  /\ UNCHANGED baseVars
  /\ UNCHANGED baseWriterVars
  /\ UNCHANGED afterVars
  /\ UNCHANGED prepLiveVars
  /\ UNCHANGED grantVars
  /\ UNCHANGED cancelledVars
  /\ UNCHANGED epochVars
  /\ UNCHANGED fileStateVars
  /\ UNCHANGED fileWriterVars
  /\ UNCHANGED capturedVars
  /\ UNCHANGED journalVars
  /\ UNCHANGED blockedVars
  /\ UNCHANGED resultVars
  /\ UNCHANGED writesVars
  /\ UNCHANGED authorityAtWriteVars
  /\ UNCHANGED preimageAtWriteVars
  /\ UNCHANGED obsKeyVars
  /\ UNCHANGED obsDigestVars
  /\ UNCHANGED obsWriterVars
  /\ UNCHANGED confirmKeyVars
  /\ UNCHANGED confirmDigestVars
  /\ UNCHANGED confirmWriterVars
  /\ UNCHANGED everUnknownVars

Next ==
  \/ \E k \in Keys, p \in Preps, c \in Clients : Dispatch(k, p, c)
  \/ \E k \in Keys, p \in Preps, d \in Intents : Replay(k, p, d)
  \/ \E k \in Keys, d \in Intents : Conflict(k, d)
  \/ \E k \in Keys, c \in Clients : HistoricalLookup(k, c)
  \/ \E k \in Keys, p \in Preps : RejectEvicted(k, p)
  \/ \E p \in Preps : Evict(p)
  \/ \E f \in Files : Capture(f)
  \/ \E c \in Clients : Revoke(c) \/ Cancel(c) \/ RotateAlias(c)
  \/ \E f \in Files : Drift(f)
  \/ \E k \in Keys, f \in Files : RivalWrite(k, f)
  \/ \E k \in Keys : Claim(k) \/ Exhaust(k) \/ BlockOwner(k) \/ UnblockOwner(k)
  \/ \E k \in Keys, f \in Files : Write(k, f) \/ Refuse(k, f) \/ Fsync(k, f)
  \/ \E k \in Keys, f \in Files : PowerLoss(k, f)
  \/ \E k \in Keys : RefuseReady(k) \/ Timeout(k) \/ InterruptBeforeClaim(k)
  \/ \E k \in Keys : InterruptActive(k) \/ Settle(k) \/ Quiesce(k)
  \/ \E k \in Keys, f \in Files : Observe(k, f) \/ CompetingObserve(k, f, "k0")
       \/ CompetingObserve(k, f, "k1") \/ Confirm(k, f) \/ ResolveNoEffect(k, f)

Spec == Init /\ [][Next]_vars

TypeOK ==
  /\ phase_k0 \in Phases /\ phase_k1 \in Phases
  /\ intent_k0 \in 0..3 /\ intent_k1 \in 0..3
  /\ reqPrep_k0 \in Preps \cup {NoPrep} /\ reqPrep_k1 \in Preps \cup {NoPrep}
  /\ reqClient_k0 \in Clients \cup {NoKey}
  /\ reqClient_k1 \in Clients \cup {NoKey}
  /\ reqEpoch_k0 \in 0..1 /\ reqEpoch_k1 \in 0..1
  /\ live_k0 \in BOOLEAN /\ live_k1 \in BOOLEAN
  /\ journal_k0 \in BOOLEAN /\ journal_k1 \in BOOLEAN
  /\ blocked_k0 \in BOOLEAN /\ blocked_k1 \in BOOLEAN
  /\ everUnknown_k0 \in BOOLEAN /\ everUnknown_k1 \in BOOLEAN
  /\ prepLive_p0 \in BOOLEAN /\ prepLive_p1 \in BOOLEAN
  /\ grant_c0 \in BOOLEAN /\ grant_c1 \in BOOLEAN
  /\ cancelled_c0 \in BOOLEAN /\ cancelled_c1 \in BOOLEAN
  /\ epoch_c0 \in 0..1 /\ epoch_c1 \in 0..1
  /\ fileState_f0 \in Digests /\ fileState_f1 \in Digests
  /\ fileWriter_f0 \in Tokens /\ fileWriter_f1 \in Tokens
  /\ captured_f0 \in BOOLEAN /\ captured_f1 \in BOOLEAN
  /\ base_k0_f0 \in Digests /\ base_k0_f1 \in Digests
  /\ base_k1_f0 \in Digests /\ base_k1_f1 \in Digests
  /\ baseWriter_k0_f0 \in Tokens /\ baseWriter_k0_f1 \in Tokens
  /\ baseWriter_k1_f0 \in Tokens /\ baseWriter_k1_f1 \in Tokens
  /\ after_k0_f0 \in Digests /\ after_k0_f1 \in Digests
  /\ after_k1_f0 \in Digests /\ after_k1_f1 \in Digests
  /\ result_k0_f0 \in Results /\ result_k0_f1 \in Results
  /\ result_k1_f0 \in Results /\ result_k1_f1 \in Results
  /\ writes_k0_f0 \in 0..1 /\ writes_k0_f1 \in 0..1
  /\ writes_k1_f0 \in 0..1 /\ writes_k1_f1 \in 0..1
  /\ authorityAtWrite_k0_f0 \in BOOLEAN
  /\ authorityAtWrite_k0_f1 \in BOOLEAN
  /\ authorityAtWrite_k1_f0 \in BOOLEAN
  /\ authorityAtWrite_k1_f1 \in BOOLEAN
  /\ preimageAtWrite_k0_f0 \in Digests
  /\ preimageAtWrite_k0_f1 \in Digests
  /\ preimageAtWrite_k1_f0 \in Digests
  /\ preimageAtWrite_k1_f1 \in Digests
  /\ obsKey_k0_f0 \in Keys \cup {NoKey}
  /\ obsKey_k0_f1 \in Keys \cup {NoKey}
  /\ obsKey_k1_f0 \in Keys \cup {NoKey}
  /\ obsKey_k1_f1 \in Keys \cup {NoKey}
  /\ obsDigest_k0_f0 \in Digests /\ obsDigest_k0_f1 \in Digests
  /\ obsDigest_k1_f0 \in Digests /\ obsDigest_k1_f1 \in Digests
  /\ obsWriter_k0_f0 \in Tokens /\ obsWriter_k0_f1 \in Tokens
  /\ obsWriter_k1_f0 \in Tokens /\ obsWriter_k1_f1 \in Tokens
  /\ confirmKey_k0_f0 \in Keys \cup {NoKey}
  /\ confirmKey_k0_f1 \in Keys \cup {NoKey}
  /\ confirmKey_k1_f0 \in Keys \cup {NoKey}
  /\ confirmKey_k1_f1 \in Keys \cup {NoKey}
  /\ confirmDigest_k0_f0 \in Digests
  /\ confirmDigest_k0_f1 \in Digests
  /\ confirmDigest_k1_f0 \in Digests
  /\ confirmDigest_k1_f1 \in Digests
  /\ confirmWriter_k0_f0 \in Tokens
  /\ confirmWriter_k0_f1 \in Tokens
  /\ confirmWriter_k1_f0 \in Tokens
  /\ confirmWriter_k1_f1 \in Tokens
  /\ resources \in 0..1
  /\ step \in 0..16

AtMostOnce ==
  /\ writes_k0_f0 <= 1 /\ writes_k0_f1 <= 1
  /\ writes_k1_f0 <= 1 /\ writes_k1_f1 <= 1
UniqueLiveClaims == ~(live_k0 /\ live_k1)
ResourceCoherence ==
  resources = (IF live_k0 THEN 1 ELSE 0) + (IF live_k1 THEN 1 ELSE 0)
JournalBeforeIO ==
  /\ (writes_k0_f0 = 1 \/
      result_k0_f0 \in {"renamed", "durable", "unknown", "confirmed"})
       => journal_k0
  /\ (writes_k0_f1 = 1 \/
      result_k0_f1 \in {"renamed", "durable", "unknown", "confirmed"})
       => journal_k0
  /\ (writes_k1_f0 = 1 \/
      result_k1_f0 \in {"renamed", "durable", "unknown", "confirmed"})
       => journal_k1
  /\ (writes_k1_f1 = 1 \/
      result_k1_f1 \in {"renamed", "durable", "unknown", "confirmed"})
       => journal_k1
ActiveClaimRetention ==
  /\ phase_k0 \in {"claimed", "unknown"} => live_k0 /\ journal_k0
  /\ phase_k1 \in {"claimed", "unknown"} => live_k1 /\ journal_k1
NoBlindRedispatch ==
  /\ everUnknown_k0 => phase_k0 # "ready"
  /\ everUnknown_k1 => phase_k1 # "ready"
AuthorizedWrite ==
  /\ writes_k0_f0 = 1 =>
       authorityAtWrite_k0_f0 /\ preimageAtWrite_k0_f0 = base_k0_f0
  /\ writes_k0_f1 = 1 =>
       authorityAtWrite_k0_f1 /\ preimageAtWrite_k0_f1 = base_k0_f1
  /\ writes_k1_f0 = 1 =>
       authorityAtWrite_k1_f0 /\ preimageAtWrite_k1_f0 = base_k1_f0
  /\ writes_k1_f1 = 1 =>
       authorityAtWrite_k1_f1 /\ preimageAtWrite_k1_f1 = base_k1_f1

ConfirmationProvenance ==
  /\ phase_k0 = "confirmed" =>
       result_k0_f0 = "confirmed" /\ result_k0_f1 = "confirmed"
  /\ phase_k1 = "confirmed" =>
       result_k1_f0 = "confirmed" /\ result_k1_f1 = "confirmed"
  /\ result_k0_f0 = "confirmed" =>
       /\ writes_k0_f0 = 1 /\ journal_k0
       /\ confirmKey_k0_f0 = "k0" /\ confirmWriter_k0_f0 = "k0"
       /\ confirmDigest_k0_f0 = after_k0_f0
  /\ result_k0_f1 = "confirmed" =>
       /\ writes_k0_f1 = 1 /\ journal_k0
       /\ confirmKey_k0_f1 = "k0" /\ confirmWriter_k0_f1 = "k0"
       /\ confirmDigest_k0_f1 = after_k0_f1
  /\ result_k1_f0 = "confirmed" =>
       /\ writes_k1_f0 = 1 /\ journal_k1
       /\ confirmKey_k1_f0 = "k1" /\ confirmWriter_k1_f0 = "k1"
       /\ confirmDigest_k1_f0 = after_k1_f0
  /\ result_k1_f1 = "confirmed" =>
       /\ writes_k1_f1 = 1 /\ journal_k1
       /\ confirmKey_k1_f1 = "k1" /\ confirmWriter_k1_f1 = "k1"
       /\ confirmDigest_k1_f1 = after_k1_f1

MetadataCoherence ==
  /\ phase_k0 = "idle" =>
       /\ intent_k0 = 0 /\ reqPrep_k0 = NoPrep /\ reqClient_k0 = NoKey
       /\ ~live_k0 /\ ~journal_k0 /\ ~everUnknown_k0
       /\ result_k0_f0 = "pending" /\ writes_k0_f0 = 0
       /\ result_k0_f1 = "pending" /\ writes_k0_f1 = 0
  /\ phase_k0 # "idle" =>
       /\ reqPrep_k0 \in Preps /\ reqClient_k0 \in Clients
       /\ PrepKey(reqPrep_k0) = "k0" /\ PrepClient(reqPrep_k0) = reqClient_k0
       /\ intent_k0 = PrepIntent(reqPrep_k0)
       /\ base_k0_f0 = PrepBase(reqPrep_k0, "f0")
       /\ base_k0_f1 = PrepBase(reqPrep_k0, "f1")
       /\ after_k0_f0 = PrepAfter(reqPrep_k0, "f0")
       /\ after_k0_f1 = PrepAfter(reqPrep_k0, "f1")
  /\ phase_k1 = "idle" =>
       /\ intent_k1 = 0 /\ reqPrep_k1 = NoPrep /\ reqClient_k1 = NoKey
       /\ ~live_k1 /\ ~journal_k1 /\ ~everUnknown_k1
       /\ result_k1_f0 = "pending" /\ writes_k1_f0 = 0
       /\ result_k1_f1 = "pending" /\ writes_k1_f1 = 0
  /\ phase_k1 # "idle" =>
       /\ reqPrep_k1 \in Preps /\ reqClient_k1 \in Clients
       /\ PrepKey(reqPrep_k1) = "k1" /\ PrepClient(reqPrep_k1) = reqClient_k1
       /\ intent_k1 = PrepIntent(reqPrep_k1)
       /\ base_k1_f0 = PrepBase(reqPrep_k1, "f0")
       /\ base_k1_f1 = PrepBase(reqPrep_k1, "f1")
       /\ after_k1_f0 = PrepAfter(reqPrep_k1, "f0")
       /\ after_k1_f1 = PrepAfter(reqPrep_k1, "f1")

PhasePredecessorCoherence ==
  /\ phase_k0 = "ready" =>
       /\ ~live_k0 /\ ~journal_k0
       /\ result_k0_f0 = "pending" /\ writes_k0_f0 = 0
       /\ result_k0_f1 = "pending" /\ writes_k0_f1 = 0
  /\ phase_k0 = "refused" =>
       result_k0_f0 = "refused" /\ result_k0_f1 = "refused"
  /\ phase_k0 = "partial" =>
       /\ (result_k0_f0 = "confirmed" \/ result_k0_f1 = "confirmed")
       /\ (result_k0_f0 = "refused" \/ result_k0_f1 = "refused")
  /\ result_k0_f0 \in {"pending", "refused"} => writes_k0_f0 = 0
  /\ result_k0_f0 \in {"renamed", "durable", "confirmed"} => writes_k0_f0 = 1
  /\ result_k0_f1 \in {"pending", "refused"} => writes_k0_f1 = 0
  /\ result_k0_f1 \in {"renamed", "durable", "confirmed"} => writes_k0_f1 = 1
  /\ phase_k1 = "ready" =>
       /\ ~live_k1 /\ ~journal_k1
       /\ result_k1_f0 = "pending" /\ writes_k1_f0 = 0
       /\ result_k1_f1 = "pending" /\ writes_k1_f1 = 0
  /\ phase_k1 = "refused" =>
       result_k1_f0 = "refused" /\ result_k1_f1 = "refused"
  /\ phase_k1 = "partial" =>
       /\ (result_k1_f0 = "confirmed" \/ result_k1_f1 = "confirmed")
       /\ (result_k1_f0 = "refused" \/ result_k1_f1 = "refused")
  /\ result_k1_f0 \in {"pending", "refused"} => writes_k1_f0 = 0
  /\ result_k1_f0 \in {"renamed", "durable", "confirmed"} => writes_k1_f0 = 1
  /\ result_k1_f1 \in {"pending", "refused"} => writes_k1_f1 = 0
  /\ result_k1_f1 \in {"renamed", "durable", "confirmed"} => writes_k1_f1 = 1

Safety ==
  /\ AtMostOnce /\ UniqueLiveClaims /\ ResourceCoherence /\ JournalBeforeIO
  /\ ActiveClaimRetention /\ NoBlindRedispatch /\ AuthorizedWrite
  /\ ConfirmationProvenance /\ PhasePredecessorCoherence

StrengthenedInv == TypeOK /\ Safety /\ MetadataCoherence

=============================================================================
