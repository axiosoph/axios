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

**Problem Domain:** The Eos Build Scheduler coordinates the distribution of build and evaluation jobs across worker nodes — both local threads and remote Eos daemon instances. It is embedded within the Eos daemon process (see [eos-network-protocol.md](eos-network-protocol.md) §Daemon Architecture), not deployed as a standalone service. The scheduler receives job submissions over the daemon's Cap'n Proto RPC surface, deduplicates identical execution plans to prevent redundant work, assigns tasks based on worker capability and input-data locality via Highest Random Weight (Rendezvous) hashing, manages worker health through lease-based ownership and periodic heartbeats, and balances cluster load through active work-stealing delegation.

To maximize network and storage efficiency, the scheduler operates under a lazy-fetching model: worker nodes do not download source code snapshots or build inputs until a task is explicitly scheduled on them.

**Model Reference:**
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §4.1 (Parallel Build Composition), §4.2 (Session Delegation / Work-Stealing)
- [eos-build-engine.md](eos-build-engine.md) — Plan and execute transitions
- [eos-network-protocol.md](eos-network-protocol.md) — Cap'n Proto wire format, `NodeIdentity`, daemon lifecycle

**Criticality Tier:** Medium — correct scheduling prevents race conditions, deadlocks, and unnecessary resource wastage in distributed deployments.

---

## Constraints

### Type Declarations

We define the following type signatures to govern the scheduler's state and operations:

```
TYPE NodeId = Data                                      -- Cyphr Principal Root (sovereign identity)
                                                        -- See eos-network-protocol.md §NodeIdentity
TYPE WorkerId = NodeId                                  -- Cryptographic worker identity
TYPE JobId = Blake3Digest                               -- Unique ID derived from EnginePlan hash
TYPE JobState = QUEUED | RUNNING | DELEGATED | COMPLETED | FAILED
TYPE ClientId = NodeId                                  -- Cryptographic client identity
TYPE Timestamp = UInt64                                  -- Unix epoch seconds

TYPE Lease = {
    job_id: JobId,
    worker_id: WorkerId,
    granted_at: Timestamp,
    expires_at: Timestamp
}

TYPE Job = {
    id: JobId,
    state: JobState,
    plan: EnginePlan,
    assigned_worker: Maybe<WorkerId>,
    subscribers: Set<ClientId>,
    lease: Maybe<Lease>
}

TYPE WorkerKind = LOCAL | REMOTE

TYPE WorkerStatus = {
    id: WorkerId,
    kind: WorkerKind,
    max_concurrency: Integer,
    active_jobs: Set<JobId>,
    cached_paths: Set<StorePath>,
    last_heartbeat: Timestamp,
    healthy: Bool
}

TYPE SchedulerState = {
    workers: Map<WorkerId, WorkerStatus>,
    queue: Map<JobId, Job>,
    lease_duration: Duration,
    heartbeat_deadline: Duration
}
```

### Worker Abstraction

The scheduler distinguishes two classes of workers but treats both uniformly through the `WorkerStatus` type:

- **Local workers** (`WorkerKind = LOCAL`): threads or processes on the same machine as the daemon. Communication occurs via in-process channels (`tokio::sync::mpsc`). Local workers share the daemon's `NodeId` and do not require independent authentication.

- **Remote workers** (`WorkerKind = REMOTE`): separate Eos daemon instances on different machines. Communication occurs over the network via the Cap'n Proto RPC surface defined in [eos-network-protocol.md](eos-network-protocol.md). Remote workers authenticate via `NodeIdentity` handshake and are addressed by their own `NodeId`.

The scheduler assigns, delegates, and monitors jobs identically regardless of worker kind. The dispatch layer resolves `WorkerId` to the appropriate communication channel.

---

### Invariants

**[eos-scheduler-lazy-fetching]**: A worker node MUST NOT fetch the source snapshot for an atom or any transitive inputs until a job requiring them is assigned to that worker.
`VERIFIED: unverified`

**[eos-scheduler-deduplication]**: For any unique `JobId`, there MUST exist at most one active (QUEUED or RUNNING) `Job` in the scheduler state. If a client submits a build request for an `EnginePlan` that is already in progress, the scheduler MUST append the client's ID to the job's `subscribers` set rather than executing a duplicate build.
`VERIFIED: unverified`

**[eos-scheduler-input-affinity]**: The scheduler MUST assign a job to the worker that maximizes input-data locality, determined by Highest Random Weight (Rendezvous) hashing. For each candidate worker satisfying concurrency limits, the scheduler computes `score(worker, job) = hash(worker_id || job_input_digest)` and selects the worker with the highest score. This provides deterministic, stable placement: a given `(worker_id, job_input_digest)` pair always produces the same score, and adding or removing workers redistributes only `1/N` of assignments (minimal disruption). The `cached_paths` set on `WorkerStatus` further informs tie-breaking — among workers with equal HRW scores, prefer those with the largest fraction of the job's input store paths already cached.
`VERIFIED: unverified`

**[eos-scheduler-concurrency-limits]**: The number of concurrently RUNNING jobs assigned to any worker node MUST NOT exceed that worker's declared `max_concurrency` limit.
`VERIFIED: unverified`

**[eos-scheduler-state-isolation]**: The scheduler's internal scheduling queue and state transitions MUST NOT depend on the internal evaluation states of L3 (Ion). The scheduler is a pure consumer of lock file plans.
`VERIFIED: unverified`

**[eos-scheduler-lease-expiry]**: Every job in the `RUNNING` state MUST be covered by a valid `Lease`. If a lease expires without renewal (i.e., `now > lease.expires_at`), the scheduler MUST revoke the lease, dissociate the job from its assigned worker, and transition the job back to the `QUEUED` state for reassignment. This prevents zombie jobs from crashed or unresponsive workers.
`VERIFIED: unverified`

**[eos-scheduler-heartbeat-liveness]**: Every registered worker MUST send periodic heartbeat signals to the scheduler. If a worker's `last_heartbeat` exceeds the configured `heartbeat_deadline`, the scheduler MUST mark that worker as `healthy = false`, MUST NOT assign new jobs to it, and MUST revoke leases on all jobs currently assigned to that worker (triggering `[eos-scheduler-lease-expiry]` for each). When a previously unhealthy worker resumes heartbeats, the scheduler MAY restore it to `healthy = true` after a configurable stabilization interval.
`VERIFIED: unverified`

---

### Transitions

**[submit-job]**: Add a new build task to the queue.
- **PRE**: A client submits a build request containing an `EnginePlan`. The request arrives via `EosDaemon.submitBuild()` over the Cap'n Proto RPC surface (see [eos-network-protocol.md](eos-network-protocol.md) §submit-build).
- **POST**: If a job with `JobId == hash(plan)` already exists, the client is added to `subscribers`. Otherwise, a new `Job` is created with state `QUEUED`, its `id` is set to `hash(plan)`, and it is added to the scheduler's queue.
`VERIFIED: unverified`

**[assign-job]**: Dispatch a queued job to an available worker.
- **PRE**: A job exists in the `QUEUED` state, and a healthy worker is available under concurrency limits.
- **POST**: The scheduler selects the optimal worker via HRW hashing (per `[eos-scheduler-input-affinity]`). The job state transitions to `RUNNING`, `assigned_worker` is set to the worker's ID, a `Lease` is created with `granted_at = now` and `expires_at = now + lease_duration`, and the task execution is dispatched to the worker.
`VERIFIED: unverified`

**[delegate-job]**: Steal/re-assign a running job to balance cluster load.
- **PRE**: A job is in the `RUNNING` state, and another healthy worker is idle and has requested work.
- **POST**: The job state transitions to `DELEGATED` and then back to `RUNNING` with `assigned_worker` updated to the target worker. The existing lease is revoked and a new `Lease` is issued for the target worker. The job's execution continuation is transferred to the new worker. For remote workers, delegation is abort-and-re-execute (safe due to input immutability); for local workers, thread-level continuation transfer MAY be used.
`VERIFIED: unverified`

**[renew-lease]**: Extend the lease on a running job.
- **PRE**: A job is in the `RUNNING` state with a valid (non-expired) `Lease`, and the assigned worker requests renewal.
- **POST**: The lease's `expires_at` is updated to `now + lease_duration`. The job continues executing on the same worker.
`VERIFIED: unverified`

**[complete-job]**: Mark execution as successfully finished.
- **PRE**: A job is in the `RUNNING` state, and the assigned worker has completed execution and registered output store paths in the `ArtifactStore`.
- **POST**: The job state transitions to `COMPLETED`, the lease is released, all subscribers are notified with the outputs, and the job is removed from the active queue.
`VERIFIED: unverified`

**[fail-job]**: Handle task failures.
- **PRE**: A job is in the `RUNNING` state, and execution failed or aborted.
- **POST**: The job state transitions to `FAILED`, the lease is released, subscribers are notified of the error, and the job is removed from the active queue.
`VERIFIED: unverified`

---

### Forbidden States

**[no-dangling-jobs]**: A job MUST NOT remain in the `RUNNING` state if its `assigned_worker` is disconnected, offline, or unhealthy. The scheduler MUST re-queue or fail the job within a bounded timeout, enforced by lease expiry (`[eos-scheduler-lease-expiry]`) and heartbeat monitoring (`[eos-scheduler-heartbeat-liveness]`).
`VERIFIED: unverified`

**[no-duplicate-execution]**: No two workers MUST be concurrently executing jobs with the same `JobId`.
`VERIFIED: unverified`

---

### Behavioral Properties

**[eventual-progress]**: Every `QUEUED` job MUST eventually transition to either `COMPLETED` or `FAILED` under the assumption of fair scheduling and non-zero healthy worker capacity.
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
| `eos-scheduler-input-affinity` | Metrics audit | UNVERIFIED | HRW score computation and cache hit rates across mock schedules |
| `eos-scheduler-concurrency-limits` | Queue check | UNVERIFIED | Concurrency limits validation |
| `eos-scheduler-state-isolation` | Dependency audit | UNVERIFIED | Check module boundaries |
| `eos-scheduler-lease-expiry` | Timeout injection | UNVERIFIED | Withhold lease renewal, verify job returns to QUEUED |
| `eos-scheduler-heartbeat-liveness` | Failure injection | UNVERIFIED | Suppress heartbeats from worker, verify health demotion and lease revocation |
| `submit-job` | Unit test | UNVERIFIED | Submit transitions audit |
| `assign-job` | Unit test | UNVERIFIED | Assign transitions audit with HRW selection verification |
| `delegate-job` | Unit test | UNVERIFIED | Work-stealing delegation simulation |
| `renew-lease` | Unit test | UNVERIFIED | Lease extension and expiry boundary test |
| `complete-job` | Unit test | UNVERIFIED | Success cleanup check |
| `fail-job` | Unit test | UNVERIFIED | Fail cleanup check |
| `no-dangling-jobs` | Timeout audit | UNVERIFIED | Heartbeat + lease expiry failure injection test |
| `no-duplicate-execution` | Mutual exclusion check | UNVERIFIED | Multi-worker execution logs audit |
| `eventual-progress` | Liveness check | UNVERIFIED | Loop/starvation check |
| `parallel-scheduling-non-interference` | Parity check | UNVERIFIED | Parallel vs sequential schedule output audit |

---

## Implications

1. **Work-Stealing Continuation Design:**
   Delegating jobs in the `RUNNING` state requires Eos to support transferring build continuations. For local workers, this is trivial (threads share the same process). For remote workers, delegation is abort-and-re-execute — the target worker re-fetches inputs and restarts execution from the beginning. This is safe because build inputs are immutable and content-addressed, ensuring identical results regardless of which worker executes the plan.

2. **Deduplication Key Stability:**
   Because `JobId` is computed from the hash of the `EnginePlan`, build input formats must be normalized (e.g., alphanumeric sorting of derivation inputs and environment variables) to ensure deterministic duplication detection.

3. **Lease Duration Tuning:**
   The `lease_duration` parameter governs the tradeoff between failure detection latency and renewal overhead. Short leases detect crashed workers quickly but impose frequent renewal traffic. Long leases reduce renewal overhead but delay reassignment of orphaned jobs. The heartbeat deadline SHOULD be set shorter than the lease duration, so that heartbeat failure triggers proactive lease revocation before leases expire passively.

4. **HRW Hashing and Cluster Membership Changes:**
   Rendezvous hashing's cardinal property is minimal disruption under membership changes: when a worker joins or departs, only the `1/N` fraction of jobs that hashed highest to that worker are redistributed. This is strictly superior to consistent hashing for scheduling workloads where the number of workers is small (tens, not thousands) and the affinity mapping must remain stable.

5. **Cap'n Proto Projection:**
   Job submissions, status updates, delegation, and lease management are projected over the wire via the Cap'n Proto schema defined in [eos-network-protocol.md](eos-network-protocol.md). The `EosDaemon.submitBuild()` capability maps to `[submit-job]`; the `BuildJob` capability provides status queries and cancellation. Scheduler state is internal to the daemon and is not directly exposed over the protocol — clients interact with jobs through capabilities, not with the scheduler itself.

6. **Daemon-Embedded Architecture:**
   The scheduler runs as a component within the Eos daemon's event loop, not as a separable microservice. It shares the daemon's `NodeId` for identity, accesses the local `ArtifactStore` directly, and communicates with local workers via channels and remote workers via Cap'n Proto RPC. This co-location eliminates a network hop for scheduling decisions and allows the scheduler to observe local store state (cached paths) without protocol overhead.
