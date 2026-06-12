--------------------------- MODULE MultiRequestModel ---------------------------
EXTENDS Naturals, Sequences, FiniteSets

\* Local Definitions representing constants
AllPossibleEntryPoints == {"A", "B", "C", "D"}
AllPossibleRequestIds == {"r1", "r2"}
Workers == {"w1", "w2"}
WorkerCap == [w \in Workers |-> 4]
PredictedLoad == [s \in AllPossibleEntryPoints |-> 2]
Outputs == [s \in AllPossibleEntryPoints |-> {s}]

CONSTANTS
    Delta,     \* Bounded dispatch window in ticks (confidence-gated; Delta=0 is strict immediacy)
    MaxTick    \* Model-checking bound on the clock (keeps the state space finite)

VARIABLES
    epStatus,          \* Map of entry point to status
    workerLoad,        \* Map of worker to current load
    artifactStore,     \* Set of completed outputs
    runningOn,         \* Map of entry point to worker
    EntryPoints,       \* Set of active entry points (dynamic)
    DependencyEdges,   \* Set of active edges (dynamic)
    requestClients,    \* Map of entry point to set of request IDs
    requestArrived,    \* Set of request IDs that have arrived
    failureReason,     \* Map: EP -> {"none", "deterministic", "cascade"}
    clock,             \* Logical tick counter (advanced by Tick)
    readySince         \* Map: EP -> tick at which it last entered "ready"

vars == <<epStatus, workerLoad, artifactStore, runningOn, EntryPoints, DependencyEdges, requestClients, requestArrived, failureReason, clock, readySince>>

-----------------------------------------------------------------------------

\* Helper to check acyclicity of DependencyEdges
BoundedSeq == UNION { [1..n -> AllPossibleEntryPoints] : n \in 2..(Cardinality(AllPossibleEntryPoints) + 1) }

IsAcyclic(edges) ==
    ~ \exists seq \in BoundedSeq :
        /\ Len(seq) >= 2
        /\ seq[1] = seq[Len(seq)]
        /\ \A i \in 1..(Len(seq)-1) : <<seq[i], seq[i+1]>> \in edges

VerifyAxioms ==
    /\ IsFiniteSet(AllPossibleEntryPoints)
    /\ AllPossibleEntryPoints /= {}
    /\ IsFiniteSet(AllPossibleRequestIds)
    /\ AllPossibleRequestIds /= {}
    /\ IsFiniteSet(Workers)
    /\ Workers /= {}
    /\ WorkerCap \in [Workers -> Nat]
    /\ PredictedLoad \in [AllPossibleEntryPoints -> Nat]
    /\ \A s \in AllPossibleEntryPoints : IsFiniteSet(Outputs[s])

ASSUME VerifyAxioms
ASSUME Delta \in Nat
ASSUME MaxTick \in Nat

-----------------------------------------------------------------------------

\* Initial State (Starts with a single request r1 containing a linear topology A -> B)
Init ==
    /\ EntryPoints = {"A", "B"}
    /\ DependencyEdges = {<<"A", "B">>}
    /\ requestClients = [s \in {"A", "B"} |-> {"r1"}]
    /\ epStatus = [s \in {"A", "B"} |-> IF s = "A" THEN "ready" ELSE "pending"]
    /\ workerLoad = [w \in Workers |-> 0]
    /\ artifactStore = {}
    /\ runningOn = [s \in {"A", "B"} |-> "none"]
    /\ requestArrived = {"r1"}
    /\ failureReason = [s \in {"A", "B"} |-> "none"]
    /\ clock = 0
    /\ readySince = [s \in {"A", "B"} |-> 0]

-----------------------------------------------------------------------------

\* State transitions (Actions)

\* Dispatch an entry point s to worker w
Dispatch(s, w) ==
    /\ s \in EntryPoints
    /\ epStatus[s] = "ready"
    /\ workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]
    /\ epStatus' = [epStatus EXCEPT ![s] = "dispatched"]
    /\ workerLoad' = [workerLoad EXCEPT ![w] = @ + PredictedLoad[s]]
    /\ runningOn' = [runningOn EXCEPT ![s] = w]
    /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, artifactStore, requestArrived, failureReason, clock, readySince>>

\* Complete the execution of entry point s
Complete(s) ==
    /\ s \in EntryPoints
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
         /\ readySince' = [s_new \in EntryPoints |->
              IF epStatus[s_new] = "pending" /\
                 (\A e \in DependencyEdges : e[2] = s_new =>
                    (e[1] = s \/ epStatus[e[1]] = "complete"))
              THEN clock
              ELSE readySince[s_new]]
         /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, requestArrived, failureReason, clock>>

\* Fail deterministically (deterministic failure propagates)
FailDeterministic(s) ==
    /\ s \in EntryPoints
    /\ epStatus[s] = "dispatched"
    /\ LET w == runningOn[s] IN
         /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
         /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
         /\ runningOn' = [runningOn EXCEPT ![s] = "none"]
         /\ failureReason' = [failureReason EXCEPT ![s] = "deterministic"]
         /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, artifactStore, requestArrived, clock, readySince>>

\* Transient failure: infrastructure/worker crash, not build failure.
\* The EP is unfrozen back to "ready" for re-dispatch to a healthy worker.
\* This does NOT trigger CascadeFail (transient failures are isolated).
FailTransient(s) ==
    /\ s \in EntryPoints
    /\ epStatus[s] = "dispatched"
    /\ LET w == runningOn[s] IN
         /\ epStatus' = [epStatus EXCEPT ![s] = "ready"]
         /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
         /\ runningOn' = [runningOn EXCEPT ![s] = "none"]
         /\ readySince' = [readySince EXCEPT ![s] = clock]
         /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, artifactStore, requestArrived, failureReason, clock>>

\* Propagate failures downstream
CascadeFail(s) ==
    /\ s \in EntryPoints
    /\ epStatus[s] \in {"pending", "ready"}
    /\ \exists e \in DependencyEdges : e[2] = s /\ epStatus[e[1]] = "failed"
    /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
    /\ failureReason' = [failureReason EXCEPT ![s] = "cascade"]
    /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, workerLoad, artifactStore, runningOn, requestArrived, clock, readySince>>

\* Merge a new request r2 containing topology B -> C (shared B dependency)
MergeRequest ==
    /\ ~ ("r2" \in requestArrived) \* Request r2 hasn't arrived yet
    /\ LET new_eps == {"B", "C"}
           new_edges == {<<"B", "C">>}
       IN
         /\ EntryPoints' = EntryPoints \cup new_eps
         /\ DependencyEdges' = DependencyEdges \cup new_edges
         /\ requestClients' = [s \in EntryPoints' |->
                IF s \in EntryPoints THEN requestClients[s] \cup {"r2"}
                ELSE {"r2"}]
         /\ epStatus' = [s \in EntryPoints' |->
                IF s \in EntryPoints THEN epStatus[s]
                ELSE IF Outputs[s] \subseteq artifactStore THEN "complete"
                ELSE IF \A e \in new_edges : e[2] = s => (e[1] \in EntryPoints /\ epStatus[e[1]] = "complete")
                THEN "ready"
                ELSE "pending"]
         /\ runningOn' = [s \in EntryPoints' |->
                IF s \in EntryPoints THEN runningOn[s]
                ELSE "none"]
         /\ requestArrived' = requestArrived \cup {"r2"}
         /\ failureReason' = [s \in EntryPoints' |->
                IF s \in EntryPoints THEN failureReason[s]
                ELSE "none"]
         /\ readySince' = [s \in EntryPoints' |->
                IF s \in EntryPoints THEN readySince[s]
                ELSE clock]
         /\ UNCHANGED <<workerLoad, artifactStore, clock>>

\* Cache-skip scan for pending/ready EPs whose outputs are already present in store
CacheSkip(s) ==
    /\ s \in EntryPoints
    /\ epStatus[s] \in {"pending", "ready"}
    /\ Outputs[s] \subseteq artifactStore
    /\ epStatus' = [epStatus EXCEPT ![s] = "complete"]
    /\ UNCHANGED <<EntryPoints, DependencyEdges, requestClients, workerLoad, artifactStore, runningOn, requestArrived, failureReason, clock, readySince>>

\* Cancel a request and prune its mutable EPs
CancelRequest(req_id) ==
    /\ \exists s \in EntryPoints : req_id \in requestClients[s]
    /\ LET new_requestClients == [s \in EntryPoints |-> requestClients[s] \ {req_id}]
           to_prune == {s \in EntryPoints : new_requestClients[s] = {} /\ epStatus[s] \in {"pending", "ready"}}
       IN
         /\ to_prune /= {}
         /\ EntryPoints' = EntryPoints \ to_prune
         /\ DependencyEdges' = {e \in DependencyEdges : e[1] \notin to_prune /\ e[2] \notin to_prune}
         /\ requestClients' = [s \in EntryPoints' |-> new_requestClients[s]]
         /\ epStatus' = [s \in EntryPoints' |-> epStatus[s]]
         /\ runningOn' = [s \in EntryPoints' |-> runningOn[s]]
         /\ failureReason' = [s \in EntryPoints' |-> failureReason[s]]
         /\ readySince' = [s \in EntryPoints' |-> readySince[s]]
         /\ UNCHANGED <<workerLoad, artifactStore, requestArrived, clock>>

\* An EP that is ready and has at least one worker with spare capacity:
\* a candidate the scheduler could dispatch immediately.
ReadyFeasible(s) ==
    /\ s \in EntryPoints
    /\ epStatus[s] = "ready"
    /\ \exists w \in Workers : workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]

\* Advance the logical clock by one tick. Time may pass only while some
\* ready+feasible EP is still inside its dispatch window, and a tick may
\* never push such an EP past its deadline (clock < readySince + Delta).
\* This makes Delta a hard upper bound on dispatch latency: at the deadline
\* Tick is disabled, so the only way to make progress is to dispatch (or
\* otherwise resolve) the waiting EP. With Delta = 0 no tick is ever enabled
\* while work is ready+feasible, collapsing the window to strict immediacy.
Tick ==
    /\ \exists s \in EntryPoints : ReadyFeasible(s)
    /\ \A s \in EntryPoints : ReadyFeasible(s) => clock < readySince[s] + Delta
    /\ clock' = clock + 1
    /\ UNCHANGED <<epStatus, workerLoad, artifactStore, runningOn, EntryPoints,
                   DependencyEdges, requestClients, requestArrived, failureReason, readySince>>

-----------------------------------------------------------------------------

\* Next State
Next ==
    \/ \exists s \in EntryPoints, w \in Workers : Dispatch(s, w)
    \/ \exists s \in EntryPoints : Complete(s)
    \/ \exists s \in EntryPoints : FailDeterministic(s)
    \/ \exists s \in EntryPoints : FailTransient(s)
    \/ \exists s \in EntryPoints : CascadeFail(s)
    \/ \exists s \in EntryPoints : CacheSkip(s)
    \/ MergeRequest
    \/ \exists req_id \in AllPossibleRequestIds : CancelRequest(req_id)
    \/ Tick

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* State constraint bounding the clock for finite model checking. Dispatch
\* never consumes a tick, so a ready+feasible EP can always still be dispatched
\* at clock = MaxTick; bounding the clock therefore prunes only the unbounded
\* transient-failure tail and never masks or fabricates a P9' violation.
ClockBound == clock <= MaxTick

-----------------------------------------------------------------------------

\* Invariants (Safety)

TypeOK ==
    /\ EntryPoints \subseteq AllPossibleEntryPoints
    /\ DependencyEdges \subseteq (EntryPoints \times EntryPoints)
    /\ epStatus \in [EntryPoints -> {"pending", "ready", "dispatched", "complete", "failed"}]
    /\ workerLoad \in [Workers -> Nat]
    /\ artifactStore \subseteq UNION {Outputs[s] : s \in AllPossibleEntryPoints}
    /\ runningOn \in [EntryPoints -> Workers \cup {"none"}]
    /\ requestClients \in [EntryPoints -> SUBSET AllPossibleRequestIds]
    /\ failureReason \in [EntryPoints -> {"none", "deterministic", "cascade"}]

OrderingSoundness ==
    \A e \in DependencyEdges :
        epStatus[e[2]] \in {"dispatched", "complete"} => epStatus[e[1]] = "complete"

CapacitySafety ==
    \A w \in Workers : workerLoad[w] <= WorkerCap[w]

ArtifactSafety ==
    \A s \in EntryPoints :
        epStatus[s] = "complete" => Outputs[s] \subseteq artifactStore

\* P11: Failure Isolation
\* An EP can only be "failed" if:
\*   (a) it failed deterministically (was dispatched, build failed), or
\*   (b) it failed via cascade (a dependency is failed).
\* This prevents spurious failure propagation to unrelated EPs.
FailureIsolation ==
    \A s \in EntryPoints :
        epStatus[s] = "failed" =>
            \/ failureReason[s] = "deterministic"
            \/ (failureReason[s] = "cascade"
                /\ \exists e \in DependencyEdges :
                      e[2] = s /\ epStatus[e[1]] = "failed")

\* Safe helper operators to prevent out-of-domain function application errors
epStatusSafe(s) == IF s \in EntryPoints THEN epStatus[s] ELSE "none"
runningOnSafe(s) == IF s \in EntryPoints THEN runningOn[s] ELSE "none"
requestClientsSafe(s) == IF s \in EntryPoints THEN requestClients[s] ELSE {}

\* P8: Frozen Stability (refined as action property)
FrozenStability == [][
    \A s \in EntryPoints :
        \A w \in Workers :
            (epStatus[s] = "dispatched" /\ runningOn[s] = w)
            => /\ epStatus'[s] \in {"dispatched", "complete", "failed", "ready"}
               /\ (epStatus'[s] = "dispatched" => runningOn'[s] = w)
]_vars

-----------------------------------------------------------------------------

NoInfiniteTransientFailures ==
    \A s \in AllPossibleEntryPoints :
        ~ []<> <<
            /\ s \in EntryPoints
            /\ epStatus[s] = "dispatched"
            /\ epStatus'[s] = "ready"
        >>_vars

CompletionPropagation ==
    NoInfiniteTransientFailures =>
        \A req_id \in AllPossibleRequestIds :
            <> (\A s \in AllPossibleEntryPoints : (req_id \in requestClientsSafe(s)) => epStatusSafe(s) \in {"complete", "failed"})

Progress ==
    NoInfiniteTransientFailures =>
        \A s \in AllPossibleEntryPoints :
            (epStatusSafe(s) = "ready" /\ (\exists w \in Workers : workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]))
                => <> (epStatusSafe(s) /= "ready")

\* P5': Head-of-Line Immunity
\* Structural guarantee: Dispatch(s, w) depends only on epStatus[s]
\* and workerLoad[w], never on epStatus[s'] for s' /= s. Therefore
\* one request's EPs cannot block another request's dispatch.
\* This uses ~> (leads-to) for stronger guarantees than <> (eventually).
\* Intentionally redundant with Progress (P5) for publication traceability.
HoLImmunity ==
    NoInfiniteTransientFailures =>
        \A s \in AllPossibleEntryPoints :
            \A req_id \in AllPossibleRequestIds :
                (req_id \in requestClientsSafe(s)
                 /\ epStatusSafe(s) = "ready"
                 /\ (\exists w \in Workers :
                        workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]))
                ~> (epStatusSafe(s) \in {"dispatched", "complete", "failed", "none"})

\* P9: Work Conservation
\* If a ready EP exists and a worker has capacity, that EP is
\* eventually dispatched, completed, failed, or canceled (none).
WorkConservation ==
    NoInfiniteTransientFailures =>
        \A s \in AllPossibleEntryPoints :
            (epStatusSafe(s) = "ready"
             /\ (\exists w \in Workers :
                    workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]))
            ~> (epStatusSafe(s) \in {"dispatched", "complete", "failed", "none"})

\* P10: Transient Recovery (explicit)
\* A dispatched EP that experiences transient failure eventually
\* reaches a terminal state (complete, failed, or pruned).
TransientRecovery ==
    NoInfiniteTransientFailures =>
        \A s \in AllPossibleEntryPoints :
            (epStatusSafe(s) = "dispatched")
            ~> (epStatusSafe(s) \in {"complete", "failed", "none"})

=============================================================================
