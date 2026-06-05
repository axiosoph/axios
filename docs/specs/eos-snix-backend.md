# SPEC: Eos Snix Backend

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
  See: eos-build-engine.md for the general BuildEngine/ArtifactStore contracts
       this backend implements.
  See: docs/models/publishing-stack-layers.md for the algebraic domain model.
-->

## Domain

**Problem Domain:** This document specifies how the `eos-snix` crate implements the Nix evaluation capability for the Eos stack using the Snix evaluator libraries (`snix-eval`, `snix-glue`, `nix-compat`). Under the gRPC-first integration architecture (see [ADR-0002](../adr/0002-decoupling-snix-backend.md)), `eos-snix` powers **eval workers** — separate processes that accept evaluation requests over Cap'n Proto, run `snix-eval` in-process, and connect to snix store daemons via gRPC for all store operations.

`eos-snix` does NOT embed snix store services (`snix-castore`, `snix-store`) or build services (`snix-build`) in-process. Store access uses gRPC clients that implement the same Rust traits (`BlobService`, `DirectoryService`, `PathInfoService`), so the evaluator's `SnixStoreIO` wiring is unchanged. Build execution is dispatched by the Eos scheduler to separate build worker shims via Cap'n Proto.

The Snix crates relevant to `eos-snix` are: a Nix-compatible data layer (`nix-compat`), an evaluator (`snix-eval`), and a glue layer (`snix-glue`) that wires the evaluator to store service trait objects. Each imposes distinct concurrency constraints, error semantics, and type boundaries that `eos-snix` must accommodate.

The central impedance mismatch is the evaluator's `!Send`/`!Sync` threading model: `snix-eval` uses `Rc<Closure>` internally, rendering all evaluation types thread-local. Store services (accessed via gRPC) are fully `async + Send + Sync`. The `eos-snix` bridge MUST reconcile this split without leaking the `!Send` constraint into the eval worker's Cap'n Proto interface.

**Model Reference:**

- [eos-build-engine.md](eos-build-engine.md) — General `BuildEngine`/`ArtifactStore` trait contracts
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore)
- [ADR-0002](../adr/0002-decoupling-snix-backend.md) — gRPC-first snix integration architecture

**Criticality Tier:** High — this backend is the sole initial implementation. Threading violations cause unsoundness; store mapping errors cause data corruption; sandbox misconfiguration compromises host isolation.

---

## Constraints

### Type Declarations

The following type mappings bridge `eos-core` abstractions to concrete Snix types. `eos-core` MUST NOT depend on any Snix crate; all conversions live in `eos-snix`.

```
-- eos-core → eos-snix concrete types

TYPE SnixEngine : BuildEngine
  where Plan   = nix_compat::derivation::Derivation
  where Output = SnixOutput { path_info: PathInfo, node: Node }
  where Error  = SnixError (structured, wraps io::Error and eval errors)

TYPE SnixStore : ArtifactStore
  delegates to BlobService + DirectoryService + PathInfoService

-- Primitive type mappings (bidirectional From/Into)
-- eos_core::Digest is a trait; eos-snix selects the concrete implementation.

TYPE Blake3Digest : Digest          -- [u8; 32] newtype, Copy, #[repr(transparent)]
  where eos-snix sets `type Digest = Blake3Digest`

MAP Blake3Digest                   ↔  snix_castore::B3Digest
    -- Both [u8; 32] newtypes, #[repr(transparent)]. Zero-cost conversion.

MAP eos_core::StorePath           ↔  nix_compat::store_path::StorePath<String>
    -- eos_core::StorePath wraps a String; nix_compat::StorePath
    -- provides validation and the 20-byte store hash.

MAP BuildEngine::Plan             ↔  nix_compat::derivation::Derivation
    -- The Derivation is the concrete plan type.

MAP BuildEngine::Output           ↔  (snix_store::pathinfoservice::PathInfo,
                                      snix_castore::Node)
    -- PathInfo carries store metadata (references, deriver, NAR hash);
    -- Node carries the content-addressed filesystem tree root.

-- Supporting types (Snix-internal, used by eos-snix only)

TYPE EvalThread                   -- Dedicated OS thread running snix-eval
TYPE EvalChannel                  -- oneshot/mpsc channel bridging eval → async
TYPE BuildRequestAdapter          -- Derivation → BuildRequest converter
```

#### Type Mapping Table

| `eos-core` Type              | Snix Type                                                    | Conversion                                                                                       | Notes                                                                                                                     |
| :--------------------------- | :----------------------------------------------------------- | :----------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------ |
| `Digest` (trait)             | `snix_castore::B3Digest`                                     | `From`/`Into` on `Blake3Digest` (zero-cost, both `[u8; 32]` newtypes via `#[repr(transparent)]`) | `eos-snix` binds `type Digest = Blake3Digest`. `Blake3Digest` implements `Copy` because BLAKE3 output is always 32 bytes. |
| `StorePath`                  | `nix_compat::store_path::StorePath<String>`                  | `TryFrom`/`Into` (`TryFrom` validates format)                                                    | Snix `StorePath` enforces the `<hash>-<name>` structure                                                                   |
| `BuildEngine::Plan`          | `nix_compat::derivation::Derivation`                         | Identity (associated type binding)                                                               | `Derivation` is `Send + Sync + Clone + Debug`                                                                             |
| `BuildEngine::plan_digest()` | —                                                            | `SnixEngine::plan_digest()` computes `Blake3Digest` of the `Derivation`'s ATerm serialization    | This mirrors Nix/Snix derivation hashing: `BLAKE3(derivation.to_aterm_bytes())`                                           |
| `BuildEngine::Output`        | `PathInfo` + `Node`                                          | Wrapped in `SnixOutput` struct                                                                   | `PathInfo` carries NAR hash, references; `Node` carries castore root                                                      |
| `ArtifactInfo::digest`       | `B3Digest`                                                   | `From`/`Into`                                                                                    | Store-level content identifier                                                                                            |
| `ArtifactInfo::references`   | `Vec<StorePath<String>>`                                     | Element-wise `Into`                                                                              | Transitive closure of runtime references                                                                                  |
| `EvalRequest`                | `snix_eval::Evaluation` (configured via `EvaluationBuilder`) | Manual construction in `SnixEngine::evaluate()`                                                  | Not a mechanical mapping; involves `SnixStoreIO` wiring                                                                   |

---

### Invariants

#### Eval Threading

**[snix-eval-dedicated-thread]**: `SnixEngine::evaluate()` MUST spawn `snix-eval` on a dedicated OS thread (not a Tokio task). The evaluator types (`Value`, `NixString`, `Evaluation`, `EvaluationResult`) contain `Rc<Closure>` and are `!Send`/`!Sync`. Executing evaluation on a Tokio worker thread would cause undefined behavior if the `Value` were dropped on a different thread.
`VERIFIED: unverified`

**[snix-eval-channel-bridge]**: `SnixEngine::evaluate()` MUST communicate the evaluation result to the caller's async context via a channel (e.g., `tokio::sync::oneshot`). The channel payload MUST contain only `Send` types — specifically the produced `Derivation` (which is `Send + Sync`), extracted from the `!Send` `EvaluationResult` before crossing the thread boundary. The `EvaluationResult` itself, including any residual `Value` or `Expr`, MUST be dropped on the eval thread.
`VERIFIED: unverified`

**[snix-eval-runtime-handle]**: The eval thread MUST receive a `tokio::runtime::Handle` (via `Handle::clone()`) so that `SnixStoreIO` can call `handle.block_on()` to execute async store operations from the synchronous `EvalIO` trait methods. The eval thread MUST NOT create its own Tokio runtime.
`VERIFIED: unverified`

**[snix-eval-no-send-leak]**: No `!Send` type from `snix-eval` (including `Value`, `NixString`, `Thunk`, `Closure`, `EvaluationResult`) SHALL appear in the signature or return type of any `eos-core` trait method. The `!Send` boundary is entirely encapsulated within `SnixEngine`.
`VERIFIED: unverified`

#### Store Mapping

**[snix-store-three-service]**: The eval worker MUST connect to Snix's three independent store services — `BlobService`, `DirectoryService`, and `PathInfoService` — via gRPC clients. Each gRPC client implements the same Rust trait (`BlobService`, `DirectoryService`, `PathInfoService`) as the in-process implementations, held as `dyn Trait` objects wrapped in `Arc`. The eval worker does NOT embed store services in-process; all store access is remote via gRPC URIs provided in the eval worker's configuration.
`VERIFIED: unverified`

**[snix-store-service-consistency]**: Operations that span multiple Snix services (e.g., `import` writes blobs via `BlobService`, constructs directory trees via `DirectoryService`, then registers metadata via `PathInfoService`) MUST execute in dependency order. If any intermediate step fails, subsequent steps MUST NOT proceed and previously-written data SHOULD be treated as orphaned (eligible for garbage collection).
`VERIFIED: unverified`

**[snix-store-digest-fidelity]**: The `From<B3Digest>` and `Into<B3Digest>` conversions between `Blake3Digest` (the `eos_core::Digest` impl) and `snix_castore::B3Digest` MUST be lossless. Both types are `#[repr(transparent)]` `[u8; 32]` newtypes representing a BLAKE3 hash. No truncation, re-encoding, or algorithm substitution is permitted.
`VERIFIED: unverified`

#### Build Execution

**[snix-build-request-conversion]**: Before invoking `BuildService::do_build()`, the **build worker shim** MUST convert the `Derivation` into a `BuildRequest` using logic equivalent to `snix-glue::builder::derivation_into_build_request()`. This conversion resolves output placeholders, constructs environment variables (including Nix-magic variables like `NIX_BUILD_TOP`), handles structured attributes (`__json`), and maps input store paths to content-addressed `Node` references. This conversion occurs in the build worker shim binary, not in the eval worker (`eos-snix`).
`VERIFIED: unverified`

**[snix-build-inputs-resolved]**: The `BuildRequest` submitted to `BuildService::do_build()` MUST have all `inputs` populated with resolved `Node` entries. Each entry's `name` component MUST correspond to a valid store path basename, and the associated `Node` MUST be retrievable from the configured `DirectoryService`/`BlobService`. Submitting a `BuildRequest` with unresolved or dangling input references is a programming error.
`VERIFIED: unverified`

**[snix-build-error-enrichment]**: `BuildService::do_build()` returns `io::Result<BuildResult>`, which is excessively coarse. `SnixEngine` MUST wrap this in a structured `SnixError` that distinguishes at minimum: sandbox setup failure, build script nonzero exit, output verification failure, and resource exhaustion. The raw `io::Error` MUST be preserved as a `.source()` for diagnostic chaining.
`VERIFIED: unverified`

#### Sandbox

**[snix-sandbox-platform-dispatch]**: Under the gRPC-first architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), sandbox backend selection is the responsibility of the **snix builder process**, not the eval worker (`eos-snix`). The eval worker produces derivations; it does not execute builds. Build worker shims forward derivations to snix builders via gRPC, and the snix builder selects the platform-appropriate sandbox:

- **Linux**: Snix's native `OCIBuildService` (using `crun` or `runc`) or `BubblewrapBuildService` (using `bwrap`). Both backends mount castore inputs via FUSE.
- **macOS**: `birdcage`-based sandbox or remote delegation. Snix provides no macOS sandbox; only `DummyBuildService` is available upstream.
- **Other / remote**: `GRPCBuildService` delegating to another remote builder.

This platform dispatch occurs within the snix builder's configuration, not within `eos-snix`.
`VERIFIED: unverified`

**[snix-sandbox-shell-path]**: The Snix OCI and Bubblewrap sandbox backends require a sandbox shell path, compiled into the binary via `env!("SNIX_BUILD_SANDBOX_SHELL")`. `eos-snix` MUST propagate this compile-time requirement or provide an equivalent configuration mechanism. If the shell path is absent or invalid at build time, compilation MUST fail.
`VERIFIED: unverified`

**[snix-sandbox-concurrency-configurable]**: Snix hardcodes build concurrency at `Semaphore::new(2)` (Bubblewrap) or `Semaphore::new(MAX_CONCURRENT_BUILDS)` where `MAX_CONCURRENT_BUILDS = 2` (OCI). Under the gRPC-first architecture, build concurrency is managed by the Eos scheduler's build worker pool — each build worker reports its `max_concurrency` during registration. The snix builder's internal semaphore provides a secondary constraint within the builder process itself.
`VERIFIED: unverified`

---

### Transitions

**[snix-evaluate]**: Evaluate a Nix expression via the Snix evaluator to produce a `Derivation`.

- **PRE**: All input atoms and plugin dependencies referenced by the `EvalRequest` are fetched, verified, and materialized as store paths. The `SnixStoreIO` is configured with valid `BlobService`, `DirectoryService`, `PathInfoService`, and `BuildService` handles. A Tokio runtime handle is available.
- **POST**: A dedicated eval thread is spawned. `EvaluationBuilder` is configured with the `SnixStoreIO` as the `EvalIO` implementation. Derivation builtins, fetcher builtins, and import builtins are registered. The expression is evaluated, producing an `EvaluationResult`. If `value` is `Some` and no errors are present, the `Derivation` is extracted (from the `KnownPaths` accumulated during evaluation) and sent across the channel. If errors are present, a structured `SnixError::Evaluation` is returned. All `!Send` types are dropped on the eval thread.
  `VERIFIED: unverified`

**[snix-build]**: Execute a `Derivation` via the build worker shim and snix builder.

- **PRE**: The `Derivation` has been validated (`derivation.validate(true).is_ok()`). The Eos scheduler has dispatched the derivation to a registered build worker via Cap'n Proto.
- **POST**: The build worker shim converts the `Derivation` to a `BuildRequest` (via `derivation_into_build_request` equivalent) and forwards it to the snix builder via gRPC `BuildService::do_build()`. The snix builder executes the build in a sandboxed environment. On success, `BuildResult.outputs` contains `BuildOutput` entries with `Node` (content-addressed filesystem root) and `output_needles` (reference scan indices). The outputs are registered in `PathInfoService` by the snix builder. On failure, the error is propagated back through the shim to the scheduler.
  `VERIFIED: unverified`

**[snix-store-import]**: Import content into the Snix store.

- **PRE**: Content is available as an `AsyncRead` stream. An expected digest MAY be provided for verification.
- **POST**: Blob data is written via `BlobService::open_write()`. If the content represents a directory tree, it is ingested via `DirectoryService::put()`. Metadata is registered via `PathInfoService::put()`. The returned `ArtifactInfo` contains the verified digest, store path, size, and transitive references.
  `VERIFIED: unverified`

**[snix-store-lookup]**: Check for existing content in the Snix store.

- **PRE**: A digest is provided.
- **POST**: `BlobService::has()` or `PathInfoService::get()` is queried. Returns `true`/`Some` if the content exists, `false`/`None` otherwise. No mutation occurs.
  `VERIFIED: unverified`

---

### Forbidden States

**[no-eval-on-tokio-worker]**: Nix evaluation MUST NOT execute on a Tokio worker thread. The `!Send` invariant of `snix-eval` types makes this unsound — a panic during evaluation could unwind through Tokio's task harness and drop `Rc`-containing types on the wrong thread.
`VERIFIED: unverified`

**[no-send-value-across-threads]**: `snix_eval::Value`, `snix_eval::NixString`, or any type containing an `Rc` from the evaluator MUST NOT be sent across a thread boundary. The only `Send` artifact produced by evaluation is the `Derivation`, which is extracted from `KnownPaths` and contains only owned, `Send`-safe data.
`VERIFIED: unverified`

**[no-unresolved-build-inputs]**: `BuildService::do_build()` MUST NOT be invoked with a `BuildRequest` containing input `Node` references that do not resolve in the configured `BlobService`/`DirectoryService`. Snix sandbox backends mount inputs via FUSE from the castore; dangling references cause runtime panics in the FUSE daemon.
`VERIFIED: unverified`

**[no-macos-snix-sandbox]**: On macOS, `SnixEngine` MUST NOT attempt to use `OCIBuildService` or `BubblewrapBuildService`. Both depend on Linux kernel namespaces and FUSE. The only upstream-provided non-Linux option is `DummyBuildService`, which unconditionally returns an error.
`VERIFIED: unverified`

---

### Behavioral Properties

**[eval-thread-isolation]**: Concurrent calls to `SnixEngine::evaluate()` MUST each spawn an independent eval thread (or reuse threads from a dedicated pool). Evaluations MUST NOT share mutable state — each evaluation receives its own `SnixStoreIO` instance with its own `RefCell<KnownPaths>`. The `RefCell` provides interior mutability within a single-threaded context; sharing it across evaluations would cause `BorrowMutError` panics.

- **Type**: Safety
  `VERIFIED: unverified`

**[store-service-thread-safety]**: The three Snix store service traits (`BlobService`, `DirectoryService`, `PathInfoService`) are `Send + Sync` by trait bound. Within a single eval worker process, multiple concurrent evaluations MAY share the same gRPC store client instances (via `Arc`). The eval thread accesses store services synchronously via `handle.block_on()`; these access patterns MUST NOT deadlock because the eval thread uses a separate Tokio runtime handle (not the same runtime's current-thread executor). Build workers are separate processes with independent gRPC store connections.

- **Type**: Safety
  `VERIFIED: unverified`

**[build-service-semaphore-backpressure]**: The Snix `OCIBuildService` and `BubblewrapBuildService` limit concurrency via `tokio::sync::Semaphore`. When all permits are exhausted, `do_build()` suspends (`.await` on `semaphore.acquire()`). Under the gRPC-first architecture, this backpressure is internal to the snix builder process. The eos scheduler manages dispatch concurrency through the build worker pool's declared `max_concurrency`, which SHOULD be set to match or slightly exceed the snix builder's semaphore capacity.

- **Type**: Liveness
  `VERIFIED: unverified`

---

## Crate Dependencies

Under the gRPC-first architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), `eos-snix` retains only the three snix crates required for evaluation. Store and build services are accessed via gRPC — their Rust crate dependencies are eliminated from `eos-snix`.

### Snix Crates (Retained)

| Snix Crate   | Version     | Purpose in `eos-snix`                                                                                                                                                                        |
| :----------- | :---------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nix-compat` | (workspace) | `StorePath`, `Derivation`, `NixHash`, derivation validation, store path computation. The protocol-level data types shared between evaluator and builder.                                     |
| `snix-eval`  | (workspace) | `Evaluation`, `EvaluationBuilder`, `EvaluationResult`, `Value`, `EvalIO` trait. The Nix language evaluator. Used exclusively on the dedicated eval thread.                                   |
| `snix-glue`  | (workspace) | `SnixStoreIO`, `KnownPaths`, `add_derivation_builtins()`, `add_fetcher_builtins()`, `add_import_builtins()`. Wires the evaluator to the store services and registers Nix built-in functions. |

### Snix Crates (Eliminated — accessed via gRPC)

| Former Crate   | ADR-0002 Replacement                                                                                |
| :------------- | :-------------------------------------------------------------------------------------------------- |
| `snix-castore` | gRPC clients for `BlobService` and `DirectoryService` (provided by `snix-castore` gRPC client impl) |
| `snix-store`   | gRPC client for `PathInfoService` (provided by `snix-store` gRPC client impl)                       |
| `snix-build`   | Build dispatch moved to the build worker shim (separate binary)                                     |

> **Note:** `snix-glue` transitively depends on `snix-castore` and `snix-store` for the trait definitions (`BlobService`, `DirectoryService`, `PathInfoService`) needed by `SnixStoreIO`. The eval worker uses `snix_store::utils::construct_services()` with gRPC URIs to instantiate remote clients that implement these same traits. The eval worker process therefore still compiles against these crates, but the runtime services are remote.

### External Dependencies (non-Snix)

| Crate   | Purpose                                                                                                                  |
| :------ | :----------------------------------------------------------------------------------------------------------------------- |
| `tokio` | Async runtime, `Handle::clone()` for eval thread bridge, `sync::oneshot` for result channel, `sync::Semaphore` awareness |
| `tonic` | gRPC client for connecting to remote snix store daemons (transitively, via `snix-store` client)                          |

---

## Derivation → BuildRequest Conversion

### Current Upstream Status

The function `snix_glue::builder::derivation_into_build_request()` is declared `pub(crate)` — it is not part of Snix's public API. This function performs the following non-trivial transformations:

1. **Command construction**: Concatenates `derivation.builder` with `derivation.arguments`, replacing output path placeholders (`$out`, `$lib`, etc.) in each component.
2. **Environment variable assembly**: Seeds Nix-magic variables (`NIX_BUILD_TOP`, `NIX_STORE`, `PATH`, `HOME`, `TERM`), then overlays the derivation's own `environment` map with placeholder replacement.
3. **Structured attributes**: If `__json` is present in the environment, delegates to `handle_structured_attrs()`, which parses the JSON and produces `additional_files` entries (for `passAsFile`).
4. **Input mapping**: Expects a `BTreeMap<StorePath<String>, Node>` providing the content-addressed root node for every input store path.
5. **Output paths**: Extracts expected output paths from the `Derivation`'s `outputs` map.
6. **Build constraints**: Derives `system`, `min_memory`, `available_ro_paths`, `network_access`, and `provide_bin_sh` from the derivation environment and output type (fixed-output derivations get network access).
7. **Reference scan needles**: Collects the nixbase32 hash portion of every input and output store path for post-build reference scanning.

### Strategy

**Primary path**: Contribute an upstream PR to Snix making `derivation_into_build_request()` (or an equivalent public entry point) `pub`. This is a visibility change, not a behavioral change, and aligns with Snix's interest in composable library usage.

**Fallback path**: If the upstream PR is declined or delayed, `eos-snix` MUST reimplement the conversion. The reimplementation MUST produce `BuildRequest` values that are byte-identical (after canonical ordering) to those produced by the upstream function for the same `(Derivation, inputs)` pair. This equivalence SHOULD be verified via a property-based test suite comparing the two implementations across a corpus of real-world derivations.

---

## Platform Sandbox Dispatch

### Dispatch Table

> **Note (ADR-0002):** Under the gRPC-first architecture, the sandbox dispatch table below documents the snix builder's internal dispatch logic. This dispatch occurs within the snix builder process, not within the eval worker or Eos daemon. The build worker shim forwards derivations to snix builders via gRPC; the builder selects the sandbox backend.

| Platform                   | Sandbox Backend          | Provider     | Mount Strategy               | Network Isolation                  |
| :------------------------- | :----------------------- | :----------- | :--------------------------- | :--------------------------------- |
| Linux (with `crun`/`runc`) | `OCIBuildService`        | `snix-build` | FUSE mount of castore inputs | OCI spec `network: none` namespace |
| Linux (with `bwrap`)       | `BubblewrapBuildService` | `snix-build` | FUSE mount of castore inputs | User namespace + network namespace |
| Other / Remote             | `GRPCBuildService`       | `snix-build` | Delegated to remote host     | Delegated to remote host           |

### Detection Logic (Snix Builder Internal)

> **Note (ADR-0002):** This detection logic runs inside the snix builder process, not within `eos-snix` or the Eos daemon. The eval worker (`eos-snix`) does not perform builds.

The snix builder selects the sandbox backend during initialization:

```
FUNCTION select_sandbox(config: SnixBuilderConfig) → SandboxBackend:
  IF config.remote_builder IS SOME:
    RETURN GRPCBuildService(config.remote_builder.endpoint)

  MATCH target_os():
    "linux" →
      IF which("crun").is_ok() OR which("runc").is_ok():
        RETURN OCIBuildService(config.bundle_root, blob_svc, dir_svc)
      ELSE IF which("bwrap").is_ok():
        RETURN BubblewrapBuildService(config.workdir, blob_svc, dir_svc)
      ELSE:
        RETURN Error("no sandbox runtime found; install crun, runc, or bwrap")

    _ →
      IF config.remote_builder IS SOME:
        RETURN GRPCBuildService(config.remote_builder.endpoint)
      ELSE:
        RETURN Error("unsupported platform; configure a remote builder")
```

The resolved backend is stored in the snix builder and reused for all subsequent `do_build()` calls.

---

## Snix gRPC Build Protocol

Snix defines a protobuf-based gRPC service at `snix/build/protos/rpc_build.proto`:

```protobuf
package snix.build.v1;

service BuildService {
  rpc DoBuild(BuildRequest) returns (BuildResponse);
}
```

Where `BuildRequest` and `BuildResponse` are defined in `snix/build/protos/build.proto`. The `BuildRequest` message carries:

- `repeated Entry inputs` — content-addressed input nodes
- `repeated string command_args` — builder command and arguments
- `string working_dir`, `string inputs_dir` — sandbox layout
- `repeated string scratch_paths`, `repeated string outputs` — writable paths and expected outputs
- `repeated EnvVar environment_vars` — build environment
- `BuildConstraints constraints` — system, memory, network access, required paths
- `repeated AdditionalFile additional_files` — passAsFile / structured attrs
- `repeated string refscan_needles` — post-build reference scan patterns

The `BuildResponse` returns `repeated Output outputs`, each containing an `Entry` (content-addressed output root) and `repeated uint64 needles` (indices of detected reference scan matches).

### Eos Usage (ADR-0002 Architecture)

Under the gRPC-first architecture, build dispatch is handled by **build worker shims** — separate binaries that bridge Eos's Cap'n Proto `BuildWorker` interface to snix's gRPC `BuildService.DoBuild()`. The Eos scheduler speaks only Cap'n Proto to workers; the shim translates.

```
Eos Scheduler  ──Cap'n Proto──▸  Build Worker Shim  ──gRPC──▸  snix Builder
```

The shim is a standalone binary (not part of `eos-snix`) that:

- Implements the Eos `BuildWorker` Cap'n Proto interface
- Converts the `Derivation` to a `BuildRequest` (using `nix-compat` types)
- Forwards the request to a snix builder via `GRPCBuildService`
- Adds cancellation, progress streaming, and lease management semantics

The `GRPCBuildService` in `snix-build` accepts any `tonic::transport::Channel` and implements the `BuildService` trait, making it a drop-in replacement for the local OCI/Bubblewrap backends. Connection management, TLS, and authentication are configured at the `Channel` level.

---

## Known Gotchas

These are constraints and design artifacts in the Snix codebase that `eos-snix` MUST accommodate. Each is a potential source of subtle failures if overlooked.

### G1: `EvalIO` is Synchronous

The `snix_eval::io::EvalIO` trait defines synchronous methods (`path_exists`, `open`, `file_type`, `read_dir`, `import_path`). The concrete implementation in `snix-glue` (`SnixStoreIO`) bridges to async store services by calling `self.tokio_handle.block_on(...)` inside each method. This means:

- The eval thread MUST hold a `tokio::runtime::Handle` to a multi-threaded runtime (not a `current_thread` runtime, which would deadlock on `block_on`).
- Each `EvalIO` call potentially suspends the eval thread while an async store operation completes. Evaluation throughput is bound by store latency.
- If the Tokio runtime is shut down before evaluation completes, `block_on` panics.

### G2: `KnownPaths` Uses `RefCell`

`SnixStoreIO` holds `pub known_paths: RefCell<KnownPaths>`. This provides interior mutability for tracking derivations and store paths discovered during evaluation. The `RefCell` is safe only because evaluation is single-threaded. Consequences:

- Each `SnixEngine::evaluate()` invocation MUST create a fresh `SnixStoreIO` instance. Sharing a `SnixStoreIO` across concurrent evaluations would cause `RefCell::borrow_mut()` panics.
- After evaluation, the `KnownPaths` contains the complete set of instantiated derivations. `eos-snix` extracts the relevant `Derivation` from `KnownPaths` before dropping the `SnixStoreIO` on the eval thread.

### G3: Hardcoded Concurrency Limits

Both `OCIBuildService` and `BubblewrapBuildService` instantiate a `tokio::sync::Semaphore` with a fixed permit count:

- `BubblewrapBuildService`: `Semaphore::new(2)`
- `OCIBuildService`: `Semaphore::new(2)` (via `MAX_CONCURRENT_BUILDS = 2`)

The source comments note `// TODO: make configurable`. The eos scheduler MUST NOT assume unbounded build parallelism. Options:

1. Wrap the Snix `BuildService` in an eos-level adapter that enforces its own configurable concurrency, treating the Snix semaphore as a secondary constraint.
2. Contribute an upstream PR adding a concurrency parameter to the constructors.
3. Construct the `BuildService` with a custom `Semaphore` (requires forking or patching the constructor).

### G4: Compile-Time Sandbox Shell Path

Both `OCIBuildService` and `BubblewrapBuildService` define:

```rust
const SANDBOX_SHELL: &str = env!("SNIX_BUILD_SANDBOX_SHELL");
```

This is resolved at compile time via the `SNIX_BUILD_SANDBOX_SHELL` environment variable. The `eos-snix` build MUST set this variable to point at a statically-linked shell (typically `bash`) that exists inside the build sandbox. If unset, compilation fails with a clear error from `env!()`.

For cross-compilation or Nix-based builds, this path is typically resolved from a Nix derivation (e.g., `${busybox}/bin/sh` or `${bashInteractive}/bin/bash`).

### G5: `BuildService` Returns `io::Result` (Coarse Errors)

The `snix-build::BuildService` trait:

```rust
async fn do_build(&self, request: BuildRequest) -> io::Result<BuildResult>;
```

All failure modes — sandbox creation failure, builder not found, build script exit code nonzero, output missing, FUSE mount failure, OOM — are flattened into `io::Error`. The error message string is the only discriminator. `SnixEngine` MUST pattern-match on error messages or add out-of-band signaling (exit code inspection, filesystem probing) to construct a meaningful `SnixError` variant. This is fragile and SHOULD be improved upstream.

### G6: `derivation_into_build_request()` is `pub(crate)`

As documented in the [Derivation → BuildRequest Conversion](#derivation--buildrequest-conversion) section, this function is not exported. The function signature:

```rust
pub(crate) fn derivation_into_build_request(
    mut derivation: Derivation,
    inputs: &BTreeMap<StorePath<String>, Node>,
) -> std::io::Result<BuildRequest>
```

The `inputs` parameter requires pre-resolved content-addressed nodes for every input store path. These nodes MUST be retrieved from the `PathInfoService` and `DirectoryService` before calling the conversion. The resolution step is non-trivial: each store path's `PathInfo` record contains the root `Node`, but nested directory nodes require recursive resolution via `DirectoryService::get_recursive()`.

### G7: No Built-In Scheduler

Snix builds dependencies ad-hoc during evaluation. When the evaluator encounters a `builtins.derivation` call whose output is needed (import-from-derivation), `SnixStoreIO` triggers an inline build via the `BuildService` handle in the `SnixStoreIO` configuration. Under the gRPC-first architecture, this IFD build is dispatched to the snix builder via the gRPC store connection within the eval worker — not through the Eos scheduler. The Eos scheduler dispatches top-level evaluation and build jobs; IFD builds within evaluation remain internal to the eval worker's `SnixStoreIO` wiring.

---

## Eval Threading Model (Detailed)

The following sequence describes the complete lifecycle of a single `SnixEngine::evaluate()` call:

```
Caller (async)                    Eval Thread (OS thread)
──────────────                    ───────────────────────
evaluate(req) called
  │
  ├─ create oneshot::channel()
  ├─ clone tokio Handle
  ├─ spawn OS thread ──────────▸  thread starts
  │                                │
  │                                ├─ construct SnixStoreIO {
  │                                │     blob_service (Arc),
  │                                │     directory_service (Arc),
  │                                │     path_info_service (Arc),
  │                                │     nar_calc_service (Arc),
  │                                │     build_service (Arc),
  │                                │     tokio_handle (Handle),
  │                                │     known_paths: RefCell::new(KnownPaths::new()),
  │                                │   }
  │                                │
  │                                ├─ io = Rc::new(store_io)
  │                                ├─ builder = Evaluation::builder(io.clone())
  │                                │     .enable_import()
  │                                ├─ builder = add_derivation_builtins(builder, io)
  │                                ├─ builder = add_fetcher_builtins(builder, io)
  │                                ├─ builder = add_import_builtins(builder, io)
  │                                ├─ eval = builder.build()
  │                                │
  │                                ├─ result = eval.evaluate(code, location)
  │                                │     (EvalIO methods call handle.block_on()
  │                                │      to access async store services)
  │                                │
  │                                ├─ IF result.errors.is_empty():
  │                                │     extract Derivation from known_paths
  │                                │     tx.send(Ok(derivation))
  │                                │  ELSE:
  │                                │     tx.send(Err(SnixError::Evaluation(...)))
  │                                │
  │                                ├─ drop result (drops Rc<Value>, !Send types)
  │                                ├─ drop io, store_io
  │                                └─ thread exits
  │
  ├─ rx.await ◂─────────────────  channel receives
  ├─ return Result<Derivation, SnixError>
  └─ done
```

Key properties of this design:

- The `Rc`-containing types never cross the channel. Only the `Derivation` (which is `Send`) traverses the thread boundary.
- Store services are `Arc`-wrapped and shared between the async caller and the sync eval thread.
- The eval thread's `block_on()` calls operate on the caller's Tokio runtime via the cloned `Handle`, avoiding the need for a second runtime.
- Thread spawning overhead is amortized if a thread pool is used (RECOMMENDED for repeated evaluations).

---

## Three-Service Store Mapping (Detailed)

The `SnixStore` implementation maps `ArtifactStore` operations to Snix's three-service architecture:

| `ArtifactStore` Method      | Snix Service(s) Used                                                               | Operation                                                                                                                                         |
| :-------------------------- | :--------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------ |
| `has(digest)`               | `PathInfoService::get(digest[..20])`                                               | Checks existence by store path hash (first 20 bytes of digest serve as the path info lookup key)                                                  |
| `get_info(digest)`          | `PathInfoService::get(digest[..20])` → `PathInfo`                                  | Retrieves store metadata: NAR hash, NAR size, reference set, deriver. Constructs `ArtifactInfo` from `PathInfo` fields                            |
| `import(content, expected)` | `BlobService::open_write()` → `DirectoryService::put()` → `PathInfoService::put()` | Streams content to blob storage, constructs directory tree if applicable, registers path metadata. Verifies digest against `expected` if provided |
| `list()`                    | `PathInfoService::list()`                                                          | Returns a stream of all registered `PathInfo` entries, each mapped to `ArtifactInfo`                                                              |

### Service Trait Bounds

All three Snix store service traits require `Send + Sync`:

```rust
// Snix uses native async fn in traits via trait_variant::make
#[trait_variant::make(Send)] pub trait BlobService: Sync { ... }
#[trait_variant::make(Send)] pub trait DirectoryService: Sync { ... }
#[trait_variant::make(Send)] pub trait PathInfoService: Sync { ... }
```

> **Note:** Snix uses `trait_variant::make` (not `#[async_trait]`) for native `async fn` in trait definitions. The `#[trait_variant::make(Send)]` attribute generates a `Send`-bounded variant of the trait automatically.

Available backend implementations: in-memory, gRPC (remote), `object_store` (S3/GCS/Azure), `redb` (embedded database), Bigtable, and cache combinators (layered caching). The `SnixStore` SHOULD be configurable to use any combination of these backends, selected at daemon startup.

---

## Verification

| Constraint                              | Method                       | Result     | Detail                                                                                                               |
| :-------------------------------------- | :--------------------------- | :--------- | :------------------------------------------------------------------------------------------------------------------- |
| `snix-eval-dedicated-thread`            | Thread identity assertion    | UNVERIFIED | Assert `std::thread::current().id() != tokio_worker_id` on eval thread                                               |
| `snix-eval-channel-bridge`              | Type-system enforcement      | UNVERIFIED | Channel type `oneshot::Sender<Result<Derivation, SnixError>>` statically requires `Send`                             |
| `snix-eval-runtime-handle`              | Integration test             | UNVERIFIED | Eval with store-backed IO that exercises `block_on()` path                                                           |
| `snix-eval-no-send-leak`                | Compile-time verification    | UNVERIFIED | `BuildEngine` trait bound `Send + Sync` rejects `!Send` associated types                                             |
| `snix-store-three-service`              | Unit test                    | UNVERIFIED | Construct `SnixStore` with mock services, verify delegation                                                          |
| `snix-store-service-consistency`        | Failure injection test       | UNVERIFIED | Fail `DirectoryService::put()` after `BlobService` write, verify no `PathInfo` registered                            |
| `snix-store-digest-fidelity`            | Property-based test          | UNVERIFIED | Round-trip `Digest ↔ B3Digest` for random 32-byte values                                                            |
| `snix-build-request-conversion`         | Equivalence test             | UNVERIFIED | Compare eos-snix conversion output against upstream `derivation_into_build_request()` for corpus of real derivations |
| `snix-build-inputs-resolved`            | Assertion in conversion      | UNVERIFIED | `debug_assert!` all input store paths have corresponding `Node` entries                                              |
| `snix-build-error-enrichment`           | Unit test                    | UNVERIFIED | Inject various `io::Error` kinds, verify correct `SnixError` variant produced                                        |
| `snix-sandbox-platform-dispatch`        | Conditional compilation test | UNVERIFIED | `#[cfg(target_os)]` gates + integration tests per platform                                                           |
| `snix-sandbox-shell-path`               | Build-system test            | UNVERIFIED | CI matrix builds with and without `SNIX_BUILD_SANDBOX_SHELL`                                                         |
| `snix-sandbox-concurrency-configurable` | Load test                    | UNVERIFIED | Submit > 2 concurrent builds, measure actual parallelism                                                             |
| `no-eval-on-tokio-worker`               | Thread assertion             | UNVERIFIED | Panic guard at eval entry point checking thread identity                                                             |
| `no-send-value-across-threads`          | Compile-time                 | UNVERIFIED | `Value: !Send` enforced by compiler; channel type excludes it                                                        |
| `no-unresolved-build-inputs`            | Pre-build validation         | UNVERIFIED | Input resolution check before `do_build()` call                                                                      |
| `no-macos-snix-sandbox`                 | Conditional compilation      | UNVERIFIED | `#[cfg(not(target_os = "macos"))]` on OCI/Bwrap paths                                                                |
| `eval-thread-isolation`                 | Concurrent eval test         | UNVERIFIED | Spawn N concurrent evaluations, verify no `RefCell` panics                                                           |
| `store-service-thread-safety`           | Concurrent access test       | UNVERIFIED | Eval + build accessing same `Arc<dyn BlobService>` concurrently                                                      |
| `build-service-semaphore-backpressure`  | Load test                    | UNVERIFIED | Submit builds exceeding semaphore capacity, verify queuing behavior                                                  |

---

## Implications

1. **Upstream Engagement Required**: Making `derivation_into_build_request()` public in snix remains desirable (the build worker shim needs this function). This is a visibility-only change and aligns with snix's interest in composable library usage. If declined, the build worker shim MUST reimplement the conversion with byte-identical output.

2. **Thread Pool for Evaluation**: While the spec mandates a dedicated OS thread per evaluation, a production eval worker implementation SHOULD use a bounded thread pool (e.g., `rayon` or a custom `std::thread` pool) to amortize thread creation overhead. The pool size constrains maximum concurrent evaluations per eval worker — a tunable that the eos scheduler MUST be aware of via the `max_concurrency` field reported during worker registration.

3. **Eval Worker Deployment**: Under the gRPC-first architecture, eval workers are separate processes. For optimal performance, eval workers SHOULD be co-located with snix store daemons to minimize gRPC round-trip latency during evaluation (each `EvalIO` call may trigger a store lookup via `handle.block_on()`).

4. **Build Worker Shim Is Separate**: Build execution is no longer within `eos-snix`'s scope. The build worker shim is a separate binary that wraps snix's gRPC `BuildService` protocol with Eos's Cap'n Proto `BuildWorker` interface, adding cancellation, progress streaming, and lease semantics. See [ADR-0002](../adr/0002-decoupling-snix-backend.md) §Tier 2 for details.

5. **Error Taxonomy**: The coarseness of `io::Result` from `BuildService::do_build()` necessitates a `SnixError` enum in `eos-snix` that provides structured diagnostics. This error type MUST implement `std::error::Error` and SHOULD implement `Display` with machine-parseable output (e.g., error codes) alongside human-readable messages, since errors are serialized over the wire to frontends.

6. **Testing Strategy**: Property-based tests (via `proptest` or `quickcheck`) are RECOMMENDED for the `Derivation → BuildRequest` conversion and the `Digest ↔ B3Digest` round-trip. Integration tests for the eval threading model SHOULD use a real Snix store (in-memory backends) to exercise the full `block_on()` bridge path. Platform-specific sandbox tests require CI runners on both Linux and macOS.
