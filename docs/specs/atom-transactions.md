# SPEC: Atom Transaction Protocol

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
-->

## Domain

**Problem Domain:** The atom protocol defines two cryptographic transactions
(claim and publish) that establish and extend the identity of publishable
source-code packages across decentralized backends. These transactions use
Coz v1.0 as the signing and verification substrate. This spec constrains the
behavioral contracts that all implementations MUST satisfy.

**Model Reference:** [publishing-stack-layers.md](../models/publishing-stack-layers.md)
— §2.1–2.3 (coalgebras), §3.1 (PublishSession), olog (identity stability).

**Criticality Tier:** High — supply chain protocol with cryptographic
guarantees. Compromised source code has potentially infinite blast radius.

## Anchor

An **anchor** is a cryptographic commitment that establishes the identity of
an atom-set. All atoms within an atom-set share the same anchor. The anchor
pins the atom-set to an immutable reference point in the source's history.

The anchor is **abstract** — its derivation is backend-specific:

| Backend | Anchor derivation                                         |
| :------ | :-------------------------------------------------------- |
| Git     | Hash of the genesis commit (first commit in repo history) |
| Other   | Backend-defined; MUST satisfy properties below            |

An anchor MUST satisfy the following properties:

1. **Immutable**: The anchor for a given source MUST NOT change over time.
   It is fixed at source creation and persists for the lifetime of the
   atom-set.
2. **Content-addressed**: The anchor MUST be a cryptographic digest derived
   from content, not from metadata that can be externally manipulated.
3. **Unique**: Two distinct sources MUST NOT produce the same anchor
   (collision resistance of the underlying hash function).
4. **Discoverable**: Given access to a source, a consumer MUST be able to
   independently derive the anchor without trusting the publisher.

The anchor feeds into identity: `AtomId = hash(anchor, label)`. Because
the anchor is immutable and the label is fixed per atom, the AtomId is
permanent.

## Implementor Abstraction

The atom protocol is intentionally agnostic to package formats, versioning
schemes, and build systems. These concerns are the responsibility of
**ecosystem adapters** — implementations that bridge between the abstract
protocol and concrete package ecosystems (e.g., Cargo crates, npm packages,
ion recipes).

### Manifest

Every package format has its own manifest (e.g., `Cargo.toml`,
`package.json`, `recipe.ion`). The atom protocol requires certain
information from these manifests but does not define their schema.

Implementors MUST satisfy the `Manifest` trait:

```
TYPE  Manifest = trait {                                           (atom-core)
        fn label(&self)   -> &Label;       -- package name
        fn version(&self) -> &RawVersion;  -- unparsed version string
      }
```

The protocol requires exactly `label` and `version` from a manifest.
Ecosystem adapters MAY expose additional metadata (dependencies,
build configuration, etc.) through their own types.

### Backend

Backend implementations handle storage and retrieval of claims, publishes,
and atom content. The atom protocol defines what MUST be stored and
retrieved, not how.

A backend MUST:

1. Store and retrieve claim `CozMessage`s by `AtomId`.
2. Store and retrieve publish `CozMessage`s by `(AtomId, version)`.
3. Derive the anchor from the source (backend-specific).
4. Enforce version uniqueness per `(AtomId, claim czd)` pair.
5. Support multiple claims for the same `AtomId` by different owners.

A backend MUST NOT:

1. Alter the content of signed `CozMessage`s (bit-perfect preservation).
2. Impose crypto requirements beyond what the protocol defines.

### Atom URI

The atom URI is a human-writable reference format:
`[source "::"] label ["@" version]`. The `::` delimiter separates the
source (where to find the atom) from the label and optional version.

URI source expansion (alias resolution, URL classification) is handled
by a generic aliasing library and is out of scope for this
specification. The atom protocol extends that library's reference
format with the `:: label @ version` suffix.

Atom URIs are a **user convenience only**. They MUST NOT appear in
signed metadata, transaction payloads, or persisted protocol state.
All persistent references MUST use canonical forms (`AtomId`, expanded
source references).

### Atom Detachment

An atom is a **detached subtree** — an isolated fragment of a larger
source, extracted for independent distribution. The atom snapshot
(`dig`) captures this subtree as a reproducible, self-contained
artifact with deterministic metadata.

This isolation is fundamental:

1. An atom carries no source history — it is a single content tree.
2. Fetching an atom does NOT require fetching the full source.
3. Provenance (which source revision the atom came from) is recorded
   in the publish payload (`src`, `path`) but is verified separately
   from the atom's content.

### Source / Store Topology

The protocol defines a two-tier data flow for atom distribution:

**Sources** are the canonical upstream locations where atoms are
published and discovered. A source is where the claim and publish
transactions live. When a user adds a dependency, they reference a
source. When a consumer verifies provenance, they trace back to the
source.

**Stores** are local caches that accumulate atoms from multiple
sources. A store ingests atoms from any number of sources into a
single, unified local collection. Build systems (eos) and package
managers (ion) operate against the store, not against individual
sources — this allows efficient action over many atoms without
repeated network access.

The protocol formalizes this topology through three roles
(model §2.1–2.3):

**AtomSource** — read-only observation (the common interface):

- `resolve(AtomId) → Option<Self::Entry>` — look up an atom
- `discover(Query) → Set<AtomId>` — search for atoms

**AtomRegistry** — extends AtomSource with write operations
(publishing front, lives at the source):

- `claim(ClaimReq) → Result<Czd>` — establish ownership
- `publish(PublishReq) → Result<()>` — publish a version

**AtomStore** — extends AtomSource with local accumulation
(consumption front, lives on the consumer's machine):

- `ingest(dyn AtomSource) → Result<()>` — import from a remote source
- `contains(AtomId) → bool` — check local availability

The critical architectural property: **a store IS a source**. The
`AtomStore` trait extends `AtomSource`, so any component that reads
from an `AtomSource` can read from either a remote source or a local
store without knowing the difference. This is the forgetful functor
from the formal model (§2.6) — downstream layers see only the read
interface.

The data flow composition:

```
ion ──populate──→ AtomStore ──forget──→ AtomSource ──read──→ eos
         ↑                                                      |
    ingest from                                           BuildEngine
    remote sources                                     reads atoms here
```

Ion (or any package manager) populates the store by ingesting from
remote sources. Eos (or any build engine) reads from the store through
the AtomSource interface. The store is transparent — eos does not know
or care whether atoms came from one source or a hundred.

**Accumulation guarantee** (model §2.3, ⊇ condition): after
for every atom in the source, the store's `resolve` MUST return at
least what the source's `resolve` returns. The store accumulates —
it never loses atoms through ingestion.

Each role uses **associated types** to remain backend-agnostic:

- `AtomSource::Entry` — what a resolved atom looks like (backend-defined)
- `AtomSource::Error` — backend-specific error type
- `AtomRegistry::Content` — what the backend needs to identify
  publishable content (e.g., a content hash, an object reference, etc.)

**Trait signature purity**: protocol trait signatures MUST NOT contain
backend-specific types (no git types, no concrete version types, no
serialization framework types). Backend specifics are expressed
exclusively through associated types.

**Session enforcement**: the protocol enforces claim-before-publish
ordering through data flow — `claim()` returns a `Czd` that
`publish()` requires as input. The type system enforces this without
typestate or builder ceremony (model §3.1).

### Surety of Source

The protocol is designed around a **surety of source** principle:
the legitimacy of an atom can always be verified by consulting the
source. The claim transaction lives at the source, and provenance
verification (steps 9–12) traces the atom's content back to the
source revision.

This principle means:

1. An atom's authenticity is not determined by which store holds it,
   but by the cryptographic chain back to its source.
2. A consumer can verify trust without trusting intermediate stores.
3. Forked sources produce competing claims for the same AtomId — the
   consumer selects which source (and therefore which claim) to trust.
4. Backend-internal machinery (ref parsing, caching strategies,
   remote protocols) is NOT protocol surface — it is private to the
   backend implementation.

## Constraints

### Type Declarations

```
TYPE  Alg         = ES256 | ES384 | ES512 | Ed25519              (coz-rs)
TYPE  Czd         = Vec<u8>                                       (coz-rs)
TYPE  Tmb         = Vec<u8>                                       (coz-rs — key thumbprint)

TYPE  Label       = String  { UAX #31 validated }                 (atom-id)
TYPE  RawVersion  = String  { opaque, unparsed }                  (atom-id)

TYPE  AtomId      = { alg: Alg, digest: Vec<u8> }                (atom-id)
  where digest = hash(canonical(anchor || label))
  -- Identity is content-addressed from anchor + label.
  -- NOT derived from any signed message. Permanent.

TYPE  ClaimPayload = {
        alg:    Alg,
        anchor: Vec<u8>,
        label:  Label,
        now:    u64,
        owner:  Vec<u8>,   -- opaque identity digest
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/claim"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).

TYPE  PublishPayload = {
        alg:     Alg,
        anchor:  Vec<u8>,
        claim:   Czd,       -- czd of authorizing claim
        dig:     Vec<u8>,   -- atom snapshot hash (the published artifact)
        label:   Label,
        now:     u64,
        path:    String,    -- subdir in source content tree
        src:     Vec<u8>,   -- source revision hash (provenance)
        tmb:     Tmb,       -- standard Coz: signing key thumbprint
        version: RawVersion,
        typ:     "atom/publish"
      }                                                           (atom-id)
  -- CozMessage MAY include `key` field (convenience for rotated keys).

TYPE  CozMessage = { pay: JSON, sig: Vec<u8>, key?: PubKey }     (coz-rs)

TYPE  Anchor     = Vec<u8>                                        (atom-id)
  -- Opaque digest. Derivation is backend-specific.
  -- See §Anchor for required properties.

TYPE  Manifest   = trait {                                         (atom-core)
        fn label(&self)   -> &Label;
        fn version(&self) -> &RawVersion;
      }

TYPE  VersionScheme = trait {                                      (atom-id)
        type Version: Display + Ord;
        type Requirement;
        fn parse_version(&RawVersion)  -> Result<Version>;
        fn parse_requirement(&str)     -> Result<Requirement>;
        fn matches(&Version, &Requirement) -> bool;
      }
```

### Invariants

**[identity-content-addressed]**: An atom's `AtomId` MUST be derived solely
from `hash(anchor, label)`. The `AtomId` MUST NOT depend on any key,
signature, or signed message. Identity is permanent and content-addressed.
`VERIFIED: unverified`

**[identity-stability]**: An atom's `AtomId` MUST NOT change across
versions, ownership transfers, or key rotations. The `AtomId` is
determined by `anchor` and `label`, neither of which changes.
(Model §1 olog: identity stability diagram.)
`VERIFIED: machine (TLC)`

**[owner-abstract]**: The `owner` field in `ClaimPayload` MUST be an
opaque byte vector representing a cryptographic identity digest. The
protocol MUST NOT impose any interpretation on its contents — it is
an opaque value whose meaning is determined by the identity framework
in use. Known targets include Coz key thumbprints (`tmb`) and Cyphr
Principal Roots (`PR`), but any identity system producing a stable
cryptographic digest MAY be used. The `owner` field is an abstract
identifier, not tied to a specific implementation — any identity
framework MAY be used.
`VERIFIED: machine (Alloy)`

**[owner-compatibility]**: For identity frameworks where a single-key
identity degenerates to a key thumbprint (e.g., Cyphr Level 1:
`PR = tmb`), the `owner` field MAY be used interchangeably with a
raw key thumbprint. Upgrading from a simpler to a richer identity
framework MUST NOT alter the `AtomId`.
`VERIFIED: unverified`

**[symmetric-payloads]**: Both `ClaimPayload` and `PublishPayload`
MUST carry raw `anchor` and `label` fields. A consumer MUST be able
to re-derive the `AtomId` from either payload independently.
`VERIFIED: unverified`

**[publish-chains-claim]**: The `claim` field in `PublishPayload` MUST
contain the `czd` of a valid claim for the same `(anchor, label)`.
This creates the cryptographic chain from publish back to claim.
`VERIFIED: machine (TLC)`

**[claim-typ]**: The `typ` field of a `ClaimPayload` MUST be the
literal string `"atom/claim"`. The protocol is the authority.
`VERIFIED: unverified`

**[publish-typ]**: The `typ` field of a `PublishPayload` MUST be the
literal string `"atom/publish"`.
`VERIFIED: unverified`

**[sig-over-pay]**: All Coz messages MUST follow Coz v1.0: the
signature (`sig`) is computed over the canonical digest (`cad`) of
the raw `pay` bytes. Payload field ordering MUST be preserved
exactly as constructed.
`VERIFIED: unverified`

**[dig-is-atom-snapshot]**: The `dig` field in `PublishPayload` MUST
be the content-addressed hash of the atom snapshot — the
reproducible, detached artifact produced by the publisher. The atom
snapshot uses deterministic metadata (e.g., constant timestamps and
authorship) to ensure reproducibility across backends.
`VERIFIED: unverified`

**[src-is-source-revision]**: The `src` field in `PublishPayload`
MUST be the content-addressed hash of the source revision from
which the atom was extracted.
`VERIFIED: unverified`

**[path-is-subdir]**: The `path` field in `PublishPayload` MUST be
the subdirectory path within the source content tree where the
atom's content resides. This MUST be the exact path needed to
navigate from the source revision's root to the atom's subtree.
`VERIFIED: unverified`

**[rawversion-opaque]**: `RawVersion` MUST be treated as an opaque
string by the protocol layer. Semantic interpretation MUST be
deferred to a `VersionScheme` implementor.
`VERIFIED: unverified`

**[claim-key-required]**: A claim `CozMessage` MUST include a `key`
field containing the public key used for signing. This enables
trust-on-first-use (TOFU) verification without external key
discovery.
`VERIFIED: unverified`

**[publish-key-optional]**: A publish `CozMessage` MAY include a
`key` field. It SHOULD be included when the signing key differs
from the claim's key (e.g., after key rotation). It
MAY be omitted when the same key signed both claim and publish.
`VERIFIED: unverified`

**[anchor-immutable]**: An anchor MUST NOT change over the lifetime
of its atom-set. Once established, the anchor is permanent.
`VERIFIED: unverified`

**[anchor-content-addressed]**: An anchor MUST be a cryptographic
digest derived from content. It MUST NOT be derived from mutable
metadata.
`VERIFIED: unverified`

**[anchor-discoverable]**: Given access to a source, any party
MUST be able to independently derive the anchor without trusting
the publisher.
`VERIFIED: unverified`

**[manifest-minimal]**: The `Manifest` trait MUST require exactly
`label` and `version`. All other metadata is ecosystem-specific
and MUST NOT be required by the protocol.
`VERIFIED: machine (Alloy)`

**[backend-bit-perfect]**: A backend MUST NOT alter the content
of stored `CozMessage`s. Signed messages are immutable binary
blobs (cf. Coz bit-perfect preservation).
`VERIFIED: unverified`

**[atomid-per-source-unique]**: Within a single source, an `AtomId`
MUST be unique — no two atoms in the same source MAY share the same
label. `AtomId = hash(anchor, label)` guarantees this by construction.
This prevents ambiguous references within a source.
`VERIFIED: machine (TLC)`

**[publish-claim-coherence]**: A publish's `claim` field MUST reference
the `czd` of a valid claim whose `(anchor, label)` matches the
publish's `(anchor, label)`. Multiple claims for the same `AtomId` by
different owners MAY coexist (e.g., forks sharing the same anchor).
The publish→claim chain is the sole mechanism for binding a publish to
a specific claim. Publishes from different claims MUST NOT
cross-pollinate — a consumer selects which claim to trust based on
the `owner`.
`VERIFIED: machine (TLC)`

**[atom-detached]**: An atom MUST be a self-contained, detached
subtree. It MUST NOT carry source history. Provenance is recorded
in the publish payload (`src`, `path`) and verified separately.
`VERIFIED: unverified`

**[uri-not-metadata]**: Atom URIs MUST NOT appear in signed
transaction payloads, persisted protocol state, or any metadata
that participates in cryptographic operations. URIs are a user
convenience — all persistent references MUST use canonical forms.
`VERIFIED: unverified`

**[trait-signature-pure]**: Protocol trait signatures (`AtomSource`,
`AtomRegistry`, `AtomStore`) MUST NOT contain backend-specific types.
Backend specifics MUST be expressed exclusively through associated
types on the traits.
`VERIFIED: unverified`

**[crypto-layer-separation]**: Within the atom crate hierarchy,
cryptographic operations (hashing, signing, verification) MUST be
owned by atom-id. atom-core MUST NOT have any direct dependency
on cryptographic libraries — all crypto MUST flow through atom-id.
`VERIFIED: unverified`

**[crypto-via-coz]**: All cryptographic operations MUST conform to
the Coz specification semantics. The atom protocol relies on Coz
for signing, verification, digest computation, and key thumbprint
derivation.
`VERIFIED: unverified`

**[key-management-deferred]**: The atom protocol MUST NOT define
mechanisms for public key storage, discovery, or trust
establishment. These concerns are deferred to the identity
framework in use (e.g., Cyphr). The atom
verification function MUST accept raw public key bytes as a
parameter.
`VERIFIED: unverified`

### Transitions

**[claim-transition]**: An atom MAY be claimed by constructing a
`ClaimPayload`, signing it with a Coz-compatible key, and producing
a `CozMessage` that includes the public key.

- **PRE**: `anchor` MUST be a valid anchor for the atom-set.
  `label` MUST pass UAX #31 validation. The signing key MUST be
  valid for the specified `alg`. The `CozMessage` MUST include a
  `key` field with the signing public key.
- **POST**: The claim message (including `key`) MUST be stored in
  a backend-specific location retrievable by `AtomId`.
  `VERIFIED: unverified`

**[publish-transition]**: A version MAY be published for a claimed
atom by constructing a `PublishPayload`, signing it, and producing
a `CozMessage`.

- **PRE**: `claim` MUST be the `czd` of a valid, non-revoked claim
  for this `(anchor, label)`. `version` MUST be a non-empty string.
  `dig` MUST be the hash of the reproducible atom snapshot. `src`
  MUST be the hash of the source revision. `path` MUST be the
  subdir path. `now` MUST be greater than `claim.now`. The signing
  key MUST be authorized by the claim's `owner`.
- **POST**: The publish transaction is stored associating
  `(AtomId, version)` with the signed `CozMessage`.
  `VERIFIED: unverified`

**[session-ordering]**: Claim MUST precede publish (model §3.1).
Enforced by:
(a) data flow — `publish` requires `claim` czd, which can only be
obtained from a completed claim; (b) temporal ordering —
`publish.now > claim.now` MUST hold.
`VERIFIED: machine (TLC)`

### Forbidden States

**[no-unclaimed-publish]**: A publish transaction MUST NOT exist for
an `AtomId` that has no corresponding claim. If a backend discovers
a publish without a verifiable claim, it MUST treat the publish as
invalid.
`VERIFIED: machine (TLC)`

**[no-duplicate-version]**: For a given `AtomId`, a backend MUST NOT
store two publish transactions with the same `version` string.
Republishing the same version MUST be rejected. Version
immutability — once published, a version is sealed.
`VERIFIED: machine (TLC)`

**[no-cross-layer-crypto]**: atom-core MUST NOT import, depend on,
or transitively require any cryptographic crate. All crypto flows
through atom-id via Coz.
`VERIFIED: unverified`

**[no-backdated-publish]**: A publish with `now` less than or equal
to the `now` of the referenced claim MUST be rejected. Publishes
MUST be temporally ordered after their authorizing claim.
`VERIFIED: unverified`

### Behavioral Properties

**[verification-local]**: Given an atom snapshot, its publish
`CozMessage`, and the corresponding claim `CozMessage`, a consumer
MUST be able to verify artifact integrity, signature validity,
claim chain, temporal ordering, and identity derivation using
only local computation — zero network round-trips.

- **Type**: Safety
  `VERIFIED: unverified`

**[verification-provenance]**: Given the additional ability to fetch
individual source objects (revision metadata and content tree
structure, without full content), a consumer MUST be able to verify
that the atom's content tree is contained within the source content
tree at the claimed `path` from the revision at `src`. This deeper
verification MUST NOT require fetching full file content or the
complete source history.

- **Type**: Safety
  `VERIFIED: unverified`

**[atom-snapshot-reproducible]**: The atom snapshot at `dig` MUST be
reproducible: given the same source content at `src`/`path` and
the same deterministic metadata, any party MUST be able to
independently construct the atom snapshot and produce the same
hash.

- **Type**: Safety
  `VERIFIED: unverified`

**[ingest-preserves-identity]**: When an atom is ingested from one
`AtomSource` into an `AtomStore`, its `AtomId` MUST be preserved.
(Model §2.3 — ingest ⊇ condition.)

- **Type**: Safety
  `VERIFIED: unverified`

**[backend-agnostic-protocol]**: All protocol-level types (`AtomId`,
`ClaimPayload`, `PublishPayload`, `RawVersion`) MUST be
backend-agnostic. Backend-specific concerns are expressed through
associated types on traits.

- **Type**: Safety
  `VERIFIED: unverified`

## Verification Pipeline

The following defines the normative verification steps for consumers.

### Local Verification (zero network)

A consumer who has the atom snapshot, publish transaction, and claim
transaction MUST be able to perform all of the following locally:

| Step | Check                            | Field(s)                                  |
| :--- | :------------------------------- | :---------------------------------------- |
| 1    | Atom snapshot hash matches `dig` | `publish.dig`                             |
| 2    | Claim signature valid            | `claim.pay`, `claim.sig`, `claim.key`     |
| 3    | Publish signature valid          | `publish.pay`, `publish.sig`, key         |
| 4    | Key thumbprint matches           | `tmb(claim.key) == claim.pay.tmb`         |
| 5    | Publish chains to claim          | `publish.claim == czd(claim)`             |
| 6    | Temporal ordering                | `publish.now > claim.now`                 |
| 7    | Signer authorized by owner       | `publish.tmb` authorized by `claim.owner` |
| 8    | AtomId derivable                 | `hash(anchor, label) == expected`         |

### Provenance Verification (minimal network)

A consumer MAY additionally verify content provenance. The cost is
backend-dependent but MUST be achievable without fetching full file
content or the complete source history:

| Step | Check                                       | Requirement                 |
| :--- | :------------------------------------------ | :-------------------------- |
| 9    | Fetch source revision metadata at `src`     | Revision metadata only      |
| 10   | Walk source content tree → `path` → subtree | Content tree structure only |
| 11   | Subtree hash equals atom content tree hash  | Local comparison            |
| 12   | Reconstruct atom snapshot → matches `dig`   | Local computation           |

## Verification

**TLA+ model**: `docs/specs/tla/AtomTransactions.tla` verified by TLC
across two configurations (fork scenario: 31,593 states; distinct-anchor:
27,817 states). All safety-critical temporal invariants pass.

**Alloy model**: `docs/specs/alloy/atom_structure.als` verified by Alloy
Analyzer 6.2.0 at scope 4. All 5 structural assertions pass (UNSAT).
Fork scenario confirmed satisfiable (SAT).

**Verification methods:**

- `machine (TLC)` / `machine (Alloy)` — formal model checker, already verified
- `rustc` — Rust type system; if code compiles, constraint holds
- `cargo-dep` — Cargo.toml dependency audit; verified by `cargo check`
- `unit-test` — deterministic test in isolation
- `integration-test` — end-to-end test requiring git backend

**Coverage:** 13 formal (TLC/Alloy), 11 rustc, 4 cargo-dep, 5 unit-test, 8 integration-test = **41 total, 0 agent-check**.

| Constraint                 | Method           | Result   | Detail                                   | Phase |
| :------------------------- | :--------------- | :------- | :--------------------------------------- | :---- |
| identity-content-addressed | machine (Alloy)  | **pass** | Alloy `identity_content_addressed`       | —     |
| identity-stability         | machine (TLC)    | **pass** | TLA+ `IdentityStability` — 2 configs     | —     |
| owner-abstract             | machine (Alloy)  | **pass** | Alloy `ownership_independence`           | —     |
| owner-compatibility        | machine (Alloy)  | **pass** | Alloy `ownership_independence`           | —     |
| symmetric-payloads         | rustc            | pending  | Both structs have `anchor` + `label`     | 1     |
| publish-chains-claim       | machine (TLC)    | **pass** | TLA+ `PublishChainsClaim` — 2 configs    | —     |
| claim-typ                  | rustc            | pending  | `TYP_CLAIM` const = `"atom/claim"`       | 1     |
| publish-typ                | rustc            | pending  | `TYP_PUBLISH` const = `"atom/publish"`   | 1     |
| sig-over-pay               | unit-test        | pending  | Coz round-trip: sign → verify payload    | 1     |
| dig-is-atom-snapshot       | unit-test        | pending  | Snapshot hash matches `dig` field        | 4     |
| src-is-source-revision     | integration-test | pending  | Git revision hash matches `src` field    | 4     |
| path-is-subdir             | rustc            | pending  | `path` field type constrains to subdir   | 1     |
| rawversion-opaque          | rustc            | pending  | Newtype, no `Deref`/`AsRef`/`Into`       | 1     |
| claim-key-required         | rustc            | pending  | `ClaimPayload` construction requires key | 1     |
| publish-key-optional       | rustc            | pending  | `PublishPayload` key is `Option<_>`      | 1     |
| crypto-layer-separation    | cargo-dep        | pending  | atom-core Cargo.toml has no coz-rs       | 3     |
| crypto-via-coz             | cargo-dep        | pending  | atom-id Cargo.toml depends on coz-rs     | 1     |
| key-management-deferred    | cargo-dep        | pending  | No key storage crate in atom workspace   | 3     |
| claim-transition           | unit-test        | pending  | Coz sign flow produces valid claim       | 1     |
| publish-transition         | unit-test        | pending  | Coz sign flow produces valid publish     | 1     |
| session-ordering           | machine (TLC)    | **pass** | TLA+ `SessionOrdering` — 2 configs       | —     |
| no-unclaimed-publish       | machine (TLC)    | **pass** | TLA+ `NoUnclaimedPublish` — 2 configs    | —     |
| no-duplicate-version       | machine (TLC)    | **pass** | TLA+ `NoDuplicateVersion` — 2 configs    | —     |
| no-cross-layer-crypto      | cargo-dep        | pending  | atom-core has zero crypto deps           | 3     |
| no-backdated-publish       | machine (TLC)    | **pass** | TLA+ `NoBackdatedPublish` — 2 configs    | —     |
| verification-local         | integration-test | pending  | Pipeline steps 1–8 offline               | 4     |
| verification-provenance    | integration-test | pending  | Pipeline steps 9–12 with source access   | 4     |
| atom-snapshot-reproducible | unit-test        | pending  | Same inputs → same snapshot hash         | 4     |
| ingest-preserves-identity  | machine (Alloy)  | **pass** | Alloy `ingest_preserves_identity`        | —     |
| backend-agnostic-protocol  | rustc            | pending  | Trait sigs use only associated types     | 3     |
| anchor-immutable           | integration-test | pending  | Anchor unchanged across operations       | 4     |
| anchor-content-addressed   | integration-test | pending  | Anchor = hash(genesis content)           | 4     |
| anchor-discoverable        | integration-test | pending  | Anchor derivable from source alone       | 4     |
| manifest-minimal           | machine (Alloy)  | **pass** | Alloy `manifest_properties` fact         | —     |
| backend-bit-perfect        | integration-test | pending  | CozMessage bytes unchanged after store   | 4     |
| atomid-per-source-unique   | machine (TLC)    | **pass** | TLA+ `AtomIdPerSourceUnique` — 2 configs | —     |
| publish-claim-coherence    | machine (TLC)    | **pass** | TLA+ `PublishClaimCoherence` — 2 configs | —     |
| atom-detached              | integration-test | pending  | Atom subtree has no parent refs          | 4     |
| uri-not-metadata           | rustc            | pending  | URI type absent from payload structs     | 1     |
| trait-signature-pure       | rustc            | pending  | No backend types in trait signatures     | 3     |

## Implications

### Scope Boundaries

This specification explicitly does NOT define:

- **Manifest schemas**: `Cargo.toml`, `package.json`, `recipe.ion`, etc.
  are ecosystem concerns.
- **Dependency resolution**: algorithms for resolving version constraints.
- **Build integration**: how atoms are consumed by build systems.
- **Network transport**: HTTP, SSH, native protocols — implementation details.
- **Key/identity management**: deferred to Cyphr.
- **Anchor derivation**: backend-specific (properties constrained, not
  mechanism).
