# PLAN: TLA+ Specification for Eos Build Scheduling

## 1. Objective and Scope

This document defines the formal specification plan for verifying the **Eos Build Scheduling Protocol (Track A)** using TLA+. The goal is to prove that the concurrent dispatch state machine satisfies critical safety (ordering soundness, capacity safety) and liveness (completion propagation, progress under fairness) properties under all nondeterministic execution interleavings, worker assignments, and task failure scenarios.

This plan establishes the exact mapping between the mathematical models in [eos-scheduling.md](../models/eos-scheduling.md) and TLA+ language primitives, serving as the blueprint for model-checking verification.

---

## 2. Mathematical to TLA+ Mapping

### 2.1 Constants

| Mathematical Model                        | TLA+ Constant     | Type / Constraints                                                              |
| :---------------------------------------- | :---------------- | :------------------------------------------------------------------------------ |
| Entry Points set $S$                      | `EntryPoints`     | Finite set of identifiers                                                       |
| Dependency edges $E_S$                    | `DependencyEdges` | Set of ordered pairs `<<s1, s2>>` where `s1, s2 \in EntryPoints`                |
| Workers set $W$                           | `Workers`         | Finite set of identifiers                                                       |
| Worker capacity vector $\text{cap}(w)$    | `WorkerCap`       | Function: `Workers -> Nat` (modeled as single-resource load for TLC simplicity) |
| Task predicted load vector $\mathbf{r}_e$ | `PredictedLoad`   | Function: `EntryPoints -> Nat`                                                  |

### 2.2 State Variables

| Mathematical Model               | TLA+ Variable   | Type / Initial State                                                                |
| :------------------------------- | :-------------- | :---------------------------------------------------------------------------------- |
| status map $Q$                   | `epStatus`      | Function: `EntryPoints -> {"pending", "ready", "dispatched", "complete", "failed"}` |
| worker load map $L$              | `workerLoad`    | Function: `Workers -> Nat`                                                          |
| artifact store $A$               | `artifactStore` | Set of entry point identifiers representing completed builds                        |
| assignment map $\sigma$ (active) | `runningOn`     | Function: `EntryPoints -> Workers \cup {"none"}`                                    |

---

## 3. State Machine Specification

### 3.1 Initial State (`Init`)

An entry point is initially `"ready"` if it has no incoming dependency edges in the entry point DAG. Otherwise, it is `"pending"`.

```tla
Init ==
  /\ epStatus = [s \in EntryPoints |->
       IF \A e \in DependencyEdges : e[2] /= s
       THEN "ready"
       ELSE "pending"]
  /\ workerLoad = [w \in Workers |-> 0]
  /\ artifactStore = {}
  /\ runningOn = [s \in EntryPoints |-> "none"]
```

### 3.2 State Transitions (Actions)

#### Dispatch

Assigns a `"ready"` entry point $s$ to a worker $w$ that has sufficient available capacity.

```tla
Dispatch(s, w) ==
  /\ epStatus[s] = "ready"
  /\ workerLoad[w] + PredictedLoad[s] <= WorkerCap[w]
  /\ epStatus' = [epStatus EXCEPT ![s] = "dispatched"]
  /\ workerLoad' = [workerLoad EXCEPT ![w] = @ + PredictedLoad[s]]
  /\ runningOn' = [runningOn EXCEPT ![s] = w]
  /\ UNCHANGED artifactStore
```

#### Complete

Signals that entry point $s$ built successfully on its assigned worker. Its outputs are added to the `artifactStore`, worker capacity is released, and downstream dependencies are updated to `"ready"` if all their parent entry points are now `"complete"`.

```tla
Complete(s) ==
  /\ epStatus[s] = "dispatched"
  /\ LET w == runningOn[s] IN
       /\ epStatus' = [s_new \in EntryPoints |->
            IF s_new = s THEN "complete"
            ELSE IF epStatus[s_new] = "pending" /\
                    \A e \in DependencyEdges : e[2] = s_new =>
                       (e[1] = s \/ epStatus[e[1]] = "complete")
                 THEN "ready"
                 ELSE epStatus[s_new]]
       /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
       /\ artifactStore' = artifactStore \cup {s}
       /\ runningOn' = [runningOn EXCEPT ![s] = "none"]
```

#### Fail

Signals a build failure of $s$. The worker capacity is released, and the entry point status is marked `"failed"`.

```tla
Fail(s) ==
  /\ epStatus[s] = "dispatched"
  /\ LET w == runningOn[s] IN
       /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
       /\ workerLoad' = [workerLoad EXCEPT ![w] = @ - PredictedLoad[s]]
       /\ runningOn' = [runningOn EXCEPT ![s] = "none"]
       /\ UNCHANGED artifactStore
```

#### CascadeFail

Propagates failures downward through the dependency graph. If a node $s$ is `"pending"` or `"ready"`, and any of its parent dependencies has `"failed"`, $s$ recursively transitions to `"failed"`.

```tla
CascadeFail(s) ==
  /\ epStatus[s] \in {"pending", "ready"}
  /\ \exists e \in DependencyEdges : e[2] = s /\ epStatus[e[1]] = "failed"
  /\ epStatus' = [epStatus EXCEPT ![s] = "failed"]
  /\ UNCHANGED <<workerLoad, artifactStore, runningOn>>
```

---

## 4. Safety Invariants

The TLA+ model checker (TLC) will assert these invariants on every reachable state:

### 4.1 Type Safety (`TypeOK`)

Verifies that all state variables conform to their expected types.

```tla
TypeOK ==
  /\ epStatus \in [EntryPoints -> {"pending", "ready", "dispatched", "complete", "failed"}]
  /\ workerLoad \in [Workers -> Nat]
  /\ artifactStore \subseteq EntryPoints
  /\ runningOn \in [EntryPoints -> Workers \cup {"none"}]
```

### 4.2 Ordering Soundness (`OrderingSoundness`)

No entry point may be dispatched or completed unless all its dependency entry points have already completed and reside in the artifact store.

```tla
OrderingSoundness ==
  \A e \in DependencyEdges :
    epStatus[e[2]] \in {"dispatched", "complete"} => epStatus[e[1]] = "complete"
```

### 4.3 Capacity Safety (`CapacitySafety`)

No worker's load may exceed its total defined capacity.

```tla
CapacitySafety ==
  \A w \in Workers : workerLoad[w] <= WorkerCap[w]
```

---

## 5. Liveness and Temporal Properties

To model progress, we define the next-state relation:

```tla
Next ==
  \/ \exists s \in EntryPoints, w \in Workers : Dispatch(s, w)
  \/ \exists s \in EntryPoints : Complete(s)
  \/ \exists s \in EntryPoints : Fail(s)
  \/ \exists s \in EntryPoints : CascadeFail(s)

Spec == Init /\ [][Next]_vars
```

Under temporal logic model checking, we assert:

### 5.1 Completion Propagation (`CompletionPropagation`)

Every request must eventually terminate: all entry points must eventually reach a terminal state (`"complete"` or `"failed"`).

```tla
CompletionPropagation ==
  <> (\A s \in EntryPoints : epStatus[s] \in {"complete", "failed"})
```

### 5.2 Progress under Fairness (`Progress`)

If an entry point is `"ready"` and there is a worker with available capacity, it must eventually be dispatched (guaranteed under weak fairness on `Dispatch` and completion actions).

```tla
FairSpec == Spec /\ WF_vars(Next)

Progress ==
  \A s \in EntryPoints :
    epStatus[s] = "ready" /\ (\exists w \in Workers : workerLoad[w] + PredictedLoad[s] <= WorkerCap[w])
      => <> (epStatus[s] /= "ready")
```

---

## 6. Model-Checking Verification Plan

To verify the specification, TLC will be configured with finite model bounds. We will model-check across four distinct DAG topologies:

### 6.1 Topology Configurations

1. **Linear Chain**:

   ```
   A -> B -> C
   ```

   _Verifies:_ Simple sequential progress and cascading failure propagation.

2. **Diamond DAG**:

   ```
     /-> B -\
   A          -> D
     \-> C -/
   ```

   _Verifies:_ Parallel dispatching of `B` and `C` on different workers, and synchronization wait at `D` until both complete.

3. **Convergence Point DAG**:

   ```
   A -\
       -> C
   B -/
   ```

   _Verifies:_ Multi-dependency synchronization.

4. **Disconnected Independent Tasks**:
   ```
   A, B, C (no edges)
   ```
   _Verifies:_ Correct independent bin-packing capacity checks on workers.

### 6.2 TLC Parameters

- `Workers` = `{"w1", "w2"}`
- `WorkerCap` = `[w1 |-> 4, w2 |-> 4]`
- `PredictedLoad` = `[s \in EntryPoints |-> 2]` (permits concurrent tasks up to capacity limits)
- `DependencyEdges` = (instantiated per topology)
- `Outputs` = `[s \in EntryPoints |-> {s}]`

Running these configurations under TLC will exhaustively verify all possible interleavings of worker dispatch, build completions, and failures, mathematically confirming the correctness of Track A before Rust implementation begins.
