# Eos Scheduling: TLA+ Protocol Verification

Model-checked proofs that the Eos dispatch protocol maintains safety and
liveness invariants under all interleavings. Each topology model exhaustively
enumerates the reachable state space via TLC, confirming that no execution
trace violates the specified properties.

## Prerequisites

- **Java runtime** (≥ 11) — required by the TLC model checker.
- **TLA+ tools** — download
  [`tla2tools.jar`](https://github.com/tlaplus/tlaplus/releases) and place
  it in this directory (or pass the full path).
- **Nix users** — a `shell.nix` is provided in this directory:
  ```sh
  nix-shell   # drops you into a shell with Java and tla2tools available
  ```

## Running Model Checking

Each topology has a `.tla` specification and a matching `.cfg` configuration.
Run TLC against each one:

```sh
java -jar tla2tools.jar -config LinearModel.cfg LinearModel.tla
java -jar tla2tools.jar -config DiamondModel.cfg DiamondModel.tla
java -jar tla2tools.jar -config ConvergenceModel.cfg ConvergenceModel.tla
java -jar tla2tools.jar -config IndependentModel.cfg IndependentModel.tla
```

TLC will report the number of distinct states explored and whether all
invariants and temporal properties hold. A successful run ends with
`Model checking completed. No error has been found.`

## Architecture

### Base Module

`EosScheduling.tla` defines the parameterised state machine that all
simple topology models instantiate.

`MultiRequestModel.tla` extends this state machine with support for
concurrent request arrivals (`MergeRequest`), dynamic topology merging,
request cancellation (`CancelRequest`), cache-skip scan (`CacheSkip`),
transient failure recovery (`FailTransient`), and client request tracking.

**State variables:**

| Variable          | Description                                   |
| :---------------- | :-------------------------------------------- |
| `epStatus`        | Map from entry point → execution status       |
| `workerLoad`      | Map from worker → current load count          |
| `artifactStore`   | Set of artifacts produced by completed steps  |
| `runningOn`       | Map from entry point → assigned worker (or ⊥) |
| `EntryPoints`     | Dynamic set of active entry points            |
| `DependencyEdges` | Dynamic set of active dependency edges        |
| `requestClients`  | Map from entry point → requesting request IDs |
| `requestArrived`  | Set of request IDs that have arrived          |
| `failureReason`   | Map from entry point → failure type (or none) |

**Transitions:**

| Action                 | Effect                                      |
| :--------------------- | :------------------------------------------ |
| `Dispatch(s, w)`       | Assign ready step `s` to worker `w`         |
| `Complete(s)`          | Mark `s` done; publish its artifact         |
| `FailDeterministic(s)` | Mark `s` failed due to build failure        |
| `FailTransient(s)`     | Release `s` back to ready pool (recovery)   |
| `CascadeFail(s)`       | Propagate failure to downstream dependants  |
| `MergeRequest`         | Dynamically merge a new request and its DAG |
| `CacheSkip(s)`         | Skip execution if outputs are in store      |
| `CancelRequest(r)`     | Prune EPs after request cancellation        |

**Static axioms** — `VerifyAxioms` asserts finite sets, DAG acyclicity,
and feasibility as preconditions before state exploration begins.

**Fairness** — the specification asserts `WF_vars(Next)` (weak fairness
over all transitions), ensuring the system cannot stall indefinitely.

### Topology Models

Each model instantiates `EosScheduling` or defines a custom multi-request scenario:

| Model               | Topology           | Primary Concern                                  |
| :------------------ | :----------------- | :----------------------------------------------- |
| `LinearModel`       | A → B → C          | Sequential cascade failure                       |
| `DiamondModel`      | A → {B,C} → D      | Fork/join synchronisation                        |
| `ConvergenceModel`  | {A,B} → C          | Multi-dependency convergence                     |
| `IndependentModel`  | A, B, C (no edges) | Capacity bin-packing                             |
| `MultiRequestModel` | Dynamic merging    | Merging, cache-skip, cancellation, liveness, HoL |

## What the Models Verify

| Property                     | Type                | Verified |
| :--------------------------- | :------------------ | :------- |
| Ordering soundness (P1)      | Safety invariant    | ✅       |
| Artifact completeness (P3)   | Safety invariant    | ✅       |
| Capacity safety (P4)         | Safety invariant    | ✅       |
| Progress (P5)                | Liveness (temporal) | ✅       |
| Completion propagation (P6)  | Liveness (temporal) | ✅       |
| HoL immunity (P5')           | Liveness (temporal) | ✅       |
| Per-request completion (P6') | Liveness (temporal) | ✅       |
| Frozen stability (P8)        | Action property     | ✅       |
| Work conservation (P9)       | Liveness (temporal) | ✅       |
| Transient recovery (P10)     | Liveness (temporal) | ✅       |
| Failure isolation (P11)      | Safety invariant    | ✅       |

`TypeOK` (type invariant) is checked in every model as a baseline
structural health property.

## Relationship to Project

This TLA+ suite is **Track A** of a two-track formal verification
strategy:

- **Track A (TLA+, here)** — protocol correctness: safety and liveness
  of the dispatch state machine.
- **Track B (Lean 4, `../lean/`)** — optimisation quality: proves bounds
  on scheduling efficiency.

See [`../eos-scheduling.md`](../eos-scheduling.md) and
[ADR-0004](../../adr/0004-learning-augmented-scheduling.md) for the full
verification design.
