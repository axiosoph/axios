# SPEC: Eos Snix Backend (Optional Legacy Executor)

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
  See: htc-sad.md §3.5, §6.8 for the executor trait and this backend's place
       within it (the optional legacy/nix-compat executor, not the default).
  See: eos-build-engine.md for the general BuildEngine/ArtifactStore contracts.
  See: docs/models/publishing-stack-layers.md for the algebraic domain model.

  Historical note (2026-07-05, ADR-0005 re-scope): this document previously
  specified `eos-snix` as the sole initial implementation, powering
  standalone eval workers built on the Snix evaluator libraries (dedicated
  OS-thread evaluation, a `!Send`/`!Sync` channel bridge, `KnownPaths`
  threading, import-from-derivation). That entire evaluation-threading
  corpus is archived — recoverable from `git log` prior to this commit —
  not carried forward here. ADR-0005 [htc-atom-dag-executor-trait] (§6)
  removes the evaluation stage from the MVP's design surface outright: the
  atom-DAG is read directly off locks, and this backend is demoted to an
  **optional legacy executor** for interoperating with pre-existing
  Nix-expression content (htc-sad.md §6.8), not the primary path. Whether
  and how that legacy executor's own Nix-expression evaluation capability is
  itself specified is future work, tracked in htc-sad.md Appendix D (item 7,
  P2 debt) and not resolved by this document.
-->

## Domain

**Problem Domain:** This document specifies the retained build/store
mechanics of the **optional legacy/nix-compat executor** — one of two named
implementations of HTC's executor trait (`build(atom_closure,
toolchain_composition, action_params) → output tree`, htc-sad.md §3.5). It
exists for interoperating with pre-existing Nix-expression content
(htc-sad.md §6.8); it is not the default and is not required for the MVP's
`eka add → resolve → lock → build → analyze → compose → run` path (ADR-0005
§8, `[htc-no-nix-mvp]`). The **primary** executor implementation — the FHS
executor most builds dispatch to — is specified in
[eos-sandboxing.md](eos-sandboxing.md) and htc-sad.md §4, not here.

What this document actually specifies, narrowly: the build-dispatch bridge
from a resolved build plan to `snix-build`'s gRPC `BuildService`, and the
store-service mapping onto `snix-castore`'s gRPC clients — the parts of the
pre-substrate `eos-snix` design that remain accurate facts about the
underlying Snix build/store surface both executors' build-dispatch
machinery is built atop (inputs arrive as content-addressed castore nodes;
the `inputs_dir` sandbox-layout parameter; `refscan_needles` is empty for
this substrate's usage — Non-Goal preserved verbatim). This document does
NOT specify how the legacy executor evaluates a Nix expression to produce
the `Derivation` its build-dispatch bridge consumes as input; that
capability is out of scope here (see the historical note above).

**Model Reference:**

- [htc-sad.md](../architecture/htc-sad.md) — §3.5 (Executor Trait), §6.8
  (The GPL Seam and the Executor Boundary)
- [ADR-0005](../adr/0005-hermetic-transactional-composition.md) — §8
  (`[htc-no-nix-mvp]`), §10 (`[htc-gpl-seam-wire-first]`), §Supersede
  ADR-0002 §Tier 3
- [eos-build-engine.md](eos-build-engine.md) — General `BuildEngine`/`ArtifactStore` trait contracts
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore)
- [ADR-0002](../adr/0002-decoupling-snix-backend.md) — gRPC-first snix
  integration architecture; Tiers 1, 2, and 4 hold, scoped to this executor
  (Tier 3 is superseded wholesale, see that ADR's inline note)
- [eos-sandboxing.md](eos-sandboxing.md) — the primary executor's sandbox
  contract; this document's own Sandbox section (below) covers only this
  legacy executor's platform-dispatch detail, not the substrate-wide
  contract

**Criticality Tier:** Legacy/Optional — not the sole or default
implementation (ADR-0005 §8). Within its own reduced scope, threading
violations in the retained build-dispatch bridge still cause unsoundness;
store-mapping errors still cause data corruption; sandbox misconfiguration
still compromises host isolation.

**G2 (open, deferred to P3):** Which wire-first implementation this
executor's build dispatch ultimately speaks to — unmodified upstream snix
binaries, or a fork-and-simplified castore+build subset — is not decided by
this document. ADR-0005 §10 (`[htc-gpl-seam-wire-first]`) resolves the
posture (wire-first, in-process linking not directional going forward) but
defers the specific fork-vs-upstream call to **P3**; htc-sad.md §6.8 and §9
(Known Gaps, item 4) track it as the open item. Every crate dependency named
below is scoped to gRPC-only access and carries no assumption about that
call's eventual outcome.

---

## Constraints

### Type Declarations

The following type mappings bridge `eos-core` abstractions to concrete Snix
types, for this legacy executor's build-dispatch path. `eos-core` MUST NOT
depend on any Snix crate; all conversions live in this executor's own
implementation crate (`eos-snix`, htc-sad.md Appendix B, eos-sad.md
Appendix B).

```
-- eos-core → eos-snix concrete types

TYPE SnixEngine : BuildEngine
  where Plan   = nix_compat::derivation::Derivation   -- legacy-executor-internal
                                                        -- cache key; the substrate's
                                                        -- own identity is action_id
                                                        -- (htc-sad §6.5), not this Plan
  where Output = SnixOutput { path_info: PathInfo, node: Node }
  where Error  = SnixError (structured, wraps io::Error and eval errors)

TYPE SnixStore : ArtifactStore
  delegates to BlobService + DirectoryService + PathInfoService
  -- Reused as HTC's shared CAS (htc-sad §2.4); this executor is one of
  -- possibly several writers/readers, not the store's owner.

-- Primitive type mappings (bidirectional From/Into)
-- eos_core::Digest is a trait; eos-snix selects the concrete implementation.

TYPE Blake3Digest : Digest          -- [u8; 32] newtype, Copy, #[repr(transparent)]
  where eos-snix sets `type Digest = Blake3Digest`

MAP Blake3Digest                   ↔  snix_castore::B3Digest
    -- Both [u8; 32] newtypes, #[repr(transparent)]. Zero-cost conversion.

MAP eos_core::StorePath           ↔  nix_compat::store_path::StorePath<String>
    -- eos_core::StorePath wraps a String; nix_compat::StorePath
    -- provides validation and the 20-byte store hash. Legacy-executor-
    -- internal; the substrate's own artifacts are addressed by castore
    -- tree digest, not a StorePath (htc-sad §2.4).

MAP BuildEngine::Plan             ↔  nix_compat::derivation::Derivation
    -- The Derivation is this executor's own concrete plan type — an
    -- internal cache key, not the substrate's action identity (§Domain).

MAP BuildEngine::Output           ↔  (snix_store::pathinfoservice::PathInfo,
                                      snix_castore::Node)
    -- PathInfo carries store metadata (references, deriver, NAR hash);
    -- Node carries the content-addressed filesystem tree root.

-- Supporting types (Snix-internal, used by this executor only)

TYPE BuildRequestAdapter          -- Derivation → BuildRequest converter
```

#### Type Mapping Table

| `eos-core` Type              | Snix Type                                                    | Conversion                                                                                       | Notes                                                                                                                     |
| :--------------------------- | :----------------------------------------------------------- | :----------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------ |
| `Digest` (trait)             | `snix_castore::B3Digest`                                     | `From`/`Into` on `Blake3Digest` (zero-cost, both `[u8; 32]` newtypes via `#[repr(transparent)]`) | This executor binds `type Digest = Blake3Digest`. `Blake3Digest` implements `Copy` because BLAKE3 output is always 32 bytes. |
| `StorePath`                  | `nix_compat::store_path::StorePath<String>`                  | `TryFrom`/`Into` (`TryFrom` validates format)                                                    | Legacy-executor-internal; enforces the `<hash>-<name>` structure                                                          |
| `BuildEngine::Plan`          | `nix_compat::derivation::Derivation`                         | Identity (associated type binding)                                                               | `Derivation` is `Send + Sync + Clone + Debug`; internal cache key only (§Domain)                                          |
| `BuildEngine::plan_digest()` | —                                                            | `SnixEngine::plan_digest()` computes `Blake3Digest` of the `Derivation`'s ATerm serialization    | Legacy-executor-internal cache key; not the substrate's `action_id`                                                       |
| `BuildEngine::Output`        | `PathInfo` + `Node`                                          | Wrapped in `SnixOutput` struct                                                                   | `PathInfo` carries NAR hash, references; `Node` carries castore root                                                      |
| `ArtifactInfo::digest`       | `B3Digest`                                                   | `From`/`Into`                                                                                    | Store-level content identifier                                                                                            |
| `ArtifactInfo::references`   | `Vec<StorePath<String>>`                                     | Element-wise `Into`                                                                              | Transitive closure of runtime references                                                                                  |

---

### Invariants

#### Store Mapping

**[snix-store-three-service]**: This executor's build-dispatch path MUST
connect to the shared CAS's three independent services —
`BlobService`, `DirectoryService`, and `PathInfoService` — via gRPC clients.
Each gRPC client implements the same Rust trait (`BlobService`,
`DirectoryService`, `PathInfoService`) as an in-process implementation would,
held as `dyn Trait` objects wrapped in `Arc`. This executor does NOT embed
store services in-process; all store access is remote via gRPC URIs
provided in its configuration.
`VERIFIED: unverified`

**[snix-store-service-consistency]**: Operations that span multiple store
services (e.g., `import` writes blobs via `BlobService`, constructs
directory trees via `DirectoryService`, then registers metadata via
`PathInfoService`) MUST execute in dependency order. If any intermediate
step fails, subsequent steps MUST NOT proceed and previously-written data
SHOULD be treated as orphaned (eligible for garbage collection).

**[snix-global-artifact-store]**: This executor MUST be configured to use
the same shared CAS (htc-sad.md §2.4) as every other executor worker in the
cluster, regardless of which executor implementation produced a given
artifact. Artifacts accumulated anywhere in the cluster MUST be instantly
available to all workers via the shared store — the critical efficiency
invariant that lets the cluster function as a unified build cache (restated
at L3 in eos-sad.md §6.1, `[eos-shared-artifact-store]`). Latency of store
access is an operational concern managed by network topology.
`VERIFIED: unverified`

**[snix-store-digest-fidelity]**: The `From<B3Digest>` and `Into<B3Digest>`
conversions between `Blake3Digest` (the `eos_core::Digest` impl) and
`snix_castore::B3Digest` MUST be lossless. Both types are
`#[repr(transparent)]` `[u8; 32]` newtypes representing a BLAKE3 hash. No
truncation, re-encoding, or algorithm substitution is permitted.
`VERIFIED: unverified`

#### Build Execution

**[snix-build-request-conversion]**: Before invoking `BuildService::do_build()`,
the **build worker shim** MUST convert the `Derivation` into a
`BuildRequest` using logic equivalent to
`snix_glue::builder::derivation_into_build_request()`. This conversion
resolves output placeholders, constructs environment variables (including
Nix-magic variables like `NIX_BUILD_TOP`), handles structured attributes
(`__json`), and maps input store paths to content-addressed `Node`
references. This conversion occurs in the build worker shim binary, not in
this executor's own evaluation-facing crate.
`VERIFIED: unverified`

**[snix-build-inputs-resolved]**: The `BuildRequest` submitted to
`BuildService::do_build()` MUST have all `inputs` populated with resolved
`Node` entries. Each entry's `name` component MUST correspond to a valid
store path basename, and the associated `Node` MUST be retrievable from the
configured `DirectoryService`/`BlobService`. Submitting a `BuildRequest`
with unresolved or dangling input references is a programming error.
`VERIFIED: unverified`

**[snix-build-error-enrichment]**: `BuildService::do_build()` returns
`io::Result<BuildResult>`, which is excessively coarse. `SnixEngine` MUST
wrap this in a structured `SnixError` that distinguishes at minimum:
sandbox setup failure, build script nonzero exit, output verification
failure, and resource exhaustion. The raw `io::Error` MUST be preserved as a
`.source()` for diagnostic chaining.
`VERIFIED: unverified`

#### Sandbox

**[snix-sandbox-platform-dispatch]**: Sandbox backend selection is the
responsibility of the **snix builder process** this executor forwards
`BuildRequest`s to via gRPC, not this executor's own build-dispatch shim
(consistent with [eos-build-sandbox-delegation],
[eos-sandboxing.md](eos-sandboxing.md)):

- **Linux**: Snix's native `OCIBuildService` (using `crun` or `runc`) or
  `BubblewrapBuildService` (using `bwrap`). Both backends mount castore
  inputs via FUSE.
- **macOS**: `birdcage`-based sandbox or remote delegation. Snix provides no
  macOS sandbox; only `DummyBuildService` is available upstream.
- **Other / remote**: `GRPCBuildService` delegating to another remote
  builder.

This platform dispatch occurs within the snix builder's own configuration,
not within this executor's build-dispatch shim.
`VERIFIED: unverified`

**[snix-sandbox-shell-path]**: The Snix OCI and Bubblewrap sandbox backends
require a sandbox shell path, compiled into the binary via
`env!("SNIX_BUILD_SANDBOX_SHELL")`. This executor's build worker shim MUST
propagate this compile-time requirement or provide an equivalent
configuration mechanism. If the shell path is absent or invalid at build
time, compilation MUST fail.
`VERIFIED: unverified`

**[snix-sandbox-concurrency-configurable]**: Snix hardcodes build
concurrency at `Semaphore::new(2)` (Bubblewrap) or
`Semaphore::new(MAX_CONCURRENT_BUILDS)` where `MAX_CONCURRENT_BUILDS = 2`
(OCI). Build concurrency for this executor is managed by the Eos
scheduler's build worker pool — each build worker reports its
`max_concurrency` during registration. The snix builder's internal
semaphore provides a secondary constraint within the builder process
itself.
`VERIFIED: unverified`

---

### Transitions

**[snix-build]**: Execute a `Derivation` via the build worker shim and snix
builder.

- **PRE**: The `Derivation` has been validated
  (`derivation.validate(true).is_ok()`). The Eos scheduler has dispatched
  the derivation to a registered build worker via Cap'n Proto.
- **POST**: The build worker shim converts the `Derivation` to a
  `BuildRequest` (via `derivation_into_build_request` equivalent) and
  forwards it to the snix builder via gRPC `BuildService::do_build()`. The
  snix builder executes the build in a sandboxed environment. On success,
  `BuildResult.outputs` contains `BuildOutput` entries with `Node`
  (content-addressed filesystem root) and `output_needles` (reference scan
  indices — empty for this substrate's usage, per the Non-Goal preserved at
  §Domain). The outputs are registered in `PathInfoService` by the snix
  builder. On failure, the error is propagated back through the shim to the
  scheduler.
  `VERIFIED: unverified`

**[snix-store-import]**: Import content into the shared CAS.

- **PRE**: Content is available as an `AsyncRead` stream. An expected digest
  MAY be provided for verification.
- **POST**: Blob data is written via `BlobService::open_write()`. If the
  content represents a directory tree, it is ingested via
  `DirectoryService::put()`. Metadata is registered via
  `PathInfoService::put()`. The returned `ArtifactInfo` contains the
  verified digest, store path, size, and transitive references.
  `VERIFIED: unverified`

**[snix-store-lookup]**: Check for existing content in the shared CAS.

- **PRE**: A digest is provided.
- **POST**: `BlobService::has()` or `PathInfoService::get()` is queried.
  Returns `true`/`Some` if the content exists, `false`/`None` otherwise. No
  mutation occurs.
  `VERIFIED: unverified`

---

### Forbidden States

**[no-unresolved-build-inputs]**: `BuildService::do_build()` MUST NOT be
invoked with a `BuildRequest` containing input `Node` references that do
not resolve in the configured `BlobService`/`DirectoryService`. Snix
sandbox backends mount inputs via FUSE from the castore; dangling
references cause runtime panics in the FUSE daemon.
`VERIFIED: unverified`

**[no-macos-snix-sandbox]**: On macOS, `SnixEngine` MUST NOT attempt to use
`OCIBuildService` or `BubblewrapBuildService`. Both depend on Linux kernel
namespaces and FUSE. The only upstream-provided non-Linux option is
`DummyBuildService`, which unconditionally returns an error.
`VERIFIED: unverified`

---

### Behavioral Properties

**[store-service-thread-safety]**: The three Snix store service traits
(`BlobService`, `DirectoryService`, `PathInfoService`) are `Send + Sync` by
trait bound. This executor's build worker shim MAY share the same gRPC
store client instances (via `Arc`) across concurrent build dispatches;
build workers are separate processes with independent gRPC store
connections.

- **Type**: Safety
  `VERIFIED: unverified`

**[build-service-semaphore-backpressure]**: The Snix `OCIBuildService` and
`BubblewrapBuildService` limit concurrency via `tokio::sync::Semaphore`.
When all permits are exhausted, `do_build()` suspends (`.await` on
`semaphore.acquire()`). This backpressure is internal to the snix builder
process. The eos scheduler manages dispatch concurrency through the build
worker pool's declared `max_concurrency`, which SHOULD be set to match or
slightly exceed the snix builder's semaphore capacity.

- **Type**: Liveness
  `VERIFIED: unverified`

---

## Crate Dependencies

This executor's build-dispatch path retains only the snix crates required
for the build side of the bridge. Store and build services are accessed via
gRPC — their Rust crate dependencies are eliminated from this executor's
build-dispatch shim.

### Snix Crates (Retained)

| Snix Crate   | Version     | Purpose                                                                                                                          |
| :----------- | :---------- | :--------------------------------------------------------------------------------------------------------------------------------- |
| `nix-compat` | (workspace) | `StorePath`, `Derivation`, `NixHash`, derivation validation, store path computation — the protocol-level data types this build-dispatch bridge consumes as input. |

> **G2 note:** Whether this executor additionally links the crates
> implementing legacy Nix-expression evaluation is gated on the
> fork-vs-upstream call this table flags as open above (§Domain,
> "G2, open, deferred to P3") and on the P2 successor-`[compose]`-semantics
> debt (htc-sad.md Appendix D, item 7). Neither is resolved by this table;
> this document specifies only the build-dispatch bridge, which does not
> itself require an evaluation capability to function once handed a
> `Derivation`.

### Snix Crates (Eliminated — accessed via gRPC)

| Former Crate   | ADR-0002 Replacement                                                                                |
| :------------- | :-------------------------------------------------------------------------------------------------- |
| `snix-castore` | gRPC clients for `BlobService` and `DirectoryService` (provided by `snix-castore` gRPC client impl) |
| `snix-store`   | gRPC client for `PathInfoService` (provided by `snix-store` gRPC client impl)                       |
| `snix-build`   | Build dispatch moved to the build worker shim (separate binary)                                     |

### External Dependencies (non-Snix)

| Crate   | Purpose                                                                                                  |
| :------ | :--------------------------------------------------------------------------------------------------------- |
| `tokio` | Async runtime, `sync::Semaphore` awareness for backpressure                                              |
| `tonic` | gRPC client for connecting to remote snix store/build daemons (transitively, via `snix-store`/`snix-build` clients) |

---

## Derivation → BuildRequest Conversion

### Current Upstream Status

The function `snix_glue::builder::derivation_into_build_request()` is
declared `pub(crate)` — it is not part of Snix's public API. This function
performs the following non-trivial transformations:

1. **Command construction**: Concatenates `derivation.builder` with
   `derivation.arguments`, replacing output path placeholders (`$out`,
   `$lib`, etc.) in each component.
2. **Environment variable assembly**: Seeds Nix-magic variables
   (`NIX_BUILD_TOP`, `NIX_STORE`, `PATH`, `HOME`, `TERM`), then overlays the
   derivation's own `environment` map with placeholder replacement.
3. **Structured attributes**: If `__json` is present in the environment,
   delegates to `handle_structured_attrs()`, which parses the JSON and
   produces `additional_files` entries (for `passAsFile`).
4. **Input mapping**: Expects a `BTreeMap<StorePath<String>, Node>`
   providing the content-addressed root node for every input store path.
5. **Output paths**: Extracts expected output paths from the `Derivation`'s
   `outputs` map.
6. **Build constraints**: Derives `system`, `min_memory`,
   `available_ro_paths`, `network_access`, and `provide_bin_sh` from the
   derivation environment and output type (fixed-output derivations get
   network access).
7. **Reference scan needles**: Collects the nixbase32 hash portion of every
   input and output store path for post-build reference scanning — empty
   for this substrate's usage (§Domain).

### Strategy

**Primary path**: Contribute an upstream PR to Snix making
`derivation_into_build_request()` (or an equivalent public entry point)
`pub`. This is a visibility change, not a behavioral change, and aligns with
Snix's interest in composable library usage.

**Fallback path**: If the upstream PR is declined or delayed, this
executor's build worker shim MUST reimplement the conversion. The
reimplementation MUST produce `BuildRequest` values that are byte-identical
(after canonical ordering) to those produced by the upstream function for
the same `(Derivation, inputs)` pair. This equivalence SHOULD be verified
via a property-based test suite comparing the two implementations across a
corpus of real-world derivations.

---

## Platform Sandbox Dispatch

### Dispatch Table

> **Note (ADR-0002):** Under the gRPC-first architecture, the sandbox
> dispatch table below documents the snix builder's internal dispatch
> logic. This dispatch occurs within the snix builder process, not within
> this executor's build-dispatch shim or the Eos daemon. The build worker
> shim forwards derivations to snix builders via gRPC; the builder selects
> the sandbox backend.

| Platform                   | Sandbox Backend          | Provider     | Mount Strategy               | Network Isolation                  |
| :-------------------------- | :----------------------- | :----------- | :---------------------------- | :----------------------------------- |
| Linux (with `crun`/`runc`) | `OCIBuildService`        | `snix-build` | FUSE mount of castore inputs | OCI spec `network: none` namespace |
| Linux (with `bwrap`)       | `BubblewrapBuildService` | `snix-build` | FUSE mount of castore inputs | User namespace + network namespace |
| Other / Remote             | `GRPCBuildService`       | `snix-build` | Delegated to remote host     | Delegated to remote host           |

### Detection Logic (Snix Builder Internal)

> **Note (ADR-0002):** This detection logic runs inside the snix builder
> process, not within this executor's build-dispatch shim or the Eos
> daemon.

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

The resolved backend is stored in the snix builder and reused for all
subsequent `do_build()` calls.

---

## Snix gRPC Build Protocol

Snix defines a protobuf-based gRPC service at
`snix/build/protos/rpc_build.proto`:

```protobuf
package snix.build.v1;

service BuildService {
  rpc DoBuild(BuildRequest) returns (BuildResponse);
}
```

Where `BuildRequest` and `BuildResponse` are defined in
`snix/build/protos/build.proto`. The `BuildRequest` message carries:

- `repeated Entry inputs` — content-addressed input nodes
- `repeated string command_args` — builder command and arguments
- `string working_dir`, `string inputs_dir` — sandbox layout
- `repeated string scratch_paths`, `repeated string outputs` — writable
  paths and expected outputs
- `repeated EnvVar environment_vars` — build environment
- `BuildConstraints constraints` — system, memory, network access, required
  paths
- `repeated AdditionalFile additional_files` — passAsFile / structured
  attrs
- `repeated string refscan_needles` — post-build reference scan patterns
  (empty for this substrate's usage)

The `BuildResponse` returns `repeated Output outputs`, each containing an
`Entry` (content-addressed output root) and `repeated uint64 needles`
(indices of detected reference scan matches).

### Eos Usage

Build dispatch is handled by **build worker shims** — separate binaries
that bridge Eos's Cap'n Proto `BuildWorker`/`ExecutorWorker` interface to
snix's gRPC `BuildService.DoBuild()`. The Eos scheduler speaks only Cap'n
Proto to workers; the shim translates.

```
Eos Scheduler  ──Cap'n Proto──▸  Build Worker Shim  ──gRPC──▸  snix Builder
```

The shim is a standalone binary (not part of this executor's own crate)
that:

- Implements the Eos `ExecutorWorker` Cap'n Proto interface
- Converts the `Derivation` to a `BuildRequest` (using `nix-compat` types)
- Forwards the request to a snix builder via `GRPCBuildService`
- Adds cancellation, progress streaming, and lease management semantics

The `GRPCBuildService` in `snix-build` accepts any
`tonic::transport::Channel` and implements the `BuildService` trait, making
it a drop-in replacement for the local OCI/Bubblewrap backends. Connection
management, TLS, and authentication are configured at the `Channel` level.

---

## Known Gotchas

These are constraints and design artifacts in the Snix codebase this
executor's build-dispatch bridge MUST accommodate. Each is a potential
source of subtle failures if overlooked.

### G3: Hardcoded Concurrency Limits

Both `OCIBuildService` and `BubblewrapBuildService` instantiate a
`tokio::sync::Semaphore` with a fixed permit count:

- `BubblewrapBuildService`: `Semaphore::new(2)`
- `OCIBuildService`: `Semaphore::new(2)` (via `MAX_CONCURRENT_BUILDS = 2`)

The source comments note `// TODO: make configurable`. The eos scheduler
MUST NOT assume unbounded build parallelism. Options:

1. Wrap the Snix `BuildService` in an eos-level adapter that enforces its
   own configurable concurrency, treating the Snix semaphore as a secondary
   constraint.
2. Contribute an upstream PR adding a concurrency parameter to the
   constructors.
3. Construct the `BuildService` with a custom `Semaphore` (requires forking
   or patching the constructor).

### G4: Compile-Time Sandbox Shell Path

Both `OCIBuildService` and `BubblewrapBuildService` define:

```rust
const SANDBOX_SHELL: &str = env!("SNIX_BUILD_SANDBOX_SHELL");
```

This is resolved at compile time via the `SNIX_BUILD_SANDBOX_SHELL`
environment variable. This executor's build worker shim MUST set this
variable to point at a statically-linked shell (typically `bash`) that
exists inside the build sandbox. If unset, compilation fails with a clear
error from `env!()`.

For cross-compilation or Nix-based builds, this path is typically resolved
from a Nix derivation (e.g., `${busybox}/bin/sh` or
`${bashInteractive}/bin/bash`).

### G5: `BuildService` Returns `io::Result` (Coarse Errors)

The `snix-build::BuildService` trait:

```rust
async fn do_build(&self, request: BuildRequest) -> io::Result<BuildResult>;
```

All failure modes — sandbox creation failure, builder not found, build
script exit code nonzero, output missing, FUSE mount failure, OOM — are
flattened into `io::Error`. The error message string is the only
discriminator. `SnixEngine` MUST pattern-match on error messages or add
out-of-band signaling (exit code inspection, filesystem probing) to
construct a meaningful `SnixError` variant. This is fragile and SHOULD be
improved upstream.

### G6: `derivation_into_build_request()` is `pub(crate)`

As documented in the
[Derivation → BuildRequest Conversion](#derivation--buildrequest-conversion)
section, this function is not exported. The function signature:

```rust
pub(crate) fn derivation_into_build_request(
    mut derivation: Derivation,
    inputs: &BTreeMap<StorePath<String>, Node>,
) -> std::io::Result<BuildRequest>
```

The `inputs` parameter requires pre-resolved content-addressed nodes for
every input store path. These nodes MUST be retrieved from the
`PathInfoService` and `DirectoryService` before calling the conversion. The
resolution step is non-trivial: each store path's `PathInfo` record
contains the root `Node`, but nested directory nodes require recursive
resolution via `DirectoryService::get_recursive()`.

---

## Verification

| Constraint                              | Method                       | Result     | Detail                                                                                                               |
| :--------------------------------------- | :---------------------------- | :--------- | :------------------------------------------------------------------------------------------------------------------- |
| `snix-store-three-service`              | Unit test                    | UNVERIFIED | Construct `SnixStore` with mock services, verify delegation                                                          |
| `snix-store-service-consistency`        | Failure injection test       | UNVERIFIED | Fail `DirectoryService::put()` after `BlobService` write, verify no `PathInfo` registered                            |
| `snix-store-digest-fidelity`            | Property-based test          | UNVERIFIED | Round-trip `Digest ↔ B3Digest` for random 32-byte values                                                            |
| `snix-build-request-conversion`         | Equivalence test             | UNVERIFIED | Compare build-dispatch conversion output against upstream `derivation_into_build_request()` for corpus of real derivations |
| `snix-build-inputs-resolved`            | Assertion in conversion      | UNVERIFIED | `debug_assert!` all input store paths have corresponding `Node` entries                                              |
| `snix-build-error-enrichment`           | Unit test                    | UNVERIFIED | Inject various `io::Error` kinds, verify correct `SnixError` variant produced                                        |
| `snix-sandbox-platform-dispatch`        | Conditional compilation test | UNVERIFIED | `#[cfg(target_os)]` gates + integration tests per platform                                                           |
| `snix-sandbox-shell-path`               | Build-system test            | UNVERIFIED | CI matrix builds with and without `SNIX_BUILD_SANDBOX_SHELL`                                                         |
| `snix-sandbox-concurrency-configurable` | Load test                    | UNVERIFIED | Submit > 2 concurrent builds, measure actual parallelism                                                             |
| `no-unresolved-build-inputs`            | Pre-build validation         | UNVERIFIED | Input resolution check before `do_build()` call                                                                      |
| `no-macos-snix-sandbox`                 | Conditional compilation      | UNVERIFIED | `#[cfg(not(target_os = "macos"))]` on OCI/Bwrap paths                                                                |
| `store-service-thread-safety`           | Concurrent access test       | UNVERIFIED | Build workers accessing same `Arc<dyn BlobService>` concurrently                                                     |
| `build-service-semaphore-backpressure`  | Load test                    | UNVERIFIED | Submit builds exceeding semaphore capacity, verify queuing behavior                                                  |

---

## Implications

1. **Upstream Engagement Required**: Making `derivation_into_build_request()`
   public in snix remains desirable (the build worker shim needs this
   function). This is a visibility-only change and aligns with snix's
   interest in composable library usage. If declined, the build worker shim
   MUST reimplement the conversion with byte-identical output.

2. **Build Worker Shim Is Separate**: The build worker shim is a separate
   binary that wraps snix's gRPC `BuildService` protocol with Eos's Cap'n
   Proto `ExecutorWorker` interface, adding cancellation, progress
   streaming, and lease semantics. See [ADR-0002](../adr/0002-decoupling-snix-backend.md)
   §Tier 2 for details.

3. **Error Taxonomy**: The coarseness of `io::Result` from
   `BuildService::do_build()` necessitates a `SnixError` enum that provides
   structured diagnostics. This error type MUST implement
   `std::error::Error` and SHOULD implement `Display` with machine-parseable
   output (e.g., error codes) alongside human-readable messages, since
   errors are serialized over the wire to frontends.

4. **Testing Strategy**: Property-based tests (via `proptest` or
   `quickcheck`) are RECOMMENDED for the `Derivation → BuildRequest`
   conversion and the `Digest ↔ B3Digest` round-trip. Platform-specific
   sandbox tests require CI runners on both Linux and macOS.
