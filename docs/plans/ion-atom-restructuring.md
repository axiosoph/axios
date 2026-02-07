# PLAN: Ion-Atom Restructuring (Revision 3 — Manifest Abstraction)

<!--
  Source sketch: .sketches/2026-02-07-ion-atom-restructuring.md
  Plan stage: SCOPE → COMMIT (revision 3)
  Confidence: 0.85

  This revision resolves the manifest placement question from challenges 3–6.
  The manifest is an abstract protocol-level trait (like VersionScheme) defined
  in atom-core, with a concrete ion.toml implementation in ion-manifest.

  Key changes from revision 2:
  - Removed shared/manifest/ — the "shared" smell indicated unclear boundaries
  - Manifest is a thin abstract trait in atom-core (metadata view)
  - Concrete ion.toml format lives in ion-manifest (ion workspace)
  - Atom protocol is explicitly manifest-agnostic per the v2 spec
  - ekala.toml deferred as non-pillar (may not survive Cyphrpass transition)
  - Phase ordering reorganized around corrected dependency graph
  - Lock file management is per-tool; atom knows format-type at most
-->

## Goal

Establish a clean, trait-bounded architecture for the **Atom protocol library**,
the **Eos runtime engine**, and the **Ion CLI tool**, housed in a monorepo
(`axios`) with independent Cargo workspaces. The architecture separates concerns
into a 5-layer stack where each layer has a well-defined role:

```
Cyphrpass (Layer 0)  →  Atom (Layer 1)  →  Eos (Layer 2)  →  Ion (Layer 3)  →  Plugins (Layer 4)
identity/auth/signing   protocol types     runtime engine     user frontend     ecosystem adapters
```

The restructuring produces three independently valuable workspaces:

- **atom/** — the protocol library. Defines identity, addressing, publishing,
  store operations, and thin abstract interfaces (`Manifest` trait,
  `VersionScheme` trait) that ecosystem implementors satisfy. The Atom protocol
  is generic — cargo crates, npm packages, and ion-managed builds can all be
  atoms. The protocol is explicitly manifest-agnostic and version-scheme-agnostic.
- **eos/** — the runtime engine. Owns build execution, store management, and
  metadata queries. Reads manifests through atom-core's abstract `Manifest`
  trait. By the time work reaches eos, dependencies are locked — no resolution
  at the engine layer.
- **ion/** — the reference user frontend. Provides CLI, dependency resolution,
  and the concrete `ion.toml` manifest format. Ion is the planner (decides
  WHAT to build); eos is the executor (DOES the builds). Ion's resolver is
  unique: it resolves dependencies across atom ecosystems (ion atoms, cargo
  crate atoms, npm atoms) into a single unified lock using the generic
  `VersionScheme` abstraction.

### Why Three Workspaces

The existing eka codebase tightly couples the CLI to the runtime. The initial
plan (revision 1) repeated this mistake by placing `IonRuntime` directly in
ion. Lesson: **don't couple the frontend to the engine.** Eos as a separate
workspace mechanically prevents ion from accumulating engine implementation
details.

The planner/executor split maps cleanly to prior art:

- **Bazel**: client (planner) submits actions to Remote Execution API (executor)
- **Nix**: `nix` CLI vs. `nix-daemon` (limited by undocumented internal coupling)
- **snix**: gRPC builder protocol with pluggable local/remote backends

### Why the Manifest Is an Abstract Trait, Not a Shared Crate

Revision 2 placed the manifest in `shared/manifest/`. The name "shared" was a
code smell indicating unclear abstraction boundaries. Through six challenge
iterations, the correct decomposition emerged:

1. The Atom protocol is **manifest-agnostic** (per v2 spec). It does not
   dictate manifest format — cargo atoms use Cargo.toml, npm atoms use
   package.json, ion atoms use ion.toml.
2. Every atom ecosystem implementor specifies its own manifest and version
   scheme. This is the point of the `VersionScheme` abstraction — and the
   manifest follows the **same pattern**.
3. atom-core defines a thin `Manifest` trait (metadata view: label, version,
   description, dependency summary). This is the uniform interface that eos
   and other consumers use to query any atom.
4. Ion provides the concrete implementation: `ion.toml` parsing, `SemVer`
   version scheme, cross-ecosystem dependency resolution, unified lock file.
5. Eos reads manifests through the abstract trait for metadata queries and
   dependency tree inspection. All deps are locked before reaching eos.

## Constraints

- Atom, eos, and ion **MUST** be separate Cargo workspaces — no circular `Cargo.toml` dependencies
- Dependency direction is strictly layered: ion → eos-core, ion → atom-core, eos → atom-core. Never upward.
- Atom's public API is trait-based — implementations are swappable backends
- Ion depends on atom-core traits; **never** on atom-git directly
- `atom-core` has near-zero non-`std` deps — budget: ≤ 5 for atom-id, ≤ 10 for atom-core
- Storage, identity, and signing will migrate to Cyphrpass — design seams, not implementations
- Both eos and ion use `AtomStore` — eos owns the source-of-truth store, ion maintains a local cache. Both backed by Cyphrpass/git with cryptographic transaction logs.
- The `VersionScheme` trait is non-negotiable — atom does not mandate semver
- The `Manifest` trait follows the same pattern as `VersionScheme` — atom defines the abstraction, implementors (ion, cargo adapter, etc.) provide concrete formats
- `serde` is decoupled from core types via feature flag or companion crate
- Runtime operations (evaluate, build, query) belong in eos, not ion. Ion submits work; eos performs it.
- The `BuildEngine` trait (eos-core) replaces the former `IonRuntime` concept
- By the time work reaches eos, dependencies are locked — no resolution at the engine layer
- Lock file format is per-tool; atom may know the format-type (TOML, JSON) but not the hard schema
- Start all traits sync; async is an eos-internal concern deferred until distributed engine

## Decisions

| ID    | Decision                   | Choice                                                                            | Rationale                                                                                                                                   |
| :---- | :------------------------- | :-------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------ |
| KD-1  | atom-core dependencies     | `std` only + atom-id + atom-uri; `serde` via feature; `nom` in atom-uri only      | Enforces protocol purity. Storage deps belong in atom-git.                                                                                  |
| KD-2  | AtomDigest representation  | Hash-agnostic (`AsRef<[u8]>` or trait-based)                                      | Cyphrpass uses multi-algorithm digests. Hardcoding `[u8; 32]` (BLAKE3) precludes algorithm agility.                                         |
| KD-3  | Version abstraction        | `trait VersionScheme` from day one                                                | Non-negotiable. Atom serves ecosystems beyond semver. Cost of late abstraction exceeds cost of early generics.                              |
| KD-4  | Dependency resolution      | Lives in `ion-resolve`                                                            | Resolution algorithms are tooling-layer concerns, not protocol.                                                                             |
| KD-5  | Manifest abstraction       | Thin `Manifest` trait in atom-core; concrete `ion.toml` in ion-manifest           | Same pattern as VersionScheme. Protocol is manifest-agnostic. Each ecosystem provides its own format. Eos reads through the abstract trait. |
| KD-6  | Workspace separation       | Three independent Cargo workspaces in monorepo                                    | Mechanical enforcement. Prevents coupling between protocol (atom), engine (eos), and frontend (ion).                                        |
| KD-7  | Runtime architecture       | `BuildEngine` trait in eos-core; `LocalEngine` impl in eos-local                  | Replaces former `IonRuntime`. Runtime is an engine concern, not a frontend concern. Local mode via feature flag on ion-cli.                 |
| KD-8  | atom-core substance        | Contains core implementation + test vectors, not just types                       | Prevents the "ghost crate" failure mode. atom-core must be independently testable.                                                          |
| KD-9  | Trait vocabulary           | Two-layer: `AtomBackend` (transaction verbs) + `AtomStore` (consumption verbs)    | Aligns publishing layer with Cyphrpass transaction grammar; store layer with Atom SPEC consumption model.                                   |
| KD-10 | Store usage model          | Both eos and ion implement `AtomStore`. Eos = source of truth; ion = local cache. | Both cryptographically tracked via Cyphrpass transaction logs. Auditors can trace how atoms entered a store.                                |
| KD-11 | Ion internal decomposition | Multi-crate: ion-cli, ion-resolve, ion-manifest (minimum)                         | Ion is the planner — CLI + resolution + concrete manifest format. ion-manifest implements the abstract Manifest trait from atom-core.       |
| KD-12 | Eos internal decomposition | eos-core (trait + types) + eos-local (LocalEngine using snix)                     | Minimal initial surface. eos-eval, eos-store, eos-scheduler are future crate candidates as eos matures.                                     |
| KD-13 | Embedded engine mode       | `embedded-engine` feature flag on ion-cli pulls in eos-local                      | Single-machine dev experience: ion ships with LocalEngine. Client mode: ion talks to eos daemon. Same `BuildEngine` trait either way.       |
| KD-14 | Sync-first traits          | All traits start synchronous. Async is internal to eos implementations.           | Avoids forcing tokio into atom-core or ion. The async runtime is an eos concern (network, scheduling).                                      |
| KD-15 | Lock file ownership        | Lock files are per-tool. atom knows format-type at most.                          | Ion produces a unified lock across all atom ecosystems via its generic resolver. The lock schema is ion-specific. Atom doesn't overspecify. |

## Risks & Assumptions

| ID   | Risk / Assumption                                   | Severity | Status    | Mitigation / Evidence                                                                                                                                                 |
| :--- | :-------------------------------------------------- | :------- | :-------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R-1  | Cyphrpass API mismatch invalidates trait signatures | MEDIUM   | Mitigated | Boundary correctness > API correctness. Transaction-centric vocabulary reduces divergence.                                                                            |
| R-2  | Plugin seam never needed                            | LOW      | Accepted  | Plugin system (WASM) explicitly deferred. Runtime is now in eos. If never built, nothing wasted.                                                                      |
| R-3  | Version abstraction kills productivity              | —        | CLOSED    | Non-negotiable per nrd. Cost accepted.                                                                                                                                |
| R-4  | serde becomes a bottleneck at scale (eos)           | MEDIUM   | Mitigated | serde decoupled via feature flag or `atom-serde` crate. Core types are plain structs.                                                                                 |
| R-5  | Three workspaces is overhead for solo dev           | —        | CLOSED    | Intentional design for team scalability. The friction is the feature.                                                                                                 |
| R-6  | Trait design against evolving protocol              | MEDIUM   | Accepted  | Traits are narrow and operation-focused. ~30% chance of signature changes when Cyphrpass integrates.                                                                  |
| R-7  | Dependency goalpost drift                           | —        | CLOSED    | Concrete budget: atom-id ≤ 5, atom-core ≤ 10 (all protocol-relevant, zero storage deps).                                                                              |
| R-8  | atom-core scope creep                               | MEDIUM   | Mitigated | Hard rule: if a type requires `gix`, `tokio`, `resolvo`, or any storage/runtime dep, it does NOT belong in atom-core.                                                 |
| R-9  | Premature eos abstraction                           | MEDIUM   | Mitigated | `BuildEngine` trait surface is small and well-understood from prior art (Bazel REAPI, Nix daemon, snix builder). Start with thin types; let them grow.                |
| R-10 | Feature-flag complexity for embedded engine         | LOW      | Mitigated | One flag only: `embedded-engine`. Everything else unconditional.                                                                                                      |
| R-11 | Manifest trait too thin / too thick                 | MEDIUM   | Mitigated | Start with minimal metadata view (label, version, deps). Grow from implementation experience. The VersionScheme pattern provides precedent for the right granularity. |
| R-12 | Three workspaces too early — yagni                  | LOW      | Accepted  | Workspace boundary of eos is cheap to maintain (1-2 crates initially). The cost of NOT having the boundary is another rearchitecture exercise when eos matures.       |
| A-1  | Atom will sit atop Cyphrpass                        | —        | Validated | nrd actively involved in Cyphrpass development. Legacy storage provides fallback.                                                                                     |
| A-2  | Existing code has proven concepts worth porting     | —        | Validated | 2 years of working dep resolution, atom publishing, URI parsing, manifest management.                                                                                 |
| A-3  | Runtimes are required, plugins are optional         | —        | Validated | Runtime is now an eos concern (BuildEngine), not an ion concern.                                                                                                      |
| A-4  | Crate boundaries are mechanically necessary         | —        | Validated | For external contributors who lack maintainer context.                                                                                                                |
| A-5  | Near-zero deps is achievable for atom-core          | —        | Validated | Full dependency audit completed. atom-id: 4-5 deps. atom-core: 8-10.                                                                                                  |
| A-6  | Manifest is an abstract protocol concern            | —        | Validated | Atom v2 spec: "protocol does not dictate the manifest format." Manifest follows the VersionScheme pattern — abstract trait in atom, concrete impl in tooling.         |
| A-7  | Store operations are cryptographically tracked      | —        | Validated | Both eos and ion use AtomStore backed by Cyphrpass/git transaction logs.                                                                                              |
| A-8  | ekala.toml is not a central architectural pillar    | —        | Validated | May not survive Cyphrpass transition. Single source of truth may shift to transaction history. Not designed around.                                                   |

## Open Questions

All context gaps identified during CHALLENGE iterations have been filled:

- **GAP-1** (Trait signatures): FILLED — `AtomBackend` for publishing, `AtomStore` for consumption.
- **GAP-2** (Runtime shape): FILLED → **REVISED** — `IonRuntime` was wrong. Runtime operations belong in eos as `BuildEngine`.
- **GAP-3** (Crate viability): FILLED — atom-id viable standalone. atom-uri viable with surgery. atom-core as aggregation.
- **GAP-4** (Dep budget): FILLED — concrete inventories enumerated.
- **GAP-5** (Prior art): FILLED — gitoxide, iroh, sigstore-rs, Bazel REAPI, snix gRPC analyzed.
- **GAP-6** (Manifest identity): FILLED → **REVISED** — manifest is NOT atom-level or "shared." It's an abstract trait in atom-core (like VersionScheme), with concrete formats per-ecosystem. ion.toml is ion's format.
- **GAP-7** (Eos-atom interaction): FILLED — both eos and ion use `AtomStore`. Eos = source of truth; ion = local cache.
- **GAP-8** (Async boundaries): DEFERRED — start sync. Async is eos-internal when distributed engine arrives.

**Remaining**: Exact `Manifest` trait associated types and `BuildEngine` associated types will emerge from porting concrete code.

## Scope

### In Scope

- Monorepo initialization (`axios/`) with three Cargo workspaces
- atom workspace: `atom-id`, `atom-uri`, `atom-core`, `atom-git` crates
- atom-core: thin `Manifest` trait, `VersionScheme` trait, `AtomStore`, `AtomBackend`
- eos workspace: `eos-core` (BuildEngine trait + types), `eos-local` (LocalEngine via snix)
- ion workspace: `ion-cli`, `ion-resolve`, `ion-manifest` crates
- ion-manifest: concrete `ion.toml` format implementing atom-core's `Manifest` trait
- Core trait definitions: `AtomBackend`, `AtomStore`, `VersionScheme`, `Manifest`, `BuildEngine`
- Porting proven types and logic from eka into the new structure
- Test vectors for protocol-level types
- `BuildEngine` trait with initial `LocalEngine` implementation (snix-based)
- `embedded-engine` feature flag on ion-cli for single-machine usage

### Out of Scope

- Finalizing the Atom Protocol SPEC (sections 4–9 remain drafts)
- Implementing Cyphrpass integration (`atom-cyphr`)
- Dynamic plugin system (WASM/RPC)
- Multi-language implementations
- Eos distributed engine / `RemoteEngine` / eos daemon / networking
- Full feature parity with current eka CLI — incremental porting
- `ion-workspace` crate (defer until workspace coordination patterns are stable)
- Build scheduling, binary cache negotiation, multi-node coordination (future eos)
- Async trait boundaries (defer until distributed eos)
- Cross-ecosystem manifest adapters (cargo, npm — future work)
- ekala.toml as an architectural pillar (may be replaced by Cyphrpass transaction history)

## Phases

Each phase is independently valuable and executable as a bounded `/core` invocation.

### Phase 1: Monorepo Scaffold

Establish the repository structure and all workspace roots.

- Initialize `axios/` with top-level README explaining the monorepo structure and layer model
- Create `atom/` workspace with `Cargo.toml` workspace root
- Create `eos/` workspace with `Cargo.toml` workspace root
- Create `ion/` workspace with `Cargo.toml` workspace root
- Create skeleton crates in each workspace:
  - atom: `atom-id`, `atom-uri`, `atom-core`, `atom-git`
  - eos: `eos-core`, `eos-local`
  - ion: `ion-cli`, `ion-resolve`, `ion-manifest`
- Wire up inter-workspace path dependencies:
  - ion-cli → atom-core, eos-core, ion-resolve, ion-manifest
  - ion-manifest → atom-core, atom-id
  - eos-core → atom-core
  - eos-local → eos-core
  - ion-resolve → atom-core, ion-manifest
  - atom-core → atom-id, atom-uri
  - atom-git → atom-core
- `embedded-engine` feature flag on ion-cli pulling in eos-local
- Verify: `cargo check` passes in all three workspaces

### Phase 2: atom-id — Identity Primitives

Port the protocol-level types that have zero storage coupling.

- Port `Label`, `Tag`, `Identifier` with `VerifiedName` trait and validation logic
- Port `AtomDigest` (generalize away from hardcoded BLAKE3 if feasible, otherwise newtype)
- Port `AtomId<R>` with `Compute` and `Genesis` traits
- Remove the `crate::storage::git::Root` coupling leak — `AtomId<R>` is already generic
- Port display implementations (`base32`, `FromStr`, `Display`)
- Port existing unit tests from `id/mod/tests`
- Add comprehensive test vectors for label validation edge cases
- Dependency budget check: must be ≤ 5 non-std deps
- Verify: `cargo test` in atom-id passes with full coverage of ported logic

### Phase 3: atom-core — Protocol Traits and Abstractions

Define the trait surface, including the thin Manifest abstraction.

- Define `AtomBackend` trait (claim, publish, resolve, discover)
- Define `AtomStore` trait (ingest, query, fetch) — used by both eos and ion
- Define `VersionScheme` trait — abstract version comparison and satisfaction
- Define `Manifest` trait — thin metadata view:
  - Associated type for version (constrained by `VersionScheme`)
  - Label, version, description accessors
  - Dependency summary (abstract — each format has its own dep syntax)
  - The trait does NOT specify manifest file format, lock schema, or resolution
- Define `AtomAddress`, `AtomContent`, `AtomEntry` placeholder types
- Define atom-core's error taxonomy
- Re-export all atom-id public types
- Stub `atom-uri` integration
- serde support behind `serde` feature flag
- Dependency budget check: ≤ 10 total (including atom-id transitives)
- Verify: `cargo check` and `cargo doc` produce clean documentation of the full trait surface

### Phase 4: atom-uri — URI Parsing

Port URI handling with reduced coupling.

- Port `Uri`, `LocalAtom` types
- Replace `gix::Url` with generic URL handling where possible
- Integrate with atom-id types (`Label`, `Tag`)
- Port `nom`-based parsing logic
- Port URI tests
- Verify: `cargo test` in atom-uri passes

### Phase 5: atom-git — Bridge Implementation

Port the git backend against atom-core traits.

- Implement `AtomBackend` for git (wrapping existing `storage/git.rs` logic)
- Port `Root` (genesis type for git — commit OID)
- Port ref layout and transport logic
- Port caching (`RemoteAtomCache`)
- Port publishing logic from `package/publish/git/`
- Port workspace management (`ekala.toml` / `EkalaManager`) as pragmatic shim (not a core interface)
- Wire up `gix` + `snix-*` dependencies
- Port existing integration tests
- Verify: `cargo test` in atom-git validates the git backend against atom-core trait contracts

### Phase 6: eos-core — Build Engine Trait

Define the engine interface between ion and eos.

- Define `BuildEngine` trait with sync interface:
  - `evaluate(expr, args) → Result<Derivation>`
  - `build(derivation) → Result<Vec<BuildOutput>>`
  - `query(path) → Result<Option<PathInfo>>`
  - `check_substitutes(paths) → Result<Vec<SubstituteResult>>`
- BuildEngine is generic over atom-core's `Manifest` trait — eos can serve
  metadata queries and inspect dependency trees for any atom ecosystem
- Define common types: `StorePath`, `Derivation`, `BuildOutput`, `PathInfo`, `Expression`, `EvalArgs`
- Define eos-core error taxonomy
- eos-core depends on atom-core (for `Manifest` trait, `AtomStore` trait, `AtomId`, etc.)
- Keep types thin and minimal — grow from implementation experience
- Verify: `cargo check` in eos workspace; trait is implementable (mock impl in tests)

### Phase 7: eos-local — Local Engine

Implement `BuildEngine` for single-machine local execution via snix.

- Implement `LocalEngine` struct satisfying `BuildEngine`
- Wire up snix dependencies (`snix-castore`, `snix-store`, `snix-glue`, `nix-compat`)
- Implement evaluate → snix evaluation path
- Implement build → snix build path
- Implement query/check_substitutes against local store
- Port nixec subprocess pattern as `NixCliEngine` fallback (optional, feature-gated)
- Feature flag: `local-engine` on eos-core re-exports `LocalEngine`
- Verify: `cargo test` with at least one evaluation and one build operation

### Phase 8: ion-manifest — Concrete Manifest Implementation

Port the ion.toml manifest format as atom-core's Manifest implementation.

- Implement atom-core's `Manifest` trait for the ion.toml format
- Port `Manifest`, `ValidManifest`, `Atom`, `Dependency`, `Compose` types
- Port `AtomSet`, `SetMirror`, `AtomReq`, `ComposerSpec` types
- Port TOML serialization/deserialization
- Port validation logic (deny_unknown_fields, dependency-set consistency)
- Port lock file types (`Lockfile`, `SetDetails`, `Dep`)
- Ion-manifest depends on atom-id (Label, Tag), atom-core (Manifest trait)
- Does NOT include IO-heavy operations (ManifestWriter, SetResolver — those port to atom-git or ion-resolve)
- Port manifest unit tests
- Verify: `cargo test` validates manifest round-tripping and Manifest trait satisfaction

### Phase 9: ion-resolve — Resolution Library

Port the cross-ecosystem SAT resolver as a standalone library.

- Port `AtomResolver` and SAT logic from `package/resolve/sat.rs`
- Integrate with atom-core's `VersionScheme` trait — resolver is generic over version schemes
- Port `resolvo` integration
- Port lock file writing / reconciliation (ManifestWriter logic)
- Port set resolution logic (SetResolver — may stay here or in atom-git depending on deps)
- Ion's resolver resolves deps across ecosystems into a unified lock
- Port resolution tests
- Verify: `cargo test` validates resolution against known dependency graphs

### Phase 10: ion-cli — CLI Entrypoint

Assemble ion as a working binary that dispatches to eos.

- Port CLI argument parsing and subcommand dispatch
- Port config handling from `crates/config/`
- Wire up `BuildEngine` via eos-core — ion-cli takes a `Box<dyn BuildEngine>` or similar
- `embedded-engine` feature: construct `LocalEngine` at startup
- Future: client mode constructs `RemoteEngine` (not implemented, just the seam)
- Wire up ion-manifest + ion-resolve as dependencies
- Implement `AtomStore`-backed local atom cache
- Verify: `ion --help` works, basic subcommands route to BuildEngine correctly

### Phase 11: Integration and Smoke Testing

End-to-end validation across all three workspaces.

- Verify atom-core → atom-git → eos-core → eos-local → ion-cli data flow
- Document the trait boundaries:
  - What ion imports from atom and eos
  - How atom-git satisfies atom-core traits
  - How eos-local satisfies BuildEngine
  - How ion-manifest satisfies Manifest
- Verify cross-workspace dependencies work via path
- Write integration tests that cross workspace boundaries
- Final dependency audit: confirm no leaking deps, no upward dependencies
- Verify `embedded-engine` feature flag correctly includes/excludes eos-local

## Verification

- [ ] `cargo check` passes cleanly in all three workspaces independently
- [ ] `cargo test` passes in all crates
- [ ] atom-id has ≤ 5 non-std dependencies
- [ ] atom-core (aggregation) has ≤ 10 total dependencies
- [ ] atom-git does NOT appear in ion's `Cargo.toml` or any ion crate's imports
- [ ] atom-git does NOT appear in eos-core's dependencies (only atom-core)
- [ ] `AtomBackend` and `AtomStore` traits are implementable outside the atom workspace
- [ ] `VersionScheme` is abstract — no `semver` types in atom-core's public API
- [ ] `Manifest` trait is abstract — no `ion.toml` types in atom-core's public API
- [ ] `Manifest` trait is implementable by alternative ecosystems (cargo, npm adapters)
- [ ] serde derives are behind feature flags, not unconditional
- [ ] `BuildEngine` trait is in eos-core, NOT in any ion crate
- [ ] `BuildEngine` is generic over `Manifest` — works with any ecosystem's format
- [ ] ion-cli does not depend on snix directly — only transitively through eos-local when `embedded-engine` is enabled
- [ ] ion-manifest implements atom-core's `Manifest` trait
- [ ] ion-resolve is usable as a library independent of ion-cli
- [ ] No dependency flows upward (ion → eos → atom is the only direction)
- [ ] `embedded-engine` feature flag on ion-cli correctly toggles eos-local inclusion
- [ ] At least one end-to-end operation works through the full stack

## References

- Sketch: `.sketches/2026-02-07-ion-atom-restructuring.md`
- ADR: `docs/adr/0001-monorepo-workspace-architecture.md`
- Atom Protocol SPEC: `atom/SPEC.md`
- Atom Protocol v2 Vision: manifest agnosticism, version abstraction
- Prior art: Bazel Remote Execution API, gitoxide (crate decomposition), iroh (protocol+CLI split), sigstore-rs (trait design), snix gRPC builder protocol
