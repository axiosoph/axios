--------------------------- MODULE IndependentModel ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS A, B, C, w1, w2, ha, hb, hc

IndependentEntryPoints == {A, B, C}
IndependentEdges == {}
IndependentWorkers == {w1, w2}
IndependentWorkerCap == [w \in IndependentWorkers |-> 4]
IndependentPredictedLoad == [s \in IndependentEntryPoints |-> 2]
IndependentOutputs == [s \in IndependentEntryPoints |-> 
                        IF s = A THEN {ha}
                        ELSE IF s = B THEN {hb}
                        ELSE {hc}]

VARIABLES epStatus, workerLoad, artifactStore, runningOn

vars == <<epStatus, workerLoad, artifactStore, runningOn>>

M == INSTANCE EosScheduling WITH
    EntryPoints <- IndependentEntryPoints,
    DependencyEdges <- IndependentEdges,
    Workers <- IndependentWorkers,
    WorkerCap <- IndependentWorkerCap,
    PredictedLoad <- IndependentPredictedLoad,
    Outputs <- IndependentOutputs

ASSUME M!VerifyAxioms

Init == M!Init
Next == M!Next
Spec == M!Spec
FairSpec == Init /\ [][Next]_vars /\ WF_vars(Next)

TypeOK == M!TypeOK
OrderingSoundness == M!OrderingSoundness
CapacitySafety == M!CapacitySafety
ArtifactSafety == M!ArtifactSafety
CompletionPropagation == M!CompletionPropagation
Progress == M!Progress
HoLFreedom == M!HoLFreedom
NoWedge == M!NoWedge

=============================================================================
