----------------------------- MODULE DiamondModel -----------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS A, B, C, D, w1, w2, ha, hb, hc, hd

DiamondEntryPoints == {A, B, C, D}
DiamondEdges == {<<A, B>>, <<A, C>>, <<B, D>>, <<C, D>>}
DiamondWorkers == {w1, w2}
DiamondWorkerCap == [w \in DiamondWorkers |-> 4]
DiamondPredictedLoad == [s \in DiamondEntryPoints |-> 2]
DiamondOutputs == [s \in DiamondEntryPoints |-> 
                    IF s = A THEN {ha}
                    ELSE IF s = B THEN {hb}
                    ELSE IF s = C THEN {hc}
                    ELSE {hd}]

VARIABLES epStatus, workerLoad, artifactStore, runningOn

vars == <<epStatus, workerLoad, artifactStore, runningOn>>

M == INSTANCE EosScheduling WITH
    EntryPoints <- DiamondEntryPoints,
    DependencyEdges <- DiamondEdges,
    Workers <- DiamondWorkers,
    WorkerCap <- DiamondWorkerCap,
    PredictedLoad <- DiamondPredictedLoad,
    Outputs <- DiamondOutputs

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
