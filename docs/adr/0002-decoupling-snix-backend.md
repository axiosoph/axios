# ADR-0002: Decoupling Snix Backend from Eos Core and Scheduler

- **Status**: PROPOSED
- **Date**: 2026-06-03
- **Deciders**: nrd
- **Source**: [Eos Build Scheduler Specification](../specs/eos-scheduler.md) | [Eos Snix Backend Specification](../specs/eos-snix-backend.md)
- **Supersedes**: None

---

**Document Classification**: Explanation (Architecture Decision Record)
**Audience**: Architects, Core Developers

---

## Context

The core scheduling and ingestion engine in `eos` is currently coupled to the `eos-snix` backend.

Currently, `eos-daemon` and `Scheduler` (`eos-daemon/src/scheduler.rs`) import and reference `eos_snix::SnixEngine`, `snix_store`, and `snix_build` directly. To initialize the content bridge (`CastoreBridge`) and path ingestion service (`SnixIngestService`), the scheduler must unpack `SnixEngine` to extract its inner services (`blob_service`, `directory_service`, `path_info_service`, `nar_calculation_service`). Additionally, the scheduler runs the orchestrated build pipeline locally in a task spawned on the daemon's local runtime.

This design introduces three major architectural problems:

1. **Layer Violations**: The scheduling engine in Layer 2 (L2) depends on a concrete L1/L2 bridge implementation (`eos-snix`).
2. **Execution Rigidity**: The scheduler cannot delegate build and evaluation tasks to remote workers, cluster nodes, or alternative engines (e.g., Docker, Cargo, Bazel).
3. **Storage Leakage**: The scheduler is exposed to storage-specific service traits and FUSE mounting mechanisms.

### Architectural Forces

- **Single Responsibility**: The scheduler coordinates task distribution, deduplicates concurrent requests, tracks leases, and manages worker health. It must not execute builds or manage store paths.
- **Worker Diversity**: Eos must support both local execution (concurrency-capped thread pools) and remote execution (delegating builds over Cap'n Proto RPC to remote daemons) using a unified interface.
- **Storage Abstraction**: Ingestion from Git (`AtomContentBridge`) or remote URLs (`ContentIngestService`) depends on the specific storage format (such as Snix castore, S3, or OCI registry). The scheduler must remain agnostic to the storage format.

---

## Decision

We decouple the scheduler and core engine from Snix by introducing a `Worker` execution abstraction and a store-directed ingestion extension.

```
                  ┌──────────────────────┐
                  │      Scheduler       │
                  └──────────┬───────────┘
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
   ┌─────────────────┐               ┌─────────────────┐
   │  LocalWorker    │               │  RemoteWorker   │
   └────────┬────────┘               └────────┬────────┘
            │                                 │
     (Local Services)                  (Cap'n Proto RPC)
            │                                 │
            ▼                                 ▼
   ┌─────────────────┐               ┌─────────────────┐
   │   SnixEngine    │               │  Remote Daemon  │
   └─────────────────┘               └─────────────────┘
```

1. **Decoupled Scheduler**: The `Scheduler` will interact only with abstract traits defined in `eos-core`. It will manage a registry of `Worker` instances and route jobs to workers using Highest Random Weight (Rendezvous) hashing.
2. **Worker Interface**: A new `Worker` trait will represent an execution worker. A `LocalWorker` wraps the local `BuildEngine` and `ArtifactStore`. A `RemoteWorker` wraps the Cap'n Proto RPC client connection.
3. **Ingestion Interface**: We isolate store-specific ingestion by adding factory methods to `ArtifactStore` via a new extension trait (`ArtifactStoreIngestExt`).

### Rust Trait Signatures

The new interfaces are defined in `eos-core`:

```rust
use std::sync::Arc;
use atom_id::AtomId;
use crate::digest::Digest;
use crate::job::{ArtifactInfo, JobId, ProgressEvent};
use crate::request::BuildRequest;
use crate::store::{ArtifactStore, StorePath};

/// A worker node capable of executing build and evaluation jobs.
#[trait_variant::make(Send)]
pub trait Worker: Send + Sync + 'static {
    /// The digest algorithm used by this worker.
    type Digest: Digest;

    /// The structured error type returned by worker operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the unique cryptographic identity of the worker.
    fn identity(&self) -> Self::Digest;

    /// Checks which of the requested store paths are cached on the worker.
    async fn check_cache(
        &self,
        paths: &[StorePath],
    ) -> Result<Vec<bool>, Self::Error>;

    /// Submits a build request to the worker.
    ///
    /// Progress updates must be emitted to the provided broadcast sender.
    async fn submit_build(
        &self,
        job_id: JobId<Self::Digest>,
        request: BuildRequest<Self::Digest>,
        progress_tx: tokio::sync::broadcast::Sender<ProgressEvent<Self::Digest>>,
    ) -> Result<Vec<ArtifactInfo<Self::Digest>>, Self::Error>;

    /// Cancels a running build task on this worker.
    async fn cancel_build(
        &self,
        job_id: JobId<Self::Digest>,
    ) -> Result<(), Self::Error>;
}
```

```rust
/// Extension trait for [`ArtifactStore`] providing ingestion capabilities.
///
/// Implemented by the storage backend to permit ingestion of atoms and files
/// directly into the store without exposing internal service handles.
pub trait ArtifactStoreIngestExt: ArtifactStore {
    /// Constructs an [`AtomContentBridge`] backed by this storage engine.
    fn create_atom_bridge(
        &self,
        source: Arc<dyn atom_core::AtomSource>,
    ) -> Arc<dyn eos_core::bridge::AtomContentBridge<Digest = Self::Digest, Error = Self::Error>>;

    /// Constructs a [`ContentIngestService`] backed by this storage engine.
    fn create_path_ingester(
        &self,
    ) -> Arc<dyn eos_core::ingest::ContentIngestService<Digest = Self::Digest, Error = Self::Error>>;
}
```

### Delegation and Execution Model

When a build request is submitted to the scheduler:

1. The scheduler computes the `JobId` and deduplicates the request.
2. The scheduler queries the registered workers' caches via `check_cache` and routes the job using Rendezvous hashing.
3. The scheduler transitions the job state to `RUNNING` and issues a lease.
4. The scheduler calls `worker.submit_build(...)`.
   - **Local Delegation**: `LocalWorker` instantiates `AtomContentBridge` and `ContentIngestService` via the local store's `ArtifactStoreIngestExt`. It opens the local git repository and executes the orchestrated build pipeline (`run_orchestrated_build`) on a background thread.
   - **Remote Delegation**: `RemoteWorker` converts the `BuildRequest` to the Cap'n Proto RPC format and transmits it over the network to the remote worker. The remote daemon's RPC handler receives the request, submits it to its own local scheduler, and streams progress updates back.

---

## Consequences

Decoupling establishes a clean boundary between task distribution and build execution.

### Positive

- **Architectural Purity**: `eos-daemon` and `Scheduler` have no dependencies on `eos-snix` or Snix crates.
- **Extensibility**: Alternative storage engines and build sandboxes can be introduced by implementing `ArtifactStore`, `ArtifactStoreIngestExt`, and `BuildEngine`.
- **Distributed Ready**: The scheduler treats local threads and remote network daemons identically, enabling seamless horizontal scaling and work-stealing.
- **Improved Testing**: The scheduler can be unit-tested using mock workers and mock stores, without requiring a real Snix installation or FUSE sandbox.

### Negative

- **Trait Scaffolding**: Proliferation of generic parameters and trait objects (`Arc<dyn Worker>`) in the scheduler.
- **Wiring Complexity**: The startup sequence in `eos-daemon/src/main.rs` must explicitly register the `LocalWorker` with the scheduler.

### Risks Accepted

- **RPC Serialization Overhead**: Converting build requests and streaming progress events over Cap'n Proto introduces minor latency, though negligible compared to compilation times.
- **Lease Synchronization**: Disagreements between the scheduler lease duration and remote worker execution limits could cause premature task revoking if network latency is high.

---

## Alternatives Considered

### Alternative 1: Monolithic In-Process Scheduler

Keep `SnixEngine` built into the scheduler.

- **Why Rejected**: This design prevents remote worker scale-out. It forces every daemon in the cluster to mount FUSE sandboxes and evaluate Nix code locally, violating the requirements for a multi-worker cluster.

### Alternative 2: Direct gRPC Delegation

Have the scheduler connect directly to Snix gRPC workers (e.g. `GRPCBuildService`) rather than Eos workers.

- **Why Rejected**: Snix gRPC is a low-level, Nix-specific building protocol that lack capabilities for progress streaming, job cancellation, evaluation, or lease tracking. Bypassing the Eos control plane prevents Eos from enforcing sovereign authentication and job state management.
