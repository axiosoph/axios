# PLAN: Ion-Atom Restructuring

<!--
  Source sketch: .sketches/2026-02-07-ion-atom-restructuring.md
  Plan stage: SCOPE → COMMIT (revision 5)
  Confidence: 0.93 — see bottom of document

  Revision history:
  - Rev 1: Separated atom from ion; runtime stayed in ion.
  - Rev 2: Extracted runtime into eos; manifest in shared/.
  - Rev 3: Removed shared/; manifest became abstract trait. Store model underdeveloped.
  - Rev 4: Store taxonomy, BuildEngine plan/apply, cryptographic chain, embedded default.
  - Rev 5: Humanizer pass. No technical changes — readability, coherence, removed process artifacts.
-->

## Goal

Split the eka codebase into three independent Cargo workspaces inside a monorepo
(`axios/`), each responsible for one layer of a 5-layer stack:

```
Cyphrpass (L0)  →  Atom (L1)  →  Eos (L2)  →  Ion (L3)  →  Plugins (L4)
identity/signing   protocol       runtime       frontend     adapters
```

## The cryptographic chain

Every atom traces an unbroken, content-addressed path from identity to output:

```
AtomId → Version → Revision → Plan → Output
 (czd)   (semver)   (commit)  (recipe) (artifact)
```

Each step is verifiable and cacheable on its own. If the artifact exists and is
trusted, skip everything. If the plan exists, skip evaluation. This is what
makes cache-skipping work.

"Plan" is the abstract term. For the snix engine, a plan is a derivation (`.drv`).
`BuildEngine::Plan` is an associated type, so other engines can define their own
plan format without touching the chain.

The chain is really a DAG: each atom's lock entry carries a `requires` field with
the content-addressed digests of its transitive dependencies. The lock file captures
the complete graph.

## Three workspaces

**atom/** — The protocol library. Identity, addressing, publishing, and the
abstract trait surface (`AtomSource`, `AtomRegistry`, `AtomStore`, `Manifest`,
`VersionScheme`). The Atom protocol does not dictate manifest formats or version
schemes — each ecosystem provides its own implementation.

**eos/** — The runtime engine. Receives locked dependencies from ion, turns them
into build plans, executes builds, stores artifacts. By the time work reaches
eos, dependencies are fully resolved. Eos reads from an `AtomStore` and does not
care where the atoms originated.

**ion/** — The reference frontend. CLI, dependency resolution, the concrete
`ion.toml` manifest, and dev workspace management. Ion decides what to build;
eos does the building.

## Manifest abstraction

The protocol is manifest-agnostic (per the v2 spec). Cargo uses `Cargo.toml`,
npm uses `package.json`, ion uses `ion.toml`. The `Manifest` trait in atom-core
follows the same pattern as `VersionScheme`: atom defines the interface, each
ecosystem implements it. The trait exposes label, version, dependency summary,
and composer configuration (how an atom declares its evaluator).

## Store taxonomy

| Store         | Purpose                                                             | Trait                      | Layer     |
| :------------ | :------------------------------------------------------------------ | :------------------------- | :-------- |
| AtomRegistry  | Publishing front. Source of truth, immutable once published.        | `AtomRegistry: AtomSource` | atom-core |
| AtomStore     | Working store. Collects atoms from registries, dev workspaces, etc. | `AtomStore: AtomSource`    | atom-core |
| ArtifactStore | Build outputs. Content-addressed blobs (snix model).                | `ArtifactStore`            | eos-store |

`AtomSource` is the read-only super-trait behind both `AtomRegistry` and
`AtomStore`. This makes `AtomStore::ingest(&dyn AtomSource)` possible: a single
store-to-store transfer mechanism that works with registries, other stores, and
dev workspaces identically.

### Store transfer

Ion populates the `AtomStore` from multiple sources:

- Published atoms: `store.ingest(&registry)` — pull from registries
- Local atoms: `store.import_path(path)` — copy from disk, stamp with dev prerelease version
- Cross-store: `store.ingest(&other_store)` — transfer between stores

Eos reads from the AtomStore. Published or local, they look the same once
they're in the store.

**Deployment modes:**

- **Embedded** (default): ion and eos share one AtomStore instance. No transfer.
- **Daemon**: ion transfers atoms to eos's store before requesting builds.
- **Remote**: daemon over the network (future).

## Constraints

- Atom, eos, and ion are separate Cargo workspaces. No circular dependencies.
- Dependency direction is strictly layered: ion → eos-core, ion → atom-core, eos → atom-core. Never upward.
- atom-id: ≤ 5 non-std deps. atom-core: ≤ 10 total.
- `VersionScheme` and `Manifest` are abstract — no semver types or ion.toml types in atom-core.
- `BuildEngine` uses associated types. Object safety is not needed — compile-time generics via feature flags.
- Storage, identity, and signing will migrate to Cyphrpass. Design seams, not implementations.
- Dependencies are locked before reaching eos. No resolution at the engine layer.
- Lock file format is per-tool. Atom knows the format type but not the hard schema.
- `serde` is behind a feature flag.
- Runtime operations belong in eos. Ion submits work; eos performs it.
- Embedded engine is the default. Daemon is opt-in.
- All traits start synchronous. Async is eos-internal, deferred until the distributed engine.
- `ekala.toml` may not survive the Cyphrpass transition.

## Decisions

| ID    | Decision             | Choice                                                                                    | Rationale                                                                       |
| :---- | :------------------- | :---------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------ |
| KD-1  | atom-core deps       | `std` only + atom-id + atom-uri; `serde` via feature; `nom` in atom-uri only              | Protocol purity. Storage deps go in atom-git.                                   |
| KD-2  | AtomDigest           | Hash-agnostic (`AsRef<[u8]>` or trait-based)                                              | Cyphrpass uses multi-algorithm digests.                                         |
| KD-3  | Version abstraction  | `trait VersionScheme` from day one                                                        | Non-negotiable. Atom serves ecosystems beyond semver.                           |
| KD-4  | Dep resolution       | Lives in ion-resolve                                                                      | Resolution is tooling-layer, not protocol.                                      |
| KD-5  | Manifest             | Thin `Manifest` trait in atom-core; concrete `ion.toml` in ion-manifest                   | Same pattern as VersionScheme.                                                  |
| KD-6  | Workspace separation | Three independent Cargo workspaces in monorepo                                            | Mechanical enforcement of layer separation.                                     |
| KD-7  | Build engine         | `BuildEngine` trait with plan/apply and associated types (Plan, Output)                   | Cache-skipping at every stage. Terraform-style.                                 |
| KD-8  | atom-core substance  | Contains core implementation + test vectors, not just types                               | Prevents "ghost crate" failure. Must be independently testable.                 |
| KD-9  | Store taxonomy       | AtomSource (read) + AtomRegistry (publish) + AtomStore (working) + ArtifactStore (output) | Four distinct concerns with different semantics.                                |
| KD-10 | Store transfer       | `AtomStore::ingest(&dyn AtomSource)` + `import_path` for local atoms                      | Published atoms via ingest; local atoms copied from disk into the same store.   |
| KD-11 | Ion decomposition    | ion-cli, ion-resolve, ion-manifest                                                        | ion-manifest implements Manifest. ion-resolve does SAT.                         |
| KD-12 | Eos modularization   | eos-core + eos-store + eos                                                                | Will be the largest component. Early split prevents monolith.                   |
| KD-13 | Embedded engine      | `embedded-engine` feature flag on ion-cli                                                 | Solo dev works immediately. Daemon is opt-in.                                   |
| KD-14 | Sync-first traits    | All traits start synchronous. Async is eos-internal.                                      | Avoids forcing tokio into atom-core or ion.                                     |
| KD-15 | Lock file            | Per-tool. Ion's lock tracks both atom deps and nix deps.                                  | Includes `AtomDep` (with transitive `requires` digests) and direct nix sources. |
| KD-16 | Plan abstraction     | `BuildEngine::Plan` is an associated type, not a concrete Derivation.                     | Future-proofs against post-nix plan formats. Costs nothing.                     |
| KD-17 | ArtifactStore        | Thin wrapper over snix BlobService/DirectoryService                                       | Trait is eos's contract. snix is the default backend.                           |
| KD-18 | AtomBackend          | Deferred. Future trait for cross-ecosystem adaptation (cargo→atom, npm→atom).             | Only ion exists now. Manifest trait covers the metadata side.                   |

## Risks and assumptions

| ID   | Risk / Assumption                               | Severity | Status    | Mitigation                                                                                        |
| :--- | :---------------------------------------------- | :------- | :-------- | :------------------------------------------------------------------------------------------------ |
| R-1  | Cyphrpass API mismatch breaks trait signatures  | MEDIUM   | Mitigated | Boundary correctness > API correctness. ~30% chance of signature changes.                         |
| R-2  | Version abstraction kills productivity          | —        | CLOSED    | Non-negotiable per nrd. Cost accepted.                                                            |
| R-3  | Three workspaces is overhead for one person     | —        | CLOSED    | Intentional. The friction is the feature.                                                         |
| R-4  | Premature eos abstraction                       | MEDIUM   | Mitigated | BuildEngine is thin (plan/apply). Prior art: Bazel REAPI, snix. Start thin, grow from experience. |
| R-5  | Manifest trait scope (too thin or too thick)    | MEDIUM   | Mitigated | Start minimal. Grow from implementation. VersionScheme is the precedent.                          |
| R-6  | atom-core scope creep                           | MEDIUM   | Mitigated | Hard rule: if it requires `gix`, `tokio`, `resolvo`, or snix, it goes elsewhere.                  |
| R-7  | ArtifactStore wrapper diverges from snix        | LOW      | Mitigated | Wrapper is deliberately thin: store, fetch, exists, substitute-check.                             |
| R-8  | BuildEngine plan/apply is over-engineered       | LOW      | Accepted  | Cache-skipping is the core value. Terraform validates the pattern.                                |
| R-9  | Three eos crates is too many up front           | LOW      | Accepted  | Early modularization is cheaper than late extraction.                                             |
| R-10 | atom-uri requires surgery                       | MEDIUM   | Accepted  | `LocalAtom` moves to ion; `gix::Url` gets genericized.                                            |
| A-1  | Atom sits atop Cyphrpass                        | —        | Validated | nrd is active in Cyphrpass development.                                                           |
| A-2  | Existing code has proven concepts worth porting | —        | Validated | 2 years of working dep resolution, publishing, URI parsing, manifests.                            |
| A-3  | Protocol is manifest-agnostic                   | —        | Validated | Per v2 spec.                                                                                      |
| A-4  | Cryptographic chain is the foundation           | —        | Validated | Every step is content-addressed and cacheable.                                                    |
| A-5  | Local atoms land in the same store as published | —        | Validated | Current eka copies local atoms into the cache repo with a dev prerelease version.                 |
| A-6  | ekala.toml is not a central pillar              | —        | Validated | May not survive Cyphrpass transition.                                                             |
| A-7  | Embedded engine is the right default            | —        | Validated | Prior art: Cargo, single-user Nix, Go.                                                            |

## Scope

### In scope

- Monorepo initialization with three Cargo workspaces
- atom workspace: atom-id, atom-uri, atom-core, atom-git
- eos workspace: eos-core, eos-store, eos
- ion workspace: ion-cli, ion-resolve, ion-manifest
- All trait surfaces described in this plan
- Porting proven types and logic from eka
- Test vectors for protocol-level types
- BuildEngine implementation with snix
- ArtifactStore with snix BlobService wrapper
- `embedded-engine` feature flag
- Store-to-store transfer via `AtomStore::ingest` and disk-to-store via `import_path`

### Out of scope

- Finalizing Atom Protocol SPEC sections 4–9
- Cyphrpass integration
- Dynamic plugin system (WASM/RPC)
- Distributed eos (daemon, networking, `RemoteEngine`)
- Full feature parity with current eka CLI
- Cross-ecosystem adapters (`AtomBackend`)
- Async trait boundaries
- Build scheduling, binary cache negotiation, multi-node coordination
- Globally syndicated stores
- `ekala.toml` redesign

## Phases

Each phase is independently valuable and can be executed as a bounded unit.

### Phase 1 — Monorepo scaffold

Set up the repository structure and workspace roots.

- [x] Initialize `axios/` with a README explaining the monorepo and layer model
- [x] Create `atom/`, `eos/`, `ion/` workspace roots with `Cargo.toml`
- [x] Create skeleton crates (10 total, strict layer ordering)
- [x] Wire inter-workspace path dependencies
- [x] Add `embedded-engine` feature flag on ion-cli
- [x] Verify: `cargo check` passes in all three workspaces

### Phase 2 — atom-id: identity primitives

Port protocol-level types with zero storage coupling.

- `Label`, `Tag`, `Identifier` with `VerifiedName` trait and validation
- `AtomDigest` (generalize toward hash-agility if feasible)
- `AtomId<R>` with `Compute` and `Genesis` traits
- Display implementations (`base32`, `FromStr`, `Display`)
- Existing unit tests + edge-case test vectors
- Budget: ≤ 5 non-std deps
- Verify: `cargo test`

### Phase 3 — atom-core: protocol traits

Define the trait surface that all ecosystems implement against.

- `AtomSource` (resolve, discover) — read-only super-trait
- `AtomRegistry: AtomSource` (claim, publish)
- `AtomStore: AtomSource` (ingest, fetch, contains)
- `VersionScheme` — abstract version comparison
- `Manifest` — label, version, description, deps, composer info
- Common types: `AtomContent`, `AtomEntry`, `Anchor`, `Snapshot`
- Error taxonomy
- Re-export atom-id public types
- serde behind feature flag
- Budget: ≤ 10 deps total
- Verify: `cargo check`, `cargo doc` produces clean trait docs

### Phase 4 — atom-uri: URI parsing

Port URI handling with reduced coupling.

- `Uri`, `LocalAtom` types (`LocalAtom` may move to ion)
- Replace `gix::Url` with generic URL handling
- Integrate with atom-id types
- Port `nom`-based parsing and tests
- Verify: `cargo test`

### Phase 5 — atom-git: bridge implementation

Port the git backend against atom-core traits.

- `AtomRegistry` for git: claim → ref creation, publish → orphan commit, resolve → ref lookup, discover → ref enumeration
- `AtomStore` for git: ingest, fetch, contains
- `Root` (genesis type — commit OID)
- Ref layout, transport, caching (existing `RemoteAtomCache` logic)
- Workspace management (`ekala.toml` / `EkalaManager`) as pragmatic shim
- Wire `gix` + `snix-*` dependencies
- Integration tests
- Verify: `cargo test` validates trait contracts

### Phase 6 — eos-core: build engine trait

Define the build engine interface.

- `BuildEngine` trait with associated types:
  - `type Plan` — engine-specific build recipe (for snix: a derivation)
  - `type Output` — engine-specific build output
  - `type Error`
  - `fn plan(&self, atom: &AtomRef) → Result<BuildPlan<Self::Plan>>`
  - `fn apply(&self, plan: &BuildPlan<Self::Plan>) → Result<Vec<Self::Output>>`
- `BuildPlan<P>` enum: `Cached`, `NeedsBuild`, `NeedsEvaluation`
- Common types: `AtomRef`, `StorePath`
- Error taxonomy
- eos-core depends on atom-core
- Verify: `cargo check`; trait is implementable (mock impl in tests)

### Phase 7 — eos-store: artifact storage

Define the artifact store interface with snix wrapper.

- `ArtifactStore` trait: store, fetch, exists, check_substitute
- Thin wrapper over snix BlobService/DirectoryService
- Output digest types (content-addressed)
- eos-store depends on eos-core
- Verify: `cargo check`; trait is implementable

### Phase 8 — eos: the engine

Implement `BuildEngine` for snix-based evaluation and building.

- Engine struct satisfying `BuildEngine`
- Wire snix dependencies (`snix-castore`, `snix-store`, `snix-glue`, `nix-compat`)
- plan(): check artifact store → check plan cache → evaluate if needed
- apply(): evaluate → build → store artifact
- `ArtifactStore` backend using snix BlobService
- Engine reads atoms from `AtomStore` (passed at construction or via trait)
- Verify: `cargo test` with at least one plan + apply cycle

### Phase 9 — ion-manifest: concrete manifest

Port the `ion.toml` format as atom-core's `Manifest` implementation.

- Implement `Manifest` trait for ion.toml
- Port `Manifest`, `ValidManifest`, `Atom`, `Dependency` types
- Port the Compose system: `With` (use another atom's evaluator) and `As` (self-contained nix expression or static config)
- Port `AtomSet`, `SetMirror`, `AtomReq`, `ComposerSpec` types
- TOML serialization/deserialization
- Lock file types: `Lockfile`, `SetDetails`, `Dep` (atom and nix variants), `AtomDep` with `requires`
- ion-manifest depends on atom-id, atom-core
- Verify: `cargo test` validates round-tripping and Manifest trait satisfaction

### Phase 10 — ion-resolve: resolution library

Port the cross-ecosystem SAT resolver.

- `AtomResolver` and SAT logic (from existing `hyperdep-resolve` and `resolve/sat.rs`)
- Integrate with `VersionScheme`
- Port `resolvo` integration
- Lock file writing / reconciliation
- Verify: `cargo test` validates resolution against known dependency graphs

### Phase 11 — ion-cli: CLI entrypoint

Assemble ion as a working binary.

- CLI argument parsing and subcommand dispatch
- Config handling
- Wire `BuildEngine` — ion-cli is generic over `E: BuildEngine`
- `embedded-engine` feature: construct `eos::Engine` directly
- Local atom import: read atom.toml from disk, build git tree, stamp dev prerelease version, write to store
- `AtomStore::ingest` for populating from registries
- plan/apply dispatch (dry-run support)
- Verify: `ion --help` works, basic subcommands function

### Phase 12 — Integration and verification

End-to-end validation across all three workspaces.

- Full data flow: ion.toml → resolve → ingest → plan → apply → artifact
- Cryptographic chain: AtomId → Version → Revision → Plan → Output
- Document trait boundaries and cross-workspace contracts
- Dependency audit: no upward dependencies
- Verify `embedded-engine` feature flag
- Integration tests crossing workspace boundaries

## Verification checklist

- [ ] `cargo check` passes in all three workspaces independently
- [ ] `cargo test` passes in all crates
- [ ] atom-id ≤ 5 non-std deps; atom-core ≤ 10 total
- [ ] atom-git does not appear in ion's or eos-core's dependencies
- [ ] `AtomSource`, `AtomRegistry`, `AtomStore` are implementable outside the atom workspace
- [ ] No `semver` types in atom-core's public API
- [ ] No `ion.toml` types in atom-core's public API
- [ ] No concrete Derivation type in eos-core's public API
- [ ] `BuildEngine::plan()` returns `BuildPlan` enum
- [ ] serde derives are behind feature flags
- [ ] `BuildEngine` is in eos-core, not in any ion crate
- [ ] ion-cli does not depend on snix directly
- [ ] ion-manifest implements atom-core's `Manifest` trait
- [ ] ion-resolve is usable as a library independent of ion-cli
- [ ] `AtomStore::ingest(&dyn AtomSource)` works with registries, stores, and dev workspaces
- [ ] `ArtifactStore` wraps snix BlobService without leaking snix types
- [ ] No dependency flows upward
- [ ] `embedded-engine` feature flag toggles eos inclusion
- [ ] At least one end-to-end plan/apply operation works through the full stack

## Confidence: 0.93

Four open questions remain, none architectural:

1. **Manifest trait types** (MEDIUM) — The trait fields are known (label, version,
   deps, composer), but the generic representation of dependency summaries and
   composer configuration will only become clear when porting ion.toml in Phase 9.
   May require iterating Phase 3.

2. **AtomStore::ingest granularity** (LOW-MEDIUM) — Bulk `ingest(&dyn AtomSource)`
   is the right abstraction, but per-atom targeting (`ingest_atom(source, id,
version)`) may be needed. Will emerge from the Phase 11 dev workspace flow.

3. **eos-store dependency direction** (LOW) — Defined as depending on eos-core, but
   ArtifactStore may only need atom-core (for digest types). Minor wiring question.

4. **BuildPlan variants** (LOW) — The three variants cover the main cases. Partial
   cache scenarios (some deps cached, some not) may need a richer structure.
   Phase 8 will tell.

## Existing crate lineage

| Current crate      | Destination                                | Notes                                    |
| :----------------- | :----------------------------------------- | :--------------------------------------- |
| `atom` (crate)     | atom-id, atom-core, atom-git, ion-manifest | Split by concern                         |
| `nixec`            | `eos`                                      | Sandboxed nix-instantiate for evaluation |
| `hyperdep-resolve` | `ion-resolve`                              | SAT resolver                             |
| `config`           | ion-cli or ion-manifest                    | User config and composer settings        |
| `eka-root-macro`   | utility                                    | May survive as shared proc-macro         |

## References

- Sketch: `.sketches/2026-02-07-ion-atom-restructuring.md` (10 challenge iterations)
- ADR: `docs/adr/0001-monorepo-workspace-architecture.md`
- Atom Protocol SPEC: `atom/SPEC.md`
- Prior art: Bazel REAPI, Terraform plan/apply, gitoxide, snix, sigstore-rs
