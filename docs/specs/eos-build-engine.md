# SPEC: Eos Build Engine

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

**Problem Domain:** Eos is a network-first daemon that serves as the atom-DAG scheduling and build-dispatch runtime (L3) for the Axios publishing stack. It bridges L4 (Ion frontend resolution) and L2 (HTC build execution) by accepting a pre-coarsened atom DAG from Ion at build submission — nodes are atoms identified by `publish_czd`, edges are the dependency relationships already resolved into the lock — dispatching build actions to executor-trait workers, caching results at the action-id granularity, and registering hermetic build outputs in HTC's shared, content-addressed artifact store. There is no evaluation stage: the DAG is read directly off locks, not produced by evaluating an expression against atom source trees (eos-sad §1.1; ADR-0005 §6, `[htc-atom-dag-executor-trait]`).

Eos is **not** an embedded library that callers link against. The `BuildEngine` and `ArtifactStore` traits defined in `eos-core` specify the _behavioral contract_ — what the daemon does, what invariants it upholds, and what state transitions it permits. The Cap'n Proto protocol defined in [eos-network-protocol.md](eos-network-protocol.md) is the _wire projection_ of these traits — how clients invoke them over the wire. The **primary** FHS-executor implementation is the primary concrete backend fulfilling the `BuildEngine` and `ArtifactStore` contracts (htc-sad §3.5, §6.8); the Snix implementation defined in [eos-snix-backend.md](eos-snix-backend.md) is the **optional legacy** backend, retained for interoperating with pre-existing Nix-expression content.

By leveraging the cryptographic nature of atoms and the action-identity formula (htc-sad §6.5, ADR-0005 §2), Eos implements one level of cache-skipping:

1. **Action caching**: If `action_id` — computed from the atom closure root, the toolchain composition root, and `ActionParams` — matches an existing `BuildRecord` in HTC's shared CAS, dispatch to an executor worker is skipped entirely and the cached output tree is returned.

**Model Reference:**

- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore), §3.2 (BuildSession)
- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries and capability advertisement
- [eos-network-protocol.md](eos-network-protocol.md) — Cap'n Proto wire protocol, capability model, daemon architecture
- [eos-snix-backend.md](eos-snix-backend.md) — Legacy passthrough-snix executor binding (store mapping, sandbox dispatch, htc-sad §6.8)
- [htc-sad.md](../architecture/htc-sad.md) — §2 (core object taxonomy), §3.5 (executor trait), §6.5 (action identity)
- [eos-sad.md](../architecture/eos-sad.md) — §4.1 (build lifecycle, two-variant `BuildPlan`), §6.5 (action-id cache)

**Criticality Tier:** Medium — correctness governs build reproducibility, sandbox isolation, host system integrity, and dependency containment.

---

## Constraints

### Type Declarations

The following types model the Eos behavioral contract at the `eos-core` layer. These are backend-agnostic — concrete type mappings (e.g., to Snix's `Derivation`, `B3Digest`, `PathInfo`) live in backend-specific crates (see [eos-snix-backend.md](eos-snix-backend.md) §Type Declarations).

```
-- Core identity types (eos-core)

-- Digest is a TRAIT, not a fixed type. It abstracts the *stored value*
-- (what gets compared, serialized, used as map keys) separately from
-- the *hasher* (computation, which lives downstream in eos-store).
-- This separation enables the BLAKE3 → Coz migration: callers bound by
-- `D: Digest` never change; only the associated type binding does.

TRAIT Digest: AsRef<[u8]> + Eq + Hash + Clone + Send + Sync + 'static
    fn algorithm(&self) -> &str              -- Algorithm identifier ("blake3", "ES256")
    fn as_bytes(&self) -> &[u8]              -- Raw digest bytes, without framing
    fn len(&self) -> usize                   -- Byte length of the digest

-- Concrete v1 implementation (lives in eos-core alongside the trait)

TYPE Blake3Digest = #[repr(transparent)] [u8; 32]  -- Fixed-size BLAKE3 content digest
                                                   -- Copy because always 32 bytes
    IMPL Digest for Blake3Digest:
        algorithm() => "blake3"

-- Migration path: add `CozDigest { alg: Alg, bytes: Vec<u8> }` impl of
-- Digest. Callers using `D: Digest` bounds don't change. Only the
-- associated type binding `type Digest = Blake3Digest` becomes
-- `type Digest = CozDigest`.

TYPE StorePath = opaque String                     -- Legacy-executor (passthrough-snix) artifact
                                                   -- path. The primary FHS executor addresses
                                                   -- outputs by output-tree digest (BuildEngine::
                                                   -- Output), not StorePath (htc-sad §2.4).
TYPE AtomRef<D: Digest> = { id: AtomId, digest: D }  -- Cryptographic reference to an atom-DAG
                                                      -- node; underlies the atom_czd_closure_root
                                                      -- component of action_id (htc-sad §6.5).
                                                      -- Generic over digest algorithm

-- Build engine associated types (generic over backend)
-- Note: eos uses native async fn in traits (edition 2024, toolchain
-- 1.90.0) with `trait_variant::make` for Send-bound variants.
-- No `#[async_trait]` / `Box<dyn Future>` overhead.

TYPE BuildEngine::Digest: crate::Digest            -- Digest algorithm for this backend
                                                   -- (FHS executor: Blake3Digest)
TYPE BuildEngine::Plan                             -- Backend-specific build recipe; the MVP
                                                   -- (FHS executor) realization is the atom
                                                   -- action itself — atom_czd_closure_root +
                                                   -- toolchain_composition_root + ActionParams
                                                   -- (ADR-0005 §2, htc-sad §6.5; Supersede-
                                                   -- ADR-0001). Legacy passthrough-snix:
                                                   -- nix_compat::derivation::Derivation
                                                   -- (htc-sad §6.8).
TYPE BuildEngine::Output                           -- Backend-specific build result (FHS
                                                   -- executor: output-tree digest in HTC's
                                                   -- shared CAS, htc-sad §2.4; legacy
                                                   -- passthrough-snix: PathInfo + Node)
TYPE BuildEngine::Error                            -- Structured error type
                                                   -- (backend-specific; legacy passthrough-
                                                   -- snix: SnixError)

-- Build engine methods (beyond plan/apply)

FN BuildEngine::plan_digest(&self, plan: &Self::Plan) -> Self::Digest
    -- Compute the content-addressed digest of a plan for deduplication.
    -- For the MVP (FHS-executor) realization this closes the gap between
    -- the trait surface and the `JobId = action_id` invariant (eos-sad
    -- §6.9; htc-sad §6.5) — the backend knows how to canonically
    -- serialize its plan type; this method makes that knowledge
    -- available to the daemon without leaking serialization details.
    -- Synchronous, pure — no async needed.

-- Action parameters (frontend → daemon)
-- The typed successor of [compose.args]'s composer arguments (the
-- pre-ADR-0005 `eval_args`). ActionParams is a component of action_id
-- (htc-sad §6.5, ADR-0005 §2) and is defined HERE ONCE: eos-network-
-- protocol.md's wire schema references these exact fields, it does not
-- redefine them. Uses `#[non_exhaustive]` for forward compatibility —
-- fields may be added in future versions without breaking downstream
-- consumers.

TYPE ActionParams = #[non_exhaustive] {
    target_system: String,                         -- Target system triple (from
                                                   -- [compose.args].system)
    variant_flags: Map<String, String>              -- Opaque variant flags (from
                                                   -- [compose.args], e.g. feature selectors)
}

-- Build plan lifecycle (the central state machine)

TYPE BuildPlan =
    Cached(BuildEngine::Output)                    -- Output tree exists in HTC's shared CAS
  | NeedsBuild(BuildEngine::Plan)                  -- Nothing cached; dispatch action to
                                                   -- an executor worker

-- Job management types (daemon-level)

TYPE JobId = BuildEngine::Digest                   -- Content-addressed: == action_id
                                                   -- (htc-sad §6.5). Identical actions produce
                                                   -- identical JobIds, enabling deduplication
                                                   -- (`[eos-scheduler-deduplication]`, eos-sad §6.9)

TYPE JobStatus =
    Queued                                         -- Waiting in scheduler queue
  | Building { phase: String, progress: Option<f32> }
  | Completed { outputs: Vec<ArtifactInfo> }
  | Failed { error: String, exit_code: Option<i32> }
  | Cancelled

TYPE ProgressEvent = {
    job_id: JobId,
    timestamp: SystemTime,
    status: JobStatus,
    log_line: Option<String>                       -- Structured log line (build output)
}

-- Artifact metadata

TYPE ArtifactInfo<D: Digest> = {
    digest: D,
    store_path: StorePath,                         -- Legacy-executor only (see StorePath)
    size: u64,
    references: Vec<StorePath>,                    -- Legacy-executor only: transitive runtime
                                                   -- references via store-path scanning. The
                                                   -- primary FHS executor's runtime closure is
                                                   -- computed by HTC's closure-computer
                                                   -- fixpoint (htc-sad §6.4), not scanned.
    deriver: Option<D>                             -- action_id that produced this artifact
}
```

#### `ActionParams` and `[compose.args]`

`ActionParams` carries action parameters originating from the lock file's `[compose.args]` section (see `lock-file-schema.md`). These are opaque key-value pairs passed through the daemon to the executor. The executor determines how to apply them (e.g., as build-system flags or toolchain-composition selectors).

```toml
# Example lock file fragment
[compose]
at = "0.4.5"
entry = "src"
use = "r9ilp2p4..."

[compose.args]
system = "x86_64-linux"
features = "wayland,pipewire"
```

The daemon MUST transmit `ActionParams` faithfully to the executor. The daemon MUST NOT interpret, filter, or modify the contents of `variant_flags` — they are opaque to the protocol layer. They ARE, however, included in `action_id` (htc-sad §6.5) because different params produce different output trees.

#### Store Delegation Model

Eos does **not** own or implement an artifact store directly. The `ArtifactStore` trait abstracts over concrete store backends — HTC's shared CAS (htc-sad §2.4) for the primary FHS executor. Under the executor-trait architecture (ADR-0005 §6, htc-sad §3.5), the daemon (scheduler) does not interact with the `ArtifactStore` trait directly — executor workers do:

- Executor workers connect to HTC's shared CAS via gRPC for store access during action execution.
- The daemon dispatches actions to workers via Cap'n Proto and consults the action-id cache, but does not hold `ArtifactStore` instances.

The `ArtifactStore` trait surface (`has`, `get_info`, `import`, `list`) is intentionally minimal. Backend crates provide the concrete wiring to underlying storage services.

#### `AtomIndex` — Discovery Seam

Every eos instance that processes atoms accumulates knowledge about their existence, versions, dependencies, and build status. The `AtomIndex` trait captures this accumulated knowledge as a queryable surface. It is an **eos-layer trait** (L2) that builds atop `AtomSource` reads — it is NOT a reimplementation of atom-core's `AtomStore`.

This trait uses native async fn in traits (edition 2024, toolchain 1.90.0) with `trait_variant::make` for Send-bound variants — no `#[async_trait]` or `Box<dyn Future>` overhead.

```
TRAIT AtomIndex: Send + Sync + 'static
    TYPE Error: std::error::Error + Send + Sync + 'static

    async fn resolve(&self, id: &AtomId) -> Result<Option<AtomMeta>, Self::Error>
        -- Look up metadata about a specific atom.

    async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error>
        -- Fast existence check (avoids deserializing full metadata).
        -- Mirrors the ArtifactStore::has() pattern.

    async fn search(&self, query: &AtomQuery) -> Result<Vec<AtomMeta>, Self::Error>
        -- Search for atoms matching a structured query.

    async fn ingest(&self, meta: AtomMeta) -> Result<(), Self::Error>
        -- Record that an atom has been observed/processed.
```

This maps to the formal model's `F_source` coalgebra: `F_source(S) = (AtomId → Option<AtomMeta>) × (Query → Set<AtomId>)`.

The discovery chain evolves across versions:

- **v1**: Backed by processed lock files + store queries (local daemon).
- **v2**: Local index with gossip sync between eos peers.
- **vN**: Distributed index (DHT) for decentralized package discovery.

See the Cap'n Proto projection of this trait as the `AtomDiscovery` capability in [eos-network-protocol.md](eos-network-protocol.md).

---

### Invariants

**[eos-verification-obligation]**: Eos MUST fetch atom snapshots and verify their content digests before applying any operations. Eos MUST NOT execute plans using unverified atom sources.
`VERIFIED: unverified`

**[eos-verify-ownership]**: Eos MUST verify that the publish transaction is validly signed and authorized according to the claim chain before using the snapshot.
`VERIFIED: unverified`

**[eos-verify-plugin-deps]**: Eos MUST fetch plugin dependencies and verify their hashes against the locked hashes using the specified algorithm (indicated by type tag).
`VERIFIED: unverified`

**[eos-no-unverified-execution]**: Eos MUST NOT execute or reference any unverified snapshot or plugin dependency in a build sandbox.
`VERIFIED: unverified`

**[eos-sandbox-network-containment]**: Build execution MUST be executed in a restricted sandboxed environment. The sandbox MUST NOT have network access except through HTC's content-addressing record/replay proxy (htc-sad §4.2, ADR-0005 §7 `[htc-fetch-set-lock-plugin]`). Normative record/replay proxy semantics (record vs. replay mode, TLS CA injection, protocol-aware handlers) are specified in [eos-sandboxing.md](eos-sandboxing.md), not restated here. Platform-specific sandbox selection is wholly the executor implementation's concern (htc-sad §6.2, §6.4) — the primary FHS executor reuses `snix-build`'s OCI/bwrap sandbox; the optional legacy passthrough-snix executor uses whatever sandbox `snix-build` provides upstream (see [eos-snix-backend.md](eos-snix-backend.md) §Platform Sandbox Dispatch).
`VERIFIED: unverified`

**[eos-sandbox-host-isolation]**: The sandbox MUST NOT have write access to any part of the host filesystem outside the designated temporary sandbox build directory.
`VERIFIED: unverified`

**[eos-sandbox-reproducibility]**: The sandbox MUST NOT leak host environment variables, system times, CPU/hardware identifiers, or other sources of non-determinism into the build process. All inputs (environment variables, files, binaries) MUST be explicitly declared in the `BuildEngine::Plan`.
`VERIFIED: unverified`

**[eos-bisimulation-equivalence]**: Two Eos engine implementations MUST be behaviorally equivalent (bisimilar) under the same inputs: for any `AtomRef` and `ActionParams`, they MUST produce the same `BuildPlan` and equivalent outputs. This equivalence MUST hold across both local and daemon-served execution paths.
`VERIFIED: unverified`

**[eos-immutable-store]**: Once a build output is registered in the `ArtifactStore` at a `StorePath`, it MUST be read-only and immutable. Eos MUST NOT permit modification, overwriting, or deletion of existing store paths during execution.
`VERIFIED: unverified`

**[eos-cache-determinism]**: If the output corresponding to a given `BuildEngine::Plan` already exists in the `ArtifactStore` (build cache) or can be substituted, Eos MUST skip the build execution (`apply`) and return the cached `StorePath`.
`VERIFIED: unverified`

**[eos-transitive-closure]**: The store path of any build output MUST be transitively self-contained. All dependencies of a store path MUST exist within the `ArtifactStore` and be immutable.
`VERIFIED: unverified`

**[eos-atom-index-ingest]**: Atoms that have been successfully processed (fetched, verified, evaluated, or built) MUST be ingested into the `AtomIndex` via `ingest()`. The daemon MUST NOT silently discard atom metadata after processing. This invariant ensures that every eos instance progressively accumulates discoverable knowledge about the atoms it has encountered.
`VERIFIED: unverified`

---

### Transitions

**[engine-plan]**: Determine an atom action's build status.

- **PRE**: The `AtomRef` is resolved and its verified content snapshot is present. `ActionParams` and the toolchain composition are known. The scheduler holds a connection to registered executor workers.
- **POST**: Returns `Cached` if the output tree is verified in HTC's shared CAS, or `NeedsBuild` if the action's `action_id` has no cached `BuildRecord`.
  `VERIFIED: unverified`

**[engine-apply]**: Execute a build plan to produce store outputs.

- **PRE**: The `BuildPlan` is `NeedsBuild`. All transitive build inputs are present and verified in the store. The backend's sandbox is initialized.
- **POST**: The builder executes inside the sandbox. Outputs are verified, registered as read-only, and committed to the `ArtifactStore` via the delegated store backend. Failed builds MUST abort and return an error without mutating existing store paths. The daemon transitions the corresponding `JobStatus` to `Completed` or `Failed` and pushes a `ProgressEvent` to all attached clients.
  `VERIFIED: unverified`

---

### Forbidden States

**[no-undeclared-inputs]**: A build execution MUST NOT reference or import any host environment variables or filesystem paths not explicitly declared in the plan's environment or capability set.
`VERIFIED: unverified`

**[no-speculative-writes]**: Eos MUST NOT commit outputs to the store for builds that fail post-evaluation verification checks.
`VERIFIED: unverified`

**[no-dirty-store-paths]**: Eos MUST NOT register a store path in the `ArtifactStore` unless its contents have been verified to match the content digest of the built output.
`VERIFIED: unverified`

---

### Behavioral Properties

**[action-cache-idempotency]**: Multiple concurrent action-cache lookups for the same `action_id` (htc-sad §6.5) MUST yield bisimilar `BuildEngine::Plan` results.

- **Type**: Safety
  `VERIFIED: unverified`

**[apply-cleanup-on-abort]**: If a build application fails or aborts, all temporary sandbox resources and partial outputs MUST be completely garbage collected and MUST NOT leak into the `ArtifactStore`.

- **Type**: Safety
  `VERIFIED: unverified`

---

## Verification

| Constraint                        | Method                         | Result     | Detail                                                                      |
| :-------------------------------- | :----------------------------- | :--------- | :-------------------------------------------------------------------------- |
| `eos-verification-obligation`     | Unit tests                     | UNVERIFIED | Pending implementation of atom snapshot fetcher                             |
| `eos-verify-ownership`            | Cryptographic sign check       | UNVERIFIED | Pending integration of Coz verification                                     |
| `eos-verify-plugin-deps`          | Hash verification tests        | UNVERIFIED | Pending lock-reader implementation                                          |
| `eos-no-unverified-execution`     | Sandbox integration tests      | UNVERIFIED | Verification deferred until sandbox integration                             |
| `eos-sandbox-network-containment` | Sandbox profile check          | UNVERIFIED | Backend-specific sandbox restriction verification (see eos-sandboxing.md)   |
| `eos-sandbox-host-isolation`      | Directory permission checks    | UNVERIFIED | Verification of filesystem write blocks                                     |
| `eos-sandbox-reproducibility`     | Binary hash equivalence        | UNVERIFIED | Double-build hash check verification                                        |
| `eos-bisimulation-equivalence`    | Equivalence tests              | UNVERIFIED | Parity checks between mock and concrete backends                            |
| `eos-immutable-store`             | Host filesystem audit          | UNVERIFIED | Verification of read-only enforcement via delegated store                   |
| `eos-cache-determinism`           | Cache hits test                | UNVERIFIED | Property-based tests for build cache                                        |
| `eos-transitive-closure`          | Reference scanner test         | UNVERIFIED | Store scanner reference tracing validation                                  |
| `eos-atom-index-ingest`           | Integration tests              | UNVERIFIED | Verify atoms are ingested into AtomIndex after processing                   |
| `engine-plan`                     | State transition audit         | UNVERIFIED | Unit tests for plan transitions via daemon protocol                         |
| `engine-apply`                    | Sandbox execute tests          | UNVERIFIED | Integration tests for builder execution via backend                         |
| `no-undeclared-inputs`            | Environment sanitization check | UNVERIFIED | Env whitelist enforcement audit                                             |
| `no-speculative-writes`           | Aborted commit audit           | UNVERIFIED | Verify store cleanup after failure                                          |
| `no-dirty-store-paths`            | Integrity validation           | UNVERIFIED | Store post-hash checks audit                                                |
| `action-cache-idempotency`        | Concurrent tests               | UNVERIFIED | Concurrency validation with `action_id`                                     |
| `apply-cleanup-on-abort`          | Temporary leak check           | UNVERIFIED | Temporary path leakage validation                                           |

---

## Implications

1. **Daemon-Served Trait Surface**:
   The `BuildEngine`, `ArtifactStore`, and `AtomIndex` traits in `eos-core` are behavioral contracts — they specify what operations the system supports, what invariants hold, and what state transitions are permitted. Under the executor-trait architecture (ADR-0005 §6, htc-sad §3.5), `BuildEngine` is implemented inside executor worker processes, not in the daemon. The daemon orchestrates the `engine-plan` → `engine-apply` transition chain by dispatching to workers via Cap'n Proto. The Cap'n Proto protocol (see [eos-network-protocol.md](eos-network-protocol.md)) projects these contracts onto the wire as the `EosDaemon`, `BuildJob`, and `AtomDiscovery` capabilities for client interaction.

2. **Backend Abstraction via Associated Types**:
   `BuildEngine::Digest`, `BuildEngine::Plan`, `BuildEngine::Output`, and `BuildEngine::Error` are associated types, not concrete types. `eos-core` carries zero dependency on any backend crate. The primary FHS executor binds `Digest = Blake3Digest`, `Plan` to the atom action (atom_czd_closure_root + toolchain_composition_root + `ActionParams`), and `Output` to an output-tree digest in HTC's shared CAS. The optional legacy passthrough-snix executor binds `Plan = Derivation`, `Output = PathInfo + Node`, and `Error = SnixError` (see [eos-snix-backend.md](eos-snix-backend.md) §Type Declarations). Future backends bind different concrete types while preserving the same behavioral invariants. The `Digest` associated type is bounded by `crate::Digest`, ensuring all backends produce values that satisfy the trait's comparison and serialization requirements.

3. **`ActionParams` Carries `[compose.args]`**:
   `ActionParams` is the conduit through which frontend-specified configuration (target system, feature flags) reaches the executor. This type maps directly to the lock file's `[compose.args]` section. The daemon treats `ActionParams.variant_flags` as opaque — it includes them in `action_id` (htc-sad §6.5) for deterministic caching but does not interpret their contents. Executor implementations determine how the params influence the build (e.g., as build-system configure flags or toolchain-composition selectors).

4. **Store Delegation, Not Ownership**:
   Eos does not own, implement, or manage the internal structure of the artifact store. The `ArtifactStore` trait is a delegation boundary. Under the executor-trait architecture, the daemon does not hold `ArtifactStore` instances — executor workers access HTC's shared CAS via gRPC. The primary FHS executor delegates to the reused Snix `BlobService`/`DirectoryService` (htc-sad §2.4). A future Cyphr/Coz-native backend would delegate to a Coz content-addressed store.

5. **Platform Sandbox Dispatch is Delegated to the Executor**:
   This spec mandates sandbox isolation ([eos-sandbox-network-containment], [eos-sandbox-host-isolation], [eos-sandbox-reproducibility]) but does not prescribe the sandbox implementation. Sandbox selection is wholly the executor implementation's concern (htc-sad §6.2, §6.4, see [eos-sandboxing.md](eos-sandboxing.md)) — the daemon and executor worker shims perform zero sandboxing (eos-sad §6.4).

6. **Testing Strategy**:
   Write property-based tests for `BuildPlan` matching and transition permutations to verify that caching decisions are 100% deterministic. Mock `BuildEngine` implementations SHOULD be used to validate daemon-level invariants (deduplication, progress streaming, abort cleanup) independently of any concrete backend. Backend-specific invariants (store mapping, sandbox isolation) are verified in their respective backend specs.

7. **Async Trait Ergonomics (Edition 2024)**:
   All async traits in `eos-core` (`BuildEngine`, `ArtifactStore`, `AtomIndex`) use native `async fn` in traits — no `#[async_trait]` macro. The `trait_variant::make` attribute generates both a `Local*` variant (non-Send, for single-threaded contexts like Cap'n Proto's `!Send` RPC loop) and a Send-bound variant (for the multi-threaded worker pool). This eliminates `Box<dyn Future>` heap allocation on every method call. Requires Rust edition 2024 (toolchain 1.90.0+).

8. **`plan_digest()` Closes the Deduplication Seam**:
   The `BuildEngine::plan_digest()` method is the sole bridge between a backend's opaque `Plan` type and the daemon's `JobId = action_id` deduplication invariant (eos-sad §6.9; htc-sad §6.5). Without it, the daemon would need to know how to serialize backend-specific plan types — a layer violation. By making digest computation a trait method, the backend controls canonical serialization while the daemon operates purely on the resulting `Digest` value.
