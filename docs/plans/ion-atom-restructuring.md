# PLAN: Ion-Atom Restructuring (Revision 4a)

<!--
  Source sketch: .sketches/2026-02-07-ion-atom-restructuring.md
  Plan stage: SCOPE → COMMIT (revision 4a)
  Confidence: see bottom of document

  Key changes from revision 3:
  - Cryptographic chain as the motivating principle (AtomId→Version→Revision→Plan→Output)
  - BuildEngine redesigned: plan/apply with cache-skipping at every stage
  - Store taxonomy: AtomSource (read super-trait) + AtomRegistry (publishing) + AtomStore (working) + ArtifactStore (build outputs)
  - eos-local removed → eos. Engine is engine regardless of deployment mode
  - eos modularized: eos-core + eos-store + eos (3 crates)
  - BuildEngine uses associated types (Plan, Output) — object safety resolved via compile-time generics
  - Store-to-store transfer model: AtomStore::ingest(&dyn AtomSource)
  - DevWorkspace implements AtomSource — one codepath for published + dev atoms
  - Embedded engine default, daemon opt-in
  - AtomBackend (package-format adapter) deferred to future
-->

## Goal

Establish a clean, trait-bounded architecture for the **Atom protocol library**,
the **Eos runtime engine**, and the **Ion CLI tool**, housed in a monorepo
(`axios`) with independent Cargo workspaces. The architecture separates concerns
into a 5-layer stack:

```
Cyphrpass (Layer 0)  →  Atom (Layer 1)  →  Eos (Layer 2)  →  Ion (Layer 3)  →  Plugins (Layer 4)
identity/auth/signing   protocol types     runtime engine     user frontend     ecosystem adapters
```

### The Motivating Principle: The Cryptographic Chain

Every atom forms an unbroken, content-addressed chain from identity to final output:

```
AtomId → Version → Revision → Plan → Output
 (czd)   (semver)   (commit)  (recipe) (artifact)
```

For the snix engine, `Plan` is a Derivation (`.drv`). The chain uses the abstract
term because `BuildEngine::Plan` is an associated type — other engines may use
different plan formats.

Each step is cryptographically verifiable. Each step is independently cacheable.
This chain is what makes cache-skipping possible — if an artifact already exists
for a given plan, skip the build. If a plan already exists for a given revision,
skip evaluation. If the output is already built and signed by a trusted key, skip
everything.

The chain is actually a **DAG**: each atom's lock entry carries a `requires` field
listing the content-addressed digests of its transitive dependencies. The lock file
captures the complete dependency graph, not just a flat list.

This chain is the foundation of everything eos does.

### Three Workspaces

- **atom/** — the protocol library. Identity, addressing, publishing, store
  operations, and thin abstract interfaces (`Manifest` trait, `VersionScheme`
  trait, `AtomSource`/`AtomRegistry`/`AtomStore` traits). The Atom protocol is
  generic — cargo crates, npm packages, and ion-managed builds can all be
  atoms. The protocol is explicitly manifest-agnostic and version-scheme-agnostic.
- **eos/** — the runtime engine. Build execution via plan/apply with
  cache-skipping, artifact storage (snix blob model), and metadata queries.
  By the time work reaches eos, dependencies are locked. Eos reads atoms
  from an `AtomStore` — it never needs to know where they came from.
- **ion/** — the reference user frontend. CLI, dependency resolution, the
  concrete `ion.toml` manifest format, and dev workspace management. Ion is
  the planner (decides WHAT to build); eos is the executor (DOES the builds).

### Why the Manifest Is an Abstract Trait

The Atom protocol is **manifest-agnostic** (per v2 spec). Cargo crates use
`Cargo.toml`, npm packages use `package.json`, ion-managed atoms use
`ion.toml`. The `Manifest` trait in `atom-core` follows the same pattern as
`VersionScheme` — atom defines the abstraction, each ecosystem provides a
concrete implementation.

### Store Taxonomy

Three distinct stores, each with different semantics:

| Store             | Semantic                                                                                                                                              | Trait                      | Layer     |
| :---------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------- | :-------- |
| **AtomRegistry**  | Publishing front — source of truth for atom identity and versions. Git-backed, globally distributed, immutable once published.                        | `AtomRegistry: AtomSource` | atom-core |
| **AtomStore**     | Central working store — atoms collected from disparate sources (registries, other stores, dev workspaces). The universal handoff between ion and eos. | `AtomStore: AtomSource`    | atom-core |
| **ArtifactStore** | Build outputs — content-addressed blobs. snix BlobService/DirectoryService model. Shareable, substitutable, cacheable.                                | `ArtifactStore`            | eos-store |

`AtomSource` is the read-only super-trait that both `AtomRegistry` and
`AtomStore` extend. This enables `AtomStore::ingest(&dyn AtomSource)` — the
universal store-to-store transfer mechanism.

### Store-to-Store Transfer

Ion populates the `AtomStore` from multiple sources:

- Published atoms: `store.ingest(&registry)` — pull from registries
- Development atoms: `store.ingest(&dev_workspace)` — add local, unpublished atoms
- Cross-store: `store.ingest(&other_store)` — transfer between stores

Eos reads from the `AtomStore` to build. It never knows (or needs to know) whether
an atom was published or being developed locally. One codepath handles both.

**Deployment scenarios:**

- **Embedded** (default): ion and eos share the SAME `AtomStore` instance. No transfer needed.
- **Daemon**: ion transfers atoms from its store to eos's store before requesting builds.
- **Remote**: same as daemon, over the network (future).

## Constraints

- Atom, eos, and ion **MUST** be separate Cargo workspaces — no circular `Cargo.toml` dependencies
- Dependency direction is strictly layered: ion → eos-core, ion → atom-core, eos → atom-core. Never upward.
- `atom-core` has near-zero non-`std` deps — budget: ≤ 5 for atom-id, ≤ 10 for atom-core
- The `VersionScheme` and `Manifest` traits are abstract — atom-core has no semver types, no ion.toml types
- The `BuildEngine` trait uses associated types — each engine defines its own plan/output formats
- Object safety is not needed — compile-time generics via feature flags resolve engine selection
- Storage, identity, and signing will migrate to Cyphrpass — design seams, not implementations
- By the time work reaches eos, dependencies are locked — no resolution at the engine layer
- Lock file format is per-tool; atom may know format-type but not the hard schema
- `serde` is decoupled from core types via feature flag or companion crate
- Runtime operations belong in eos, not ion. Ion submits work; eos performs it.
- Embedded engine is the default deployment; daemon is opt-in for teams
- Start all traits sync; async is an eos-internal concern deferred until distributed engine
- `ekala.toml` is not an architectural pillar — may not survive Cyphrpass transition

## Decisions

| ID    | Decision                   | Choice                                                                                     | Rationale                                                                                                                 |
| :---- | :------------------------- | :----------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------ |
| KD-1  | atom-core dependencies     | `std` only + atom-id + atom-uri; `serde` via feature; `nom` in atom-uri only               | Enforces protocol purity. Storage deps belong in atom-git.                                                                |
| KD-2  | AtomDigest representation  | Hash-agnostic (`AsRef<[u8]>` or trait-based)                                               | Cyphrpass uses multi-algorithm digests.                                                                                   |
| KD-3  | Version abstraction        | `trait VersionScheme` from day one                                                         | Non-negotiable. Atom serves ecosystems beyond semver.                                                                     |
| KD-4  | Dependency resolution      | Lives in `ion-resolve`                                                                     | Resolution is tooling-layer, not protocol.                                                                                |
| KD-5  | Manifest abstraction       | Thin `Manifest` trait in atom-core; concrete `ion.toml` in ion-manifest                    | Same pattern as VersionScheme. Protocol is manifest-agnostic.                                                             |
| KD-6  | Workspace separation       | Three independent Cargo workspaces in monorepo                                             | Mechanical enforcement of layer separation.                                                                               |
| KD-7  | Build engine design        | `BuildEngine` trait with plan/apply and associated types (Plan, Output)                    | Cache-skipping at every stage. Terraform-style plan/apply. Engine-specific plan formats via associated types.             |
| KD-8  | atom-core substance        | Contains core implementation + test vectors, not just types                                | Prevents "ghost crate" failure. Must be independently testable.                                                           |
| KD-9  | Store taxonomy             | AtomSource (read) + AtomRegistry (publish) + AtomStore (working) + ArtifactStore (outputs) | Four distinct concerns with different semantics. AtomSource is the unifying read super-trait.                             |
| KD-10 | Store transfer             | `AtomStore::ingest(&dyn AtomSource)` — universal transfer                                  | Registries, other stores, and dev workspaces all implement AtomSource. One codepath for published and dev atoms.          |
| KD-11 | Ion internal decomposition | ion-cli, ion-resolve, ion-manifest                                                         | ion-manifest implements Manifest. ion-resolve does cross-ecosystem SAT.                                                   |
| KD-12 | Eos modularization         | eos-core (trait + plan types) + eos-store (ArtifactStore + snix wrapper) + eos (engine)    | Eos will be the largest component. Early modularization prevents monolith.                                                |
| KD-13 | Embedded engine mode       | `embedded-engine` feature flag on ion-cli compiles in `eos` directly                       | Solo dev: `ion build` works immediately. Daemon: opt-in for teams. No process management overhead by default.             |
| KD-14 | Sync-first traits          | All traits start synchronous. Async is internal to eos.                                    | Avoids forcing tokio into atom-core or ion.                                                                               |
| KD-15 | Lock file ownership        | Per-tool. Ion produces a unified lock containing both atom deps and engine-specific deps.  | Lock schema is ion-specific. Includes `AtomDep` (with `requires` for transitive digests) and direct nix deps.            |
| KD-16 | Plan abstraction           | Associated type `BuildEngine::Plan` — not a concrete Derivation in eos-core               | Future-proofs against post-nix plan formats (Guix G-expressions, hypothetical Bazel actions). Costs nothing.              |
| KD-17 | ArtifactStore wrapping     | Thin wrapper in eos-store over snix BlobService/DirectoryService                           | Store interface is eos's contract, not snix's. snix is an implementation detail of the default backend.                   |
| KD-18 | AtomBackend (adapter)      | Deferred. Future trait for package-format→atom adaptation (cargo→atom, npm→atom).          | Only ion exists as an implementor now. Cross-ecosystem adapters are future work. Manifest trait covers the metadata side. |

## Risks & Assumptions

| ID   | Risk / Assumption                                         | Severity | Status    | Mitigation / Evidence                                                                                                                      |
| :--- | :-------------------------------------------------------- | :------- | :-------- | :----------------------------------------------------------------------------------------------------------------------------------------- |
| R-1  | Cyphrpass API mismatch invalidates trait signatures       | MEDIUM   | Mitigated | Boundary correctness > API correctness. Transaction-centric vocabulary reduces divergence. ~30% chance of signature changes.               |
| R-2  | Version abstraction kills productivity                    | —        | CLOSED    | Non-negotiable per nrd. Cost accepted.                                                                                                     |
| R-3  | Three workspaces is overhead for solo dev                 | —        | CLOSED    | Intentional design for team scalability. The friction is the feature.                                                                      |
| R-4  | Premature eos abstraction                                 | MEDIUM   | Mitigated | BuildEngine is thin (plan/apply + associated types). Prior art: Bazel REAPI, snix builder. Start thin, grow from experience.               |
| R-5  | Manifest trait too thin / too thick                       | MEDIUM   | Mitigated | Start minimal (label, version, deps). VersionScheme pattern provides precedent. Grow from implementation.                                  |
| R-6  | atom-core scope creep                                     | MEDIUM   | Mitigated | Hard rule: if a type requires `gix`, `tokio`, `resolvo`, or snix, it does NOT belong in atom-core.                                         |
| R-7  | ArtifactStore wrapper diverges from snix                  | LOW      | Mitigated | Wrapper is deliberately thin: store, fetch, exists, substitute-check. snix is the reference backend.                                       |
| R-8  | BuildEngine plan/apply is over-engineered                 | LOW      | Accepted  | The cache-skipping chain is the core value proposition. plan/apply makes it explicit and enables dry-run. Terraform validates the pattern. |
| R-9  | eos-core + eos-store + eos is too many crates early on    | LOW      | Accepted  | Eos will grow. Early modularization is cheaper than late extraction. Each crate has a clear, distinct purpose.                             |
| R-10 | atom-uri requires surgery                                 | MEDIUM   | Accepted  | `LocalAtom` moves to ion; `gix::Url` becomes generic.                                                                                      |
| A-1  | Atom will sit atop Cyphrpass                              | —        | Validated | nrd actively involved in Cyphrpass development.                                                                                            |
| A-2  | Existing code has proven concepts worth porting           | —        | Validated | 2 years of working dep resolution, publishing, URI parsing, manifests.                                                                     |
| A-3  | Atom protocol is manifest-agnostic                        | —        | Validated | v2 spec: "protocol does not dictate the manifest format." KI confirms.                                                                     |
| A-4  | The cryptographic chain (AtomId→Output) is the foundation | —        | Validated | Motivating principle from nrd. Every step is content-addressed and cacheable.                                                              |
| A-5  | Store-to-store transfer unifies published + dev atoms     | —        | Validated | Current eka implementation uses internal AtomStore for exactly this purpose.                                                               |
| A-6  | ekala.toml is not a central architectural pillar          | —        | Validated | May not survive Cyphrpass transition.                                                                                                      |
| A-7  | Embedded engine is the right default                      | —        | Validated | Prior art: Cargo, single-user Nix, Go, Bazel local mode — none require daemons.                                                            |

## Open Questions

All major gaps have been filled through 9 challenge iterations:

- **GAP-1** (Trait signatures): FILLED — AtomSource/AtomRegistry/AtomStore decomposition.
- **GAP-2** (Runtime shape): FILLED → **REVISED** → **REVISED AGAIN** — BuildEngine plan/apply with associated types. Cache-skipping at every stage.
- **GAP-3** (Crate viability): FILLED — all crates have clear purposes.
- **GAP-4** (Dep budget): FILLED — concrete inventories enumerated.
- **GAP-5** (Prior art): FILLED — Bazel REAPI, Terraform plan/apply, snix, gitoxide.
- **GAP-6** (Manifest identity): FILLED — abstract trait in atom-core, concrete in ion-manifest.
- **GAP-7** (Store taxonomy): FILLED — AtomSource + AtomRegistry + AtomStore + ArtifactStore.
- **GAP-8** (Async boundaries): DEFERRED — start sync. Async is eos-internal.
- **GAP-9** (Cryptographic chain): FILLED — AtomId→Version→Revision→Plan→Output. Each step cacheable. Chain is a DAG via `requires`.
- **GAP-10** (Dev atom flow): FILLED — DevWorkspace implements AtomSource. ingest() unifies codepaths.

**Remaining**: Exact trait associated types, method signatures, and error taxonomies will emerge from porting concrete code.

## Scope

### In Scope

- Monorepo initialization (`axios/`) with three Cargo workspaces
- atom workspace: `atom-id`, `atom-uri`, `atom-core`, `atom-git`
- atom-core: `AtomSource`, `AtomRegistry`, `AtomStore`, `Manifest`, `VersionScheme` traits
- atom-core: `Manifest` trait includes composer information (how atoms declare their evaluator)
- eos workspace: `eos-core` (BuildEngine plan/apply), `eos-store` (ArtifactStore), `eos` (engine impl)
- ion workspace: `ion-cli`, `ion-resolve`, `ion-manifest`
- ion-manifest: concrete `ion.toml` format implementing atom-core's `Manifest` trait
- ion-manifest: Compose system (With/As variants) ported alongside manifest types
- Core trait definitions with all trait surfaces described above
- Porting proven types and logic from eka into the new structure
- Test vectors for protocol-level types
- BuildEngine plan/apply implementation with `eos` (snix-based)
- ArtifactStore with thin snix BlobService wrapper
- `embedded-engine` feature flag on ion-cli
- Store-to-store transfer via `AtomStore::ingest`

### Out of Scope

- Finalizing the Atom Protocol SPEC (sections 4–9 remain drafts)
- Implementing Cyphrpass integration
- Dynamic plugin system (WASM/RPC)
- Eos distributed engine / daemon / networking / `RemoteEngine`
- Full feature parity with current eka CLI — incremental porting
- Cross-ecosystem adapters (cargo→atom, npm→atom adapters)
- `AtomBackend` adapter trait (future, when cross-ecosystem support lands)
- Async trait boundaries
- Build scheduling, binary cache negotiation, multi-node coordination
- Globally syndicated stores (long-term vision, but the trait surface supports it)
- `ekala.toml` redesign

## Phases

Each phase is independently valuable and executable as a bounded `/core` invocation.

### Phase 1: Monorepo Scaffold

Establish the repository structure and all workspace roots.

- Initialize `axios/` with top-level README explaining the monorepo and layer model
- Create `atom/`, `eos/`, `ion/` workspace roots with `Cargo.toml`
- Create skeleton crates:
  - atom: `atom-id`, `atom-uri`, `atom-core`, `atom-git`
  - eos: `eos-core`, `eos-store`, `eos`
  - ion: `ion-cli`, `ion-resolve`, `ion-manifest`
- Wire inter-workspace path dependencies (10 crates, strict layer ordering)
- `embedded-engine` feature flag on ion-cli pulling in `eos`
- Verify: `cargo check` passes in all three workspaces

### Phase 2: atom-id — Identity Primitives

Port the protocol-level types that have zero storage coupling.

- Port `Label`, `Tag`, `Identifier` with `VerifiedName` trait and validation
- Port `AtomDigest` (generalize toward hash-agility if feasible)
- Port `AtomId<R>` with `Compute` and `Genesis` traits
- Port display implementations (`base32`, `FromStr`, `Display`)
- Port existing unit tests + add edge-case test vectors
- Dependency budget: ≤ 5 non-std deps
- Verify: `cargo test` in atom-id

### Phase 3: atom-core — Protocol Traits

Define the full trait surface.

- Define `AtomSource` trait (resolve, discover) — read-only super-trait
- Define `AtomRegistry` trait extending `AtomSource` (claim, publish)
- Define `AtomStore` trait extending `AtomSource` (ingest, fetch, contains)
- Define `VersionScheme` trait — abstract version comparison
- Define `Manifest` trait — thin metadata view (label, version, description, deps, composer)
- Define common types: `AtomContent`, `AtomEntry`, `Anchor`, `Snapshot`
- Define error taxonomy
- Re-export all atom-id public types
- serde support behind feature flag
- Dependency budget: ≤ 10 total
- Verify: `cargo check`, `cargo doc` produces clean trait documentation

### Phase 4: atom-uri — URI Parsing

Port URI handling with reduced coupling.

- Port `Uri`, `LocalAtom` types (LocalAtom may move to ion)
- Replace `gix::Url` with generic URL handling
- Integrate with atom-id types
- Port `nom`-based parsing logic and tests
- Verify: `cargo test` in atom-uri

### Phase 5: atom-git — Bridge Implementation

Port the git backend against atom-core traits.

- Implement `AtomRegistry` for git (claim→ref creation, publish→orphan commit, resolve→ref lookup, discover→ref enumeration)
- Implement `AtomStore` for git (ingest, fetch, contains)
- Port `Root` (genesis type for git — commit OID)
- Port ref layout and transport logic
- Port caching (`RemoteAtomCache`)
- Port workspace management (`ekala.toml` / `EkalaManager`) as pragmatic shim
- Wire up `gix` + `snix-*` dependencies
- Port integration tests
- Verify: `cargo test` validates git backend against atom-core trait contracts

### Phase 6: eos-core — Build Engine Trait

Define the build engine interface with plan/apply and cache-skipping.

- Define `BuildEngine` trait with associated types:
  - `type Plan` — engine-specific build recipe (for the snix engine, this is a Derivation)
  - `type Output` — engine-specific build output
  - `type Error`
  - `fn plan(&self, atom: &AtomRef) → Result<BuildPlan<Self::Plan>>`
  - `fn apply(&self, plan: &BuildPlan<Self::Plan>) → Result<Vec<Self::Output>>`
- Define `BuildPlan<P>` enum: `Cached`, `NeedsBuild`, `NeedsEvaluation`
- Define common types: `AtomRef`, `StorePath`
- Define error taxonomy
- eos-core depends on atom-core
- Verify: `cargo check`; trait is implementable (mock impl in tests)

### Phase 7: eos-store — Artifact Storage

Define the artifact store interface + snix thin wrapper.

- Define `ArtifactStore` trait: store, fetch, exists, check_substitute
- Thin wrapper over snix BlobService/DirectoryService
- Define output digest types (content-addressed)
- eos-store depends on eos-core
- Verify: `cargo check`; trait is implementable

### Phase 8: eos — The Engine

Implement `BuildEngine` for snix-based evaluation and building.

- Implement the engine struct satisfying `BuildEngine`
- Wire up snix dependencies (`snix-castore`, `snix-store`, `snix-glue`, `nix-compat`)
- Implement plan(): check artifact store → check plan cache → full eval needed
- Implement apply(): evaluate (if needed) → build (if needed) → store artifact
- Implement `ArtifactStore` backend using snix BlobService
- Engine reads atoms from `AtomStore` (passed at construction or via trait)
- Verify: `cargo test` with at least one plan + apply cycle

### Phase 9: ion-manifest — Concrete Manifest Implementation

Port the ion.toml format as atom-core's `Manifest` implementation.

- Implement `Manifest` trait for ion.toml format
- Port `Manifest`, `ValidManifest`, `Atom`, `Dependency`, `Compose` (With/As) types
- Port `AtomSet`, `SetMirror`, `AtomReq`, `ComposerSpec` types
- Port Compose system: how atoms declare their evaluator (With = use another atom's evaluator, As = self-contained nix/static)
- Port TOML serialization/deserialization
- Port lock file types (`Lockfile`, `SetDetails`, `Dep` with atom + nix dep variants, `AtomDep` with `requires`)
- ion-manifest depends on atom-id, atom-core
- Verify: `cargo test` validates round-tripping and `Manifest` trait satisfaction

### Phase 10: ion-resolve — Resolution Library

Port the cross-ecosystem SAT resolver.

- Port `AtomResolver` and SAT logic (from existing `hyperdep-resolve` and `resolve/sat.rs`)
- Integrate with `VersionScheme` — resolver is generic over version schemes
- Port `resolvo` integration
- Port lock file writing / reconciliation
- Verify: `cargo test` validates resolution against known dependency graphs

### Phase 11: ion-cli — CLI Entrypoint

Assemble ion as a working binary.

- Port CLI argument parsing and subcommand dispatch
- Port config handling
- Wire up `BuildEngine` — ion-cli is generic over `E: BuildEngine`
- `embedded-engine` feature: construct eos `Engine` directly
- Implement `DevWorkspace` as `AtomSource` for local dev atoms
- Wire up `AtomStore::ingest` for populating the store
- Implement plan/apply dispatch (dry-run support)
- Verify: `ion --help` works, basic subcommands function

### Phase 12: Integration and Verification

End-to-end validation across all three workspaces.

- Verify the full data flow: ion.toml → resolve → ingest → plan → apply → artifact
- Verify the cryptographic chain: AtomId→Version→Revision→Plan→Output
- Document trait boundaries and cross-workspace contracts
- Final dependency audit: no leaking deps, no upward dependencies
- Verify `embedded-engine` feature flag
- Write integration tests crossing workspace boundaries

## Verification

- [ ] `cargo check` passes in all three workspaces independently
- [ ] `cargo test` passes in all crates
- [ ] atom-id has ≤ 5 non-std dependencies
- [ ] atom-core has ≤ 10 total dependencies
- [ ] atom-git does NOT appear in ion's or eos-core's dependencies
- [ ] `AtomSource`, `AtomRegistry`, `AtomStore` traits are implementable outside atom workspace
- [ ] `VersionScheme` is abstract — no `semver` types in atom-core's public API
- [ ] `Manifest` trait is abstract — no `ion.toml` types in atom-core's public API
- [ ] `BuildEngine` uses associated types (Plan, Output) — no concrete Derivation type in eos-core's public API
- [ ] `BuildEngine::plan()` returns cache-aware `BuildPlan` enum
- [ ] serde derives are behind feature flags
- [ ] `BuildEngine` is in eos-core, NOT in any ion crate
- [ ] ion-cli does not depend on snix directly
- [ ] ion-manifest implements atom-core's `Manifest` trait
- [ ] ion-resolve is usable as a library independent of ion-cli
- [ ] `AtomStore::ingest(&dyn AtomSource)` works with registries, other stores, and dev workspaces
- [ ] `ArtifactStore` wraps snix BlobService without leaking snix types
- [ ] No dependency flows upward
- [ ] `embedded-engine` feature flag correctly toggles eos inclusion
- [ ] At least one end-to-end plan/apply operation works through the full stack

## Confidence Assessment

**CONFIDENCE: 0.93**

(Revised from 0.92 after challenge 10 codebase audit confirmed no architectural
gaps. Terminology incoherence fixed: chain now uses 'Plan' consistently.)

Remaining uncertainties (why not 1.0):

1. **Exact `Manifest` trait associated types** (MEDIUM): We know the trait is thin
   (label, version, description, dep summary, composer), but the exact types —
   especially how the dependency summary and composer configuration are represented
   generically across ecosystems — will only become clear when porting concrete
   ion.toml types. This may require iterating the trait design during Phase 3/Phase 9.

2. **`AtomStore::ingest` granularity** (LOW-MEDIUM): `ingest(&dyn AtomSource)` is
   the right abstraction, but the method may need to be more targeted — e.g.,
   `ingest_atom(source, id, version)` rather than bulk ingestion. The right
   signature will emerge from implementing the dev workspace flow in Phase 11.

3. **eos-store dependency direction** (LOW): eos-store is defined as depending
   on eos-core, but it's unclear whether ArtifactStore actually NEEDS eos-core
   types, or if it's fully self-contained. May end up depending only on atom-core
   (for digest types) rather than eos-core. Minor wiring question, not architectural.

4. **`BuildPlan` variants may need refinement** (LOW): The three variants
   (Cached/NeedsBuild/NeedsEvaluation) capture the main cases, but partial
   cache scenarios (some deps cached, some not) may require a richer structure.
   This will be discovered during Phase 8 implementation.

None of these uncertainties are architectural — they're all signature-level
details that will resolve during implementation. The layer boundaries, trait
decomposition, store taxonomy, and data flow are all settled.

## References

- Sketch: `.sketches/2026-02-07-ion-atom-restructuring.md` (10 challenge iterations)
- ADR: `docs/adr/0001-monorepo-workspace-architecture.md`
- Atom Protocol SPEC: `atom/SPEC.md`
- Prior art: Bazel REAPI, Terraform plan/apply, gitoxide, snix, sigstore-rs
