# ADR-0001: Monorepo with independent workspace architecture

- **Status**: PROPOSED (SUPERSEDED IN PART by ADR-0003)
- **Date**: 2026-02-07 (revised)
- **Deciders**: nrd
- **Source**: [Plan](../plans/ion-atom-restructuring.md) | [Sketch](../../.sketches/2026-02-07-ion-atom-restructuring.md)
- **Supersedes**: Revisions 1–3
- **Related**: [ADR-0002](0002-decoupling-snix-backend.md), [ADR-0003](0003-composable-deployment-modes.md)

## Context

The eka project has validated its core concepts over ~2 years. The codebase
tightly couples three concerns: the Atom Protocol (identity, addressing,
publishing), the runtime engine (evaluation, builds, store management), and
the user frontend (CLI, manifests, resolution).

Previous revisions made incremental progress:

- **Rev 1**: Separated atom from ion. Runtime stayed in ion.
- **Rev 2**: Extracted runtime into eos. Manifest landed in `shared/manifest/`.
- **Rev 3**: Removed `shared/`, made manifest an abstract trait. Store model was underdeveloped, eos crate structure was unclear, and the cryptographic chain was not identified as the motivating principle.

This revision fills the remaining gaps.

### The cryptographic chain

Every atom traces a content-addressed path from identity to output:

```
AtomId → Version → Revision → Plan → Output
 (czd)   (semver)   (commit)  (drv) (artifact)
```

"Plan" is the abstract term. For the snix engine, a plan is a derivation
(`.drv`). `BuildEngine::Plan` is an associated type, so other engines can
define their own format.

Each step is verifiable and cacheable independently, which makes cache-skipping
possible at every stage. The chain is a DAG: each atom's lock entry carries a
`requires` field listing content-addressed digests of its transitive
dependencies, so the lock file captures the full graph.

### Forces

- **Cache-skipping is the value proposition.** Every stage of the chain must be independently verifiable and skippable. BuildEngine makes this explicit.
- **Three distinct stores.** Registries (publishing, immutable), working stores (collected from disparate sources), and artifact stores (content-addressed blobs). Different semantics — cannot be conflated.
- **The store is the interface.** Ion hands atoms to eos through `AtomStore`. Published and dev atoms enter the same store via `ingest`. Eos never knows where atoms came from.
- **Atom is generic.** Manifest-agnostic, version-scheme-agnostic. Any ecosystem can publish atoms.
- **Embedded default, daemon opt-in (superseded by ADR-0003).** Cargo, single-user Nix, Go — none require daemons. Neither should ion. (Refined in [ADR-0003](0003-composable-deployment-modes.md) to support three composable deployment modes: Monolithic Ion, Monolithic Eos, and Distributed Eos.)
- **Eos will be large.** Early modularization (eos-core + eos-snix + eos-proto + eos-daemon + eos) prevents a monolith.

## Decision

Three independent Cargo workspaces in a monorepo, mapped to a 5-layer stack:

```
Cyphr (L0) → Atom (L1) → Eos (L2) → Ion (L3) → Plugins (L4)
```

### atom/ — Protocol workspace (L1)

| Crate       | Responsibility                                                                 | Dependencies               |
| :---------- | :----------------------------------------------------------------------------- | :------------------------- |
| `atom-id`   | Identity primitives: `Label`, `Tag`, `AtomDigest`, `AtomId<R>`, `Compute`      | ≤ 5 deps                   |
| `atom-uri`  | URI parsing, version trait abstraction                                         | atom-id, `nom`             |
| `atom-core` | Traits: `AtomSource`, `AtomRegistry`, `AtomStore`, `Manifest`, `VersionScheme` | atom-id, atom-uri          |
| `atom-git`  | Git backend: implements `AtomRegistry` + `AtomStore`                           | atom-core, `gix`, `snix-*` |

**Store traits** — three-layer model with a read super-trait:

```rust
/// Read-only atom access
trait AtomSource {
    fn resolve(&self, id, version) → AtomContent;
    fn discover(&self, anchor) → Vec<(Label, AtomId)>;
}

/// Publishing — registries, git backends, Cyphr
trait AtomRegistry: AtomSource {
    fn claim(&self, anchor, label) → AtomId;
    fn publish(&self, id, version, snapshot) → ();
}

/// Working store — atoms collected from disparate sources
trait AtomStore: AtomSource {
    fn ingest(&self, source: &dyn AtomSource) → ();
    fn import_path(&self, path) → ();
    fn contains(&self, id, version) → bool;
}
// fetch(id) → Path removed: couples trait to local storage,
// contradicts store-agnosticism. Content retrieval is covered
// by AtomSource::resolve. import_path added per store transfer
// design (see below).
```

`AtomStore::ingest(&dyn AtomSource)` is the store-to-store transfer mechanism.
Registries, other stores, and dev workspaces all implement `AtomSource`, so
ingestion from any source uses the same codepath.

### eos/ — Runtime engine workspace (L2)

| Crate        | Responsibility                                     | Dependencies                        |
| :----------- | :------------------------------------------------- | :---------------------------------- |
| `eos-core`   | `BuildEngine` trait with plan/apply + assoc. types | atom-core                           |
| `eos-snix`   | Snix-specific store and evaluator implementations   | eos-core, `nix-compat`, `snix-*`    |
| `eos-proto`  | Cap'n Proto interface schemas and serialization    | `capnp`, `capnp-rpc`                |
| `eos-daemon` | Dynamic build scheduler and RPC server daemon      | eos-core, eos-proto, eos-snix, eos  |
| `eos`        | Core orchestration engine and worker registry      | eos-core, eos-snix                  |

**BuildEngine** — plan/apply with cache-skipping:

```rust
trait BuildEngine {
    type Plan;    // for snix: Derivation
    type Output;
    type Error;

    fn plan(&self, atom: &AtomRef) → Result<BuildPlan<Self::Plan>>;
    fn apply(&self, plan: &BuildPlan<Self::Plan>) → Result<Vec<Self::Output>>;
}

enum BuildPlan<P> {
    Cached { outputs: Vec<ArtifactRef> },   // output exists, trusted
    NeedsBuild { plan: P },                  // plan cached, build needed
    NeedsEvaluation { atom: AtomRef },       // nothing cached
}
```

Associated types let each engine define its own formats. Object safety is not
needed — ion uses compile-time generics via feature flags to select the engine.

**ArtifactStore** — content-addressed build outputs (snix blob model):

```rust
trait ArtifactStore {
    fn store(&self, digest, data) → ();
    fn fetch(&self, digest) → Box<dyn Read>;
    fn exists(&self, digest) → bool;
    fn check_substitute(&self, digests) → Vec<bool>;
}
```

Thin wrapper over snix BlobService/DirectoryService. The trait is eos's
contract; snix is the default backend. (Note: Refined in [ADR-0002](0002-decoupling-snix-backend.md) to decouple the snix runtime by accessing these store backends over remote gRPC service boundaries.)

### ion/ — Frontend workspace (L3)

| Crate          | Responsibility                                       | Dependencies                        |
| :------------- | :--------------------------------------------------- | :---------------------------------- |
| `ion-manifest` | Concrete `ion.toml` format, Compose system (With/As) | atom-core, atom-id                  |
| `ion-resolve`  | SAT resolver, dependency variant formulation         | atom-core, ion-manifest             |
| `ion-lock`     | Unified lockfile management and serialization        | atom-id, ion-manifest               |
| `ion-eos`      | Bridge between frontend and build engine             | ion-manifest, eos-core              |
| `ion-cli`      | CLI interface and command dispatch                   | ion-*, eos-core, atom-core          |

### Monorepo layout

```
axios/
├── atom/                        ← protocol workspace (L1)
│   ├── Cargo.toml
│   ├── atom-id/
│   ├── atom-uri/
│   ├── atom-core/
│   └── atom-git/
├── eos/                         ← runtime engine workspace (L2)
│   ├── Cargo.toml
│   ├── eos-core/
│   ├── eos-snix/
│   ├── eos-proto/
│   ├── eos-daemon/
│   └── eos/
├── ion/                         ← frontend workspace (L3)
│   ├── Cargo.toml
│   ├── ion-manifest/
│   ├── ion-resolve/
│   ├── ion-lock/
│   ├── ion-eos/
│   └── ion-cli/
├── docs/
│   ├── plans/
│   └── adr/
└── README.md
```

### Data flow

```
AtomRegistry (publishing front)
        ↓ resolve/discover
    AtomSource (read interface)
        ↓ ingest
    AtomStore (working store)  ←── import_path (local atoms copied from disk)
        ↓ read
    BuildEngine (plan/apply)
        ↓ produce
    ArtifactStore (build outputs)
```

Ion populates the AtomStore. Eos reads from it. The store is the handoff.

- **Embedded** (`--features embedded-engine`, default): `eos::Engine` compiled into ion-cli. `ion build` works immediately. (Note: Superseded in part by [ADR-0003](0003-composable-deployment-modes.md) where monolithic and distributed wiring are achieved via feature flags and dependency injection rather than divergent engine types.)
- **Client** (future): `RemoteEngine` connects to eos daemon. Distributed builds, shared caches. (Note: Superseded in part by [ADR-0003](0003-composable-deployment-modes.md).)

Both satisfy `BuildEngine`. Ion's code is generic: `fn run(engine: impl BuildEngine)`.

## Consequences

### Positive

- The cryptographic chain maps directly to plan/apply cache-skipping. The DAG structure via `requires` captures the full dependency graph.
- BuildEngine + ArtifactStore are in place. Distributed eos slots in without touching ion.
- Any ecosystem can publish atoms by implementing Manifest.
- AtomSource as universal read interface enables mirrors, syndicated stores, and dev workspaces through one mechanism.
- ArtifactStore enables binary caches and globally syndicated blob stores.
- Local atoms are copied into the same store from disk — no special codepath once they're ingested.
- Runtime is in eos from day one. Contributor isolation via workspace boundaries.

### Negative

- 10 crate skeletons up front. Significant scaffolding.
- Trait changes in atom-core propagate through eos and ion.
- BuildEngine plan/apply is designed from prior art, not experience. May need revision.
- Manifest/VersionScheme generics permeate. Accepted cost of abstraction.

### Risks accepted

- ~30% chance of trait signature breakage when Cyphr integrates.
- BuildEngine plan/apply may need refinement as cache-skipping edge cases emerge.
- ArtifactStore wrapper may diverge from snix as snix evolves.
- atom-uri requires surgery: `LocalAtom` moves to ion, `gix::Url` gets genericized.

## Alternatives considered

**Shared manifest crate** (rev 2): Treated manifest as neutral infrastructure. "Shared" was a code smell. Rejected — manifest follows the VersionScheme pattern.

**Concrete Derivation type in eos-core** (rev 3): Hardcoded nix derivations. Rejected — the associated `Plan` type costs nothing and future-proofs against post-nix formats.

**Always-daemon for eos**: Unified architecture. Rejected — unacceptable friction for solo dev. `ion build` must work without process management.

**Single AtomStore trait for publishing and working**: One trait, role enum. Rejected — publishing (append-only, signed, distributed) and working (mutable, local, collected) have different operations and semantics.

**eos as single crate**: Simpler initially. Rejected — eos will be the largest component. Early modularization prevents costly extraction later.## Simplicity and Volatility Boundaries (Hickey/Lowy Audits)

The 5-layer monorepo structure implements spatial simplicity and temporal volatility insulation at the codebase scale:

1. **Spatial Simplicity (Hickey Audit):**
   - **Strict Downward Dependency Layers**: The 5-layer architecture (`Cyphr (L0) → Atom (L1) → Eos (L2) → Ion (L3) → Plugins (L4)`) strictly prohibits circular dependencies. Lower layers have no reference to or knowledge of upper layers. This isolates concerns cleanly and keeps the foundation simple.
   - **Decoupled Stores**: Immutable source registries (`AtomRegistry`), mutable local working stores (`AtomStore`), and content-addressed artifact stores (`ArtifactStore`) are modeled as separate logical interfaces. They are never conflated or unified into a single store type, avoiding complected states.

2. **Temporal Volatility (Lowy Audit):**
   - **Volatility Axis (User Frontends and Manifests)**: CLI requirements, manifest syntaxes (`ion.toml`), and dependency resolution strategies are the most volatile parts of the publishing stack. By isolating these entirely in L3 (`ion/`), we protect the stability of the L1 protocol and L2 engine cores from frequent frontend changes.
   - **Stability Cores**: The identity layer (`atom-id`) and build contracts (`eos-core`) represent the stable cores of the stack. They contain virtually no dependencies and change only when fundamental protocol changes occur, insulating the system from external package upgrades or dependency decay.

## Formal backing

The trait boundaries described in this ADR are formally validated in
[publishing-stack-layers.md](../models/publishing-stack-layers.md)
using coalgebras (behavioral observation), session types (protocol
ordering), and an olog (domain ontology). Key validated properties:

- **Implementation interchangeability:** Bisimulation on all trait
  coalgebras guarantees that any two implementations producing the
  same observations are interchangeable — formally justifying the
  embedded/daemon/remote deployment modes.
- **Operation ordering:** Session types enforce claim-before-publish,
  plan-before-apply, and BuildPlan variant handling as protocol
  invariants.
- **Scheduling correctness:** Parallel builds are non-interfering
  (plan is observation), work-stealing preserves session type safety,
  and scheduling strategy is bisimulation-invariant.
- **Error asymmetry:** Plan failure has no delegatable continuation;
  apply failure does. Recovery strategy selection belongs to eos.
- **Sync-first validation:** The coalgebra and session type structures
  extend to async without modification, validating KD-14.
