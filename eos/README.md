# Eos: Axios Build-Scheduling Engine (L3)

Eos is the atom-DAG scheduling layer (L3) of the Axios decentralized
publishing stack. It serves as the bridge reconciling the developer-facing
declarations in **Ion** (L4) with the build-execution contract owned by
**HTC** (L2) and the content-addressed cryptographic primitives in **Atom**
(L1).

```
          ┌────────────────────────────────────────┐
          │               L4  ion                  │ (CLI, manifests, resolution)
          └──────────────────┬─────────────────────┘
                             │ Cap'n Proto RPC (UDS)
                             ▼
          ┌────────────────────────────────────────┐
          │               L3  eos                  │ (Atom-DAG scheduling)
          └──────────────────┬─────────────────────┘
                             │ dispatch: build(...) via HTC's executor trait
                             ▼
          ┌────────────────────────────────────────┐
          │               L2  HTC                  │ (Build execution, CAS, composition)
          └──────────────────┬─────────────────────┘
                             │ Local path dependency
                             ▼
          ┌────────────────────────────────────────┐
          │               L1  atom                 │ (Protocol, identity, addressing)
          └────────────────────────────────────────┘
```

> [!NOTE]
> **Re-scope in progress.** This crate's implementation predates the
> atom-DAG re-scope decided in [ADR-0005](../docs/adr/0005-hermetic-transactional-composition.md).
> Ground truth for build execution and composition now lives in
> [htc-sad.md](../docs/architecture/htc-sad.md) (L2/HTC) and
> [eos-sad.md](../docs/architecture/eos-sad.md) (L3/Eos) — spec-first
> doctrine treats this codebase as throwaway relative to those documents,
> and it is being realigned incrementally. Where this README and those SADs
> disagree, the SADs win.

---

## Role in the Axios Stack

Eos coordinates the conversion of a resolved atom-DAG into verified, built
artifacts. It mediates the boundary between:

- **L4 (Ion)**: The planning layer. Ion parses manifests, resolves a
  dependency graph, and hands eos a pre-coarsened atom-DAG read directly off
  the lock — nodes are atoms identified by `publish_czd`, edges are the
  dependency relationships already resolved into the lock.
- **L1 (Atom)**: The identity layer. Atom verifies sovereign signatures, maps
  human-readable labels to the `(anchor, label)` identity pair, and manages
  Git-based snapshot retrieval.

Eos does not build; building is HTC's (L2) contract. Eos dispatches build
actions to executor workers implementing HTC's executor trait — the primary
executor materializes the declared atom closure and toolchain composition
into a sandboxed FHS view and runs upstream's own, unmodified build process
against it; the optional legacy passthrough-snix executor exists only for
interoperating with pre-existing Nix-expression content. Workers write their
output tree and its derived interface manifest into HTC's shared,
content-addressed artifact store and return a `BuildRecord` to the daemon.

To avoid redundant work, Eos maintains a single **action-id cache**:

```text
action_id = H( atom_czd_closure_root        // what to build (signed intent)
             , toolchain_composition_root   // what to build WITH
             , action_params )              // target system, variant flags
```

Before dispatching an action to a worker, the scheduler computes `action_id`
and checks the cache. If the key has already been built, the cached
`BuildRecord` — and its output tree in the shared store — is returned
without ever contacting a worker.

---

## Crate Architecture and Layer Discipline

The Eos workspace is decomposed into five crates. Dependencies flow strictly downward, preserving strict layer discipline:

```
           ┌──────────────┐
           │     eos      │ (Orchestrator)
           └──────┬───────┘
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
  ┌──────────┐        ┌──────────┐
  │eos-daemon│        │ eos-snix │ (Legacy Nix-compat executor)
  └─────┬────┘        └─────┬────┘
        │   ┌───────────────┘
        ▼   ▼
  ┌──────────┐
  │eos-proto │ (Cap'n Proto schemas & codegen)
  └─────┬────┘
        ▼
  ┌──────────┐
  │ eos-core │ (Domain traits & core types)
  └──────────┘
```

### 1. `eos-core`

The foundational layer defining the L3 interface boundaries. It contains no backend-specific code. It declares:

- Primary domain traits: `BuildEngine`, `ArtifactStore`, and `AtomIndex`.
- Fundamental types: `Digest` (abstract trait implemented concretely by `Blake3Digest`), `StorePath` (strongly typed path references), `AtomRef`, `JobStatus`, and `ProgressEvent`.
- Async ergonomics: Employs native `async fn` in traits (Rust edition 2024) via `trait_variant::make` to generate thread-safe (`Send`) signatures without heap allocation overhead.

### 2. `eos-proto`

Defines the wire format protocol schemas (`eos.capnp`) and handles the compilation of Cap'n Proto schemas into Rust bindings at build time.

### 3. `eos-snix` (optional legacy executor)

Wraps HTC's optional passthrough-snix executor for interoperating with pre-existing Nix-expression content — not the default backend (see [htc-sad.md](../docs/architecture/htc-sad.md) §6.8). It integrates `nix-compat`, `snix-castore`, `snix-store`, `snix-eval`, and `snix-build`, encapsulating the thread-locality constraint of the Nix language evaluator (`snix-eval` types contain `Rc<Closure>` and are `!Send`) by running its evaluation in isolated worker threads. Full detail: [eos-snix-backend.md](../docs/specs/eos-snix-backend.md).

### 4. `eos-daemon`

Hosts the `eosd` server executable: the scheduler, its executor worker pool, and the Cap'n Proto RPC server that dispatches build actions read off the atom-DAG to executor workers, and maintains the action-id cache (`action_id → BuildRecord`, §Role in the Axios Stack above) so dispatch is skipped for previously-built actions.

### 5. `eos`

The top-level coordination crate. It ties the scheduler, index, and backend services together, materializing the complete orchestrator for the local node.

---

## Build Sandboxing

Building is delegated entirely to executor workers implementing HTC's
executor trait: `build(atom_closure, toolchain_composition, action_params)
→ output tree` (htc-sad.md §3.5). The daemon and its scheduler perform
**zero** sandboxing and hold no opinion on how a given executor isolates
its work — isolation is wholly the executor implementation's concern
(htc-sad.md §6.2).

### The Primary FHS Executor

The primary executor materializes a composed FHS view from
content-addressed trees (the atom closure plus the toolchain composition)
and runs upstream's own, unmodified build process against it, under
OCI/bwrap sandboxing. The sandbox is deny-by-default: the only bytes a
build process can read are those declared in the atom closure and
toolchain composition, plus whatever the fetch proxy explicitly permits.
The build's observed read set is checked against the declared closure —
reads ⊆ declared — and that containment is enforced by the sandbox, not
trusted from the build's own behavior (`[htc-declared-closure-enforced]`,
htc-sad.md §1.1). On success, the executor writes the output tree and its
derived interface manifest to the shared CAS and returns a `BuildRecord`
(`action_id`, `output_tree_digest`, `build_composition_root`,
`observed_read_set_digest`, builder signature) to the daemon.

### Legacy: the Passthrough-Snix Executor

For interoperating with pre-existing Nix-expression content only, the
optional legacy passthrough-snix executor links `snix-eval`/`snix-glue`
in-process to run a Nix expression's own build process unmodified,
confined to whatever isolation `snix-eval` itself provides upstream. This
is legacy-executor-internal detail, not part of the scheduler's contract —
see [eos-snix-backend.md](../docs/specs/eos-snix-backend.md).

### Platform Isolation Dispatch

- **Linux (Bubblewrap)**: The sandbox disables network access
  (`--unshare-net`) except through HTC's record/replay fetch proxy,
  unshares namespaces (user, PID, UTS, IPC), maps the composed FHS view
  and toolchain trees as read-only mounts (`--ro-bind`), and restricts
  write access to the build's temporary workspace and local database
  directories (`--bind`). Host configuration directories like `/etc` are
  deliberately excluded to prevent impurity.
- **macOS (Birdcage)**: Applies macOS Seatbelt sandbox policies via the
  `birdcage` library, whitelisting read-only access to the composed view
  and workspace directories, permitting read/write access to database
  paths, and denying network connections and other filesystem
  modifications outside the sandbox.

---

## Cap'n Proto RPC Protocol

Eos communicates with client frontends (Ion) using Cap'n Proto RPC over a Unix Domain Socket (UDS) transport. This capability-based model provides unforgeable references to active jobs and discovery interfaces.

### Core Capabilities

- **`EosDaemon`**: The bootstrap capability. Exposes:
  - `submitBuild(planDigest, evalArgs)`: Submits a build plan. Employs `JobId` deduplication (returns a reference to an active job if one exists).
  - `queryStatus(jobId)`: Obtains the status of a specific job.
  - `getCapabilities()`: Queries supported compiler backends and API versions.
  - `discover()`: Returns the `AtomDiscovery` capability.
- **`BuildJob`**: A reference to an active build task. Exposes:
  - `attachProgress(callback)`: Attaches a `ProgressStream` callback to receive real-time updates.
  - `cancel()`: Aborts the active build task.
  - `getJobId()`: Retrieves the cryptographic identifier of the job.
- **`ProgressStream`**: A callback interface implemented by the client. The daemon streams status transitions (`queued`, `fetching`, `building`, `completed`, `failed`, `cancelled`) through the `update` method.
- **`AtomDiscovery`**: A read-only interface to query Eos's accumulated knowledge of observed atoms:
  - `resolve(id)`: Resolves metadata for a specific atom.
  - `contains(id)`: Fast membership check.
  - `search(query)`: Searches the local index by label patterns or set filters.

---

## Building, Checking, and Testing

All workspace commands must be executed from the `eos` workspace root.

### Prerequisites

- Rust toolchain version `1.90.0` or newer (configured automatically via the workspace `rust-toolchain.toml`).
- On Linux: Bubblewrap (`bwrap`) must be installed on the system `$PATH`.

### Commands

- **Compile the workspace**:
  ```bash
  cargo build
  ```
- **Run tests**:
  ```bash
  cargo test
  ```
- **Execute linter (Clippy)**:
  ```bash
  cargo clippy --all-targets -- -D warnings
  ```
- **Verify code formatting**:
  ```bash
  cargo fmt --check
  ```
- **Run the daemon**:
  ```bash
  cargo run --bin eosd -- --socket-path /tmp/eos.sock
  ```
