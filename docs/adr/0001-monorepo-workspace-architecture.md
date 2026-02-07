# ADR-0001: Monorepo with Independent Workspace Architecture (Revision 2)

- **Status**: PROPOSED
- **Date**: 2026-02-07 (revised)
- **Deciders**: nrd
- **Source**: [Plan](../plans/ion-atom-restructuring.md) | [Sketch](../../.sketches/2026-02-07-ion-atom-restructuring.md)
- **Supersedes**: Revision 1 (two-workspace design)

## Context

The eka project has validated its core concepts over ~2 years of development: decentralized atom publishing, git-backed storage, URI addressing, dependency resolution, and a Nix-targeting CLI. However, the codebase has accumulated tight coupling between three distinct concerns:

1. **Protocol** (identity, addressing, publishing) — the Atom Protocol
2. **Runtime engine** (evaluation, builds, store management) — what we now call Eos
3. **User frontend** (CLI, manifests, resolution) — Ion

The coupling manifests at every layer. The `atom` crate directly imports runtime dependencies (`snix-*`, `tokio`). The CLI directly invokes nix evaluation. Manifest parsing is entangled with IO. An initial restructuring plan (revision 1) correctly separated atom from ion but **repeated the coupling mistake** by placing runtime logic (`IonRuntime` trait with evaluate/build/query) directly in ion. This was the same pattern that caused problems in eka — coupling the frontend to the engine.

This revision addresses a critical architectural insight: **ion is a planner, not an executor.** Ion decides what to build; a separate engine (eos) performs the builds, manages stores, and (eventually) distributes work across machines.

### Forces

- **The eka lesson**: Tight coupling between CLI and runtime was the original problem. Any architecture that puts runtime operations in the CLI crate repeats this mistake, regardless of how clean the trait boundary is. The solution must physically separate the engine from the frontend.
- **Eos is inevitable**: The long-term vision includes distributed builds, shared stores, coordinated caching, and cryptographic build auditing. If runtime logic starts in ion, it will need to be extracted later — at significant cost. Designing the seam now while the codebase is being restructured is the right time.
- **Store operations are cryptographically significant**: Both ion and eos use `AtomStore`, but their stores serve different purposes (ion: local cache, eos: source of truth). Both are backed by Cyphrpass/git with transaction logs, enabling auditing of how atoms enter and move between stores.
- **The manifest is ecosystem-level**: Ion's manifest format is consumed by both ion (for resolution) and eos (to know what to build). It also supports multiple runtimes (nix today, potentially guix). It is not tool-specific and should not be owned by a single workspace.
- **Coupling enforcement must be mechanical, not conventional**. Crate boundaries provide compiler-enforced API surfaces. Module discipline doesn't scale to team.
- **Prior art validates the planner/executor split**:
  - **Bazel**: Client (planner) ↔ Remote Execution API (executor). Multiple independent server implementations exist behind the same protocol.
  - **Nix**: `nix` CLI ↔ `nix-daemon`. The daemon protocol's limitations (no scheduling, no affinity) are the problems eos aims to solve.
  - **snix**: gRPC builder protocol with pluggable local/remote backends — closest to the right answer and already in the dependency tree.
- **But also**: three workspaces + shared crates have real costs. IDE configuration, multi-step `cargo check`, cross-workspace version coordination, and initial scaffold complexity.

## Decision

We adopt a **monorepo with three independent Cargo workspaces** and a shared library crate. The monorepo (`axios/`) cleanly maps to a 5-layer stack:

```
Cyphrpass (L0) → Atom (L1) → Eos (L2) → Ion (L3) → Plugins (L4)
```

### `atom/` — The Protocol Workspace (Layer 1)

Decomposes the Atom Protocol into focused crates:

| Crate       | Responsibility                                                                | Dependencies                                                                   |
| :---------- | :---------------------------------------------------------------------------- | :----------------------------------------------------------------------------- |
| `atom-id`   | Identity primitives: `Label`, `Tag`, `AtomDigest`, `AtomId<R>`, `Compute`     | ≤ 5: `unicode-ident`, `unicode-normalization`, `blake3`, `base32`, `thiserror` |
| `atom-uri`  | URI parsing, version trait abstraction                                        | atom-id + `nom`, `semver`, `url`, `addr`                                       |
| `atom-core` | Aggregation: `AtomBackend`, `AtomStore`, `VersionScheme` traits, test vectors | atom-id, atom-uri (aggregation only)                                           |
| `atom-git`  | Git backend: implements `AtomBackend` + `AtomStore` for git repositories      | atom-core, `gix`, `snix-*`, `nix-compat`                                       |

Critical invariant: **atom-core has zero storage dependencies**. If a type requires `gix`, `tokio`, or `snix`, it belongs in atom-git.

### `eos/` — The Runtime Engine Workspace (Layer 2)

Houses the build engine that ion dispatches work to:

| Crate       | Responsibility                                                                | Dependencies                                      |
| :---------- | :---------------------------------------------------------------------------- | :------------------------------------------------ |
| `eos-core`  | `BuildEngine` trait + common types (`StorePath`, `Derivation`, `BuildOutput`) | atom-core                                         |
| `eos-local` | `LocalEngine` impl: snix-based local evaluation and builds                    | eos-core, shared/manifest, `snix-*`, `nix-compat` |

Future crates (not in this plan): `eos-remote` (gRPC client to eos daemon), `eos-scheduler` (distributed build coordination), `eos-cache` (binary substitution management).

**BuildEngine** — the execution interface (replaces the former `IonRuntime`):

- `evaluate(expr, args) → Derivation` — evaluate expressions to build plans
- `build(derivation) → Vec<BuildOutput>` — realize derivations
- `query(path) → Option<PathInfo>` — query store state
- `check_substitutes(paths) → Vec<SubstituteResult>` — check binary availability

All operations are synchronous in this initial design. Async is an eos-internal concern that will be introduced when the distributed engine arrives, without affecting the trait's external interface.

### `ion/` — The Frontend Workspace (Layer 3)

The user-facing planner that dispatches to eos:

| Crate         | Responsibility                                                      | Dependencies                                              |
| :------------ | :------------------------------------------------------------------ | :-------------------------------------------------------- |
| `ion-resolve` | Dependency resolution (SAT solver, version matching)                | atom-core, `resolvo`                                      |
| `ion-cli`     | CLI entrypoint, subcommand dispatch, config, `BuildEngine` dispatch | ion-resolve, atom-core, eos-core, shared/manifest, `clap` |

Ion-cli does NOT depend on snix directly. In `embedded-engine` mode (default), it transitively depends on snix through eos-local. In future client mode, it connects to an eos daemon with zero snix dependency.

### `shared/manifest/` — Ecosystem Manifest

A library crate shared between ion and eos:

- Manifest parsing (the project's dependency declaration format)
- Lock file types
- Atom-set handling
- VersionScheme-abstract version requirements
- Runtime-agnostic — supports nix today, extensible to guix and others

The manifest filename (ion.toml, atom.toml, manifest.toml) is a UX decision deferred from this architectural decision.

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
│       ├── ion-cli/
│       └── ion-resolve/
├── shared/
│   └── manifest/                ← ecosystem manifest library
├── docs/
│   ├── plans/
│   └── adr/
└── README.md
```

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

- **Ion's store** is a local atom cache. Atoms are ingested from remote sources, cached locally, and forwarded to eos when needed.
- **Eos's store** is the authoritative record. Eos may prune atoms, but conceptually it is a stateless store tracking all atoms it's aware of.
- **Both stores are cryptographically tracked** via Cyphrpass transaction logs. An auditor asking "how did this atom enter our store?" can trace the full transaction history.

### Trait Surface

Two protocol-level trait families (in atom-core):

**AtomBackend** — publishing layer (transaction-centric vocabulary):

- `claim(anchor, label) → AtomDigest`
- `publish(digest, version, snapshot) → ()`
- `resolve(digest, version) → Snapshot`
- `discover(anchor) → Vec<(Label, AtomDigest)>`

**AtomStore** — consumption layer (store-centric vocabulary):

- `ingest(source) → ()`
- `query(digest) → AtomEntry`
- `fetch(digest) → Path`

One engine trait (in eos-core):

**BuildEngine** — execution substrate:

- `evaluate(expr, args) → Derivation`
- `build(derivation) → Vec<BuildOutput>`
- `query(path) → Option<PathInfo>`
- `check_substitutes(paths) → Vec<SubstituteResult>`

### Embedded vs. Client Mode

Ion supports two modes via feature flag:

- **Embedded** (`--features embedded-engine`, default): `LocalEngine` from eos-local is compiled into ion-cli. Single-machine development experience. No eos daemon needed.
- **Client** (future): `RemoteEngine` connects to an eos daemon over the network. Distributed builds, shared caches, team workflows. Not implemented in this plan phase.

Both modes satisfy the same `BuildEngine` trait. Ion's code is identical regardless of which mode is active.

## Consequences

### Positive

- **Cyphrpass readiness**: `AtomBackend` trait boundary is in place. atom-git becomes a legacy backend.
- **Eos readiness**: `BuildEngine` trait boundary is in place. When distributed eos arrives, `RemoteEngine` slots in without modifying ion.
- **No repeated mistake**: Runtime logic is in eos from day one. Ion won't accumulate engine internals that need costly extraction later.
- **External consumability**: CI tools and registries can depend on atom-core (≤ 10 deps) without the full tree.
- **Contributor isolation**: Contributors in different workspaces cannot accidentally create coupling.
- **Library reuse**: ion-resolve, shared/manifest, and eos-core are all independently useful.
- **Cryptographic auditability**: Store operations in both ion and eos are transaction-logged from the start.

### Negative

- **Three-workspace coordination cost**: Trait changes in atom-core propagate through eos-core and ion. This friction is worst during early development while traits stabilize.
- **IDE complexity**: rust-analyzer needs per-workspace configuration. `cargo check` runs per workspace.
- **Initial scaffold burden**: 8+ crate skeletons, 3 workspaces, shared crate, inter-workspace path deps. Significant upfront work before logic is ported.
- **Premature eos abstraction risk**: Eos has no battle-tested code. The `BuildEngine` trait is designed from prior art, not experience. It may need revision.
- **Shared manifest dependency direction**: eos → shared/manifest is architecturally clean (shared library), but the manifest format is conceptually a "frontend concern" that the engine consumes. This inversion is acceptable but warrants awareness.

### Risks Accepted

- **~30% chance of trait signature breakage** when Cyphrpass integrates.
- **BuildEngine trait may need revision** as eos matures from concept to implementation. The Bazel/snix prior art provides confidence in the operation vocabulary, but associated types will evolve.
- **VersionScheme generics permeate resolution code**. Accepted cost of version abstraction.
- **atom-uri requires surgery**: `LocalAtom` moves to ion; `gix::Url` becomes generic.

## Alternatives Considered

**Two workspaces (atom + ion), IonRuntime in ion** (revision 1 of this ADR): Places runtime operations directly in ion. Rejected because it repeats eka's coupling mistake. When eos matures, runtime logic would need extraction from ion — the exact expensive rearchitecture exercise this restructuring aims to prevent.

**Single-crate with module boundaries**: ~90% of decoupling at ~10% of cost. Genuinely compelling for solo development. Rejected because module discipline doesn't scale to team, and three distinct architectural layers (protocol, engine, frontend) have fundamentally different dependency profiles and evolution rates.

**No generalization (concrete git library)**: Avoids the "wrong abstraction" problem. Rejected per nrd: easier to fix a broken abstraction than to abstract overly concrete implementations.

**Protocol-based separation (gRPC for eos from day one)**: True network decoupling between ion and eos. Rejected as premature — adds serialization, transport, and daemon management complexity before the concepts are validated. The trait-based approach (Option A from sketch) allows migration to protocol-based separation later when eos is ready for distributed deployment.

**Manifest owned by ion (eos depends cross-workspace)**: Simpler initial layout. Rejected because the manifest is genuinely shared — eos needs it for builds, not as an ion courtesy. Shared crate location reflects the reality.
