# ADR-0001: Monorepo with Independent Workspace Architecture (Revision 4)

- **Status**: PROPOSED
- **Date**: 2026-02-07 (revised)
- **Deciders**: nrd
- **Source**: [Plan](../plans/ion-atom-restructuring.md) | [Sketch](../../.sketches/2026-02-07-ion-atom-restructuring.md)
- **Supersedes**: Revisions 1–3

## Context

The eka project has validated its core concepts over ~2 years of development. However, the codebase tightly couples three distinct concerns: the Atom Protocol (identity, addressing, publishing), the runtime engine (evaluation, builds, store management), and the user frontend (CLI, manifests, resolution). Previous revisions incrementally clarified the workspace boundaries:

- **Revision 1**: Separated atom from ion but placed runtime logic in ion.
- **Revision 2**: Extracted runtime into eos but placed the manifest in `shared/manifest/`.
- **Revision 3**: Removed `shared/`, made manifest an abstract trait. But conflated store concepts, had an unclear eos crate structure, and missed the cryptographic chain as the motivating design principle.

This revision resolves the remaining architectural questions through challenges 7–9.

### The Cryptographic Chain

The fundamental principle driving the architecture is the unbroken, content-addressed chain:

```
AtomId → Version → Revision → Derivation → Output
 (czd)   (semver)   (commit)     (.drv)    (artifact)
```

Each step is cryptographically verifiable and independently cacheable. This chain enables cache-skipping at every stage — the core value proposition of eos. If the output exists and is trusted, no work is needed. If the derivation exists, skip evaluation. This chain must be expressible through the trait boundaries.

### Forces

- **Cache-skipping is the killer feature**: Every stage of the chain must be independently verifiable and skippable. BuildEngine must make this explicit, not hide it.
- **Three distinct stores**: Atom registries (publishing front, source of truth), atom stores (working copies from disparate sources), and artifact stores (build outputs, content-addressed blobs). These have fundamentally different semantics and must not be conflated.
- **The store IS the interface**: Ion hands atoms to eos through the `AtomStore`. Published atoms and local dev atoms are ingested into the same store via the same mechanism (`AtomStore::ingest`). Eos never needs to know where atoms came from.
- **Atom is generic**: The protocol is manifest-agnostic and version-scheme-agnostic. Any package ecosystem can publish atoms.
- **Embedded default, daemon opt-in**: Cargo, single-user Nix, Go — none require daemons for local builds. Neither should ion.
- **Eos will be the largest component**: Early modularization (eos-core + eos-store + eos) prevents a monolith.

## Decision

We adopt a **monorepo with three independent Cargo workspaces** mapped to a 5-layer stack:

```
Cyphrpass (L0) → Atom (L1) → Eos (L2) → Ion (L3) → Plugins (L4)
```

### `atom/` — The Protocol Workspace (Layer 1)

| Crate       | Responsibility                                                                 | Dependencies               |
| :---------- | :----------------------------------------------------------------------------- | :------------------------- |
| `atom-id`   | Identity primitives: `Label`, `Tag`, `AtomDigest`, `AtomId<R>`, `Compute`      | ≤ 5 deps                   |
| `atom-uri`  | URI parsing, version trait abstraction                                         | atom-id, `nom`             |
| `atom-core` | Traits: `AtomSource`, `AtomRegistry`, `AtomStore`, `Manifest`, `VersionScheme` | atom-id, atom-uri          |
| `atom-git`  | Git backend: implements `AtomRegistry` + `AtomStore`                           | atom-core, `gix`, `snix-*` |

**Trait decomposition** — three-layer store model with a read super-trait:

```rust
/// Read-only atom access — implemented by everything that provides atoms
trait AtomSource {
    fn resolve(&self, id, version) → AtomContent;
    fn discover(&self, anchor) → Vec<(Label, AtomId)>;
}

/// Publishing operations — registries, git backends, Cyphrpass
trait AtomRegistry: AtomSource {
    fn claim(&self, anchor, label) → AtomId;
    fn publish(&self, id, version, snapshot) → ();
}

/// Central working store — atoms collected from disparate sources
trait AtomStore: AtomSource {
    fn ingest(&self, source: &dyn AtomSource) → ();  // universal transfer
    fn fetch(&self, id) → Path;
    fn contains(&self, id, version) → bool;
}
```

`AtomStore::ingest(&dyn AtomSource)` is the universal store-to-store transfer mechanism. Registries, other stores, and dev workspaces all implement `AtomSource`, so ingestion from any source uses the same codepath.

### `eos/` — The Runtime Engine Workspace (Layer 2)

| Crate       | Responsibility                                         | Dependencies                 |
| :---------- | :----------------------------------------------------- | :--------------------------- |
| `eos-core`  | `BuildEngine` trait with plan/apply + associated types | atom-core                    |
| `eos-store` | `ArtifactStore` trait + thin snix BlobService wrapper  | eos-core                     |
| `eos`       | The engine: evaluation, building, cache management     | eos-core, eos-store, snix-\* |

**BuildEngine** — plan/apply with cache-skipping (Terraform-style):

```rust
trait BuildEngine {
    type Plan;      // engine-specific build recipe (e.g., Derivation)
    type Output;    // engine-specific build output
    type Error;

    fn plan(&self, atom: &AtomRef) → Result<BuildPlan<Self::Plan>>;
    fn apply(&self, plan: &BuildPlan<Self::Plan>) → Result<Vec<Self::Output>>;
}

enum BuildPlan<P> {
    Cached { outputs: Vec<ArtifactRef> },     // output exists + trusted
    NeedsBuild { plan: P },                    // derivation cached, build needed
    NeedsEvaluation { atom: AtomRef },         // nothing cached, full pipeline
}
```

Associated types (Plan, Output) allow each engine to define its own formats. Object safety is not needed — ion uses compile-time generics via feature flags to select the engine.

**ArtifactStore** — build output storage (content-addressed, snix blob model):

```rust
trait ArtifactStore {
    fn store(&self, digest, data) → ();
    fn fetch(&self, digest) → Box<dyn Read>;
    fn exists(&self, digest) → bool;
    fn check_substitute(&self, digests) → Vec<bool>;
}
```

Thin wrapper over snix BlobService/DirectoryService. The trait is eos's contract; snix is the default backend.

### `ion/` — The Frontend Workspace (Layer 3)

| Crate          | Responsibility                                                 | Dependencies                |
| :------------- | :------------------------------------------------------------- | :-------------------------- |
| `ion-manifest` | Concrete `ion.toml` format — implements atom-core's `Manifest` | atom-core, atom-id          |
| `ion-resolve`  | Cross-ecosystem SAT resolver, unified lock file                | atom-core, ion-manifest     |
| `ion-cli`      | CLI, BuildEngine dispatch, dev workspace management            | ion-\*, eos-core, atom-core |

### Monorepo Layout

```
axios/
├── atom/                        ← atom protocol workspace (L1)
│   ├── Cargo.toml
│   └── crates/
│       ├── atom-id/
│       ├── atom-uri/
│       ├── atom-core/
│       └── atom-git/
├── eos/                         ← runtime engine workspace (L2)
│   ├── Cargo.toml
│   └── crates/
│       ├── eos-core/
│       ├── eos-store/
│       └── eos/
├── ion/                         ← user frontend workspace (L3)
│   ├── Cargo.toml
│   └── crates/
│       ├── ion-manifest/
│       ├── ion-resolve/
│       └── ion-cli/
├── docs/
│   ├── plans/
│   └── adr/
└── README.md
```

### Store Architecture and Data Flow

```
AtomRegistry (publishing front)
        ↓ resolve/discover
    AtomSource (read interface)
        ↓ ingest
    AtomStore (working store)  ←── DevWorkspace (local dev atoms, also an AtomSource)
        ↓ read
    BuildEngine (plan/apply)
        ↓ produce
    ArtifactStore (build outputs, snix blob model)
```

Ion populates the AtomStore. Eos reads from it. The store is the handoff.

- **Embedded mode** (default): Ion and eos share one AtomStore instance.
- **Daemon mode**: Ion transfers atoms to eos's store before requesting builds.

### Embedded vs. Client Mode

- **Embedded** (`--features embedded-engine`, default): `eos::Engine` compiled into ion-cli. `ion build` works immediately.
- **Client** (future): `RemoteEngine` connects to eos daemon. Distributed builds, shared caches.

Both satisfy the same `BuildEngine` trait. Ion's code is generic: `fn run(engine: impl BuildEngine)`.

## Consequences

### Positive

- **Cryptographic chain is expressible**: AtomId→Output chain maps directly to plan/apply cache-skipping behavior.
- **Eos readiness**: BuildEngine trait + ArtifactStore are in place. Distributed eos slots in without modifying ion.
- **Manifest extensibility**: Any ecosystem can publish atoms by implementing the thin Manifest trait.
- **Store model supports federation**: AtomSource as universal read interface enables mirrors, syndicated stores, dev workspaces through one mechanism.
- **Artifact sharing**: ArtifactStore (snix blob model) enables binary caches and globally syndicated blob stores.
- **Dev workflow unified**: DevWorkspace implements AtomSource — no special codepath for unpublished atoms.
- **No coupling**: Runtime in eos from day one. Contributor isolation via workspace boundaries.

### Negative

- **10 crate skeletons**: Significant upfront scaffold (3 workspaces, 10 crates).
- **Three-workspace coordination**: Trait changes in atom-core propagate through eos and ion.
- **Eos untested**: BuildEngine plan/apply designed from prior art, not experience. May need revision.
- **Manifest/VersionScheme generics permeate**: Accepted cost of abstraction.

### Risks Accepted

- **~30% chance of trait signature breakage** when Cyphrpass integrates.
- **BuildEngine plan/apply may need refinement** as cache-skipping edge cases emerge (partial caches, cross-platform builds).
- **ArtifactStore wrapper may diverge from snix** as snix evolves.
- **atom-uri requires surgery** (`LocalAtom` moves to ion, `gix::Url` genericized).

## Alternatives Considered

**Shared manifest crate** (rev 2): Treated manifest as neutral infrastructure. Rejected — "shared" was a code smell. Manifest follows VersionScheme pattern.

**Concrete Derivation type in eos-core** (rev 3): Hardcoded nix derivations. Rejected — associated types cost nothing and future-proof against post-nix formats.

**Always-daemon** for eos: Unified architecture. Rejected — unacceptable friction for solo dev. `ion build` must just work without process management.

**Single AtomStore trait for both publishing and working**: One trait, role enum. Rejected — publishing (append-only, signed, distributed) and working (mutable, local, collected) have different operations and semantics.

**eos as single crate**: Simpler initially. Rejected — eos will be the largest component. Early modularization (eos-core/eos-store/eos) prevents costly extraction later.
