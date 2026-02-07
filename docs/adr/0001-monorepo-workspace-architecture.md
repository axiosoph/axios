# ADR-0001: Monorepo with Independent Workspace Architecture

- **Status**: PROPOSED
- **Date**: 2026-02-07
- **Deciders**: nrd
- **Source**: [Plan](../plans/ion-atom-restructuring.md) | [Sketch](../../.sketches/2026-02-07-ion-atom-restructuring.md)

## Context

The eka project has validated its core concepts over ~2 years of development: decentralized atom publishing, git-backed storage, URI addressing, dependency resolution, and a Nix-targeting CLI. However, the codebase has accumulated tight coupling between protocol-level concerns (identity, addressing, publishing) and tooling-level concerns (CLI, resolution, manifests, runtime dispatch). The `atom` crate directly imports `gix`, `snix-*`, `resolvo`, `tokio`, `toml_edit`, and ~25 other dependencies — most of which are irrelevant to the Atom Protocol itself.

This coupling is not accidental; it reflects the natural trajectory of a validating prototype. But it now blocks three strategic objectives:

1. **Cyphrpass integration** — the Atom Protocol's identity and storage layers will migrate to Cyphrpass as their substrate. This requires clean trait boundaries between protocol operations and storage implementations. The current codebase has no such boundary — git storage internals leak into identity construction, manifest parsing, and resolution.

2. **Team scalability** — external contributors cannot work on ion's CLI without understanding atom's storage internals, and vice versa. Module-level `pub(crate)` boundaries are a convention; crate boundaries are enforcement.

3. **Ecosystem breadth** — atom is a protocol with potential consumers beyond ion. CI tools, metadata indexers, registry frontends, and alternative package managers should be able to depend on atom's protocol types without pulling in gix, tokio, or nix dependencies.

Additionally, ion itself has grown complex enough that its internal concerns — manifest parsing, dependency resolution, runtime dispatch — warrant library-level decomposition so they can be tested, documented, and versioned independently of the CLI binary.

### Forces

- **Coupling enforcement must be mechanical, not conventional**. A solo developer can maintain module discipline; a team cannot. Crate boundaries provide compiler-enforced API surfaces.
- **Atom is a protocol; ion is a tool**. Their release cadences, stability guarantees, and dependency profiles are fundamentally different. A manifest format change in ion should not trigger a version bump in atom.
- **The 4-layer stack** (Cyphrpass → Atom → Ion → Plugins) places atom and ion at different architectural layers. Shared workspaces would allow — and eventually encourage — dependencies to flow in the wrong direction.
- **Prior art** (gitoxide, iroh, sigstore-rs, cargo) consistently demonstrates that protocol libraries and CLI tools benefit from crate-level separation when the protocol is intended for external consumption.
- **But also**: multi-workspace monorepos have real costs. IDE support (rust-analyzer) requires per-workspace configuration. `cargo check` only checks one workspace at a time. Cross-workspace path dependencies don't publish cleanly. Version coordination is manual.

## Decision

We adopt a **monorepo with independent Cargo workspaces** architecture. The monorepo (`axios/`) contains two top-level workspaces:

### `atom/` — The Protocol Workspace

Decomposes the Atom Protocol into focused crates:

| Crate       | Responsibility                                                                                                     | Dependencies                                                                   |
| :---------- | :----------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------- |
| `atom-id`   | Identity primitives: `Label`, `Tag`, `AtomDigest`, `AtomId<R>`, `Genesis`, `Compute`                               | ≤ 5: `unicode-ident`, `unicode-normalization`, `blake3`, `base32`, `thiserror` |
| `atom-uri`  | URI parsing, version trait abstraction                                                                             | atom-id + `nom`, `semver`, `url`, `addr`                                       |
| `atom-core` | Aggregation: re-exports atom-id/atom-uri, defines `AtomBackend`, `AtomStore`, `VersionScheme` traits, test vectors | atom-id, atom-uri (aggregation)                                                |
| `atom-git`  | Git backend: implements `AtomBackend` + `AtomStore` for git repositories                                           | atom-core, `gix`, `snix-*`, `nix-compat`                                       |

The critical invariant: **atom-core has zero storage dependencies**. If a type requires `gix`, `tokio`, or `snix`, it belongs in atom-git, not atom-core. This is enforced by the crate boundary — atom-core's `Cargo.toml` cannot list these dependencies.

### `ion/` — The Tooling Workspace

Decomposes ion into library crates consumed by a CLI binary:

| Crate          | Responsibility                                                          | Dependencies                                                     |
| :------------- | :---------------------------------------------------------------------- | :--------------------------------------------------------------- |
| `ion-manifest` | Manifest parsing (`ion.toml`), lock files, atom-set metadata            | atom-core, `toml_edit`, `serde`                                  |
| `ion-resolve`  | Dependency resolution (SAT solver, version matching)                    | atom-core, `resolvo`                                             |
| `ion-cli`      | CLI entrypoint, subcommand dispatch, `IonRuntime` trait + impls, config | ion-manifest, ion-resolve, atom-core, atom-git, `clap`, `snix-*` |

Ion-manifest and ion-resolve are **libraries** — independently testable, documentable, and potentially reusable by other atom consumers. Ion-cli is the binary that wires them together.

### Monorepo Layout

```
axios/
├── atom/                        ← atom protocol workspace
│   ├── Cargo.toml               ← [workspace] members
│   ├── crates/
│   │   ├── atom-id/
│   │   ├── atom-uri/
│   │   ├── atom-core/
│   │   └── atom-git/
│   └── SPEC.md
├── ion/                         ← ion tooling workspace
│   ├── Cargo.toml               ← [workspace] members
│   ├── crates/
│   │   ├── ion-cli/
│   │   ├── ion-manifest/
│   │   └── ion-resolve/
│   └── ...
├── docs/
│   ├── plans/
│   └── adr/
└── README.md
```

### Trait Surface

Two protocol-level trait families bridge atom and ion:

**AtomBackend** — publishing layer (transaction-centric vocabulary):

- `claim(anchor, label) → AtomDigest` — establish atom identity
- `publish(digest, version, snapshot) → ()` — record a version
- `resolve(digest, version) → Snapshot` — retrieve a version
- `discover(anchor) → Vec<(Label, AtomDigest)>` — enumerate atoms

**AtomStore** — consumption layer (store-centric vocabulary):

- `ingest(source) → ()` — pull atoms from a backend
- `query(digest) → AtomEntry` — look up metadata
- `fetch(digest) → Path` — materialize content

**IonRuntime** — execution substrate (required, not optional):

- `evaluate(expr, args) → StorePath` — evaluate expressions
- `build(derivation) → Vec<OutputPath>` — realize derivations
- `query_path(path) → PathInfo` — query store

The two-layer trait vocabulary (AtomBackend + AtomStore) is derived from the Atom SPEC's two-layer data flow: publishing layer (`label@version`) and store layer (`atom-digest`). The Cyphrpass-aligned terminology (`claim`, `publish`) is chosen deliberately — these operations are transaction-like, and the eventual Cyphrpass substrate will express them as signed transactions.

### Runtime vs. Plugin Distinction

A critical architectural distinction: **runtimes are not plugins**.

- **Runtimes** (snix, nix CLI, guix): Required backends. Ion dispatches to a runtime the way a compiler dispatches to a code generator. No runtime = no useful ion. Modeled by the `IonRuntime` trait.
- **Plugins** (hypothetical: `ion deploy`, `ion audit`): Optional CLI extensions. User-facing, additive. May use WASM component model or similar. Explicitly deferred — not designed in this plan.

## Consequences

### Positive

- **Cyphrpass readiness**: When Cyphrpass integration begins, it implements `AtomBackend` — the trait boundary is already in place. atom-git becomes a legacy/compatibility backend, not the only option.
- **External consumability**: CI tools, indexers, and registries can depend on `atom-core` (≤ 10 deps) without pulling the full dependency tree.
- **Contributor isolation**: A contributor working on ion-resolve cannot accidentally import atom-git internals. A contributor working on atom-id has no exposure to gix or snix.
- **Library reuse**: ion-manifest and ion-resolve are independently useful. Another atom consumer (hypothetical `guix-atom`) could use ion-resolve without ion-cli.
- **Test isolation**: atom-id can be tested with zero fixtures — pure validation logic. atom-git tests can focus on git-specific behavior against atom-core trait contracts.

### Negative

- **Cross-crate coordination overhead**: Changes to atom-core trait signatures require coordinated updates across atom-git + ion crates. This is real friction, especially during early development when traits are still stabilizing.
- **IDE configuration**: rust-analyzer needs per-workspace configuration for multi-workspace monorepos. This adds developer setup friction.
- **Publishing complexity**: When atom crates are published to crates.io, cross-workspace path dependencies must be replaced with version dependencies. This is a well-understood but tedious process.
- **Initial velocity cost**: Setting up 7+ crate skeletons, workspaces, and inter-crate dependencies is significant upfront work before any concrete logic is ported.

### Risks Accepted

- **~30% chance of trait signature breakage** when Cyphrpass integrates. The boundary location is correct; the method signatures may need revision. This is an acceptable cost — the alternative (no boundary) would make integration harder, not easier.
- **atom-uri crate requires surgery**: `LocalAtom` must move to ion (it's a resolution concern), and `gix::Url` must be replaced with a backend-agnostic URL type. This is known work, not speculative.
- **VersionScheme generics permeate ion-resolve**: Every resolution function carries `V: VersionScheme` bounds. This is the cost of version abstraction, explicitly accepted by nrd.

### Alternatives Considered

**Single-crate with module boundaries**: A single `atom` crate with `pub(crate)` module boundaries achieves ~90% of decoupling at ~10% of the cost. Genuinely compelling for solo development. Rejected because crate boundaries provide mechanical enforcement that external contributors need — module discipline doesn't scale to team.

**No generalization (concrete git library)**: Build atom as a clean, git-specific library with no traits. Extract traits later when a second backend exists. Avoids the "wrong abstraction" problem. Rejected because nrd's thesis is that it's easier to fix a broken abstraction than to abstract an overly concrete implementation — and the existing codebase is evidence of what happens when boundaries are deferred.

**Monolithic ion**: Keep all ion concerns in a single crate with internal modules. Rejected because ion-manifest and ion-resolve are library concerns that should be independently testable and potentially reusable by other atom consumers.
