# ADR-0002: Snix Integration via Service Boundaries

- **Status**: PROPOSED (REVISED DRAFT)
- **Date**: 2026-06-05
- **Deciders**: nrd
- **Source**: [Eos Build Scheduler Specification](../specs/eos-scheduler.md) | [Eos Snix Backend Specification](../specs/eos-snix-backend.md) | [Eos Network Protocol](../specs/eos-network-protocol.md)
- **Supersedes**: ADR-0002 (2026-06-03 draft, "Decoupling Snix Backend from Eos Core and Scheduler")

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

Snix is a microservice architecture. Its store services (`BlobService`, `DirectoryService`, `PathInfoService`), build service (`BuildService`), and evaluator (`snix-eval`) are designed as independently deployable components connected by gRPC contracts. The protobuf definitions are MIT-licensed specifically to encourage third-party clients.

Our current integration inverts this architecture. We embed snix's store services, build services, and evaluator as linked Rust crates, creating a monolith where snix designed composable network services. This manifests as five concrete coupling violations:

1. **`eos-daemon/src/main.rs`** directly calls `snix_store::utils::construct_services()` to create in-process store service instances.
2. **`eos-daemon/src/scheduler.rs`** is parameterized on the concrete `SnixEngine` type, reaching into its public fields (`engine.blob_service.clone()`, `engine.directory_service.clone()`) to construct `SnixIngestService` and `CastoreBridge`.
3. **`eos/src/bridge.rs`** (`CastoreBridge`) directly holds `Arc<dyn BlobService>`, `Arc<dyn DirectoryService>`, `Arc<dyn PathInfoService>` — snix-specific trait objects that couple the orchestrator to snix's storage layer.
4. **`eos/src/fetch.rs`** imports `nix_compat::nixbase32` for hash decoding.
5. **`eos-snix`** embeds all six snix crates in-process, including store and build services that have dedicated gRPC interfaces.

The prior draft of this ADR (2026-06-03) correctly identified the scheduler → SnixEngine coupling but misdiagnosed the root cause. It proposed a `Worker` trait abstraction that decouples the scheduler from a specific engine, but if the `LocalWorker` still embeds the full snix stack in-process, the result is indirection over the same monolith. The core question is not "how does the scheduler talk to the engine" but rather: **why are we embedding snix runtime code at all, when gRPC clients would suffice for stores and builders?**

### Snix's gRPC Service Catalog

Snix defines three gRPC service domains:

| Service            | Proto Package     | Remotely Accessible | Protocol                            |
| :----------------- | :---------------- | :-----------------: | :---------------------------------- |
| `BlobService`      | `snix.castore.v1` |         ✅          | Stat, streaming Read/Put            |
| `DirectoryService` | `snix.castore.v1` |         ✅          | Streaming Get/Put                   |
| `PathInfoService`  | `snix.store.v1`   |         ✅          | Get, Put, List, CalculateNAR        |
| `BuildService`     | `snix.build.v1`   |         ✅          | Unary DoBuild                       |
| Evaluator          | —                 |         ❌          | In-process library only (by design) |

The evaluator is intentionally not a network service. It is a language interpreter that calls _out_ to stores and builders. However, with atom encapsulation, Eos can wrap evaluation as a schedulable network service — something the Nix ecosystem has never achieved because flakes lack true encapsulation.

### Architectural Forces

- **Snix is a service, not a library.** Its store and build components are designed for remote access. Embedding them as linked crates bypasses the service boundaries snix itself maintains.
- **Eos must schedule across multiple snix instances.** The daemon is a scheduler, not a frontend to a single snix installation. Scheduling requires network interfaces.
- **Atom encapsulation enables remote evaluation.** Unlike flakes (which copy entire repos), atoms are self-contained units. An eval worker can receive an atom-id, fetch the encapsulated content, and evaluate it against a remote store. No ambient repo context needed.
- **Single Responsibility.** The scheduler coordinates task distribution, deduplicates concurrent requests, tracks leases, and manages worker health. It must not execute builds, manage store paths, or run evaluations.

---

## Decision

We integrate with snix primarily through its gRPC service interfaces rather than linking its runtime code. The integration is organized into four tiers, each with a distinct rationale and coupling budget.

### Tier 1: Store Access via gRPC

The three store services (`BlobService`, `DirectoryService`, `PathInfoService`) SHALL be accessed as remote gRPC services. The eos daemon connects to one or more snix store daemons via gRPC URIs (e.g., `grpc+http://[::1]:8301`).

**Current state:** `eos-daemon/src/main.rs` calls `snix_store::utils::construct_services()` to create in-process store instances.

**Target state:** `eos-daemon` takes store URIs as configuration and connects via gRPC clients. The snix store daemon runs as an independent process.

**Rationale:** Snix's store services already have full gRPC servers with multiple backend implementations (memory, redb, Bigtable, S3, gRPC proxy). The gRPC clients implement the same Rust traits (`BlobService`, `DirectoryService`, `PathInfoService`), so the eval worker's `SnixStoreIO` works identically with remote stores.

**Coupling eliminated:** `snix-store` and `snix-castore` runtime dependencies removed from `eos-daemon` and `eos`.

### Tier 2: Build Dispatch via gRPC with Cap'n Proto Shim

Build execution SHALL be dispatched to snix builder instances via the `snix.build.v1.BuildService` gRPC protocol, wrapped in a thin Cap'n Proto shim that adds scheduling-layer concerns.

The snix build protocol is unary (`DoBuild(BuildRequest) → Build`) and lacks progress streaming, job cancellation, and lease tracking. These are Eos scheduling concerns. A thin **build worker shim** bridges the protocols:

```
┌──────────────┐  Cap'n Proto   ┌───────────────────┐  gRPC   ┌───────────────┐
│ Eos Scheduler │ ────────────→ │ Build Worker Shim  │ ──────→ │ snix Builder  │
│              │ ← progress,   │ (cancel, progress, │         │ (OCI/bwrap)   │
│              │   cancel       │  lease, dedup)     │         │               │
└──────────────┘                └───────────────────┘         └───────────────┘
```

The shim:

- Implements the Eos `BuildWorker` Cap'n Proto interface
- Translates `BuildRequest` to snix's gRPC format
- Wraps the unary gRPC call with a `CancellationToken` for abort semantics
- Reports state transitions (queued → building → done) as `ProgressStream` updates
- Heartbeats lease renewals back to the scheduler

**Coupling eliminated:** `snix-build` runtime dependency removed from `eos-daemon`. The shim is a standalone binary that depends on `snix-build` for protobuf types only.

### Tier 3: Evaluation via Remote Eval Workers

Evaluation SHALL be schedulable across a pool of remote eval workers, enabled by atom encapsulation. This is the key architectural differentiator from the Nix ecosystem.

An eval worker:

1. Receives an `EvalRequest` over Cap'n Proto (atom-id, label, expression entry point, store URIs, pre-resolved inputs)
2. Connects to a snix store daemon via gRPC (using the provided store URIs)
3. Runs `snix-eval` + `snix-glue` in-process on a dedicated OS thread (required by the `!Send` constraint)
4. Writes produced derivations back to the store via gRPC
5. Returns derivation digests to the scheduler

The existing `SandboxedEvalConfig` already takes store service addresses as strings. The eval worker connects to snix store daemons via gRPC URIs, which support multiple backend implementations. **Pointing eval workers at remote stores requires zero code changes to the evaluation path itself.**

#### Pure Evaluation and Sandboxing

Eval workers run snix in pure evaluation mode. Pure eval confines the evaluator to the atom's encapsulation boundary — it cannot import code or data external to the atom being evaluated. Content-addressed fetches (where a hash is pre-declared) are permitted because they are safe by construction. This language-level confinement eliminates the need for OS-level process sandboxing (Bubblewrap, Birdcage) during evaluation, significantly reducing operational complexity.

#### Cap'n Proto Interfaces

```capnp
interface EvalWorker {
  evaluate @0 (request :EvalRequest) -> (result :EvalResult);
  cancel @1 (jobId :Data) -> ();
  status @2 (jobId :Data) -> (state :EvalStatus);
}

interface BuildWorker {
  build @0 (request :WorkerBuildRequest) -> (result :WorkerBuildResult);
  cancel @1 (jobId :Data) -> ();
  status @2 (jobId :Data) -> (state :BuildStatus);
  attachProgress @3 (jobId :Data, callback :ProgressStream) -> ();
}
```

**Eval workers and build workers are separate services.** They have distinct resource profiles (eval is memory-bound and thread-per-eval; builds are CPU/disk-bound and semaphore-gated), distinct security profiles (eval needs atom content access; builds need sandbox capabilities), and distinct scheduling policies (no work-stealing for eval; work-stealing for builds). Operators who want both on a single machine simply run both binaries.

**Coupling budget:** `eos-snix` retains `snix-eval`, `snix-glue`, and `nix-compat` as in-process dependencies. These are required — there is no gRPC interface for evaluation, and this is by design. All store and build access from within the eval worker goes through gRPC.

### Tier 4: Type Dependencies (Compile-Time Only)

`nix-compat` provides protocol-level data structures (`Derivation`, `StorePath`, `NixHash`, `nixbase32`). These are compile-time type definitions, not runtime service dependencies. They are analogous to protobuf-generated types — necessary for serialization but not coupling to a runtime.

`nix-compat` is an acceptable dependency in `eos-snix` and in the build worker shim (for `Derivation → BuildRequest` conversion). It SHALL NOT appear in `eos-core`, `eos`, or `eos-daemon`.

### Architecture Diagram

```
                    ┌─────────────────────┐
                    │    Eos Scheduler     │
                    │   (eos-daemon)       │
                    │  Cap'n Proto RPC     │
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
    ┌─────────────────┐ ┌──────────────┐ ┌──────────────────┐
    │  Eval Worker    │ │ Eval Worker  │ │ Build Worker     │
    │  (eos-snix)     │ │ (eos-snix)   │ │ Shim             │
    │  snix-eval +    │ │              │ │ Cap'n Proto →    │
    │  snix-glue      │ │              │ │ snix gRPC        │
    └────────┬────────┘ └──────┬───────┘ └────────┬─────────┘
             │                 │                  │
             │    gRPC         │     gRPC         │  gRPC
             ▼                 ▼                  ▼
    ┌─────────────────────────────────────────────────────────┐
    │                  snix Store Daemon(s)                   │
    │  BlobService  │  DirectoryService  │  PathInfoService   │
    └─────────────────────────────────────────────────────────┘
                               │
                          ┌────┴────┐
                          │ snix    │
                          │ Builder │
                          │ (OCI/   │
                          │  bwrap) │
                          └─────────┘
```

### Dependency Delta

| Eos Crate                | Current snix deps |           Target snix deps           | Change        |
| :----------------------- | :---------------: | :----------------------------------: | :------------ |
| `eos-core`               |         0         |                  0                   | —             |
| `eos-proto`              |         0         |                  0                   | —             |
| `eos-snix` (eval worker) |         6         | 3 (snix-eval, snix-glue, nix-compat) | −3            |
| `eos` (orchestrator)     |         3         |                  0                   | −3            |
| `eos-daemon` (scheduler) |   3 + eos-snix    |                  0                   | −3 − eos-snix |
| build-worker-shim (new)  |         —         |  1 (nix-compat, for protobuf types)  | new crate     |

### Deployment Model

The default deployment uses **process composition** (e.g., `process-compose`) to start all required services for local development with a single command. The repository provides a canonical composition file.

Eos does NOT manage the lifecycle of snix store daemons, builders, or eval workers. Lifecycle management is delegated to the operator's chosen orchestrator (process-compose for development, systemd for production, k8s for clusters).

### Scheduler Integration

The scheduler manages two worker pools with separate registries:

- **Eval worker pool**: Dynamic registration via Cap'n Proto handshake. Rendezvous hashing for routing. Deduplication via `compute_eval_cache_key()` (BLAKE3 hash of normalized request). No work-stealing (eval durations have low variance).
  - **Metadata**: `ifdSystems: List<Text>` (systems the worker's IFD builders handle; empty if no IFD), `speedFactor: UInt32` (relative priority).

- **Build worker pool**: Dynamic registration via Cap'n Proto handshake. Rendezvous hashing with cache affinity. Deduplication via `plan_digest()`. Work-stealing enabled for load balancing across heterogeneous machines.
  - **Metadata**: `systems: List<Text>` (hard predicate — derivation system ∈ worker systems), `supportedFeatures: List<Text>` (hard predicate — derivation required features ⊆ worker features), `speedFactor: UInt32`.

- **`maxConcurrency`** is operational config (admission control), not a scheduling predicate. It limits concurrent job slots per worker but does not influence job routing.

Both pools use the same lease-based health monitoring defined in the [scheduler spec](../specs/eos-scheduler.md). Workers send periodic heartbeats to the scheduler via `registration.heartbeat()` on the `Registration` capability returned at registration time (worker→scheduler keepalive model). The scheduler tracks `last_heartbeat` per worker and marks workers unhealthy if the deadline is exceeded.

---

## Consequences

### Positive

- **True decoupling.** The scheduler has zero snix dependencies. It speaks Cap'n Proto to abstract worker interfaces.
- **Multi-instance scheduling.** Eos can dispatch eval and build jobs across any number of snix instances on disparate machines.
- **Eval parallelism.** Large batches of atom evaluations are distributed across an eval worker pool, eliminating the single-evaluator bottleneck that plagues Nix deployments.
- **Independent scaling.** Eval worker count and build worker count are independent knobs. A cluster can run 32 eval workers and 8 builders, or vice versa.
- **Stateless scheduler.** The scheduler holds only ephemeral in-flight state. The artifact store (snix blob service) is the durable source of truth. External orchestrators can swap scheduler instances freely.
- **No eval sandboxing overhead.** Pure eval provides language-level confinement. No Bubblewrap, Birdcage, or namespace management needed for eval workers.
- **Clean testing.** The scheduler is testable with mock Cap'n Proto workers. No snix installation needed.
- **Global shared artifact store.** All workers and builders in the cluster use the same network store. Artifacts accumulated anywhere are instantly available cluster-wide, eliminating redundant builds.

### Negative

- **Operational complexity.** Local development requires starting multiple processes (store daemon, eval worker, builder, scheduler). Mitigated by process-compose.
- **Latency.** gRPC store access adds network round-trips compared to in-process calls. Negligible for builds (which dwarf store latency) but measurable for eval-heavy workloads with many store lookups. Mitigated by deploying eval workers co-located with store daemons.
- **Two wire formats.** Eos uses Cap'n Proto; snix uses gRPC/protobuf. The build worker shim bridges them. This is intentional — Eos's protocol carries scheduling semantics (capabilities, progress streaming, cancellation) that snix's protocol does not.

### Risks Accepted

- **gRPC compatibility.** We depend on snix's gRPC interface stability. The MIT-licensed protos suggest this is intended as a stable contract, but snix has not formally committed to API versioning. Mitigation: pin to a specific snix commit (already done via `eos-patches` branch).
- **Eval worker complexity.** The eval worker still links snix-eval, snix-glue, and nix-compat. This is the tightest coupling remaining. Mitigation: the eval worker is a separate binary with a focused responsibility; changes to snix-eval affect only this binary.

---

## Deferred Decisions

### ADR-0003 (proposed): Import-from-Derivation Topology

IFD is an internal concern of the snix evaluator — the Eos scheduler is not aware of IFD builds. Eval workers handle IFD internally via `SnixStoreIO`'s `BuildService` gRPC handle. Operators configure IFD builder topology:

1. **Shared cluster builders**: IFD builds dispatch to the same snix builders used for top-level builds (simple, may cause contention)
2. **Dedicated IFD builders**: Reserved builder instances handle only IFD builds (isolates IFD load)

The critical invariant: IFD build outputs MUST populate the global cluster artifact store, making results available cluster-wide.

Snix handles IFD asynchronously — unlike the Nix C++ implementation which blocks entirely, snix can continue evaluating other derivations while waiting for an IFD build, making the overall cost less severe.

An ADR-0003 MAY still be warranted to formalize IFD topology recommendations, monitoring of IFD builds outside the scheduler's visibility, and interaction with eval cache invalidation.

### ADR-0004 (proposed): Eos Caching and High Availability

Two interrelated concerns deferred from this ADR:

1. **Eval/build result caching.** Atom metatags (cryptographically signed by the atom owner) can declare derivation digests, providing a distributed, decentralized cache. For third-party evaluations, an Eos-level cache backed by the snix blob service provides the fallback. The interface design (latency requirements, cache invalidation, consistency model) warrants dedicated analysis.

2. **High availability.** The stateless scheduler design enables external orchestrators to swap instances, but the transition semantics (in-flight job recovery, worker re-registration, lease handoff) need specification.

---

## Related Documents

- [Eos Software Architecture Document](../architecture/eos-sad.md) — the comprehensive architecture blueprint that this ADR supports
- [Eos Scheduler Specification](../specs/eos-scheduler.md)
- [Eos Snix Backend Specification](../specs/eos-snix-backend.md)
- [Eos Network Protocol Specification](../specs/eos-network-protocol.md)
- [Ion–Eos Contract](../specs/ion-eos-contract.md)

---

## Alternatives Considered

### Alternative 1: Embedded Snix with Worker Trait Abstraction (Prior ADR-0002 Draft)

Decouple the scheduler from `SnixEngine` via a `Worker` trait, but keep snix services embedded in-process within a `LocalWorker`.

- **Why reconsidered**: This addresses the scheduler → engine coupling but not the fundamental issue. The `LocalWorker` still embeds the full snix stack (6 crates). The orchestrator (`eos/src/bridge.rs`) still holds snix service trait objects. The daemon still cannot schedule across multiple snix instances because "local" and "remote" are architecturally different paths. The Worker trait is retained in the revised design but applied to Cap'n Proto clients, not in-process engines.

### Alternative 2: Direct snix gRPC with No Eos Protocol Layer

Have the scheduler speak snix's gRPC protocols directly to stores and builders, without the Cap'n Proto shim.

- **Why rejected**: Snix's gRPC build protocol is unary and lacks progress streaming, job cancellation, lease tracking, and deduplication. These are essential scheduling concerns. Additionally, Eos needs sovereign authentication on all daemon-to-worker communication, which is handled at the Cap'n Proto capability level. Adopting snix's gRPC as the scheduler's wire format would require extending snix's protocol with Eos-specific features — a poor separation of concerns.

### Alternative 3: Monolithic `ion` Binary with Embedded Services

Compile all services (scheduler, eval worker, builder, store) into a single `ion` binary via feature flags, eliminating the need for process composition.

- **Why deferred**: Attractive for developer ergonomics but premature. It would create two integration paths (monolithic and microservice) before either is mature, doubling the testing and maintenance surface. It also violates layer discipline (L3 embedding L2 internals). May be revisited once the gRPC-first architecture is proven, as an optional convenience feature.

### Alternative 4: Custom Store Protocol (Cap'n Proto for Stores)

Define Eos-native store interfaces in Cap'n Proto rather than using snix's gRPC store protocols.

- **Why rejected**: Snix's store protocols are mature, production-tested (Replit runs tvix-store at scale: 6TB → 1.2TB via content-addressed dedup), and MIT-licensed for exactly this purpose. Reimplementing them in Cap'n Proto would duplicate effort without clear benefit. If future store requirements diverge (e.g., Cyphr/Coz-native storage), a new store backend can implement snix's existing gRPC interfaces rather than requiring a protocol change.
