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

**Problem Domain:** The Eos Build Scheduler coordinates the distribution of build and evaluation jobs across worker nodes — both local threads and remote Eos daemon instances. It is embedded within the Eos daemon process (see [eos-network-protocol.md](eos-network-protocol.md) §Daemon Architecture), not deployed as a standalone service. The scheduler receives job submissions over the daemon's Cap'n Proto RPC surface, deduplicates identical execution plans to prevent redundant work, coarsens uncached plan subgraphs into **entry points** (EPs), and assigns EPs to workers by minimizing predicted locality-adjusted completion time (see `[eos-scheduler-placement]`). It manages worker health through lease-based ownership and periodic heartbeats and enforces a bounded dispatch window (P9′) that permits prediction-gated scheduling holds.

To maximize network and storage efficiency, the scheduler operates under a lazy-fetching model: worker nodes do not download source code snapshots or build inputs until an EP is explicitly dispatched to them.

**Model Reference:**

- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §4.1 (Parallel Build Composition)
- [eos-build-engine.md](eos-build-engine.md) — Plan and execute transitions; `BuildEngine::Digest` abstract type; `JobId` computation
- [eos-network-protocol.md](eos-network-protocol.md) — Cap'n Proto wire format, `NodeIdentity`, `SubstitutionService`, daemon lifecycle
- [docs/adr/0004-learning-augmented-scheduling.md](../adr/0004-learning-augmented-scheduling.md) — PEFT dispatch protocol (§2b), Option C duration model (§3), profile store (§1), EP coarsening (§2a), formal guarantees

**Criticality Tier:** Medium — correct scheduling prevents race conditions, deadlocks, and unnecessary resource wastage in distributed deployments.

---

## Constraints

### Type Declarations

We define the following type signatures to govern the scheduler's state and operations:

```
TYPE NodeId = Data                                      -- Cyphr cryptographic node identity
                                                        -- See eos-network-protocol.md §NodeIdentity
TYPE WorkerId = NodeId                                  -- Cryptographic worker identity
TYPE JobId = BuildEngine::Digest                        -- Content-addressed plan digest
                                                        -- (abstract over backend — see eos-build-engine.md §JobId)
                                                        -- [boundary-no-backend-leakage]: MUST NOT be bound
                                                        -- to a concrete type such as Blake3Digest
TYPE JobState = QUEUED | RUNNING | COMPLETED | FAILED
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
    plan: BuildEngine::Plan,
    assigned_worker: Maybe<WorkerId>,
    subscribers: Set<ClientId>,
    lease: Maybe<Lease>
}

TYPE WorkerKind = EVAL | BUILD

TYPE ResourceVector = Map<ResourceDimension, u64>       -- Abstract capacity/load vector
                                                        -- Dimensions: CPU, memory, disk, optionally egress_bandwidth
                                                        -- Worker reports physical or virtual capacity;
                                                        -- enforcement mechanism is a worker-side concern

TYPE WorkerStatus = {
    id: WorkerId,
    kind: WorkerKind,
    capacity: ResourceVector,                           -- Reported capacity (physical or virtual)
    active_jobs: Set<JobId>,
    cached_paths: Set<StorePath>,                       -- Build workers only; eval workers: empty
    last_heartbeat: Timestamp,
    healthy: Bool
}

TYPE SchedulerState = {
    eval_workers: Map<WorkerId, WorkerStatus>,
    build_workers: Map<WorkerId, WorkerStatus>,
    eval_queue: Map<JobId, Job>,
    build_queue: Map<JobId, Job>,
    lease_duration: Duration,
    heartbeat_deadline: Duration,
    scheduling_table: SchedulingTable,                  -- EP-level dispatch model (see below)
    profile_store: ProfileStore                         -- Persistent build profiles with hot cache
}

-- Entry Point (EP) scheduling model
-- The scheduler coarsens the uncached plan subgraph into EPs.
-- T = (S, E_S) is a persistent lightweight scheduling table layered over
-- the unified global plan graph G∪; it is not a second graph.
-- |S| is typically in the tens. See ADR-0004 §2 for construction.

TYPE EpId = JobId                                       -- Content-addressed: digest of the EP's entry plan

TYPE EpStatus = PENDING | READY | DISPATCHED | COMPLETE | FAILED

TYPE EpRecord = {
    scope: Set<PlanHash>,                               -- G∪ plan nodes covered by this EP
    time_cost: Duration,                                -- Σ d(v) for v ∈ scope (aggregate predicted cost)
    mem_peak: Bytes,                                    -- max predicted peak memory across scope
    status: EpStatus,
    deps: List<EpId>,                                   -- EP-level dependency pointers (E_S adjacency list)
    oct: Map<WorkerId, f64>                             -- per-worker Optimistic Cost Table values
}

TYPE SchedulingTable = {
    eps: Map<EpId, EpRecord>                            -- Active EP records; |S| entries (typically tens)
}

-- Profile store: persistent per-plan historical observations with in-memory hot cache.
-- Profiles are keyed by plan_name (human-readable, version-stable from StorePath).
-- See ADR-0004 §1 for the full schema and prediction resolution order.

TYPE ProfileRecord = {
    build_duration:  ExponentialMovingAverage,
    build_memory:    ExponentialMovingAverage,
    build_cpu_cores: ExponentialMovingAverage,
    output_size:     ExponentialMovingAverage,
    confidence:      f64,                               -- 1 - EMA(|η|) from prediction error history
    atom-id:         Maybe<atom-id>,                     -- secondary index for cross-version aggregation
    atom_metadata:   Maybe<SchedulingMetadata>          -- developer-provided scheduling hints
}

TYPE ProfileStore = {
    persistent: KVStore<PlanName, ProfileRecord>,       -- Persistent database; grows unboundedly with
                                                        -- distinct plan names seen over daemon lifetime
    hot_cache:  Map<PlanName, ProfileRecord>            -- In-memory cache for plans in active G∪;
                                                        -- loaded on RequestArrival, evicted on terminal GC
}
```

### Worker Pools

The scheduler manages two functionally distinct worker pools, each accessed exclusively over Cap'n Proto RPC:

- **Eval workers** (`WorkerKind = EVAL`): Separate processes (local or remote) that run `snix-eval` + `snix-glue` in-process on a dedicated OS thread. They connect to snix store daemons via gRPC for store access. Communication between the scheduler and eval workers occurs over Cap'n Proto via the `EvalWorker` interface (see [eos-network-protocol.md](eos-network-protocol.md)). Eval workers are memory-bound with low I/O variance.

- **Build workers** (`WorkerKind = BUILD`): Separate processes (local or remote) that wrap snix's gRPC `BuildService.DoBuild()` in a Cap'n Proto shim adding cancellation, progress streaming, and lease management. Communication between the scheduler and build workers occurs over Cap'n Proto via the `BuildWorker` interface. Build workers are CPU/disk-bound with high I/O variance.

Both pools use the same lease-based health monitoring, heartbeat liveness, and PEFT-based EP dispatch. Local Rendezvous Hash (LRH) affinity enters worker selection exclusively through the per-worker predicted duration `d(ep, w)` — it is not an independent scoring term (see `[eos-scheduler-placement]`). Neither pool supports mid-execution EP reassignment; re-dispatch of a failed EP occurs exclusively through the transient-failure path (`[fail-job]`, P10). Eval workers have low duration variance, making prediction-gated dispatch holds less valuable; the bounded dispatch window Δ is expected to be smaller for eval EPs than for build EPs.

Workers register dynamically via Cap'n Proto handshake. The scheduler does NOT manage worker lifecycles — starting, stopping, and scaling workers is delegated to an external orchestrator (process-compose, systemd, k8s).

---

### Invariants

**[eos-scheduler-lazy-fetching]**: A worker node MUST NOT fetch the source snapshot for an atom or any transitive inputs until an EP requiring them is dispatched to that worker.
`VERIFIED: unverified`

**[eos-scheduler-deduplication]**: For any unique `JobId`, there MUST exist at most one active (QUEUED or RUNNING) `Job` in the scheduler state. If a client submits a build request for a `BuildEngine::Plan` that is already in progress, the scheduler MUST append the client's ID to the job's `subscribers` set rather than executing a duplicate build.
`VERIFIED: unverified`

**[eos-scheduler-placement]**: The scheduler MUST assign each ready EP to the worker minimizing predicted locality-adjusted completion time. The placement contract is: for each feasible worker `w` and ready EP `ep`, the scheduler selects the worker minimizing `EFT(ep, w) + OCT(ep, w)`, where `EFT` is the estimated finish time and `OCT` is the per-worker Optimistic Cost Table value from `EpRecord.oct`. Local Rendezvous Hash (LRH) affinity and multi-resource fit enter placement exclusively through the per-worker predicted duration `d(ep, w)` — they are not independent scoring terms and do not bypass the EFT+OCT objective. For the duration model (Option C) and PEFT worker-selection algorithm, see [docs/adr/0004-learning-augmented-scheduling.md](../adr/0004-learning-augmented-scheduling.md) §2b and §3. This invariant supersedes the prior standard HRW invariant, which did not account for downstream DAG cost (OCT) or predicted duration.
`VERIFIED: unverified`

**[eos-scheduler-profile-store]**: The scheduler MUST maintain a persistent profile store of historical build observations keyed by `plan_name`. The persistent store is the authoritative source of truth; the in-memory hot cache holds profiles for plans in the current active G∪ and is written through to the persistent store on every EP completion. When no historical observation exists for a `plan_name`, the scheduler MUST fall back to developer-provided scheduling metadata (`atom_metadata`), then to system defaults. See [docs/adr/0004-learning-augmented-scheduling.md](../adr/0004-learning-augmented-scheduling.md) §1 for the full prediction resolution order.
`VERIFIED: unverified`

**[eos-scheduler-concurrency-limits]**: The number of concurrently RUNNING jobs assigned to any worker node MUST NOT exceed that worker's declared capacity. The dispatch guard is: `current_load(w) + predicted_load(ep) ≤ capacity(w)`.
`VERIFIED: TLA+ CapacitySafety — docs/models/tla/MultiRequestModel.tla:257-258`

**[eos-scheduler-state-isolation]**: The scheduler's internal scheduling queue and state transitions MUST NOT depend on the internal evaluation states of L3 (Ion). The scheduler is a pure consumer of L2-native plan digests and DAG structures; lock file parsing MUST NOT occur in the daemon.
`VERIFIED: unverified`

**[eos-scheduler-lease-expiry]**: Every job in the `RUNNING` state MUST be covered by a valid `Lease`. If a lease expires without renewal (i.e., `now > lease.expires_at`), the scheduler MUST revoke the lease, dissociate the job from its assigned worker, and transition the job back to the `QUEUED` state for reassignment. This prevents zombie jobs from crashed or unresponsive workers.
`VERIFIED: unverified`

**[eos-scheduler-heartbeat-liveness]**: Every registered worker MUST send periodic heartbeat signals to the scheduler. If a worker's `last_heartbeat` exceeds the configured `heartbeat_deadline`, the scheduler MUST mark that worker as `healthy = false`, MUST NOT assign new jobs to it, and MUST revoke leases on all jobs currently assigned to that worker (triggering `[eos-scheduler-lease-expiry]` for each). When a previously unhealthy worker resumes heartbeats, the scheduler MAY restore it to `healthy = true` after a configurable stabilization interval.
`VERIFIED: unverified`

**[eos-scheduler-frozen-stability]** (P8): Once an EP transitions to `DISPATCHED` status, its coverage scope (`EpRecord.scope`) and worker assignment MUST NOT change. Re-coarsening MUST operate only on the MUTABLE partition (EPs in `PENDING` or `READY` status and unassigned plan nodes). An EP that has been dispatched MUST NOT be reassigned to a different worker by any mechanism other than the transient-failure path (`[fail-job]` with `failure_kind = transient`), which reverts the EP to `READY` for re-dispatch rather than transferring a running EP.
`VERIFIED: TLA+ FrozenStability (P8) — docs/models/tla/MultiRequestModel.tla:284-290`

**[eos-scheduler-bounded-window]** (P9′): When a ready EP has a feasible worker, the EP MUST be dispatched, reach `COMPLETE`, or reach `FAILED` within a bounded window of Δ ticks of entering the `READY` state. The window is confidence-gated: when prediction confidence is low, Δ = 0, degenerating to strict immediate dispatch (the original P9). Under high confidence, the scheduler MAY hold a ready EP for up to Δ to wait for a higher-affinity worker predicted to free up soon. Formally:
```
P9': □( Q(s) = ready ∧ ∃w: feasible(s, w) ) ⟹ ◇≤Δ Q(s) ∈ {dispatched, complete, failed}
```
The bounded window is what makes PEFT's OCT look-ahead actionable; strict work conservation (Δ = 0 always) forces immediate dispatch and renders OCT inert.
`VERIFIED: TLA+ WorkConservation (P9′) — docs/models/tla/MultiRequestModel.tla:339-344`

**[eos-scheduler-cache-filter-seam]**: The scheduler MUST query artifact existence ONLY through the `eos-core` `ArtifactStore` abstraction or the Cap'n Proto `SubstitutionService.query` surface (see [eos-network-protocol.md](eos-network-protocol.md):282-300). The scheduler MUST NOT import snix types, hold gRPC client code, or call `PathInfoService` directly. The `SubstitutionService` shim (deployable inside a worker or standalone) absorbs per-digest fan-out to the underlying store internally, presenting the scheduler with a single logical existence query per batch. This preserves `[eos-scheduler-state-isolation]` even when artifact existence checks are required.
`VERIFIED: unverified`

---

### Transitions

**[submit-job]**: Add a new build task to the queue.

- **PRE**: A client submits a build request containing a `BuildEngine::Plan`. The request arrives via `EosDaemon.submitBuild()` over the Cap'n Proto RPC surface (see [eos-network-protocol.md](eos-network-protocol.md) §submit-build).
- **POST**: If a job with `JobId == plan_digest(plan)` already exists, the client is added to `subscribers`. Otherwise, a new `Job` is created with state `QUEUED`, its `id` is set to `plan_digest(plan)`, and it is added to the scheduler's queue.
  `VERIFIED: unverified`

**[assign-job]**: Dispatch a ready EP to an available worker.

- **PRE**: A ready EP exists in the scheduling table with `EpStatus = READY`, a healthy worker is available under capacity limits (see `[eos-scheduler-concurrency-limits]`), and all EP-level dependencies are in `COMPLETE` status (ordering soundness, P1). The bounded dispatch window `[eos-scheduler-bounded-window]` permits the dispatch to proceed now.
- **POST**: The scheduler selects the worker minimizing `EFT(ep, w) + OCT(ep, w)` per `[eos-scheduler-placement]`. The job state transitions to `RUNNING`, `assigned_worker` is set to the selected worker's ID, a `Lease` is created with `granted_at = now` and `expires_at = now + lease_duration`, and the EP transitions to `DISPATCHED` with its worker assignment frozen per `[eos-scheduler-frozen-stability]`.
  `VERIFIED: TLA+ OrderingSoundness (P1) — docs/models/tla/MultiRequestModel.tla:253-255`

**[renew-lease]**: Extend the lease on a running job.

- **PRE**: A job is in the `RUNNING` state with a valid (non-expired) `Lease`, and the assigned worker requests renewal.
- **POST**: The lease's `expires_at` is updated to `now + lease_duration`. The job continues executing on the same worker.
  `VERIFIED: unverified`

**[complete-job]**: Mark execution as successfully finished.

- **PRE**: A job is in the `RUNNING` state. The assigned worker has completed execution and published all output artifacts to the artifact store before reporting completion (publication precedes the completion signal to preserve P3).
- **POST**: The job state transitions to `COMPLETED`, the EP transitions to `COMPLETE`, the lease is released, all subscribers are notified with the outputs, and the job is removed from the active queue. The profile store is updated with observed duration, memory, and CPU via EMA. Downstream pending EPs whose EP-level dependencies are now satisfied transition to `READY`.
  `VERIFIED: TLA+ ArtifactSafety (P3) — docs/models/tla/MultiRequestModel.tla:261-263`

**[fail-job]**: Handle task failures.

- **PRE**: A job is in the `RUNNING` state and execution has failed or aborted.
- **POST** (deterministic failure — build error, reproducible failure): The job state transitions to `FAILED`, the EP transitions to `FAILED`, the lease is released, subscribers are notified of the error, failure cascades to downstream dependent EPs (P11), and the job is removed from the active queue.
- **POST** (transient failure — infrastructure failure, worker crash, network partition): The EP is reverted to `READY` for re-dispatch to a healthy worker (P10). The lease is revoked. PEFT re-planning runs. The scheduler selects a new worker per `[eos-scheduler-placement]`. Work-stealing of a running EP is FORBIDDEN — this re-dispatch path exists only when the executing worker becomes unavailable, never as a load-balancing mechanism.
  `VERIFIED: TLA+ FailureIsolation (P11) — docs/models/tla/MultiRequestModel.tla:269-275; TransientRecovery (P10) — docs/models/tla/MultiRequestModel.tla:349-353`

---

### Forbidden States

**[no-dangling-jobs]**: A job MUST NOT remain in the `RUNNING` state if its `assigned_worker` is disconnected, offline, or unhealthy. The scheduler MUST re-queue or fail the job within a bounded timeout, enforced by lease expiry (`[eos-scheduler-lease-expiry]`) and heartbeat monitoring (`[eos-scheduler-heartbeat-liveness]`).
`VERIFIED: unverified`

**[no-duplicate-execution]**: No two workers MUST be concurrently executing jobs with the same `JobId`.
`VERIFIED: unverified`

**[no-dispatched-ep-reassignment]**: A `DISPATCHED` EP's coverage scope and worker assignment MUST NOT be modified by any event other than the transient-failure path (`[fail-job]` → revert to `READY`). Re-coarsening, new request arrivals, and cache-skip events MUST NOT touch the FROZEN partition of the scheduling table (EPs in `DISPATCHED`, `COMPLETE`, or `FAILED` status). This is the operational expression of `[eos-scheduler-frozen-stability]` (P8).
`VERIFIED: TLA+ FrozenStability (P8) — docs/models/tla/MultiRequestModel.tla:284-290`

---

### Behavioral Properties

**[eventual-progress]**: Every `QUEUED` job MUST eventually transition to either `COMPLETED` or `FAILED` under the assumption of fair scheduling and non-zero healthy worker capacity.

- **Type**: Liveness
  `VERIFIED: TLA+ Progress (P5) — docs/models/tla/MultiRequestModel.tla:307-311; CompletionPropagation (P6/P6′) — docs/models/tla/MultiRequestModel.tla:302-305`

**[parallel-scheduling-non-interference]**: Scheduling independent jobs concurrently MUST yield a state equivalent to scheduling them sequentially in some order (concurrency safety).

- **Type**: Safety
  `VERIFIED: unverified`

**[head-of-line-immunity]** (P5′): A large or slow EP on one worker MUST NOT block dispatch of unrelated ready EPs to other workers. The PEFT event loop evaluates all ready EPs on each pass; dispatch of EP `s` depends only on `epStatus[s]` and worker load, never on the status of unrelated EPs.

- **Type**: Liveness
  `VERIFIED: TLA+ HoLImmunity (P5′) — docs/models/tla/MultiRequestModel.tla:319-328`

**[bounded-window-dispatch]** (P9′): A ready EP with a feasible worker MUST be dispatched, reach `COMPLETE`, or reach `FAILED` within Δ ticks of entering the `READY` state. When prediction confidence is low, Δ = 0 and dispatch is immediate. The bounded window is stated operationally in `[eos-scheduler-bounded-window]`.

- **Type**: Liveness
  `VERIFIED: TLA+ WorkConservation (P9′) — docs/models/tla/MultiRequestModel.tla:339-344`

---

## Verification

| Constraint                              | Method                           | Result                    | Detail                                                                                           |
| :-------------------------------------- | :------------------------------- | :------------------------ | :----------------------------------------------------------------------------------------------- |
| `eos-scheduler-lazy-fetching`           | Simulation test                  | UNVERIFIED                | Verify worker network logs during build                                                          |
| `eos-scheduler-deduplication`           | Integration test                 | UNVERIFIED                | Concurrent build submissions test                                                                |
| `eos-scheduler-placement`              | Metrics audit                    | UNVERIFIED                | EFT+OCT minimization and LRH cache hit rates across mock schedules                              |
| `eos-scheduler-profile-store`          | Integration test                 | UNVERIFIED                | Profile write-through, hot-cache eviction, fallback resolution order                            |
| `eos-scheduler-concurrency-limits`     | TLA+ model check                 | ✅ VERIFIED               | `CapacitySafety` (P4) — `docs/models/tla/MultiRequestModel.tla:257-258`                         |
| `eos-scheduler-state-isolation`        | Dependency audit                 | UNVERIFIED                | Check module boundaries; confirm no lock parsing in daemon                                      |
| `eos-scheduler-lease-expiry`           | Timeout injection                | UNVERIFIED                | Withhold lease renewal, verify job returns to QUEUED                                            |
| `eos-scheduler-heartbeat-liveness`     | Failure injection                | UNVERIFIED                | Suppress heartbeats from worker, verify health demotion and lease revocation                    |
| `eos-scheduler-frozen-stability`       | TLA+ model check                 | ✅ VERIFIED               | `FrozenStability` (P8) — `docs/models/tla/MultiRequestModel.tla:284-290`                        |
| `eos-scheduler-bounded-window` (P9′)   | TLA+ model check                 | ✅ VERIFIED               | `WorkConservation` (P9′) — `docs/models/tla/MultiRequestModel.tla:339-344`                      |
| `eos-scheduler-cache-filter-seam`      | Dependency audit + integration   | UNVERIFIED                | Confirm no snix imports or gRPC client code in scheduler crate                                  |
| `submit-job`                            | Unit test                        | UNVERIFIED                | Submit transitions audit                                                                         |
| `assign-job`                            | TLA+ model check + unit test     | ✅ VERIFIED (ordering)    | `OrderingSoundness` (P1) — `docs/models/tla/MultiRequestModel.tla:253-255`; placement: unit test |
| `renew-lease`                           | Unit test                        | UNVERIFIED                | Lease extension and expiry boundary test                                                         |
| `complete-job`                          | TLA+ model check + unit test     | ✅ VERIFIED (artifacts)   | `ArtifactSafety` (P3) — `docs/models/tla/MultiRequestModel.tla:261-263`                         |
| `fail-job` (deterministic)              | TLA+ model check + unit test     | ✅ VERIFIED (isolation)   | `FailureIsolation` (P11) — `docs/models/tla/MultiRequestModel.tla:269-275`                       |
| `fail-job` (transient)                  | TLA+ model check + unit test     | ✅ VERIFIED (recovery)    | `TransientRecovery` (P10) — `docs/models/tla/MultiRequestModel.tla:349-353`                     |
| `no-dangling-jobs`                      | Timeout audit                    | UNVERIFIED                | Heartbeat + lease expiry failure injection test                                                  |
| `no-duplicate-execution`               | Mutual exclusion check           | UNVERIFIED                | Multi-worker execution logs audit                                                                |
| `no-dispatched-ep-reassignment`        | TLA+ model check                 | ✅ VERIFIED               | `FrozenStability` (P8) — `docs/models/tla/MultiRequestModel.tla:284-290`                        |
| `eventual-progress`                     | TLA+ model check                 | ✅ VERIFIED               | `Progress` (P5) — `docs/models/tla/MultiRequestModel.tla:307-311`; `CompletionPropagation` (P6/P6′) — `:302-305` |
| `parallel-scheduling-non-interference` | Parity check                     | UNVERIFIED                | Parallel vs sequential schedule output audit                                                     |
| `head-of-line-immunity` (P5′)          | TLA+ model check                 | ✅ VERIFIED               | `HoLImmunity` (P5′) — `docs/models/tla/MultiRequestModel.tla:319-328`                           |
| `bounded-window-dispatch` (P9′)        | TLA+ model check                 | ✅ VERIFIED               | `WorkConservation` (P9′) — `docs/models/tla/MultiRequestModel.tla:339-344`                      |

---

## Implications

1. **No Work-Stealing of Running EPs:**
   Mid-execution EP reassignment (work-stealing) is architecturally prohibited by Frozen Stability (P8, `[eos-scheduler-frozen-stability]`). Once an EP is dispatched, its coverage scope and worker assignment are immutable. Re-dispatch occurs exclusively through the transient-failure path: when a worker becomes unavailable, the EP reverts to `READY` — it is not transferred mid-execution. This is safe because build inputs are immutable and content-addressed (abort-and-re-execute produces identical outputs), but the scheduler never initiates re-dispatch for load-balancing reasons — only for failure recovery. The prior `[delegate-job]` work-stealing transition, which reassigned RUNNING jobs and mid-execution, is removed; it conflicted with Frozen Stability (P8) and is superseded by the transient-failure re-dispatch path.

2. **Deduplication Key Stability:**
   Because `JobId` is computed from the `BuildEngine::Digest` of the plan (via `BuildEngine::plan_digest()`), build input formats must be normalized (e.g., alphanumeric sorting of plan inputs and environment variables) to ensure deterministic deduplication. The `JobId` type is abstract (`BuildEngine::Digest`) — backend-specific digest types MUST NOT appear in scheduler type signatures (`[boundary-no-backend-leakage]`, layer-boundaries.md:221-235).

3. **Lease Duration Tuning:**
   The `lease_duration` parameter governs the tradeoff between failure detection latency and renewal overhead. Short leases detect crashed workers quickly but impose frequent renewal traffic. Long leases reduce renewal overhead but delay reassignment of orphaned jobs. The heartbeat deadline SHOULD be set shorter than the lease duration, so that heartbeat failure triggers proactive lease revocation before leases expire passively.

4. **LRH Affinity as Duration-Model Input:**
   Local Rendezvous Hash (LRH) affinity supersedes the prior single-criterion Rendezvous-hash placement approach. LRH restricts candidate selection to a cache-local window on a virtual ring, achieving near-optimal load balance with ~6.8× higher throughput by exploiting CPU cache locality. In this scheduler, LRH affinity enters worker selection exclusively through the per-worker predicted duration `d(ep, w)` (Option C folding, ADR-0004 §3) — it is not an independent score competing with EFT+OCT. Adding or removing workers redistributes only the affected `1/N` fraction of assignments. See ADR-0004 §4 for the LRH algorithm.

5. **Cap'n Proto as Universal Internal Protocol and the Zero-Snix Implication:**
   All scheduler-to-worker communication uses Cap'n Proto RPC. The scheduler speaks Cap'n Proto to both `EvalWorker` and `BuildWorker` interfaces — it carries zero snix dependencies and no gRPC client code. This zero-snix/zero-gRPC invariant (`[eos-scheduler-state-isolation]`) is preserved even for artifact existence checks: the scheduler queries artifact existence exclusively through the Cap'n Proto `SubstitutionService.query` interface ([eos-network-protocol.md](eos-network-protocol.md):282-300). The `SubstitutionService` shim absorbs per-digest fan-out to the snix `PathInfoService` internally — the scheduler sees one logical existence query per batch and holds no knowledge of the underlying store wire protocol. Worker shims translate between Cap'n Proto (Eos protocol) and gRPC (snix protocol) where needed. Scheduler state is internal to the daemon and is not directly exposed over the protocol.

6. **Daemon-Embedded Scheduler, Service-External Workers:**
   The scheduler runs as a component within the Eos daemon's event loop, sharing the daemon's `NodeId` for identity. However, all workers (eval and build) are external processes accessed exclusively via Cap'n Proto RPC — even when co-located on the same machine. The scheduler does not access the snix store directly; store operations are performed by workers. This uniform access pattern means there is no architectural distinction between "local" and "remote" workers — co-located workers simply have lower network latency.
