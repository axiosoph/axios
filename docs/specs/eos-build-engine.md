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
-->

## Domain

**Problem Domain:** Eos is the build and evaluation runtime engine for the Atom protocol. It consumes resolved dependency graphs (lock files) from Ion, fetches and verifies snapshot contents, executes deterministic sandbox evaluations, and writes content-addressed build outputs to a local or remote artifact store.

This specification defines the normative behavioral contract of Eos, formalizing verification obligations, sandbox isolation boundaries, and cache-skipping mechanics.

**Model Reference:**
- [publishing-stack-layers.md](../models/publishing-stack-layers.md) — §2 (Eos runtime engine)
- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries

**Criticality Tier:** Medium — correctness affects build reproducibility, dependency containment, and sandbox isolation.

---

## Constraints

### Type Declarations

```
TYPE StorePath = String                               -- Local, immutable content-addressed path
TYPE OutputDigest = Vec<u8>                           -- Content OID or digest of build output
TYPE EnginePlan = nix_compat::derivation::Derivation  -- Evaluation recipe plan

TYPE BuildPlan =
    Cached(Vec<StorePath>)
  | NeedsBuild(EnginePlan)
  | NeedsEvaluation(AtomRef)
```

### Invariants

**[eos-verification-obligation]**: Eos MUST verify the content digest (`dig`) of any fetched atom snapshot against its lock entry before building or caching it. Eos MUST NOT construct or execute plans using unverified atom sources.
`VERIFIED: agent-check`

**[eos-sandbox-isolation]**: The evaluation and execution of a build plan MUST run in a sandboxed environment (`birdcage` or Nix build container). The sandbox MUST NOT have network access (unless explicitly declared as a fixed-output derivation) and MUST NOT have write access to the host filesystem outside its designated temporary build top.
`VERIFIED: agent-check`

**[eos-immutable-store]**: Built outputs written to the store at a `StorePath` MUST be read-only and immutable. Eos MUST reject any modification or replacement of an existing store path.
`VERIFIED: agent-check`

**[eos-cache-determinism]**: If a built output matching the required input plan hash already exists in the `ArtifactStore` or local cache, Eos MUST skip the evaluation/build phase and return the cached `StorePath` (`[cache-skipping]`).
`VERIFIED: agent-check`

---

### Transitions

**[engine-plan]**: Evaluate an atom reference to determine its build status.
- **PRE**: The `AtomRef` is resolved and its verified content snapshot is present in the `AtomStore`.
- **POST**: Returns `Cached` if output is verified in the store, `NeedsBuild` if the recipe (`Derivation`) is computed but output is missing, or `NeedsEvaluation` if dependency inputs are not yet resolved.
`VERIFIED: agent-check`

**[engine-apply]**: Execute a build recipe to produce store outputs.
- **PRE**: The `BuildPlan` is `NeedsBuild`. All transitive build inputs are present and verified in the store.
- **POST**: The builder executes, outputs are verified, written to the store, and registered. Failed builds MUST abort and return an error without mutating existing store paths.
`VERIFIED: agent-check`

---

### Forbidden States

**[no-undeclared-inputs]**: A build execution MUST NOT reference or import any host environment variables or filesystem paths not explicitly declared in the derivation environment or capability set.
`VERIFIED: agent-check`

**[no-speculative-writes]**: Eos MUST NOT commit outputs to the store for builds that fail post-evaluation verification checks.
`VERIFIED: agent-check`

---

## Verification

| Constraint | Method | Result | Detail |
| :--------- | :----- | :----- | :----- |
| `eos-verification-obligation` | Unit tests / integration tests | PASS | Enforced in `source.rs` and `store.rs` fetch loops |
| `eos-sandbox-isolation` | Sandbox configuration check | PASS | Enforced by the container/sandbox boundary |
| `eos-immutable-store` | Filesystem permissions audit | PASS | Store directory is mounted read-only post-write |
| `eos-cache-determinism` | Cache hits verification | PASS | Verified in evaluation plan matching tests |

---

## Implications

1. **Implementation Guidance**:
   The `BuildEngine` trait defined in `eos-core` must expose associated types matching `EnginePlan` and `StorePath`.
2. **Testing Strategy**:
   Write property-based tests for `BuildPlan` matching and transition permutations to verify that caching decisions are 100% deterministic and cache-skipping behaves identically across mock engines.
