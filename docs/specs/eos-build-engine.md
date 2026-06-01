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

Eos is **not** an embedded library that callers link against. The `BuildEngine` and `ArtifactStore` traits defined in `eos-core` specify the *behavioral contract* — what the daemon does, what invariants it upholds, and what state transitions it permits. The Cap'n Proto protocol defined in [eos-network-protocol.md](eos-network-protocol.md) is the *wire projection* of these traits — how clients invoke them over the wire. The Snix implementation defined in [eos-snix-backend.md](eos-snix-backend.md) is one *concrete backend* fulfilling the `BuildEngine` and `ArtifactStore` contracts.

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

TYPE Digest = [u8; 32]                             -- Algorithm-agile content digest
                                                   -- (currently BLAKE3, future: Coz digest)
TYPE StorePath = opaque String                     -- Immutable path in ArtifactStore
                                                   -- (backend validates format internally)
TYPE AtomRef = { id: AtomId, digest: Digest }      -- Cryptographic snapshot reference

-- Build engine associated types (generic over backend)

TYPE BuildEngine::Plan                             -- Backend-specific build recipe
                                                   -- (Snix: nix_compat::derivation::Derivation)
TYPE BuildEngine::Output                           -- Backend-specific build result
                                                   -- (Snix: PathInfo + Node)
TYPE BuildEngine::Error                            -- Structured error type
                                                   -- (Snix: SnixError)

-- Evaluation request (frontend → daemon)

TYPE EvalTarget =
    File(PathBuf)                                  -- Evaluate a file path
  | Expression(String)                             -- Evaluate a string expression

TYPE ResolvedInput = {
    digest: Digest,                                -- Content-addressed digest of this input
    store_path: StorePath                          -- Store path where input is materialized
}

TYPE ComposerConfig = {
    atom_id: AtomId,                               -- Atom providing composition logic
    entry: String,                                 -- Evaluation entrypoint within the atom
    version: String                                -- Composer atom version
}

TYPE EvalRequest = {
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

TYPE JobId = Digest                                -- Content-addressed: hash(plan)
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

TYPE ArtifactInfo = {
    digest: Digest,
    store_path: StorePath,
    size: u64,
    references: Vec<StorePath>,                    -- Transitive runtime references
    deriver: Option<Digest>                        -- Plan digest that produced this artifact
}

-- Cache keys

TYPE EvalCacheKey = (Digest, Vec<(String, String)>) -- (snapshot digest, eval_args)
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

Eos does **not** own or implement an artifact store directly. The `ArtifactStore` trait abstracts over concrete store backends. The daemon delegates all store operations to the backend's `ArtifactStore` implementation:

- **Current**: Snix store — delegates to `BlobService` + `DirectoryService` + `PathInfoService` (see [eos-snix-backend.md](eos-snix-backend.md) §Three-Service Store Mapping)
- **Future**: Cyphr/Coz store — content-addressed via Coz digests

The `ArtifactStore` trait surface (`has`, `get_info`, `import`, `list`) is intentionally minimal. Backend crates provide the concrete wiring to underlying storage services.

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

---

### Transitions

**[engine-plan]**: Evaluate an `AtomRef` to determine its build status.
- **PRE**: The `AtomRef` is resolved and its verified content snapshot is present. The daemon holds a valid `BuildEngine` backend.
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

| Constraint | Method | Result | Detail |
| :--------- | :----- | :----- | :----- |
| `eos-verification-obligation` | Unit tests | UNVERIFIED | Pending implementation of atom snapshot fetcher |
| `eos-verify-ownership` | Cryptographic sign check | UNVERIFIED | Pending integration of Coz verification |
| `eos-verify-plugin-deps` | Hash verification tests | UNVERIFIED | Pending lock-reader implementation |
| `eos-no-unverified-execution` | Sandbox integration tests | UNVERIFIED | Verification deferred until sandbox integration |
| `eos-sandbox-network-containment` | Sandbox profile check | UNVERIFIED | Backend-specific sandbox restriction verification (see eos-snix-backend.md) |
| `eos-sandbox-host-isolation` | Directory permission checks | UNVERIFIED | Verification of filesystem write blocks |
| `eos-sandbox-reproducibility` | Binary hash equivalence | UNVERIFIED | Double-build hash check verification |
| `eos-bisimulation-equivalence` | Equivalence tests | UNVERIFIED | Parity checks between mock and concrete backends |
| `eos-immutable-store` | Host filesystem audit | UNVERIFIED | Verification of read-only enforcement via delegated store |
| `eos-cache-determinism` | Cache hits test | UNVERIFIED | Property-based tests for build cache |
| `eos-eval-cache-determinism` | Cache hits test | UNVERIFIED | Property-based tests for evaluation cache with `EvalCacheKey` |
| `eos-transitive-closure` | Reference scanner test | UNVERIFIED | Store scanner reference tracing validation |
| `engine-plan` | State transition audit | UNVERIFIED | Unit tests for plan transitions via daemon protocol |
| `engine-apply` | Sandbox execute tests | UNVERIFIED | Integration tests for builder execution via backend |
| `engine-eval` | Sandbox eval tests | UNVERIFIED | Integration tests for evaluation via backend (see eos-snix-backend.md) |
| `no-undeclared-inputs` | Environment sanitization check | UNVERIFIED | Env whitelist enforcement audit |
| `no-speculative-writes` | Aborted commit audit | UNVERIFIED | Verify store cleanup after failure |
| `no-dirty-store-paths` | Integrity validation | UNVERIFIED | Store post-hash checks audit |
| `eval-caching-idempotency` | Concurrent tests | UNVERIFIED | Concurrency validation with `EvalCacheKey` |
| `apply-cleanup-on-abort` | Temporary leak check | UNVERIFIED | Temporary path leakage validation |

---

## Implications

1. **Daemon-Served Trait Surface**:
   The `BuildEngine` and `ArtifactStore` traits in `eos-core` are behavioral contracts — they specify what operations the daemon supports, what invariants hold, and what state transitions are permitted. Clients never invoke these traits directly. Instead, the Cap'n Proto protocol (see [eos-network-protocol.md](eos-network-protocol.md)) projects these contracts onto the wire as the `EosDaemon` and `BuildJob` capabilities. A `submitBuild` RPC corresponds to the `engine-plan` → `engine-eval` → `engine-apply` transition chain; `attachProgress` corresponds to `ProgressEvent` streaming.

2. **Backend Abstraction via Associated Types**:
   `BuildEngine::Plan`, `BuildEngine::Output`, and `BuildEngine::Error` are associated types, not concrete types. `eos-core` carries zero dependency on any backend crate. The Snix backend binds `Plan = Derivation`, `Output = PathInfo + Node`, and `Error = SnixError` (see [eos-snix-backend.md](eos-snix-backend.md) §Type Declarations). Future backends (subprocess, Guix, remote delegation) bind different concrete types while preserving the same behavioral invariants.

3. **Evaluation Caching as Primary Optimization Boundary**:
   Evaluation caching is the dominant cost-avoidance mechanism. The daemon MUST provide a storage-backed evaluation cache indexed by `EvalCacheKey` — the tuple of `(Digest, eval_args)`. This cache SHOULD be publishable and substitutable in the same manner as the build cache, enabling pre-computed plans to be distributed across nodes.

4. **`EvalRequest` Carries `[compose.args]`**:
   The `eval_args` field in `EvalRequest` is the conduit through which frontend-specified configuration (target system, feature flags, override paths) reaches the evaluator. This field maps directly to the lock file's `[compose.args]` section. The daemon treats `eval_args` as opaque — it includes them in the `EvalCacheKey` for deterministic caching but does not interpret their contents. Backend implementations determine how `eval_args` influence evaluation (e.g., Snix injects them as Nix attrset overlays).

5. **Store Delegation, Not Ownership**:
   Eos does not own, implement, or manage the internal structure of the artifact store. The `ArtifactStore` trait is a delegation boundary. The Snix backend delegates to three independent Snix services (`BlobService`, `DirectoryService`, `PathInfoService`) — see [eos-snix-backend.md](eos-snix-backend.md) §Three-Service Store Mapping. A future Cyphr/Coz backend would delegate to a Coz content-addressed store. The daemon interacts with the store exclusively through the trait surface.

6. **Platform Sandbox Dispatch is Backend-Specific**:
   This spec mandates sandbox isolation ([eos-sandbox-network-containment], [eos-sandbox-host-isolation], [eos-sandbox-reproducibility]) but does not prescribe the sandbox implementation. Platform-specific sandbox selection (Linux OCI/bwrap, macOS birdcage, remote gRPC builder) is the responsibility of the backend crate. See [eos-snix-backend.md](eos-snix-backend.md) §Platform Sandbox Dispatch for the Snix-specific dispatch table and detection logic.

7. **Testing Strategy**:
   Write property-based tests for `BuildPlan` matching and transition permutations to verify that caching decisions are 100% deterministic. Mock `BuildEngine` implementations SHOULD be used to validate daemon-level invariants (deduplication, progress streaming, abort cleanup) independently of any concrete backend. Backend-specific invariants (eval threading, store mapping, sandbox isolation) are verified in their respective backend specs.
