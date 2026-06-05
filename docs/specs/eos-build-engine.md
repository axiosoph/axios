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

**Problem Domain:** Eos is a network-first daemon that serves as the build and evaluation runtime for the Axios publishing stack. It bridges L3 (Ion frontend planning) and L1 (Atom content-addressing) by accepting resolved dependency graphs (lock files) from Ion, orchestrating sandbox evaluations to produce build plans, caching results at two distinct levels, and registering hermetic build outputs in an immutable artifact store.

Eos is **not** an embedded library that callers link against. The `BuildEngine` and `ArtifactStore` traits defined in `eos-core` specify the _behavioral contract_ — what the daemon does, what invariants it upholds, and what state transitions it permits. The Cap'n Proto protocol defined in [eos-network-protocol.md](eos-network-protocol.md) is the _wire projection_ of these traits — how clients invoke them over the wire. The Snix implementation defined in [eos-snix-backend.md](eos-snix-backend.md) is one _concrete backend_ fulfilling the `BuildEngine` and `ArtifactStore` contracts.

By leveraging the cryptographic nature of Atoms, Eos implements two levels of cache-skipping:

1. **Evaluation caching**: If the atom snapshot digest and evaluation arguments are identical, evaluation is bypassed to retrieve the pre-computed build recipe (plan).
2. **Build caching**: If the build recipe (plan) matches an existing artifact in the store, the build execution is skipped entirely.

**Model Reference:**

- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore), §3.2 (BuildSession)
- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries and capability advertisement
- [eos-network-protocol.md](eos-network-protocol.md) — Cap'n Proto wire protocol, capability model, daemon architecture
- [eos-snix-backend.md](eos-snix-backend.md) — Snix backend implementation (eval threading, store mapping, sandbox dispatch)

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

TYPE StorePath = opaque String                     -- Immutable path in ArtifactStore
                                                   -- (backend validates format internally)
TYPE AtomRef<D: Digest> = { id: AtomId, digest: D }  -- Cryptographic snapshot reference
                                                      -- Generic over digest algorithm

-- Build engine associated types (generic over backend)
-- Note: eos uses native async fn in traits (edition 2024, toolchain
-- 1.90.0) with `trait_variant::make` for Send-bound variants.
-- No `#[async_trait]` / `Box<dyn Future>` overhead.

TYPE BuildEngine::Digest: crate::Digest            -- Digest algorithm for this backend
                                                   -- (Snix: Blake3Digest)
TYPE BuildEngine::Plan                             -- Backend-specific build recipe
                                                   -- (Snix: nix_compat::derivation::Derivation)
TYPE BuildEngine::Output                           -- Backend-specific build result
                                                   -- (Snix: PathInfo + Node)
TYPE BuildEngine::Error                            -- Structured error type
                                                   -- (Snix: SnixError)

-- Build engine methods (beyond evaluate/apply)

FN BuildEngine::plan_digest(&self, plan: &Self::Plan) -> Self::Digest
    -- Compute the content-addressed digest of a plan for deduplication.
    -- This closes the gap between the trait surface and the
    -- `JobId = hash(plan)` invariant. The backend knows how to
    -- canonically serialize its plan type; this method makes that
    -- knowledge available to the daemon without leaking serialization
    -- details. Synchronous, pure — no async needed.

-- Evaluation request (frontend → daemon)
-- Note: EvalRequest and other message structs use `#[non_exhaustive]`
-- for forward compatibility. Fields may be added in future versions
-- without breaking downstream consumers. Constructors must use
-- struct update syntax or builder patterns.

TYPE EvalTarget =
    File(PathBuf)                                  -- Evaluate a file path
  | Expression(String)                             -- Evaluate a string expression

TYPE ResolvedInput = {
    digest: D,                                     -- Content-addressed digest of this input
                                                   -- (generic over Digest impl)
    store_path: StorePath                          -- Store path where input is materialized
}

TYPE ComposerConfig = {
    atom_id: AtomId,                               -- Atom providing composition logic
    entry: String,                                 -- Evaluation entrypoint within the atom
    version: String                                -- Composer atom version
}

TYPE EvalRequest = #[non_exhaustive] {
    expression: EvalTarget,                        -- What to evaluate
    inputs: Map<String, ResolvedInput>,            -- Pre-resolved inputs (atoms, nix sources)
    composer: Option<ComposerConfig>,              -- Composer configuration (from [compose])
    eval_args: Vec<(String, String)>               -- Evaluation arguments (from [compose.args])
}

-- Build plan lifecycle (the central state machine)

TYPE BuildPlan =
    Cached(Vec<StorePath>)                         -- Artifacts exist in store
  | NeedsBuild(BuildEngine::Plan)                  -- Plan evaluated, outputs missing
  | NeedsEvaluation(AtomRef)                       -- Needs sandbox evaluation

-- Job management types (daemon-level)

TYPE JobId = BuildEngine::Digest                   -- Content-addressed: plan_digest(plan)
                                                   -- Identical plans produce identical JobIds,
                                                   -- enabling deduplication

TYPE JobStatus =
    Queued                                         -- Waiting in scheduler queue
  | Evaluating { message: String }                 -- Expression → plan
  | Building { phase: String, progress: Option<f32> }
  | Completed { outputs: Vec<ArtifactInfo> }
  | Failed { error: String, exit_code: Option<i32> }
  | Cancelled

TYPE ProgressEvent = {
    job_id: JobId,
    timestamp: SystemTime,
    status: JobStatus,
    log_line: Option<String>                       -- Structured log line (build output, eval trace)
}

-- Artifact metadata

TYPE ArtifactInfo<D: Digest> = {
    digest: D,
    store_path: StorePath,
    size: u64,
    references: Vec<StorePath>,                    -- Transitive runtime references
    deriver: Option<D>                             -- Plan digest that produced this artifact
}

-- Cache keys

TYPE EvalCacheKey<D: Digest> = (D, Vec<(String, String)>) -- (snapshot digest, eval_args)
```

#### `EvalRequest` and `[compose.args]`

The `EvalRequest.eval_args` field carries evaluation arguments originating from the lock file's `[compose.args]` section (see `lock-file-schema.md`). These are opaque key-value pairs passed through the daemon to the evaluation backend. The backend determines how to inject them into the evaluator (e.g., as Nix attrset overlays in the Snix backend).

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

The daemon MUST transmit `eval_args` faithfully to the backend. The daemon MUST NOT interpret, filter, or modify the contents of `eval_args` — they are opaque to the protocol layer.

#### Store Delegation Model

Eos does **not** own or implement an artifact store directly. The `ArtifactStore` trait abstracts over concrete store backends. Under the gRPC-first architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), the daemon (scheduler) does not interact with the `ArtifactStore` trait directly — workers do:

- Eval workers connect to snix store daemons via gRPC for store access during evaluation.
- Build workers interact with stores via the snix builder's gRPC store connections.
- The daemon dispatches jobs to workers via Cap'n Proto and consults the evaluation cache, but does not hold `ArtifactStore` instances.

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

**[eos-verification-obligation]**: Eos MUST fetch atom snapshots and verify their content digests before applying any operations. Eos MUST NOT evaluate or execute plans using unverified atom sources.
`VERIFIED: unverified`

**[eos-verify-ownership]**: Eos MUST verify that the publish transaction is validly signed and authorized according to the claim chain before using the snapshot.
`VERIFIED: unverified`

**[eos-verify-plugin-deps]**: Eos MUST fetch plugin dependencies and verify their hashes against the locked hashes using the specified algorithm (indicated by type tag).
`VERIFIED: unverified`

**[eos-no-unverified-execution]**: Eos MUST NOT execute, evaluate, or reference any unverified snapshot or plugin dependency in a build sandbox.
`VERIFIED: unverified`

**[eos-sandbox-network-containment]**: Build execution and evaluation MUST be executed in a restricted sandboxed environment. The sandbox MUST NOT have network access unless the plan is explicitly marked as a fixed-output derivation (carrying a pre-declared expected hash). Platform-specific sandbox selection — see [eos-snix-backend.md](eos-snix-backend.md) §Platform Sandbox Dispatch for the Snix-specific dispatch (Linux OCI/bwrap, macOS birdcage).
`VERIFIED: unverified`

**[eos-sandbox-host-isolation]**: The sandbox MUST NOT have write access to any part of the host filesystem outside the designated temporary sandbox build directory.
`VERIFIED: unverified`

**[eos-sandbox-reproducibility]**: The sandbox MUST NOT leak host environment variables, system times, CPU/hardware identifiers, or other sources of non-determinism into the build process. All inputs (environment variables, files, binaries) MUST be explicitly declared in the `BuildEngine::Plan`.
`VERIFIED: unverified`

**[eos-bisimulation-equivalence]**: Two Eos engine implementations MUST be behaviorally equivalent (bisimilar) under the same inputs: for any `AtomRef` and evaluation arguments, they MUST produce the same `BuildPlan` and equivalent outputs. This equivalence MUST hold across both local and daemon-served execution paths.
`VERIFIED: unverified`

**[eos-immutable-store]**: Once a build output is registered in the `ArtifactStore` at a `StorePath`, it MUST be read-only and immutable. Eos MUST NOT permit modification, overwriting, or deletion of existing store paths during execution.
`VERIFIED: unverified`

**[eos-cache-determinism]**: If the output corresponding to a given `BuildEngine::Plan` already exists in the `ArtifactStore` (build cache) or can be substituted, Eos MUST skip the build execution (`apply`) and return the cached `StorePath`.
`VERIFIED: unverified`

**[eos-eval-cache-determinism]**: If the plan corresponding to a given `EvalCacheKey` (atom snapshot digest + evaluation arguments) already exists in the evaluation cache, Eos MUST skip the evaluation phase and return the cached plan (or `BuildPlan`).
`VERIFIED: unverified`

**[eos-transitive-closure]**: The store path of any build output MUST be transitively self-contained. All dependencies of a store path MUST exist within the `ArtifactStore` and be immutable.
`VERIFIED: unverified`

**[eos-atom-index-ingest]**: Atoms that have been successfully processed (fetched, verified, evaluated, or built) MUST be ingested into the `AtomIndex` via `ingest()`. The daemon MUST NOT silently discard atom metadata after processing. This invariant ensures that every eos instance progressively accumulates discoverable knowledge about the atoms it has encountered.
`VERIFIED: unverified`

---

### Transitions

**[engine-plan]**: Evaluate an `AtomRef` to determine its build status.

- **PRE**: The `AtomRef` is resolved and its verified content snapshot is present. The scheduler holds a connection to registered eval and build workers.
- **POST**: Returns `Cached` if output is verified in the store, `NeedsBuild` if the plan is computed but output is missing, or `NeedsEvaluation` if dependency inputs are not yet resolved.
  `VERIFIED: unverified`

**[engine-apply]**: Execute a build plan to produce store outputs.

- **PRE**: The `BuildPlan` is `NeedsBuild`. All transitive build inputs are present and verified in the store. The backend's sandbox is initialized.
- **POST**: The builder executes inside the sandbox. Outputs are verified, registered as read-only, and committed to the `ArtifactStore` via the delegated store backend. Failed builds MUST abort and return an error without mutating existing store paths. The daemon transitions the corresponding `JobStatus` to `Completed` or `Failed` and pushes a `ProgressEvent` to all attached clients.
  `VERIFIED: unverified`

**[engine-eval]**: Evaluate an expression inside an atom snapshot to produce a build plan.

- **PRE**: The plan is `NeedsEvaluation`. The atom snapshot is verified and present. An `EvalRequest` is constructed with the appropriate `eval_args`, `composer`, and `inputs`.
- **POST**: The expression is evaluated in a sandboxed evaluation context. The evaluation produces a `BuildEngine::Plan`. The evaluation result is committed to the evaluation cache keyed by `EvalCacheKey`. Backend-specific details (e.g., Snix's dedicated eval thread for `!Send` types) are encapsulated within the `BuildEngine` implementation — see [eos-snix-backend.md](eos-snix-backend.md) §Eval Threading Model.
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

**[eval-caching-idempotency]**: Multiple concurrent evaluations of the same `EvalCacheKey` (snapshot digest + eval_args) MUST yield bisimilar `BuildEngine::Plan` results.

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
| `eos-sandbox-network-containment` | Sandbox profile check          | UNVERIFIED | Backend-specific sandbox restriction verification (see eos-snix-backend.md) |
| `eos-sandbox-host-isolation`      | Directory permission checks    | UNVERIFIED | Verification of filesystem write blocks                                     |
| `eos-sandbox-reproducibility`     | Binary hash equivalence        | UNVERIFIED | Double-build hash check verification                                        |
| `eos-bisimulation-equivalence`    | Equivalence tests              | UNVERIFIED | Parity checks between mock and concrete backends                            |
| `eos-immutable-store`             | Host filesystem audit          | UNVERIFIED | Verification of read-only enforcement via delegated store                   |
| `eos-cache-determinism`           | Cache hits test                | UNVERIFIED | Property-based tests for build cache                                        |
| `eos-eval-cache-determinism`      | Cache hits test                | UNVERIFIED | Property-based tests for evaluation cache with `EvalCacheKey`               |
| `eos-transitive-closure`          | Reference scanner test         | UNVERIFIED | Store scanner reference tracing validation                                  |
| `eos-atom-index-ingest`           | Integration tests              | UNVERIFIED | Verify atoms are ingested into AtomIndex after processing                   |
| `engine-plan`                     | State transition audit         | UNVERIFIED | Unit tests for plan transitions via daemon protocol                         |
| `engine-apply`                    | Sandbox execute tests          | UNVERIFIED | Integration tests for builder execution via backend                         |
| `engine-eval`                     | Sandbox eval tests             | UNVERIFIED | Integration tests for evaluation via backend (see eos-snix-backend.md)      |
| `no-undeclared-inputs`            | Environment sanitization check | UNVERIFIED | Env whitelist enforcement audit                                             |
| `no-speculative-writes`           | Aborted commit audit           | UNVERIFIED | Verify store cleanup after failure                                          |
| `no-dirty-store-paths`            | Integrity validation           | UNVERIFIED | Store post-hash checks audit                                                |
| `eval-caching-idempotency`        | Concurrent tests               | UNVERIFIED | Concurrency validation with `EvalCacheKey`                                  |
| `apply-cleanup-on-abort`          | Temporary leak check           | UNVERIFIED | Temporary path leakage validation                                           |

---

## Implications

1. **Daemon-Served Trait Surface**:
   The `BuildEngine`, `ArtifactStore`, and `AtomIndex` traits in `eos-core` are behavioral contracts — they specify what operations the system supports, what invariants hold, and what state transitions are permitted. Under the gRPC-first architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), `BuildEngine` is implemented inside worker processes (eval workers and build workers), not in the daemon. The daemon orchestrates the `engine-plan` → `engine-eval` → `engine-apply` transition chain by dispatching to workers via Cap'n Proto. The Cap'n Proto protocol (see [eos-network-protocol.md](eos-network-protocol.md)) projects these contracts onto the wire as the `EosDaemon`, `BuildJob`, and `AtomDiscovery` capabilities for client interaction.

2. **Backend Abstraction via Associated Types**:
   `BuildEngine::Digest`, `BuildEngine::Plan`, `BuildEngine::Output`, and `BuildEngine::Error` are associated types, not concrete types. `eos-core` carries zero dependency on any backend crate. The Snix backend binds `Digest = Blake3Digest`, `Plan = Derivation`, `Output = PathInfo + Node`, and `Error = SnixError` (see [eos-snix-backend.md](eos-snix-backend.md) §Type Declarations). Future backends (subprocess, Guix, remote delegation) bind different concrete types while preserving the same behavioral invariants. The `Digest` associated type is bounded by `crate::Digest`, ensuring all backends produce values that satisfy the trait's comparison and serialization requirements.

3. **Evaluation Caching as Primary Optimization Boundary**:
   Evaluation caching is the dominant cost-avoidance mechanism. The daemon MUST provide a storage-backed evaluation cache indexed by `EvalCacheKey` — the tuple of `(BuildEngine::Digest, eval_args)`. This cache SHOULD be publishable and substitutable in the same manner as the build cache, enabling pre-computed plans to be distributed across nodes.

4. **`EvalRequest` Carries `[compose.args]`**:
   The `eval_args` field in `EvalRequest` is the conduit through which frontend-specified configuration (target system, feature flags, override paths) reaches the evaluator. This field maps directly to the lock file's `[compose.args]` section. The daemon treats `eval_args` as opaque — it includes them in the `EvalCacheKey` for deterministic caching but does not interpret their contents. Backend implementations determine how `eval_args` influence evaluation (e.g., Snix injects them as Nix attrset overlays).

5. **Store Delegation, Not Ownership**:
   Eos does not own, implement, or manage the internal structure of the artifact store. The `ArtifactStore` trait is a delegation boundary. Under the gRPC-first architecture, the daemon does not hold `ArtifactStore` instances — workers access stores via gRPC to snix store daemons. The Snix backend delegates to three independent Snix services (`BlobService`, `DirectoryService`, `PathInfoService`) — see [eos-snix-backend.md](eos-snix-backend.md) §Three-Service Store Mapping. A future Cyphr/Coz backend would delegate to a Coz content-addressed store.

6. **Platform Sandbox Dispatch is Builder-Specific**:
   This spec mandates sandbox isolation ([eos-sandbox-network-containment], [eos-sandbox-host-isolation], [eos-sandbox-reproducibility]) but does not prescribe the sandbox implementation. Under the gRPC-first architecture, sandbox selection is the responsibility of the snix builder process (see [eos-sandboxing.md](eos-sandboxing.md)). Eval workers rely on snix's pure evaluation mode for confinement — no OS-level sandboxing is required (see [eos-sandboxing.md](eos-sandboxing.md) §Evaluation Isolation).

7. **Testing Strategy**:
   Write property-based tests for `BuildPlan` matching and transition permutations to verify that caching decisions are 100% deterministic. Mock `BuildEngine` implementations SHOULD be used to validate daemon-level invariants (deduplication, progress streaming, abort cleanup) independently of any concrete backend. Backend-specific invariants (eval threading, store mapping, sandbox isolation) are verified in their respective backend specs.

8. **Async Trait Ergonomics (Edition 2024)**:
   All async traits in `eos-core` (`BuildEngine`, `ArtifactStore`, `AtomIndex`) use native `async fn` in traits — no `#[async_trait]` macro. The `trait_variant::make` attribute generates both a `Local*` variant (non-Send, for single-threaded contexts like Cap'n Proto's `!Send` RPC loop) and a Send-bound variant (for the multi-threaded worker pool). This eliminates `Box<dyn Future>` heap allocation on every method call. Requires Rust edition 2024 (toolchain 1.90.0+).

9. **`plan_digest()` Closes the Deduplication Seam**:
   The `BuildEngine::plan_digest()` method is the sole bridge between a backend's opaque `Plan` type and the daemon's `JobId = hash(plan)` deduplication invariant. Without it, the daemon would need to know how to serialize backend-specific plan types — a layer violation. By making digest computation a trait method, the backend controls canonical serialization while the daemon operates purely on the resulting `Digest` value.
