# SPEC: Eos Build Scheduler

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
  See: docs/models/publishing-stack-layers.md for the algebraic domain model.
-->

## Domain

**Problem Domain:** The Eos Build Scheduler coordinates the distribution of build and evaluation jobs across a cluster of Eos worker nodes. Unlike traditional build systems where scheduling is local and linear, Eos manages concurrent build requests from multiple clients, deduplicates identical execution plans to prevent redundant work, schedules tasks based on worker capability and input-data affinity, and balances cluster load through active work-stealing delegation.

To maximize network and storage efficiency, the scheduler operates under a lazy-fetching model: worker nodes do not download source code snapshots or build inputs until a task is explicitly scheduled on them.

**Model Reference:**
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §4.1 (Parallel Build Composition), §4.2 (Session Delegation / Work-Stealing)
- [eos-build-engine.md](eos-build-engine.md) — Plan and execute transitions

**Criticality Tier:** Medium — correct scheduling prevents race conditions, deadlocks, and unnecessary resource wastage in distributed deployments.

---

## Constraints

### Type Declarations

We define the following type signatures to govern the scheduler's state and operations:

```
TYPE WorkerId = String
TYPE JobId = Blake3Digest                              -- Unique ID derived from EnginePlan hash
TYPE JobState = QUEUED | RUNNING | DELEGATED | COMPLETED | FAILED
TYPE ClientId = String

TYPE Job = {
    id: JobId,
    state: JobState,
    plan: EnginePlan,
    assigned_worker: Maybe<WorkerId>,
    subscribers: Set<ClientId>
}

TYPE WorkerStatus = {
    id: WorkerId,
    max_concurrency: Integer,
    active_jobs: Set<JobId>,
    cached_paths: Set<StorePath>
}

TYPE SchedulerState = {
    workers: Map<WorkerId, WorkerStatus>,
    queue: Map<JobId, Job>
}
```

---

### Invariants

**[eos-scheduler-lazy-fetching]**: A worker node MUST NOT fetch the source snapshot for an atom or any transitive inputs until a job requiring them is assigned to that worker.
`VERIFIED: unverified`

**[eos-scheduler-deduplication]**: For any unique `JobId`, there MUST exist at most one active (QUEUED or RUNNING) `Job` in the scheduler state. If a client submits a build request for an `EnginePlan` that is already in progress, the scheduler MUST append the client's ID to the job's `subscribers` set rather than executing a duplicate build.
`VERIFIED: unverified`

**[eos-scheduler-input-affinity]**: The scheduler SHOULD assign a job to a worker that already has the largest fraction of the job's input store paths cached, minimizing inter-node data replication.
`VERIFIED: unverified`

**[eos-scheduler-concurrency-limits]**: The number of concurrently RUNNING jobs assigned to any worker node MUST NOT exceed that worker's declared `max_concurrency` limit.
`VERIFIED: unverified`

**[eos-scheduler-state-isolation]**: The scheduler's internal scheduling queue and state transitions MUST NOT depend on the internal evaluation states of L3 (Ion). The scheduler is a pure consumer of lock file plans.
`VERIFIED: unverified`

---

### Transitions

**[submit-job]**: Add a new build task to the queue.
- **PRE**: A client submits a build request containing an `EnginePlan`.
- **POST**: If a job with `JobId == hash(plan)` already exists, the client is added to `subscribers`. Otherwise, a new `Job` is created with state `QUEUED`, its `id` is set to `hash(plan)`, and it is added to the scheduler's queue.
`VERIFIED: unverified`

**[assign-job]**: Dispatch a queued job to an available worker.
- **PRE**: A job exists in the `QUEUED` state, and a worker is available under concurrency limits.
- **POST**: The job state transitions to `RUNNING`, `assigned_worker` is set to the worker's ID, and the task execution is dispatched to the worker.
`VERIFIED: unverified`

**[delegate-job]**: Steal/re-assign a running job to balance cluster load.
- **PRE**: A job is in the `RUNNING` state, and another worker is idle and has requested work.
- **POST**: The job state transitions to `DELEGATED` and then back to `RUNNING` with `assigned_worker` updated to the target worker. The job's execution continuation is transferred to the new worker.
`VERIFIED: unverified`

**[complete-job]**: Mark execution as successfully finished.
- **PRE**: A job is in the `RUNNING` state, and the assigned worker has completed execution and registered output store paths in the `ArtifactStore`.
- **POST**: The job state transitions to `COMPLETED`, all subscribers are notified with the outputs, and the job is removed from the active queue.
`VERIFIED: unverified`

**[fail-job]**: Handle task failures.
- **PRE**: A job is in the `RUNNING` state, and execution failed or aborted.
- **POST**: The job state transitions to `FAILED`, subscribers are notified of the error, and the job is removed from the active queue.
`VERIFIED: unverified`

---

### Forbidden States

**[no-dangling-jobs]**: A job MUST NOT remain in the `RUNNING` state if its `assigned_worker` is disconnected or offline. The scheduler MUST re-queue or fail the job within a bounded timeout.
`VERIFIED: unverified`

**[no-duplicate-execution]**: No two workers MUST be concurrently executing jobs with the same `JobId`.
`VERIFIED: unverified`

---

### Behavioral Properties

**[eventual-progress]**: Every `QUEUED` job MUST eventually transition to either `COMPLETED` or `FAILED` under the assumption of fair scheduling and non-zero worker capacity.
- **Type**: Liveness
`VERIFIED: unverified`

**[parallel-scheduling-non-interference]**: Scheduling independent jobs concurrently MUST yield a state equivalent to scheduling them sequentially in some order (concurrency safety).
- **Type**: Safety
`VERIFIED: unverified`

---

## Verification

| Constraint | Method | Result | Detail |
| :--------- | :----- | :----- | :----- |
| `eos-scheduler-lazy-fetching` | Simulation test | UNVERIFIED | Verify worker network logs during build |
| `eos-scheduler-deduplication` | Integration test | UNVERIFIED | Concurrent build submissions test |
| `eos-scheduler-input-affinity` | Metrics audit | UNVERIFIED | Verify cache hit rates across mock schedules |
| `eos-scheduler-concurrency-limits` | Queue check | UNVERIFIED | Concurrency limits validation |
| `eos-scheduler-state-isolation` | Dependency audit | UNVERIFIED | Check module boundaries |
| `submit-job` | Unit test | UNVERIFIED | Submit transitions audit |
| `assign-job` | Unit test | UNVERIFIED | Assign transitions audit |
| `delegate-job` | Unit test | UNVERIFIED | Work-stealing delegation simulation |
| `complete-job` | Unit test | UNVERIFIED | Success cleanup check |
| `fail-job` | Unit test | UNVERIFIED | Fail cleanup check |
| `no-dangling-jobs` | Timeout audit | UNVERIFIED | Heartbeat failure injection test |
| `no-duplicate-execution` | Mutual exclusion check | UNVERIFIED | Multi-worker execution logs audit |
| `eventual-progress` | Liveness check | UNVERIFIED | Loop/starvation check |
| `parallel-scheduling-non-interference` | Parity check | UNVERIFIED | Parallel vs sequential schedule output audit |

---

## Implications

1. **Work-Stealing Continuation Design**:
   Delegating jobs in the `RUNNING` state requires Eos to support transferring build continuations. In local/Snix environments, this is trivial (threads). In remote environments, this requires transferring serialized build state or aborting and restarting the build on the target worker (which is safe due to input immutability).

2. **Deduplication Key Stability**:
   Because `JobId` is computed from the hash of the `EnginePlan`, build input formats must be normalized (e.g. alphanumeric sorting of derivation inputs and environment variables) to ensure deterministic duplication detection.
