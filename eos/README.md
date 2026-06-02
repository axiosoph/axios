# Eos: Axios Build Engine, Store, and Runtime (L2)

Eos is the intermediate execution and orchestration layer (L2) of the Axios decentralized publishing stack. It serves as the bridge reconciling the developer-facing declarations in **Ion** (L3) with the content-addressed cryptographic primitives in **Atom** (L1).

```
          ┌────────────────────────────────────────┐
          │               L3  ion                  │ (CLI, manifests, resolution)
          └──────────────────┬─────────────────────┘
                             │ Cap'n Proto RPC (UDS)
                             ▼
          ┌────────────────────────────────────────┐
          │               L2  eos                  │ (Build engine, stores, runtime)
          └──────────────────┬─────────────────────┘
                             │ Local path dependency
                             ▼
          ┌────────────────────────────────────────┐
          │               L1  atom                 │ (Protocol, identity, addressing)
          └────────────────────────────────────────┘
```

---

## Role in the Axios Stack

Eos coordinates the conversion of static source snapshots into verified, built artifacts. It mediates the boundary between:

- **L3 (Ion)**: The planning layer. Ion parses manifests, computes dependency graphs, writes lock files, and transmits evaluation and execution requests to the Eos daemon.
- **L1 (Atom)**: The identity layer. Atom verifies sovereign signatures, maps human-readable labels to content-addressed digests, and manages Git-based snapshot retrieval.

Eos ingests resolved target requests, fetches the necessary source snapshots from Atom, and evaluates or executes them inside strictly containerized sandboxes to materialize hermetic outputs.

To maximize throughput and minimize redundant work, Eos deploys a two-tier caching architecture:

1. **Evaluation Caching**: Maps the input source snapshot's cryptographic digest combined with the evaluation arguments (`EvalCacheKey`) to a pre-computed build recipe (**Plan**). If the inputs match, the evaluation phase is bypassed.
2. **Build Caching**: Maps the content-addressed digest of the computed **Plan** to registered, read-only outputs (**Artifacts**) in the store. If the plan already corresponds to an existing artifact, build execution is skipped.

---

## Crate Architecture and Layer Discipline

The Eos workspace is decomposed into six crates. Dependencies flow strictly downward, preserving strict layer discipline:

```
           ┌──────────────┐
           │     eos      │ (Orchestrator)
           └──────┬───────┘
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
  ┌──────────┐        ┌──────────┐
  │eos-daemon│        │ eos-snix │ (Concrete Nix backend)
  └─────┬────┘        └─────┬────┘
        │   ┌───────────────┘
        ▼   ▼
  ┌──────────┐
  │eos-proto │ (Cap'n Proto schemas & codegen)
  └─────┬────┘
        ▼
  ┌──────────┐
  │eos-store │ (Store ingestion pipelines)
  └─────┬────┘
        ▼
  ┌──────────┐
  │ eos-core │ (Domain traits & core types)
  └──────────┘
```

### 1. `eos-core`

The foundational layer defining the L2 interface boundaries. It contains no backend-specific code. It declares:

- Primary domain traits: `BuildEngine`, `ArtifactStore`, and `AtomIndex`.
- Fundamental types: `Digest` (abstract trait implemented concretely by `Blake3Digest`), `StorePath` (strongly typed path references), `AtomRef`, `JobStatus`, and `ProgressEvent`.
- Async ergonomics: Employs native `async fn` in traits (Rust edition 2024) via `trait_variant::make` to generate thread-safe (`Send`) signatures without heap allocation overhead.

### 2. `eos-store`

Implements the storage interface, handling the binary ingestion pipeline, content verification, and file layout mechanics.

### 3. `eos-proto`

Defines the wire format protocol schemas (`eos.capnp`) and handles the compilation of Cap'n Proto schemas into Rust bindings at build time.

### 4. `eos-snix`

The concrete implementation of `BuildEngine` and `ArtifactStore` backed by the Snix suite. It integrates `nix-compat`, `snix-castore`, `snix-store`, `snix-eval`, and `snix-build`. It encapsulates the thread-locality constraint of the Nix language evaluator (`snix-eval` types contain `Rc<Closure>` and are `!Send`) by executing evaluations inside isolated worker threads.

### 5. `eos-daemon`

Hosts the `eosd` server executable. It parses CLI configurations, binds to the Unix Domain Socket (UDS) transport, runs the `!Send` Cap'n Proto RPC server loop inside a `LocalSet` thread, and dispatches long-running build tasks to a thread pool managed by the `Scheduler`. It also exposes the hidden `--eval-worker` subcommand used for evaluation isolation.

### 6. `eos`

The top-level coordination crate. It ties the scheduler, index, and backend services together, materializing the complete orchestrator for the local node.

---

## Evaluation Sandboxing

Evaluating expressions (such as Nix/Snix code) is a dangerous operation because evaluation can trigger arbitrary local file reads (`builtins.readFile`) or shell execution. To guarantee hermeticity and prevent leakage of host system impurities, Eos isolates the evaluator using platform-native containerization.

### The Subprocess Worker Model

Instead of evaluating expressions directly inside the main `eosd` daemon process, the daemon forks itself:

1. The daemon resolves the path to its own binary using `std::env::current_exe()`.
2. It spawns the binary as a child subprocess, passing the hidden subcommand `--eval-worker` along with service connection parameters.
3. The daemon serializes the evaluation parameters (`EvalRequestDto`) as a JSON payload over the child's standard input (`stdin`).
4. The worker executes the evaluation within a sandbox and prints the computed **Plan** (serialized as Nix-compatible ATerm derivation bytes) to standard output (`stdout`).
5. The daemon reads stdout, registers the plan in the evaluation cache, and terminates the worker.

### Platform Isolation Dispatch

- **Linux (Bubblewrap)**: Spawns the worker wrapped in `bwrap`. The sandbox disables network access (omits `--share-net`), unshares namespaces, maps the Axios workspace directory and the evaluated files as read-only mounts (`--ro-bind`), and restricts write access to the temporary workspace and the local database sockets (`--bind`).
- **macOS (Birdcage)**: Spawns the worker within a blocking task using the `birdcage` library. It applies macOS Seatbelt sandbox policies, whitelisting read-only access to the daemon binary and workspace directories, permitting read/write access to Unix socket paths, and denying network connections and other filesystem modifications.

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
- **`ProgressStream`**: A callback interface implemented by the client. The daemon streams status transitions (`queued`, `evaluating`, `building`, `completed`, `failed`, `cancelled`) through the `update` method.
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
