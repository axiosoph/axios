# ADR-0001: Monorepo with Independent Workspace Architecture (Revision 3)

- **Status**: PROPOSED
- **Date**: 2026-02-07 (revised)
- **Deciders**: nrd
- **Source**: [Plan](../plans/ion-atom-restructuring.md) | [Sketch](../../.sketches/2026-02-07-ion-atom-restructuring.md)
- **Supersedes**: Revision 2 (shared/manifest crate), Revision 1 (two-workspace design)

## Context

The eka project has validated its core concepts over ~2 years of development: decentralized atom publishing, git-backed storage, URI addressing, dependency resolution, and a Nix-targeting CLI. However, the codebase has accumulated tight coupling between three distinct concerns:

1. **Protocol** (identity, addressing, publishing) — the Atom Protocol
2. **Runtime engine** (evaluation, builds, store management) — what we now call Eos
3. **User frontend** (CLI, manifests, resolution) — Ion

The coupling manifests at every layer. The `atom` crate directly imports runtime dependencies (`snix-*`, `tokio`). The CLI directly invokes nix evaluation. Manifest parsing is entangled with IO. Previous revisions incrementally clarified the workspace boundaries:

- **Revision 1** correctly separated atom from ion but placed runtime logic (`IonRuntime`) in ion.
- **Revision 2** correctly extracted the runtime into eos but placed the manifest in `shared/manifest/` — a code smell indicating unclear abstraction boundaries.

This revision resolves the manifest question through a critical protocol-level insight: **the Atom Protocol is manifest-agnostic.** Atoms are a generic package format. Cargo crates, npm packages, and ion-managed builds can all be atoms. The protocol does not dictate the manifest format (per the v2 spec). The manifest follows the same abstraction pattern as `VersionScheme`: atom defines the abstract trait, each ecosystem provides a concrete implementation.

### Forces

- **The eka lesson**: Tight coupling between CLI and runtime was the original problem. The solution must physically separate the engine from the frontend.
- **Atom is generic**: The Atom Protocol is manifest-agnostic, version-scheme-agnostic, and content-agnostic. Any package ecosystem can publish atoms. This means the manifest format is NOT a protocol concern — it's a per-ecosystem tooling concern.
- **The VersionScheme pattern**: Atom already abstracts version semantics (VersionScheme trait). The manifest follows the same pattern — a thin abstract trait that each ecosystem implementor satisfies.
- **Ion's unique value**: Ion resolves dependencies across atom ecosystems. An ion build can depend on cargo crate atoms, npm atoms, etc. Ion's resolver produces a single unified lock file using generic VersionScheme. Lock files are per-tool.
- **Eos reads manifests abstractly**: Eos needs manifest metadata (dep trees, package info) for queries. But eos reads through the abstract Manifest trait — it doesn't know about ion.toml specifically. By the time work reaches eos, dependencies are locked.
- **"shared/" was a smell**: The name "shared" in revision 2 indicated we hadn't understood what the manifest IS. Through six challenge iterations, the correct decomposition emerged: abstract trait (atom-core) + concrete format (ion-manifest).
- **ekala.toml is not a pillar**: The workspace manifest may not survive the Cyphrpass transition, where transaction history becomes the source of truth for what constitutes an atom. It should not be designed around.
- **Coupling enforcement must be mechanical, not conventional**. Crate boundaries provide compiler-enforced API surfaces.
- **Prior art validates the planner/executor split**:
  - **Bazel**: Client ↔ Remote Execution API
  - **snix**: gRPC builder protocol with pluggable backends
  - **Cargo/crates.io**: Cargo.toml is cargo's format, not the registry protocol's format

## Decision

We adopt a **monorepo with three independent Cargo workspaces** mapped to a 5-layer stack:

```
Cyphrpass (L0) → Atom (L1) → Eos (L2) → Ion (L3) → Plugins (L4)
```

### `atom/` — The Protocol Workspace (Layer 1)

Decomposes the Atom Protocol into focused crates:

| Crate       | Responsibility                                                              | Dependencies                                                                   |
| :---------- | :-------------------------------------------------------------------------- | :----------------------------------------------------------------------------- |
| `atom-id`   | Identity primitives: `Label`, `Tag`, `AtomDigest`, `AtomId<R>`, `Compute`   | ≤ 5: `unicode-ident`, `unicode-normalization`, `blake3`, `base32`, `thiserror` |
| `atom-uri`  | URI parsing, version trait abstraction                                      | atom-id + `nom`, `semver`, `url`, `addr`                                       |
| `atom-core` | Aggregation: `AtomBackend`, `AtomStore`, `VersionScheme`, `Manifest` traits | atom-id, atom-uri (aggregation only)                                           |
| `atom-git`  | Git backend: implements `AtomBackend` + `AtomStore` for git repositories    | atom-core, `gix`, `snix-*`, `nix-compat`                                       |

**Critical design**: atom-core defines a **thin `Manifest` trait** — a metadata view that any atom ecosystem implementor must satisfy. This trait surfaces label, version, description, and dependency summary. It does NOT specify file formats, lock schemas, or resolution strategies. This follows the same abstraction pattern as `VersionScheme`.

Critical invariant: **atom-core has zero storage dependencies**. If a type requires `gix`, `tokio`, or `snix`, it belongs in atom-git.

### `eos/` — The Runtime Engine Workspace (Layer 2)

Houses the build engine that ion dispatches work to:

| Crate       | Responsibility                                               | Dependencies       |
| :---------- | :----------------------------------------------------------- | :----------------- |
| `eos-core`  | `BuildEngine` trait + common types — generic over `Manifest` | atom-core          |
| `eos-local` | `LocalEngine` impl: snix-based local evaluation and builds   | eos-core, `snix-*` |

**BuildEngine** — the execution interface (generic over Manifest):

- `evaluate(expr, args) → Derivation` — evaluate expressions to build plans
- `build(derivation) → Vec<BuildOutput>` — realize derivations
- `query(path) → Option<PathInfo>` — query store state
- `check_substitutes(paths) → Vec<SubstituteResult>` — check binary availability

BuildEngine can serve metadata queries about any atom using the abstract Manifest trait, without knowing the concrete format.

### `ion/` — The Frontend Workspace (Layer 3)

The user-facing planner with its concrete manifest format:

| Crate          | Responsibility                                                       | Dependencies                                           |
| :------------- | :------------------------------------------------------------------- | :----------------------------------------------------- |
| `ion-manifest` | Concrete `ion.toml` format — implements atom-core's `Manifest` trait | atom-core, atom-id, `toml_edit`                        |
| `ion-resolve`  | Cross-ecosystem SAT resolver, unified lock file                      | atom-core, ion-manifest, `resolvo`                     |
| `ion-cli`      | CLI entrypoint, subcommand dispatch, `BuildEngine` dispatch          | ion-resolve, ion-manifest, eos-core, atom-core, `clap` |

Ion-cli does NOT depend on snix directly. In `embedded-engine` mode (default), it transitively depends on snix through eos-local.

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
│       └── eos-local/
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

### Manifest Abstraction Architecture

The manifest follows the same trait pattern as VersionScheme:

```
Protocol layer (atom-core):  thin Manifest trait (metadata view)
                              VersionScheme trait (version comparison)

Ion (reference tooling):      IonManifest (ion.toml) implements Manifest
                              SemVer implements VersionScheme

Future adapters:              CargoManifest, NpmManifest, etc.
```

- Atom-core defines WHAT a manifest must expose (label, version, deps)
- Each ecosystem defines HOW their manifest is structured and parsed
- Eos reads manifests through the abstract trait
- Lock files are per-tool — atom knows format-type at most, not the hard schema
- Ion's resolver is unique: resolves across ecosystems into a single unified lock

### Store Architecture

Both eos and ion implement `AtomStore`, but at different scopes:

```
Ion (local cache)  ──send atoms──→  Eos (source of truth store)
     ↓                                    ↓
  AtomStore impl                      AtomStore impl
     ↓                                    ↓
  Cyphrpass/git                      Cyphrpass/git
  (local tx log)                     (shared tx log)
```

### Trait Surface

Two protocol-level trait families + two abstraction traits (in atom-core):

**AtomBackend** — publishing layer (transaction-centric vocabulary):

- `claim(anchor, label) → AtomDigest`
- `publish(digest, version, snapshot) → ()`
- `resolve(digest, version) → Snapshot`
- `discover(anchor) → Vec<(Label, AtomDigest)>`

**AtomStore** — consumption layer (store-centric vocabulary):

- `ingest(source) → ()`
- `query(digest) → AtomEntry`
- `fetch(digest) → Path`

**VersionScheme** — version abstraction:

- `parse(s) → Version`
- `satisfies(version, requirement) → bool`

**Manifest** — metadata view (thin):

- `label() → &Label`
- `version() → &V` (associated type constrained by VersionScheme)
- `description() → Option<&str>`
- `dependencies() → DependencySummary` (abstract)

One engine trait (in eos-core):

**BuildEngine** — execution substrate (generic over Manifest):

- `evaluate(expr, args) → Derivation`
- `build(derivation) → Vec<BuildOutput>`
- `query(path) → Option<PathInfo>`
- `check_substitutes(paths) → Vec<SubstituteResult>`

### Embedded vs. Client Mode

- **Embedded** (`--features embedded-engine`, default): `LocalEngine` compiled into ion-cli. Single-machine development.
- **Client** (future): `RemoteEngine` connects to eos daemon. Distributed builds.

Both satisfy the same `BuildEngine` trait.

## Consequences

### Positive

- **Cyphrpass readiness**: `AtomBackend` trait boundary is in place. atom-git becomes a legacy backend.
- **Eos readiness**: `BuildEngine` trait boundary is in place. `RemoteEngine` slots in without modifying ion.
- **No repeated mistake**: Runtime logic is in eos from day one. Ion won't accumulate engine internals.
- **Manifest extensibility**: Any ecosystem can publish atoms by implementing the thin Manifest trait. ion.toml is the reference format, not the only format.
- **External consumability**: CI tools and registries can depend on atom-core (≤ 10 deps) without the full tree.
- **Contributor isolation**: Contributors in different workspaces cannot accidentally create coupling.
- **Library reuse**: ion-resolve, ion-manifest, and eos-core are all independently useful.
- **Cross-ecosystem resolution**: Ion can resolve deps across atom ecosystems into a unified lock — a unique capability enabled by the abstract VersionScheme.

### Negative

- **Three-workspace coordination cost**: Trait changes in atom-core propagate through eos-core and ion.
- **IDE complexity**: rust-analyzer needs per-workspace configuration. `cargo check` runs per workspace.
- **Initial scaffold burden**: 9 crate skeletons, 3 workspaces, inter-workspace path deps.
- **Premature eos abstraction risk**: Eos has no battle-tested code. `BuildEngine` trait designed from prior art, not experience.
- **Manifest trait granularity risk**: The thin Manifest trait might be too thin (missing needed metadata) or too thick (overspecifying). VersionScheme provides precedent for the right balance.

### Risks Accepted

- **~30% chance of trait signature breakage** when Cyphrpass integrates.
- **BuildEngine trait may need revision** as eos matures.
- **VersionScheme generics permeate resolution code**. Accepted cost of version abstraction.
- **Manifest trait generics add complexity** to eos and ion. Accepted — same cost profile as VersionScheme.
- **atom-uri requires surgery**: `LocalAtom` moves to ion; `gix::Url` becomes generic.

## Alternatives Considered

**Shared manifest crate in `shared/manifest/`** (revision 2): Treated the manifest as a neutral infrastructure concern shared between ion and eos. Rejected because "shared" was a code smell — the manifest isn't neutral infrastructure. The Atom Protocol is manifest-agnostic per v2 spec. The correct pattern is abstract trait (atom-core) + concrete implementation (ion-manifest), matching VersionScheme.

**Two workspaces (atom + ion), IonRuntime in ion** (revision 1): Places runtime operations in ion. Rejected because it repeats eka's coupling mistake.

**Single-crate with module boundaries**: ~90% of decoupling at ~10% of cost. Rejected because module discipline doesn't scale to team.

**Manifest owned solely by ion**: Simpler, but prevents eos from querying manifest metadata through a uniform interface. Eos needs the abstract trait to support metadata queries across atom ecosystems.

**Manifest defined in eos-core**: The engine defines what inputs it accepts. Rejected because the manifest transcends any particular engine — it's a protocol-level metadata view, not an engine input format. Swapping engines shouldn't change the manifest format.

**Protocol-based separation (gRPC for eos from day one)**: Rejected as premature. Trait-based approach allows migration to protocol-based separation later.
