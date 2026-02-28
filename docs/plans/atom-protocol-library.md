# PLAN: Atom Protocol Library

<!--
  Source sketch: .sketches/2026-02-15-atom-protocol-plan.md
  Plan stage: SCOPE (all phases defined, spec-driven)
  Confidence: 0.92 — spec defines 40 normative constraints, 13 machine-verified
              (TLA+ + Alloy). Phase items aligned with spec types and traits.

  Charter: Workstream 2 — Atom Protocol Library
  Spec: docs/specs/atom-transactions.md
  Supersedes: phases 2–5 of docs/plans/ion-atom-restructuring.md
-->

## Goal

Implement the four crates composing the atom protocol layer (atom-id,
atom-uri, atom-core, atom-git) plus a new standalone URL aliasing library
(alurl). This is charter workstream 2 — the protocol foundation that
everything else depends on.

### Transaction Model

Atom identity and publishing are Coz cryptographic transactions:

- **`atom/claim`** — establishes an atom's identity. The `czd` of this
  signed Coz message becomes the atom's `AtomId`.
- **`atom/publish`** — declares a new version of a claimed atom.

Each transaction is a Coz message (`{pay, sig}`) where `pay` contains
`alg`, `owner`, `typ`, and application-defined fields:

**Claim** — establishes identity (spec `[claim-typ]`):

```json
{
  "pay": {
    "alg": "Ed25519",
    "anchor": "<anchor-b64ut>",
    "label": "my-package",
    "now": "2026-02-28T12:00:00Z",
    "owner": "<identity-digest>",
    "typ": "atom/claim"
  },
  "sig": "<signature-b64ut>"
}
```

**Publish** — declares a version (spec `[publish-typ]`):

```json
{
  "pay": {
    "alg": "Ed25519",
    "anchor": "<anchor-b64ut>",
    "claim": "<claim-czd>",
    "dig": "<atom-snapshot-hash>",
    "label": "my-package",
    "now": "2026-02-28T12:01:00Z",
    "owner": "<identity-digest>",
    "path": "packages/my-package",
    "src": "<source-revision-hash>",
    "typ": "atom/publish",
    "version": "1.0.0"
  },
  "sig": "<signature-b64ut>"
}
```

`typ` uses bare paths (`atom/claim`, `atom/publish`) — no domain prefix.
Domains imply centralization which conflicts with atom's decentralized model.

All cryptographic operations conform to the Coz specification semantics
(spec `[crypto-via-coz]`). atom-id owns the payload struct definitions
and verification logic. atom-core defines only protocol traits — no
direct crypto dependency (spec `[crypto-layer-separation]`).

## Constraints

- Atom, eos, and ion are separate Cargo workspaces. No circular deps.
- All constraints from `docs/specs/atom-transactions.md` are normative (BCP 14).
- Crypto flows through atom-id via Coz (spec `[crypto-layer-separation]`).
- atom-id: ≤ 5 non-std deps. atom-core: ≤ 10 total.
- atom-core MUST NOT depend on any cryptographic crate (spec `[no-cross-layer-crypto]`).
- `VersionScheme` and `Manifest` are abstract — no semver or ion.toml types in atom-core.
  Concrete implementations live in format-specific crates (ion-manifest, etc.).
- `Manifest` requires exactly `label` and `version` (spec `[manifest-minimal]`).
- Storage, identity, and signing will migrate to Cyphrpass. Design seams, not implementations.
  Cyphrpass uses Coz internally — coz-rs dependency is forward-compatible.
- `serde` behind feature flag.
- All traits start synchronous. Async deferred.
- Rust edition 2024, toolchain 1.90.0.

## Decisions

| Decision                             | Choice                                                                                                                   | Rationale                                                                                                    |
| :----------------------------------- | :----------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------- |
| URI alias sigil                      | `+` prefix marks aliases                                                                                                 | Eliminates alias-vs-URL ambiguity — the root cause of parser bugs. See sketch Round 3–5.                     |
| Alias path delimiter                 | `/` (not `:`)                                                                                                            | Avoids all colon ambiguity. `+gh/owner/repo` not `+gh:owner/repo`.                                           |
| URL aliasing as separate crate       | `alurl` at `axios/alurl/`                                                                                                | URL aliasing is a general-purpose concern, not atom-specific. Reusable outside eka.                          |
| alurl structured output              | Depends on gix-url for URL classification                                                                                | gix-url handles SCP, scheme URLs, file paths, credentials. alurl doesn't replicate.                          |
| atom-uri preserved as own crate      | Depends on alurl + atom-id                                                                                               | Thin layer: `::` splitting, label validation, `@version` extraction.                                         |
| AliasResolver trait placement        | Defined in alurl                                                                                                         | The trait is part of the aliasing library's contract, not atom-specific.                                     |
| atom-id dot delimiter                | `label.tag` (dot-separated)                                                                                              | AtomId = `alg.b64ut` display format. See atom-id sketch.                                                     |
| atom-core = formal model coalgebras  | AtomSource, AtomRegistry, AtomStore                                                                                      | Directly derived from the formal layer model. No legacy traits ported. See sketch round 7.                   |
| Existing traits not protocol         | QueryStore, QueryVersion, UnpackRef, RemoteAtomCache, Init, EkalaStorage — all git internals or ion concerns             | Evaluated against formal model. None correspond to protocol-level concepts.                                  |
| No gix/semver in atom-core           | All trait signatures use associated types or atom-id abstractions                                                        | KD-1 + VersionScheme abstraction. Concrete types stay in atom-git/ion-manifest.                              |
| All crypto in atom-id via Coz        | atom-id owns payload structs, verification, identity. atom-core has no crypto dep.                                       | All hashing and signing follows Coz specification semantics. No standalone hash crates.                      |
| `typ` convention                     | `atom/claim`, `atom/publish` — bare paths, no domain prefix                                                              | Domains imply centralization. If canonical home needed, use a separate `pay` field like `src`.               |
| Claim includes `now`                 | `atom/claim` includes `now` for fork disambiguation; czd incorporates timestamp                                          | Without `now`, identical (anchor, label, key) across forks would collide. Spec `[atomid-per-source-unique]`. |
| Publish uses `dig` (Coz standard)    | Content reference via Coz's `dig` field (`Vec<u8>`), not a custom `rev`                                                  | `dig` is the Coz-native content-addressed reference field. Backend-specific hash goes here.                  |
| `Manifest` trait in atom-core        | Trait abstracts label, version, deps. Concrete formats (ion.toml, Cargo.toml, etc.) implemented by downstream crates.    | Atom is a generic packaging protocol — every format needs a manifest, but the format is theirs.              |
| Key/identity management is Cyphrpass | atom-id provides verification function taking raw key bytes. Storage and discovery of public keys is not atom's concern. | No reason to duplicate. Cyphrpass is already all about key and identity management.                          |
| Minimal protocol types               | No concrete types in atom-core. `Entry`, `Content`, `Error` are associated types on traits.                              | Spec removed `Dependency` — dependency edges are a manifest/resolver concern, not protocol.                  |
| Spec-driven implementation           | Types implement spec constraints directly; `cargo check` = constraint verification.                                      | 15 constraints verifiable by type system. See spec §Verification.                                            |
| Recursive alias resolution           | alurl resolves recursively as library surface                                                                            | An alias can expand to another `+alias`. alurl handles the chain, not the caller.                            |
| alurl workspace independence         | Standalone crate at `axios/alurl/`, no top-level workspace                                                               | atom-uri depends on alurl via path dep. `cargo test` from `atom/` won't run alurl tests.                     |
| Session-type enforcement             | Natural data flow: `claim() → AtomId`, `publish(AtomId, ...)`                                                            | No typestate/builder needed. Can't publish without an id.                                                    |
| atom-git = implement, not port       | Fresh design informed by existing code, not mechanical porting                                                           | Existing traits shaped by monolithic crate. Decomposition changes the design constraints.                    |
| VersionScheme trait                  | In atom-id                                                                                                               | Identity-level abstraction. Version scheme is part of how atoms are named.                                   |

## Risks & Assumptions

| Risk / Assumption                                     | Severity | Status    | Mitigation / Evidence                                                                              |
| :---------------------------------------------------- | :------- | :-------- | :------------------------------------------------------------------------------------------------- |
| alurl scope creep (replicating gix-url)               | MEDIUM   | Mitigated | Strict scope: alias detection + expansion only. gix-url handles classification.                    |
| Bare SCP without user (`host:path`) remains ambiguous | LOW      | Accepted  | Narrow case. gix-url handles it after alias expansion. Not alurl's problem.                        |
| atom-uri "too thin" to justify a crate                | LOW      | Accepted  | It bridges alurl + atom-id. Thin is correct — it's a format adapter.                               |
| atom-core trait surface premature                     | MEDIUM   | Mitigated | Grounded in formal model coalgebras. Sketch round 7 validated against existing code.               |
| atom-git scope underestimated                         | MEDIUM   | Open      | Existing git code is 2000+ lines. Clean decomposition may surface surprises during implementation. |
| Cyphrpass API mismatch breaks trait signatures        | MEDIUM   | Mitigated | Design seams, not implementations. ~30% chance of signature changes.                               |
| gix-url as alurl dependency adds weight               | LOW      | Accepted  | gix-url is already needed transitively via atom-git. alurl uses it for structured output.          |
| VersionScheme too abstract for practical use          | MEDIUM   | Open      | Must support semver + NixOS epoch scheme at minimum. Design shapes during Phase 1b.                |

## Open Questions

_None remaining. All resolved — see Decisions table._

## Scope

### In Scope

- `alurl` crate: standalone URL aliasing library with `AliasResolver` trait
- `atom-uri` crate: atom URI grammar (`[source]::<label>[@<version>]`)
- `atom-core` crate: protocol traits (AtomSource, AtomRegistry, AtomStore, Manifest)
- `atom-git` crate: git backend implementing atom-core traits via gix
- Porting proven types and logic from existing `crates/atom/`
- Test vectors for all crates

### Out of Scope

- Cyphrpass integration (design seams only)
- Public key storage and discovery (Cyphrpass's concern)
- Atom Protocol SPEC sections 4–9
- Ion-specific manifest implementation (ion-manifest)
- Dependency resolution (ion-resolve)
- eos workspace crates
- Async trait boundaries
- Global alias registries (alurl resolves from local config only)
- `ekala.toml` redesign

## Phases

<!--
  Phase 1a is COMPLETE. Phase 1b (VersionScheme + serde) is remaining.
  Phase 2 design converged via sketch. Phases 3–4 scoped from formal model.
-->

1. **Phase 1: atom-id** — Identity primitives and crypto
   - [x] `Label`, `Tag`, `Identifier` with UAX #31 validation
   - [x] `AtomId` as `(Anchor, Label)` — protocol identity pair (spec revision 2026-02-28)
         Replaced `AtomId { alg, czd }` with `AtomId { anchor, label }`.
         Identity is the abstract pair, algorithm-free and permanent.
         `AtomDigest { alg, cad }` (store-level multihash) deferred to Phase 3.
   - [x] `Anchor` newtype — opaque `Vec<u8>` with b64ut Display/Serde
   - [x] `::` delimiter for `anchor_b64ut::label` format
   - [x] Display/FromStr/Serde implementations
   - [x] 39 tests (naming, anchor, atom-id structural equality)
   - [x] ≤ 5 non-std deps verified (4 required + 2 optional behind `serde` feature)
   - [x] `RawVersion` newtype — unparsed version string
         Marks a string as an unresolved version needing implementor parsing.
         Lives in atom-id alongside `VersionScheme`.
   - [x] `VersionScheme` trait — abstract version comparison
     - `type Version: Display + Ord` — parsed, comparable version
     - `type Requirement` — version constraint
     - `parse_version(raw: &RawVersion) → Result<Self::Version>`
     - `parse_requirement(raw: &str) → Result<Self::Requirement>`
     - `matches(version, req) → bool`
     - No concrete version types (semver stays in ion-manifest)
   - [x] Feature-gate serde behind `serde` crate feature (default on)
   - [x] **Coz transaction payload types** (spec §Types)
     - `ClaimPayload` struct: `alg`, `anchor`, `label`, `now`, `owner`, `typ = "atom/claim"`
       `owner` is an opaque identity digest (spec `[owner-abstract]`).
       `now` included for fork disambiguation (spec `[atomid-per-source-unique]`).
     - `PublishPayload` struct: `alg`, `anchor`, `claim` (czd), `dig`, `label`,
       `now`, `owner`, `path`, `src`, `typ = "atom/publish"`, `version`
       `claim` chains to the authorizing claim czd (spec `[publish-chains-claim]`).
       `dig` is the atom snapshot hash (spec `[dig-is-atom-snapshot]`).
       `src` is the source revision hash (spec `[src-is-source-revision]`).
       `path` is the subdirectory path (spec `[path-is-subdir]`).
     - `TYP_CLAIM` and `TYP_PUBLISH` constants (bare paths, no domain)
     - `Tmb` type alias → `coz_rs::Thumbprint` re-export
     - `serde_alg` module — serde bridge for `Alg` via `name()`/`from_str()`
     - `serde_b64` module — serde bridge for `Vec<u8>` via base64url-unpadded
   - [x] **Verification function** — takes `(pay_json, sig, alg, pub_key)`, returns `Result<Payload, VerifyError>`
         `verify_claim` → `Result<ClaimPayload>`, `verify_publish` → `Result<PublishPayload>`.
         Key bytes are provided by the caller. Key storage/discovery is Cyphrpass's concern.
   - **Spec constraints verified at Phase 1 completion:**
     - rustc: `symmetric-payloads`, `claim-typ`, `publish-typ`, `path-is-subdir`,
       `rawversion-opaque`, `claim-key-required`, `publish-key-optional`, `uri-not-metadata`
     - cargo-dep: `crypto-via-coz` (atom-id depends on coz-rs)
     - unit-test: `sig-over-pay`, `claim-transition`, `publish-transition`
   - **Formal model informs implementation:**
     - TLA+ `PublishChainsClaim`: publish must reference claim czd — enforces `claim` field
     - TLA+ `SessionOrdering`: claim must precede publish — enforces data-flow API
     - TLA+ `AtomIdPerSourceUnique`: czd includes `now` — enforces timestamp in claim
   - Verify: `cargo test` in atom/ workspace

2. **Phase 2: alurl + atom-uri** — URL aliasing and atom URI grammar
   - [ ] **alurl crate** (`axios/alurl/`)
     - [ ] Standalone Cargo crate with README, docs
     - [ ] `AliasResolver` trait (abstract config interface)
     - [ ] `+` sigil alias detection
     - [ ] Alias expansion: `+name/path` → resolve(name) + `/` + path
     - [ ] Recursive resolution: expanded result checked for `+` prefix, resolved again
     - [ ] Structured output via gix-url (SCP, scheme, file, etc.)
     - [ ] `AliasedUrl` enum: Expanded (with original alias + resolved URL) / Raw (pass-through)
     - [ ] Error types for resolution failures
     - [ ] Unit tests with HashMap-based mock resolver
   - [ ] **atom-uri updates** (`axios/atom/atom-uri/`)
     - [ ] Depend on alurl + atom-id
     - [ ] `::` delimiter: `rsplit_once("::")` → source vs atom-ref
     - [ ] `@` version extraction: `rsplit_once("@")` → `RawVersion` (from atom-id)
           Unparsed version wrapped in the `RawVersion` newtype. Implementors
           parse via their `VersionScheme`. First: ion (semver).
     - [ ] `Label` validation via atom-id
     - [ ] `RawAtomUri` type (parsed, unresolved, version as `Option<RawVersion>`)
     - [ ] `AtomUri` type (parsed, resolved via AliasResolver)
     - [ ] Port and adapt existing test vectors from `crates/atom/src/uri/tests/`
   - Verify: `cargo test` in alurl and atom/ workspace

3. **Phase 3: atom-core** — Protocol trait surface

   Directly derived from the formal layer model's L1 coalgebras. No
   legacy traits ported — the existing `QueryStore`, `QueryVersion`,
   `UnpackRef`, `Init`, `EkalaStorage`, etc. are git internals or ion
   concerns, not protocol concepts (see sketch round 7).

   atom-core defines trait surfaces only. No crypto — all identity and
   verification logic lives in atom-id (which owns coz-rs). atom-core
   consumes atom-id's types (`AtomId`, `ClaimPayload`, etc.) without
   needing a direct coz-rs dependency.
   - [ ] **`AtomSource` trait** — read-only observation (model §2.1, spec §Source/Store)
     - `type Entry` — backend-defined observation type
     - `type Error`
     - `resolve(&self, id: &AtomId) → Result<Option<Entry>, Error>`
     - `discover(&self, query) → Result<Vec<Entry>, Error>`
   - [ ] **`AtomRegistry: AtomSource` trait** — claiming and publishing (model §2.2, spec §Source/Store)
     - `type Content` — backend-defined content reference (git: ObjectId, etc.)
     - `claim(req: ClaimReq) → Result<Czd, Error>` — establish ownership
     - `publish(req: PublishReq) → Result<(), Error>` — publish a version
     - Session ordering enforced by data flow: can't publish without a claim czd
   - [ ] **`AtomStore: AtomSource` trait** — working store (model §2.3, spec §Source/Store)
     - `ingest(&self, source: &dyn AtomSource) → Result<(), Error>`
       Accumulation guarantee: store ⊇ source after ingest (spec `[ingest-preserves-identity]`)
     - `contains(id: &AtomId) → bool`
   - [ ] **`Manifest` trait** — minimal metadata (spec `[manifest-minimal]`)
     - `label() → &Label`
     - `version() → &RawVersion` — unparsed, implementor resolves via VersionScheme
     - No `dependencies()`, no `composer()` — these are ecosystem-specific concerns.
     - No serde, no TOML. Concrete formats (ion.toml, Cargo.toml, etc.) implement this.
   - [ ] **`AtomDigest` type** — store-level multihash (spec §Types)
     - `AtomDigest { alg: Alg, cad: Cad }` — compact, self-describing
     - `compute(id: &AtomId, alg: Alg) -> AtomDigest` via `canonical_hash_for_alg`
     - Display as `alg.b64ut`, used for git ref paths and store keys
     - Multiple valid digests per AtomId (one per algorithm)
     - **Debt**: verify `AtomId`'s derived `Hash` impl is consistent with
       `AtomDigest.compute()` (structural hash vs content hash)
   - [ ] **Error taxonomy** — per-trait error types via associated `type Error`
   - [ ] Re-export atom-id and atom-uri public types
   - [ ] serde behind feature flag for re-exported types
   - [ ] Crate-level documentation explaining the coalgebra-trait mapping
   - Deps: atom-id, atom-uri, coz-rs (for Cad/canonical_hash_for_alg). No gix, no semver, no tokio.
   - **Spec constraints verified at Phase 3 completion:**
     - rustc: `backend-agnostic-protocol`, `trait-signature-pure`
     - cargo-dep: `crypto-layer-separation`, `no-cross-layer-crypto`, `key-management-deferred`
     - unit-test: `digest-algorithm-agile`
   - **Formal model informs implementation:**
     - Alloy `ingest_preserves_identity`: store ⊇ source after ingest — shapes `AtomStore.ingest` contract
     - Alloy `anchor_set_coherence`: shared anchor → shared atom-set — shapes `AtomSource.discover` semantics
     - Alloy `manifest_properties`: label + version only — shapes `Manifest` trait
   - Verify: `cargo check`, `cargo doc --no-deps` clean, `cargo test`

4. **Phase 4: atom-git** — Git backend

   Implements the atom-core traits for git-backed stores. The existing
   `crates/atom/src/storage/git.rs` (1099 lines) and `cache.rs` (~300
   lines) are **reference material**, not a porting source. Internal
   architecture should be designed fresh — the existing `QueryStore`,
   `UnpackRef`, `RemoteAtomCache` traits may or may not survive in any
   recognizable form.

   atom-git is the signing context — it constructs Coz messages using
   coz-rs (via atom-id), signs them, and writes the resulting transactions
   to git refs. Anchor is derived from the git genesis commit. All crypto
   goes through coz-rs — no standalone hash crates.
   - [ ] **`Root(ObjectId)` newtype** — genesis/anchor type for git stores
   - [ ] **Anchor derivation** — derive anchor from git genesis commit via Coz
   - [ ] **Coz signing context** — construct `ClaimPayload`/`PublishPayload`,
         sign with coz-rs, produce AtomId (czd) values
   - [ ] **`impl AtomSource for GitRemote`** — discover + resolve via git ref querying
     - Internal implementation uses gix ref listing (`refs/eka/atoms/*`)
     - Ref parsing into `(Label, Version, ObjectId)` — internal, not trait surface
   - [ ] **`impl AtomRegistry for GitRegistry`** — claim and publish
     - Claim: fill `ClaimPayload`, sign, write ref. AtomId = czd of signed claim.
     - Publish: fill `PublishPayload`, sign, write version ref
   - [ ] **`impl AtomStore for GitStore`** — local store management
     - Ingest: fetch from remote into local cache repo
     - Import: copy local atom into cache with dev prerelease version
     - Contains: check ref existence
   - [ ] **Wire dependencies** (gix, gix-protocol, coz-rs, tempfile)
   - [ ] **Tests**: integration tests exercising trait contracts
         (ideally against in-memory or tmpdir repos, not network)
   - Deps: atom-core, atom-id, gix, gix-protocol, coz-rs, tempfile
   - **Spec constraints verified at Phase 4 completion:**
     - unit-test: `dig-is-atom-snapshot`, `atom-snapshot-reproducible`
     - integration-test: `src-is-source-revision`, `verification-local`,
       `verification-provenance`, `anchor-immutable`, `anchor-content-addressed`,
       `anchor-discoverable`, `backend-bit-perfect`, `atom-detached`
   - **Formal model informs implementation:**
     - TLA+ `NoDuplicateVersion`: reject duplicate version publish — shapes `GitRegistry.publish` guard
     - TLA+ `NoBackdatedPublish`: version ordering — shapes publish validation
     - TLA+ `NoUnclaimedPublish`: publish requires prior claim — shapes ref validation
   - Verify: `cargo test` in atom/ workspace

## Verification

- [x] Phase 1a: `cargo test` passes in atom/ workspace (31 tests) — VERIFIED
- [ ] Phase 1b: `cargo test` with VersionScheme tests, serde feature-gated
- [ ] Phase 1b: Payload round-trip test — construct ClaimPayload, sign, verify, extract AtomId
- [ ] Phase 1b: Reproducibility test — same inputs → same czd
- [ ] Phase 2: `cargo test` passes in alurl crate and atom/ workspace
- [ ] Phase 2: Existing URI test vectors from `crates/atom/src/uri/tests/` adapted and passing
- [ ] Phase 3: `cargo check`, `cargo doc --no-deps`, `cargo test` clean in atom/ workspace
- [ ] Phase 3: Trait signatures compile without coz-rs/gix/semver/tokio in atom-core deps
- [ ] Phase 3: `Manifest` has exactly `label` + `version` (spec `[manifest-minimal]`)
- [ ] Phase 4: `cargo test` with git integration tests (tmpdir repos)
- [ ] All phases: `cargo clippy` clean, no warnings
- [ ] All phases: `cargo doc --no-deps` clean per-crate
- [ ] All phases: spec constraint coverage — 15 `agent-check` constraints verified via type system

### Formal Verification (complete)

- [x] TLA+ temporal safety — 8 invariants, 2 configs (fork + distinct-anchor) — VERIFIED
- [x] Alloy structural assertions — 5 assertions, scope 4 — VERIFIED
- [x] Fork scenario satisfiable (Alloy SAT) — VERIFIED

## Technical Debt

| Item | Severity | Why Introduced | Follow-Up | Resolved |
| :--- | :------- | :------------- | :-------- | :------: |

## Technical Debt

| Item                                                                       | Phase | Severity | Follow-up                                                                      |
| :------------------------------------------------------------------------- | :---: | :------: | :----------------------------------------------------------------------------- |
| `serde_alg` bridge should move upstream to coz-rs behind a `serde` feature |   1   |   Low    | Open issue/PR on coz-rs — every consumer needing Alg serde will duplicate this |
| Verify `AtomId`'s derived `Hash` is consistent with `AtomDigest.compute()` |   3   |   Low    | Structural hash vs content hash — verify when implementing AtomDigest          |
| `serde_json` promoted to runtime dep for verification; review dep budget   |   1   |   Low    | May need feature-gating if consumers want types without JSON parsing           |

## Deviation Log

| Commit        |  Delta   | Description                                                                                                                                                  |
| :------------ | :------: | :----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Payload types | EXPANDED | Added `serde_alg.rs`, `serde_b64.rs` modules (not in plan). Required for correct Coz wire format — `Alg` lacks native serde, `Vec<u8>` needs b64ut encoding. |
| Payload types | EXPANDED | Re-exported `Thumbprint` from coz-rs, added `Tmb` type alias. Spec uses `Tmb` shorthand.                                                                     |
| Payload types | REFINED  | Constructors take `AtomId` instead of separate `anchor`+`label`. Stronger type-level guarantee that identity pair is validated.                              |
| Verification  | REFINED  | Returns `Result<Payload>` instead of `Result<AtomId>` — parsed payload is more useful, caller can extract AtomId trivially.                                  |
| Verification  | EXPANDED | `serde_json` promoted from dev-dep to runtime dep. Required for `from_slice` in verification functions.                                                      |
| Serde gate    | REFINED  | Verification functions also gated behind `serde` feature. Verification inherently depends on JSON deserialization.                                           |

## Retrospective

<!-- Filled in after execution is complete. -->

### Process

_Not yet complete._

### Outcomes

_Not yet complete._

### Pipeline Improvements

_Not yet complete._

## References

- Charter: [decentralized-publishing-stack](../charters/decentralized-publishing-stack.md)
- Spec: [atom-transactions](../specs/atom-transactions.md) (40 constraints, BCP 14)
- TLA+: [AtomTransactions](../specs/tla/AtomTransactions.tla) (temporal safety)
- Alloy: [atom_structure](../specs/alloy/atom_structure.als) (structural assertions)
- Sketch: [atom-protocol-plan](../../.sketches/2026-02-15-atom-protocol-plan.md)
- Sketch: [formal-layer-model](../../.sketches/2026-02-15-formal-layer-model.md) (Coz payload design, crate responsibilities)
- Sketch: [ion-atom-restructuring](../../.sketches/2026-02-07-ion-atom-restructuring.md) (trait design history, dep budgets, gap analysis)
- Prior plan: [ion-atom-restructuring](ion-atom-restructuring.md) (phases 2–5 superseded)
- Formal model: [publishing-stack-layers](../models/publishing-stack-layers.md) (L1 coalgebras → trait mapping)
