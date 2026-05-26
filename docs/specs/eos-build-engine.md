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

**Problem Domain:** Eos is the build and evaluation runtime engine for the Axios publishing stack. It acts as the execution substrate, bridging L3 (Ion frontend planning) and L1 (Atom content-addressing). Eos consumes resolved dependency graphs (lock files) from Ion, fetches and verifies snapshot contents from git mirrors, executes deterministic sandbox evaluations to produce build plans (derivations), caches evaluation results to prevent redundant computation, and compiles hermetic build outputs registered in an immutable artifact store.

By leveraging the cryptographic nature of Atoms, Eos implements two levels of cache-skipping:
1. **Evaluation caching**: If the atom snapshot and evaluation arguments are identical, evaluation is bypassed to retrieve the pre-computed build recipe.
2. **Build caching**: If the build recipe (derivation) matches an existing store path, the build execution is skipped entirely.

**Model Reference:**
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore), §3.2 (BuildSession)
- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries and capability advertisement

**Criticality Tier:** Medium — correctness governs build reproducibility, sandbox isolation, host system integrity, and dependency containment.

---

## Constraints

### Type Declarations

We define the following type signatures to model Eos state and operations:

```
TYPE StorePath = String                               -- Absolute, immutable path in ArtifactStore
TYPE OutputDigest = Blake3Digest                       -- Blake3 hash of output contents
TYPE EnginePlan = nix_compat::derivation::Derivation  -- Hermetic build recipe
TYPE AtomRef = { id: AtomId, dig: Blake3Digest }      -- Cryptographic snapshot reference
TYPE EvalArgs = Map<String, String>                   -- Evaluation parameters
TYPE EvalCacheKey = (Blake3Digest, EvalArgs)          -- Key for evaluation caching

TYPE BuildPlan =
    Cached(Vec<StorePath>)                            -- Artifacts exist in store
  | NeedsBuild(EnginePlan)                            -- Plan evaluated, outputs missing
  | NeedsEvaluation(AtomRef)                          -- Needs sandbox evaluation
```

---

### Invariants

**[eos-verification-obligation]**: Eos MUST fetch atom snapshots and verify their Blake3 content digests (`dig`) before applying any operations. Eos MUST NOT evaluate or execute plans using unverified atom sources.
`VERIFIED: unverified`

**[eos-verify-ownership]**: Eos MUST verify that the publish transaction at `czd` is validly signed and authorized according to the claim chain before using the snapshot.
`VERIFIED: unverified`

**[eos-verify-plugin-deps]**: Eos MUST fetch plugin dependencies and verify their hashes against the locked hashes using the specified algorithm (indicated by type tag).
`VERIFIED: unverified`

**[eos-no-unverified-execution]**: Eos MUST NOT execute, evaluate, or reference any unverified snapshot or plugin dependency in a build sandbox.
`VERIFIED: unverified`

**[eos-sandbox-network-containment]**: Build execution and evaluation MUST be executed in a restricted sandboxed environment (using `birdcage`, bubblewrap, or kernel namespaces). The sandbox MUST NOT have network access unless the derivation is explicitly marked as a fixed-output derivation (carrying a pre-declared expected hash).
`VERIFIED: unverified`

**[eos-sandbox-host-isolation]**: The sandbox MUST NOT have write access to any part of the host filesystem outside the designated temporary sandbox build directory.
`VERIFIED: unverified`

**[eos-sandbox-reproducibility]**: The sandbox MUST NOT leak host environment variables, system times, CPU/hardware identifiers, or other sources of non-determinism into the build process. All inputs (environment variables, files, binaries) MUST be explicitly declared in the `EnginePlan`.
`VERIFIED: unverified`

**[eos-bisimulation-equivalence]**: Two Eos engine implementations (e.g., LocalEngine vs RemoteEngine) MUST be behaviorally equivalent (bisimilar) under the same inputs: for any `AtomRef` and evaluation arguments, they MUST produce the same `BuildPlan` and equivalent outputs.
`VERIFIED: unverified`

**[eos-immutable-store]**: Once a build output is registered in the `ArtifactStore` at a `StorePath`, it MUST be read-only and immutable. Eos MUST NOT permit modification, overwriting, or deletion of existing store paths during execution.
`VERIFIED: unverified`

**[eos-cache-determinism]**: If the output corresponding to a given `EnginePlan` already exists in the `ArtifactStore` (build cache) or can be substituted, Eos MUST skip the build execution (`apply`) and return the cached `StorePath`.
`VERIFIED: unverified`

**[eos-eval-cache-determinism]**: If the evaluation plan (the `EnginePlan`) corresponding to a given `AtomSnapshotDigest` and evaluation arguments already exists in the evaluation cache, Eos MUST skip the evaluation phase and return the cached `EnginePlan` (or `BuildPlan`).
`VERIFIED: unverified`

**[eos-transitive-closure]**: The store path of any build output MUST be transitively self-contained. All dependencies of a store path MUST exist within the `ArtifactStore` and be immutable.
`VERIFIED: unverified`

---

### Transitions

**[engine-plan]**: Evaluate an `AtomRef` to determine its build status.
- **PRE**: The `AtomRef` is resolved and its verified content snapshot is present in the `AtomStore`.
- **POST**: Returns `Cached` if output is verified in the store, `NeedsBuild` if the recipe (`Derivation`) is computed but output is missing, or `NeedsEvaluation` if dependency inputs are not yet resolved.
`VERIFIED: unverified`

**[engine-apply]**: Execute a build recipe to produce store outputs.
- **PRE**: The `BuildPlan` is `NeedsBuild`. All transitive build inputs are present and verified in the store.
- **POST**: The builder executes inside the sandbox. Outputs are verified, registered as read-only, and added to the store. Failed builds MUST abort and return an error without mutating existing store paths.
`VERIFIED: unverified`

**[engine-eval]**: Evaluate an expression inside an atom snapshot to produce a build recipe.
- **PRE**: The plan is `NeedsEvaluation`. The atom snapshot is verified and present.
- **POST**: The expression is evaluated in a sandboxed evaluation context (such as a restricted Nix/Tvix VM), producing an `EnginePlan`. The evaluation result is committed to the evaluation cache.
`VERIFIED: unverified`

---

### Forbidden States

**[no-undeclared-inputs]**: A build execution MUST NOT reference or import any host environment variables or filesystem paths not explicitly declared in the derivation environment or capability set.
`VERIFIED: unverified`

**[no-speculative-writes]**: Eos MUST NOT commit outputs to the store for builds that fail post-evaluation verification checks.
`VERIFIED: unverified`

**[no-dirty-store-paths]**: Eos MUST NOT register a store path in the `ArtifactStore` unless its contents have been verified to match the content hash of the built output.
`VERIFIED: unverified`

---

### Behavioral Properties

**[eval-caching-idempotency]**: Multiple concurrent evaluations of the same `(AtomSnapshotDigest, EvalArgs)` MUST yield bisimilar `EnginePlan` results.
- **Type**: Safety
`VERIFIED: unverified`

**[apply-cleanup-on-abort]**: If a build application fails or aborts, all temporary sandbox resources and partial outputs MUST be completely garbage collected and MUST NOT leak into the `ArtifactStore`.
- **Type**: Safety
`VERIFIED: unverified`

---

## Verification

| Constraint | Method | Result | Detail |
| :--------- | :----- | :----- | :----- |
| `eos-verification-obligation` | Unit tests | UNVERIFIED | Pending implementation of AtomStore fetcher |
| `eos-verify-ownership` | Cryptographic sign check | UNVERIFIED | Pending integration of Coz verification |
| `eos-verify-plugin-deps` | Hash verification tests | UNVERIFIED | Pending lock-reader implementation |
| `eos-no-unverified-execution` | Sandbox integration tests | UNVERIFIED | Verification deferred until sandbox integration |
| `eos-sandbox-network-containment` | Birdcage profile check | UNVERIFIED | Birdcage capability restrictions verification |
| `eos-sandbox-host-isolation` | Directory permission checks | UNVERIFIED | Verification of filesystem write blocks |
| `eos-sandbox-reproducibility` | Binary hash equivalence | UNVERIFIED | Double-build hash check verification |
| `eos-bisimulation-equivalence` | Equivalence tests | UNVERIFIED | Parity checks between mock and LocalEngine |
| `eos-immutable-store` | Host filesystem audit | UNVERIFIED | Verification of chmod 0444 and readonly mounts |
| `eos-cache-determinism` | Cache hits test | UNVERIFIED | Property-based tests for build cache |
| `eos-eval-cache-determinism` | Cache hits test | UNVERIFIED | Property-based tests for evaluation cache |
| `eos-transitive-closure` | Reference scanner test | UNVERIFIED | Store scanner reference tracing validation |
| `engine-plan` | State transition audit | UNVERIFIED | Unit tests for plan transitions |
| `engine-apply` | Sandbox execute tests | UNVERIFIED | Integration tests for builder execution |
| `engine-eval` | Sandbox eval tests | UNVERIFIED | Integration tests for Nix/Tvix evaluation |
| `no-undeclared-inputs` | Environment sanitization check | UNVERIFIED | Env whitelist enforcement audit |
| `no-speculative-writes` | Aborted commit audit | UNVERIFIED | Verify store cleanup after failure |
| `no-dirty-store-paths` | Integrity validation | UNVERIFIED | Store post-hash checks audit |
| `eval-caching-idempotency` | Concurrent tests | UNVERIFIED | Concurrency validation tests |
| `apply-cleanup-on-abort` | Temporary leak check | UNVERIFIED | Temporary path leakage validation |

---

## Implications

1. **Snix Type Mapping**:
   In Snix/Tvix integration, `EnginePlan` maps to `nix_compat::derivation::Derivation`. `StorePath` maps to `nix_compat::store_path::StorePath<String>`. Evaluation outputs map to `snix_castore::Node`. Eos must preserve this mapping internally to reuse the high-performance Nix-compatible parser and executor.

2. **Evaluation Caching Implementation**:
   Evaluation caching is the primary optimization boundary for Eos. Eos must provide a storage-backed `EvalCache` that indexes `(Blake3Digest, EvalArgs) -> EnginePlan`. This cache should be publishable/substitutable just like the build cache.

3. **Sandboxed Evaluation**:
   Evaluation must be executed inside a sandboxed wrapper (e.g. Nix restricted mode or Snix evaluation in restricted state). The evaluator must only be allowed to read from the atom snapshot directory and designated store paths. Any access to paths outside this closure must cause an evaluation error.

4. **Testing Strategy**:
   Write property-based tests for `BuildPlan` matching and transition permutations to verify that caching decisions are 100% deterministic and cache-skipping behaves identically across mock engines.
