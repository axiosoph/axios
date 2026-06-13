--------------------------- MODULE ConvergenceModel ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS A, B, C, w1, w2, ha, hb, hc

ConvergenceEntryPoints == {A, B, C}
ConvergenceEdges == {<<A, C>>, <<B, C>>}
ConvergenceWorkers == {w1, w2}
ConvergenceWorkerCap == [w \in ConvergenceWorkers |-> 4]
ConvergencePredictedLoad == [s \in ConvergenceEntryPoints |-> 2]
ConvergenceOutputs == [s \in ConvergenceEntryPoints |-> 
                        IF s = A THEN {ha}
                        ELSE IF s = B THEN {hb}
                        ELSE {hc}]

VARIABLES epStatus, workerLoad, artifactStore, runningOn

vars == <<epStatus, workerLoad, artifactStore, runningOn>>

M == INSTANCE EosScheduling WITH
    EntryPoints <- ConvergenceEntryPoints,
    DependencyEdges <- ConvergenceEdges,
    Workers <- ConvergenceWorkers,
    WorkerCap <- ConvergenceWorkerCap,
    PredictedLoad <- ConvergencePredictedLoad,
    Outputs <- ConvergenceOutputs

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

=============================================================================
