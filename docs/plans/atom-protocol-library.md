# PLAN: Atom Protocol Library

<!--
  Source sketch: .sketches/2026-02-15-atom-protocol-plan.md
  Plan stage: SCOPE (all phases defined, pending challenge pass)
  Confidence: 0.85 — all phases scoped, Phase 1 mostly validated, atom-core
              grounded in formal model, atom-git intentionally light

  Charter: Workstream 2 — Atom Protocol Library
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
`alg`, `tmb`, `typ`, and application-defined fields:

```json
{
  "pay": {
    "alg": "Ed25519",
    "anchor": "<anchor-b64ut>",
    "label": "my-package",
    "tmb": "<key-thumbprint>",
    "typ": "atom/claim"
  },
  "sig": "<signature-b64ut>"
}
```

Claims deliberately omit `now` — the payload must be reproducible so that
the same (anchor, label, key) triple always produces the same czd. This
prevents double-claiming the same label from the same source.

`typ` uses bare paths (`atom/claim`, `atom/publish`) — no domain prefix.
Domains imply centralization which conflicts with atom's decentralized model.

All cryptographic operations (hashing, signing, verification) are handled
by coz-rs. atom-id owns the payload struct definitions and verification
logic. atom-core defines only protocol traits — no direct crypto dep.

## Constraints

- Atom, eos, and ion are separate Cargo workspaces. No circular deps.
- Dependency direction: ion → eos → atom. Never upward.
- atom-id: ≤ 5 non-std deps. atom-core: ≤ 10 total.
- All crypto (hashing, signing, verification) handled by coz-rs. No BLAKE3 or other standalone hash crates.
- `VersionScheme` and `Manifest` are abstract — no semver or ion.toml types in atom-core.
  Concrete implementations live in format-specific crates (ion-manifest, etc.).
- Storage, identity, and signing will migrate to Cyphrpass. Design seams, not implementations.
  Cyphrpass uses Coz internally — coz-rs dependency is forward-compatible.
- `serde` behind feature flag.
- All traits start synchronous. Async deferred.
- Rust edition 2024, toolchain 1.90.0.

## Decisions

| Decision                             | Choice                                                                                                                   | Rationale                                                                                       |
| :----------------------------------- | :----------------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------- |
| URI alias sigil                      | `+` prefix marks aliases                                                                                                 | Eliminates alias-vs-URL ambiguity — the root cause of parser bugs. See sketch Round 3–5.        |
| Alias path delimiter                 | `/` (not `:`)                                                                                                            | Avoids all colon ambiguity. `+gh/owner/repo` not `+gh:owner/repo`.                              |
| URL aliasing as separate crate       | `alurl` at `axios/alurl/`                                                                                                | URL aliasing is a general-purpose concern, not atom-specific. Reusable outside eka.             |
| alurl structured output              | Depends on gix-url for URL classification                                                                                | gix-url handles SCP, scheme URLs, file paths, credentials. alurl doesn't replicate.             |
| atom-uri preserved as own crate      | Depends on alurl + atom-id                                                                                               | Thin layer: `::` splitting, label validation, `@version` extraction.                            |
| AliasResolver trait placement        | Defined in alurl                                                                                                         | The trait is part of the aliasing library's contract, not atom-specific.                        |
| atom-id dot delimiter                | `label.tag` (dot-separated)                                                                                              | AtomId = `alg.b64ut` display format. See atom-id sketch.                                        |
| atom-core = formal model coalgebras  | AtomSource, AtomRegistry, AtomStore                                                                                      | Directly derived from the formal layer model. No legacy traits ported. See sketch round 7.      |
| Existing traits not protocol         | QueryStore, QueryVersion, UnpackRef, RemoteAtomCache, Init, EkalaStorage — all git internals or ion concerns             | Evaluated against formal model. None correspond to protocol-level concepts.                     |
| No gix/semver in atom-core           | All trait signatures use associated types or atom-id abstractions                                                        | KD-1 + VersionScheme abstraction. Concrete types stay in atom-git/ion-manifest.                 |
| All crypto in atom-id via coz-rs     | atom-id owns payload structs, verification, identity. atom-core has no crypto dep.                                       | All hashing and signing is Coz. No standalone hash crates (no BLAKE3).                          |
| `typ` convention                     | `atom/claim`, `atom/publish` — bare paths, no domain prefix                                                              | Domains imply centralization. If canonical home needed, use a separate `pay` field like `src`.  |
| Claim reproducibility                | `atom/claim` omits `now` — same (anchor, label, key) always yields same czd                                              | Prevents double-claiming the same label from the same source location.                          |
| Publish uses `dig` (Coz standard)    | Content reference via Coz's `dig` field (`Vec<u8>`), not a custom `rev`                                                  | `dig` is the Coz-native content-addressed reference field. Backend-specific hash goes here.     |
| `Manifest` trait in atom-core        | Trait abstracts label, version, deps. Concrete formats (ion.toml, Cargo.toml, etc.) implemented by downstream crates.    | Atom is a generic packaging protocol — every format needs a manifest, but the format is theirs. |
| Key/identity management is Cyphrpass | atom-id provides verification function taking raw key bytes. Storage and discovery of public keys is not atom's concern. | No reason to duplicate. Cyphrpass is already all about key and identity management.             |
| Minimal protocol types               | Only `Dependency` is concrete in atom-core. `Entry`, `Content`, `Error` are associated types on traits.                  | If something can be removed without losing expressive power, remove it.                         |
| Recursive alias resolution           | alurl resolves recursively as library surface                                                                            | An alias can expand to another `+alias`. alurl handles the chain, not the caller.               |
| alurl workspace independence         | Standalone crate at `axios/alurl/`, no top-level workspace                                                               | atom-uri depends on alurl via path dep. `cargo test` from `atom/` won't run alurl tests.        |
| Session-type enforcement             | Natural data flow: `claim() → AtomId`, `publish(AtomId, ...)`                                                            | No typestate/builder needed. Can't publish without an id.                                       |
| atom-git = implement, not port       | Fresh design informed by existing code, not mechanical porting                                                           | Existing traits shaped by monolithic crate. Decomposition changes the design constraints.       |
| VersionScheme trait                  | In atom-id                                                                                                               | Identity-level abstraction. Version scheme is part of how atoms are named.                      |

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
   - [x] `AtomId` as `czd(alg, digest)` — coz-native identity
   - [x] Dot delimiter for `alg.b64ut` format
   - [x] Display/FromStr implementations
   - [x] 31 tests, macro dedup, snapshot tests
   - [ ] ≤ 5 non-std deps verified (blocked on serde feature-gating)
   - [ ] `RawVersion` newtype — unparsed version string
         Marks a string as an unresolved version needing implementor parsing.
         Lives in atom-id alongside `VersionScheme`.
   - [ ] `VersionScheme` trait — abstract version comparison
     - `type Version: Display + Ord` — parsed, comparable version
     - `type Requirement` — version constraint
     - `parse_version(raw: &RawVersion) → Result<Self::Version>`
     - `parse_requirement(raw: &str) → Result<Self::Requirement>`
     - `matches(version, req) → bool`
     - No concrete version types (semver stays in ion-manifest)
   - [ ] Feature-gate serde behind `serde` crate feature
   - [ ] **Coz transaction payload types**
     - `ClaimPayload` struct: `alg`, `anchor`, `label`, `tmb`, `typ = "atom/claim"`
       No `now` — payload must be reproducible (same inputs → same czd)
     - `PublishPayload` struct: `alg`, `atom_id`, `dig`, `now`, `tmb`, `typ = "atom/publish"`, `version`
       `dig` is the Coz standard field for content-addressed references (`Vec<u8>`).
       `now` records publication time (atom commits use epoch for reproducibility).
     - `TYP_CLAIM` and `TYP_PUBLISH` constants (bare paths, no domain)
   - [ ] **Verification function** — takes `(pay_json, sig, pub_key, alg)`, returns `Result<AtomId>`
         Key bytes are provided by the caller. Key storage/discovery is Cyphrpass's concern.
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
   - [ ] **`AtomSource` trait** — read-only observation (model §2.1)
     - `type Entry` — backend-defined observation type
     - `type Error`
     - `resolve(&self, id: &AtomId) → Result<Option<Entry>, Error>`
     - `discover(&self, query) → Result<Vec<Entry>, Error>`
   - [ ] **`AtomRegistry: AtomSource` trait** — claiming and publishing (model §2.2)
     - `type Content` — backend-defined content reference (git: ObjectId, etc.)
     - `claim(label: &Label) → Result<AtomId, Error>` — establish identity
     - `publish(id: &AtomId, version: &RawVersion, content: &Self::Content) → Result<(), Error>`
     - Session ordering enforced by data flow: can't publish without an `AtomId`
   - [ ] **`AtomStore: AtomSource` trait** — working store (model §2.3)
     - `ingest(&self, source: &dyn AtomSource) → Result<(), Error>`
     - `import_path(path) → Result<AtomId, Error>`
     - `contains(id: &AtomId) → bool`
   - [ ] **`Manifest` trait** — abstract metadata every package format must expose
     - `label() → &Label`
     - `version() → &RawVersion` — unparsed, implementor resolves via VersionScheme
     - `dependencies() → &[Dependency]`
     - `composer() → Option<&str>` — evaluation strategy hint
     - No serde, no TOML. Concrete formats (ion.toml, Cargo.toml, etc.) implement this.
   - [ ] **`Dependency`** — the only concrete protocol type
     - `label: Label` + `version: RawVersion`
   - [ ] **Error taxonomy** — per-trait error types via associated `type Error`
   - [ ] Re-export atom-id and atom-uri public types
   - [ ] serde behind feature flag for Dependency and re-exported types
   - [ ] Crate-level documentation explaining the coalgebra-trait mapping
   - Deps: atom-id, atom-uri. No coz-rs, no gix, no semver, no tokio.
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
   - Verify: `cargo test` in atom/ workspace

## Verification

- [x] Phase 1a: `cargo test` passes in atom/ workspace (31 tests) — VERIFIED
- [ ] Phase 1b: `cargo test` with VersionScheme tests, serde feature-gated
- [ ] Phase 1b: Payload round-trip test — construct ClaimPayload, sign, verify, extract AtomId
- [ ] Phase 1b: Reproducibility test — same (anchor, label, key) → same czd
- [ ] Phase 2: `cargo test` passes in alurl crate and atom/ workspace
- [ ] Phase 2: Existing URI test vectors from `crates/atom/src/uri/tests/` adapted and passing
- [ ] Phase 3: `cargo check`, `cargo doc --no-deps`, `cargo test` clean in atom/ workspace
- [ ] Phase 3: Trait signatures compile without coz-rs/gix/semver/tokio in atom-core deps
- [ ] Phase 4: `cargo test` with git integration tests (tmpdir repos)
- [ ] All phases: `cargo clippy` clean, no warnings
- [ ] All phases: `cargo doc --no-deps` clean per-crate

## Technical Debt

| Item | Severity | Why Introduced | Follow-Up | Resolved |
| :--- | :------- | :------------- | :-------- | :------: |

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
- Sketch: [atom-protocol-plan](../../.sketches/2026-02-15-atom-protocol-plan.md)
- Sketch: [formal-layer-model](../../.sketches/2026-02-15-formal-layer-model.md) (Coz payload design, crate responsibilities)
- Sketch: [ion-atom-restructuring](../../.sketches/2026-02-07-ion-atom-restructuring.md) (trait design history, dep budgets, gap analysis)
- Prior plan: [ion-atom-restructuring](ion-atom-restructuring.md) (phases 2–5 superseded)
- Formal model: [publishing-stack-layers](../models/publishing-stack-layers.md) (L1 coalgebras → trait mapping)
