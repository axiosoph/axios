--------------------------- MODULE EosScheduling ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS 
    EntryPoints,       \* Set of all entry points
    DependencyEdges,   \* Set of dependency edges: <<s1, s2>> means s2 depends on s1
    Workers,           \* Set of all workers
    WorkerCap,         \* Map of worker to its maximum resource capacity (Nat)
    PredictedLoad,     \* Map of entry point to its predicted resource load (Nat)
    Outputs            \* Map of entry point to its produced artifact set

-----------------------------------------------------------------------------

\* Bounded sequences of length at most N+1 representing paths
BoundedSeq == UNION { [1..n -> EntryPoints] : n \in 2..(Cardinality(EntryPoints) + 1) }

\* A relation is acyclic if there are no cycles of length >= 1
IsAcyclic ==
    ~ \exists seq \in BoundedSeq :
        /\ Len(seq) >= 2
        /\ seq[1] = seq[Len(seq)]
        /\ \A i \in 1..(Len(seq)-1) : <<seq[i], seq[i+1]>> \in DependencyEdges

\* Formal preconditions and axioms of the scheduling domain
VerifyAxioms ==
    /\ IsFiniteSet(EntryPoints)
    /\ IsFiniteSet(Workers)
    /\ DependencyEdges \subseteq (EntryPoints \times EntryPoints)
    /\ IsAcyclic
    /\ WorkerCap \in [Workers -> Nat]
    /\ PredictedLoad \in [EntryPoints -> Nat]
    /\ \A s \in EntryPoints : IsFiniteSet(Outputs[s])
    /\ \A s \in EntryPoints : \exists w \in Workers : PredictedLoad[s] <= WorkerCap[w]

ASSUME VerifyAxioms

VARIABLES
    epStatus,          \* Map of entry point to status
    workerLoad,        \* Map of worker to current load
    artifactStore,     \* Set of completed entry points' outputs
    runningOn          \* Map of entry point to worker it is running on

vars == <<epStatus, workerLoad, artifactStore, runningOn>>

-----------------------------------------------------------------------------

\* Initial state
Init ==
    /\ epStatus = [s \in EntryPoints |-> 
         IF \A e \in DependencyEdges : e[2] /= s
         THEN "ready"
         ELSE "pending"]
    /\ workerLoad = [w \in Workers |-> 0]
    /\ artifactStore = {}
    /\ runningOn = [s \in EntryPoints |-> "none"]

-----------------------------------------------------------------------------

\* State transitions (Actions)

\* Dispatch an entry point s to worker w
Dispatch(s, w) ==
    /\ epStatus[s] = "ready"
    /\ workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]
    /\ epStatus' = [epStatus EXCEPT ![s] = "dispatched"]
    /\ workerLoad' = [workerLoad EXCEPT ![w] = @ + PredictedLoad[s]]
    /\ runningOn' = [runningOn EXCEPT ![s] = w]
    /\ UNCHANGED artifactStore

\* Complete the execution of entry point s
Complete(s) ==
    /\ epStatus[s] = "dispatched"
    /\ LET w == runningOn[s] IN
         /\ epStatus' = [s_new \in EntryPoints |->
              IF s_new = s THEN "complete"
              ELSE IF epStatus[s_new] = "pending" /\
                      (\A e \in DependencyEdges : e[2] = s_new => 
                         (e[1] = s \/ epStatus[e[1]] = "complete"))
                   THEN "ready"
                   ELSE epStatus[s_new]]
         /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
         /\ artifactStore' = artifactStore \cup Outputs[s]
         /\ runningOn' = [runningOn EXCEPT ![s] = "none"]

\* Fail the execution of entry point s
Fail(s) ==
    /\ epStatus[s] = "dispatched"
    /\ LET w == runningOn[s] IN
         /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
         /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
         /\ runningOn' = [runningOn EXCEPT ![s] = "none"]
         /\ UNCHANGED artifactStore

\* Propagate failures downstream
CascadeFail(s) ==
    /\ epStatus[s] \in {"pending", "ready"}
    /\ \exists e \in DependencyEdges : e[2] = s /\ epStatus[e[1]] = "failed"
    /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
    /\ UNCHANGED <<workerLoad, artifactStore, runningOn>>

-----------------------------------------------------------------------------

\* Next state relation
Next ==
    \/ \exists s \in EntryPoints, w \in Workers : Dispatch(s, w)
    \/ \exists s \in EntryPoints : Complete(s)
    \/ \exists s \in EntryPoints : Fail(s)
    \/ \exists s \in EntryPoints : CascadeFail(s)

\* Temporal formula (system specification)
Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------

\* Invariants (Safety)

\* Type-correctness of state variables
TypeOK ==
    /\ epStatus \in [EntryPoints -> {"pending", "ready", "dispatched", "complete", "failed"}]
    /\ workerLoad \in [Workers -> Nat]
    /\ artifactStore \subseteq UNION {Outputs[s] : s \in EntryPoints}
    /\ runningOn \in [EntryPoints -> Workers \cup {"none"}]

\* Ordering soundness: dependencies must be complete before a task can start or finish
OrderingSoundness ==
    \A e \in DependencyEdges :
        epStatus[e[2]] \in {"dispatched", "complete"} => epStatus[e[1]] = "complete"

\* Capacity safety: worker load cannot exceed capacity
CapacitySafety ==
    \A w \in Workers : workerLoad[w] <= WorkerCap[w]

\* Artifact safety: if all entry points completed successfully, the artifact store contains all outputs
ArtifactSafety ==
    (\A s \in EntryPoints : epStatus[s] = "complete")
        => artifactStore = UNION {Outputs[s] : s \in EntryPoints}

-----------------------------------------------------------------------------

\* Liveness Properties

\* Every entry point eventually reaches a terminal state (complete or failed)
CompletionPropagation ==
    <> (\A s \in EntryPoints : epStatus[s] \in {"complete", "failed"})

\* If an entry point is ready and there is capacity, it must eventually be dispatched
Progress ==
    \A s \in EntryPoints :
        epStatus[s] = "ready" /\ (\exists w \in Workers : workerLoad[w] + PredictedLoad[s] <= WorkerCap[w])
            => <> (epStatus[s] /= "ready")

=============================================================================
