----------------------------- MODULE LinearModel -----------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS A, B, C, w1, w2, ha, hb, hc

LinearEntryPoints == {A, B, C}
LinearEdges == {<<A, B>>, <<B, C>>}
LinearWorkers == {w1, w2}
LinearWorkerCap == [w \in LinearWorkers |-> 4]
LinearPredictedLoad == [s \in LinearEntryPoints |-> 2]
LinearOutputs == [s \in LinearEntryPoints |-> 
                   IF s = A THEN {ha}
                   ELSE IF s = B THEN {hb}
                   ELSE {hc}]

VARIABLES epStatus, workerLoad, artifactStore, runningOn

vars == <<epStatus, workerLoad, artifactStore, runningOn>>

M == INSTANCE EosScheduling WITH
    EntryPoints <- LinearEntryPoints,
    DependencyEdges <- LinearEdges,
    Workers <- LinearWorkers,
    WorkerCap <- LinearWorkerCap,
    PredictedLoad <- LinearPredictedLoad,
    Outputs <- LinearOutputs

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
