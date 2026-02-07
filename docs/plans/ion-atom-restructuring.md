# PLAN: Ion-Atom Restructuring

<!--
  Source sketch: .sketches/2026-02-07-ion-atom-restructuring.md
  Plan stage: SCOPE → COMMIT
  Confidence: 0.90

  This plan formalizes the restructuring of the eka project into a layered,
  trait-bounded architecture across two independent workspaces in a shared
  monorepo (axios). The atom protocol library is decoupled from the ion CLI
  tool, and both are internally decomposed into focused library crates.
-->

## Goal

Establish a clean, trait-bounded architecture for the **Atom protocol library** and the **Ion CLI tool**, housed in a monorepo (`axios`) with independent Cargo workspaces. The architecture must accommodate future integration with Cyphrpass (identity/auth/storage) without requiring redesign, while preserving proven concepts from the existing eka codebase. This is a port with architectural discipline — not a rewrite, not a greenfield experiment.

The restructuring produces two independently valuable workspaces:

- **atom/** — the protocol library. Defines identity, addressing, publishing, and store operations. Backend-agnostic by design; git is the initial backend but not architecturally privileged.
- **ion/** — a tooling layer that consumes atom. Provides CLI, dependency resolution, manifest management, workspace coordination, and runtime dispatch. Ion is one of many potential atom consumers, not a co-dependent.

## Constraints

- Atom and ion **MUST** be separate Cargo workspaces — no mutual `Cargo.toml` dependencies
- Atom's public API is trait-based — implementations are swappable backends
- Ion depends on atom-core traits; **never** on atom-git directly
- `atom-core` (and `atom-id` within it) has near-zero non-`std` dependencies — concrete budget: ≤ 5 for atom-id, ≤ 10 for atom-core aggregation
- Storage, identity, and signing will migrate to Cyphrpass — design seams, not implementations
- The `VersionScheme` trait is non-negotiable — atom does not mandate semver
- `serde` is decoupled from core types via feature flag or companion crate
- Ion's runtime interface (`IonRuntime`) is a first-class architectural boundary — runtimes are required backends, not optional plugins

## Decisions

| ID    | Decision                   | Choice                                                                         | Rationale                                                                                                                            |
| :---- | :------------------------- | :----------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------- |
| KD-1  | atom-core dependencies     | `std` only + atom-id + atom-uri; `serde` via feature; `nom` in atom-uri only   | Enforces protocol purity. Storage deps belong in atom-git.                                                                           |
| KD-2  | AtomDigest representation  | Hash-agnostic (`AsRef<[u8]>` or trait-based)                                   | Cyphrpass uses multi-algorithm digests. Hardcoding `[u8; 32]` (BLAKE3) precludes algorithm agility.                                  |
| KD-3  | Version abstraction        | `trait VersionScheme` from day one                                             | Non-negotiable per nrd. Atom serves ecosystems beyond semver. Cost of late abstraction exceeds cost of early generics.               |
| KD-4  | Dependency resolution      | Lives in `ion-resolve`                                                         | Resolution algorithms are tooling-layer concerns, not protocol.                                                                      |
| KD-5  | Manifest format            | Lives in `ion-manifest`                                                        | Manifest schemas (`ion.toml`, lock files) are ion-specific. Other atom consumers define their own.                                   |
| KD-6  | Workspace separation       | Independent Cargo workspaces in monorepo                                       | Mechanical enforcement for contributors. Single workspace + convention doesn't scale to team.                                        |
| KD-7  | Plugin architecture        | Runtime trait now; dynamic plugins deferred                                    | Runtimes ≠ plugins. `IonRuntime` is required infrastructure. WASM/plugin system is a separate, future design.                        |
| KD-8  | atom-core substance        | Contains core implementation + test vectors, not just types                    | Prevents the "ghost crate" failure mode. atom-core must be independently testable.                                                   |
| KD-9  | Trait vocabulary           | Two-layer: `AtomBackend` (transaction verbs) + `AtomStore` (consumption verbs) | Aligns publishing layer with Cyphrpass transaction grammar; store layer with Atom SPEC consumption model.                            |
| KD-10 | IonRuntime                 | First-class trait with evaluate/build/query operations                         | snix is preferred impl; nixec pattern (subprocess) is compatibility fallback. Trait shaped by snix API, not by subprocess semantics. |
| KD-11 | Ion internal decomposition | Multi-crate: ion-cli, ion-manifest, ion-resolve (minimum)                      | Ion is already complex. Manifest and resolver are library concerns consumed by the CLI binary.                                       |

## Risks & Assumptions

| ID  | Risk / Assumption                                   | Severity | Status    | Mitigation / Evidence                                                                                                                                                                                                                                |
| :-- | :-------------------------------------------------- | :------- | :-------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R-1 | Cyphrpass API mismatch invalidates trait signatures | MEDIUM   | Mitigated | Boundary correctness > API correctness. Changing method signatures within an established boundary is painful but doable; moving a boundary after coupling cements is sometimes impossible. Transaction-centric vocabulary reduces future divergence. |
| R-2 | Plugin seam never needed                            | LOW      | Accepted  | Runtimes ≠ plugins. The runtime seam (`IonRuntime`) is required infrastructure. The plugin system (WASM) is explicitly deferred. If never built, nothing is wasted — the seam is an internal trait.                                                  |
| R-3 | Version abstraction kills productivity              | —        | CLOSED    | Non-negotiable per nrd. Cost accepted.                                                                                                                                                                                                               |
| R-4 | serde becomes a bottleneck at scale (eos)           | MEDIUM   | Mitigated | serde decoupled via feature flag or `atom-serde` crate. Core types are plain structs.                                                                                                                                                                |
| R-5 | Two workspaces is overhead for solo dev             | —        | CLOSED    | Intentional design for team scalability. The friction is the feature.                                                                                                                                                                                |
| R-6 | Trait design against evolving protocol              | MEDIUM   | Accepted  | Traits are narrow and operation-focused (claim/publish/resolve/discover). Unlikely to change shape fundamentally even if implementation substrate changes. ~30% chance of breaking signature changes when Cyphrpass integrates.                      |
| R-7 | Dependency goalpost drift                           | —        | CLOSED    | Concrete budget defined: atom-id ≤ 5, atom-core ≤ 10 (all protocol-relevant, zero storage deps).                                                                                                                                                     |
| R-8 | atom-core scope creep                               | MEDIUM   | Mitigated | Hard rule: if a type requires `gix`, `tokio`, `resolvo`, or any storage/runtime dep, it does NOT belong in atom-core. Protocol-level deps only.                                                                                                      |
| A-1 | Atom will sit atop Cyphrpass                        | —        | Validated | nrd is actively involved in Cyphrpass development. Legacy storage interface provides fallback.                                                                                                                                                       |
| A-2 | Existing code has proven concepts worth porting     | —        | Validated | 2 years of working dep resolution, atom publishing, URI parsing, manifest management.                                                                                                                                                                |
| A-3 | Runtimes are required, plugins are optional         | —        | Validated | nrd's explicit correction: runtimes (nix/snix/guix) are the execution substrate. Ion dispatches to a runtime like a compiler dispatches to a code generator.                                                                                         |
| A-4 | Crate boundaries are mechanically necessary         | —        | Validated | For external contributors who lack maintainer context.                                                                                                                                                                                               |
| A-5 | Near-zero deps is achievable for atom-core          | —        | Validated | Full dependency audit completed. atom-id: 4-5 deps. atom-core aggregation: 8-10. All protocol-relevant.                                                                                                                                              |

## Open Questions

All context gaps identified during CHALLENGE have been filled through research:

- **GAP-1** (Trait signatures): FILLED — two-layer vocabulary derived from Atom SPEC + Cyphrpass analysis. `AtomBackend` for publishing, `AtomStore` for consumption. Draft signatures in sketch.
- **GAP-2** (IonRuntime shape): FILLED — snix already in dependency tree. Trait models evaluate/build/query. Two impls: `SnixRuntime` (preferred) + `NixCliRuntime` (compat).
- **GAP-3** (Crate viability): FILLED — atom-id viable standalone. atom-uri viable with surgery (remove `LocalAtom`, replace `gix::Url`). atom-core as aggregation crate.
- **GAP-4** (Dep budget): FILLED — complete inventory enumerated from `Cargo.toml` source analysis. no_std feasible for atom-id (WASM target) but low priority.
- **GAP-5** (Prior art): FILLED — gitoxide, iroh, sigstore-rs, cargo analyzed. Sweet spot: 4-6 crates per workspace.

**Remaining uncertainty**: Exact trait method signatures will be refined during implementation. The vocabulary and structure are settled; the precise type-level details (associated types, error bounds, async boundaries) will emerge from porting concrete code against the trait definitions.

## Scope

### In Scope

- Monorepo initialization (`axios/`) with two Cargo workspaces
- atom workspace: `atom-id`, `atom-uri`, `atom-core`, `atom-git` crates
- ion workspace: `ion-cli`, `ion-manifest`, `ion-resolve` crates (minimum)
- Core trait definitions: `AtomBackend`, `AtomStore`, `VersionScheme`, `IonRuntime`
- Porting proven types and logic from eka into the new structure
- Test vectors for protocol-level types
- `IonRuntime` trait with initial `SnixRuntime` implementation

### Out of Scope

- Finalizing the Atom Protocol SPEC (sections 4–9 remain drafts)
- Implementing Cyphrpass integration (`atom-cyphr`)
- Dynamic plugin system (WASM/RPC)
- Multi-language implementations
- Building eos (distributed evaluation)
- Full feature parity with current eka CLI — incremental porting
- `ion-workspace` crate (defer until workspace coordination patterns are stable)

## Phases

Each phase is independently valuable and executable as a bounded `/core` invocation.

1. **Phase 1: Monorepo Scaffold** — establish the repository structure and workspace roots
   - Initialize `axios/` with top-level README explaining the monorepo structure
   - Create `atom/` workspace with `Cargo.toml` workspace root
   - Create `ion/` workspace with `Cargo.toml` workspace root
   - Create skeleton `atom-id` crate (lib, `Cargo.toml`, empty module structure)
   - Create skeleton `atom-core` crate (lib, depends on atom-id)
   - Create skeleton `atom-git` crate (lib, depends on atom-core)
   - Create skeleton `ion-manifest` crate (lib)
   - Create skeleton `ion-resolve` crate (lib)
   - Create skeleton `ion-cli` crate (bin, depends on ion-manifest, ion-resolve, atom-core)
   - Verify: `cargo check` passes in both workspaces

2. **Phase 2: atom-id — Identity Primitives** — port the protocol-level types that have zero storage coupling
   - Port `Label`, `Tag`, `Identifier` with `VerifiedName` trait and validation logic
   - Port `AtomDigest` (generalize away from hardcoded BLAKE3 if feasible, otherwise newtype)
   - Port `AtomId<R>` with `Compute` and `Genesis` traits
   - Remove the `crate::storage::git::Root` coupling leak — `AtomId<R>` is already generic
   - Port display implementations (`base32`, `FromStr`, `Display`)
   - Port existing unit tests from `id/mod/tests`
   - Add comprehensive test vectors for label validation edge cases
   - Dependency budget check: must be ≤ 5 non-std deps
   - Verify: `cargo test` in atom-id passes with full coverage of ported logic

3. **Phase 3: atom-core — Protocol Traits and Aggregation** — define the trait surface and re-export atom-id
   - Define `AtomBackend` trait (claim, publish, resolve, discover)
   - Define `AtomStore` trait (ingest, query, fetch)
   - Define `VersionScheme` trait
   - Define `AtomAddress`, `AtomContent`, `AtomEntry` placeholder types
   - Define atom-core's error taxonomy
   - Re-export all atom-id public types
   - Stub `atom-uri` integration (URI types can be ported here or in a sub-crate)
   - serde support behind `serde` feature flag
   - Dependency budget check: ≤ 10 total (including atom-id transitives)
   - Verify: `cargo check` and `cargo doc` produce clean documentation of the full trait surface

4. **Phase 4: atom-git — Bridge Implementation** — port the git backend against atom-core traits
   - Implement `AtomBackend` for git (wrapping existing `storage/git.rs` logic)
   - Port `Root` (genesis type for git — commit OID)
   - Port ref layout and transport logic
   - Port caching (`RemoteAtomCache`)
   - Port publishing logic from `package/publish/git/`
   - Wire up `gix` + `snix-*` dependencies
   - Port existing integration tests
   - Verify: `cargo test` in atom-git validates the git backend against atom-core trait contracts

5. **Phase 5: ion-manifest — Manifest Library** — port the manifest/lock parsing as a standalone library
   - Port `Manifest`, `ValidManifest`, `Lockfile` types from `package/metadata/`
   - Port `ManifestBuilder`/`ManifestWriter` from `manifest/write/`
   - Port atom-set handling from `package/metadata/manifest/set.rs`
   - Define `ion-manifest`'s own error types
   - Port existing manifest tests
   - Verify: `cargo test` validates manifest round-tripping

6. **Phase 6: ion-resolve — Resolution Library** — port the SAT resolver as a standalone library
   - Port `AtomResolver` and SAT logic from `package/resolve/sat.rs`
   - Integrate with atom-core's `VersionScheme` trait (not concrete `semver`)
   - Port `resolvo` integration
   - Port resolution tests
   - Verify: `cargo test` validates resolution against known dependency graphs

7. **Phase 7: ion-cli — CLI Entrypoint** — assemble ion as a working binary
   - Port CLI argument parsing and subcommand dispatch
   - Port config handling from `crates/config/`
   - Define `IonRuntime` trait in ion-cli (or `ion-runtime` crate if warranted)
   - Implement initial `SnixRuntime` using existing snix dependencies
   - Implement `NixCliRuntime` fallback (port from nixec pattern)
   - Wire up ion-manifest + ion-resolve as dependencies
   - Verify: `ion --help` works, basic subcommands route correctly

8. **Phase 8: Integration and Smoke Testing** — end-to-end validation
   - Verify atom-core → atom-git → ion-cli data flow works for at least one operation (e.g., atom discovery or publish)
   - Document the trait boundary: what ion imports from atom, how atom-git satisfies atom-core traits
   - Verify cross-workspace dependency works (ion workspace depends on atom workspace via path)
   - Write integration tests that cross the atom ↔ ion boundary
   - Final dependency audit: confirm no leaking deps

## Verification

- [ ] `cargo check` passes cleanly in both workspaces independently
- [ ] `cargo test` passes in all crates
- [ ] atom-id has ≤ 5 non-std dependencies
- [ ] atom-core (aggregation) has ≤ 10 total dependencies
- [ ] atom-git does NOT appear in ion's `Cargo.toml` or any ion crate's imports
- [ ] `AtomBackend` and `AtomStore` traits are implementable outside the atom workspace
- [ ] `VersionScheme` is abstract — no `semver` types in atom-core's public API
- [ ] serde derives are behind feature flags, not unconditional
- [ ] ion-manifest and ion-resolve are usable as libraries independent of ion-cli
- [ ] At least one end-to-end operation (e.g., atom discovery) works through the full stack

## References

- Sketch: `.sketches/2026-02-07-ion-atom-restructuring.md`
- ADR: `docs/adr/0001-monorepo-workspace-architecture.md`
- Atom Protocol SPEC: `atom/SPEC.md`
- Prior art: gitoxide (crate decomposition), iroh (protocol+CLI split), sigstore-rs (trait design)
