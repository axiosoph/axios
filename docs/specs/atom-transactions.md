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
pins the atom-set to an owned, signed declaration over the source's history.

**(Amended 2026-07-08 — the charter.)** The anchor is the coz digest of the
set's **founding charter transaction**: `Anchor := czd(charter₀)`. This
replaces the earlier backend-specific derivation (git: genesis-commit hash),
and makes anchor derivation **backend-agnostic**: the charter is a coz
object regardless of backend; only the interpretation of its `src` field is
backend-specific. The genesis commit is not lost — a revision hash commits
to its entire ancestry, so `charter.src` transitively pins the genesis; it
simply stops being the identity.

An anchor MUST satisfy the following properties:

1. **Immutable**: The anchor is fixed at chartering and persists for the
   lifetime of the atom-set. Successor charters (key rotation, ownership
   succession) chain to the founding charter and MUST NOT change the
   anchor (`[charter-succession]`).
2. **Content-addressed and owned**: The anchor is a coz digest of a
   _signed_ payload — the trust chain roots at an owned object, not at
   unowned repository metadata.
3. **Unique**: Two distinct charters produce distinct anchors (collision
   resistance of the coz digest; distinct `owner`/`src`/`now` inputs).
   Two charters over the _same_ source history are two deliberately
   distinct atom-sets — this is the fork-distinction property
   (`[charter-fork-distinction]`), not a defect.
4. **Resolvable**: Given a source and an anchor, a consumer MUST be able
   to locate the charter (stored in the source's atom refs like any
   transaction) and verify it against the anchor without trusting the
   publisher. Given a source alone, a consumer MUST be able to enumerate
   candidate charters; selecting among them is the consumer's trust
   decision (the anchor in a lock or URI is that decision, recorded).

**Note on git hash agility** (`[anchor-hash-agile]`): a SHA-256 re-hash of
a SHA-1 repository rewrites history, so a prior charter's `src` does not
exist in the re-hashed repository. Continuity across a re-hash is an
explicit act: a successor charter in the new history, chaining to the
founding charter (`[charter-succession]`). Absent succession, the
re-hashed repository is a distinct atom-set — now by explicit rule rather
than silent consequence.

The anchor feeds into identity: `AtomId = (anchor, label)`. Because
the anchor is immutable and the label is fixed per atom, the AtomId is
permanent. The `AtomId` is the abstract pair — no identity-layer digest
is derived from it. Algorithm agility for content addressing lives only
in the Coz `czd`.

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

- `resolve(AtomId) → Result<Option<Self::Entry>, Self::Error>` — look up an atom.
  `Ok(None)` means the atom is not present; `Err` means the backend
  failed (network, disk, permission, etc.).
- `discover(Query) → Result<Vec<AtomId>, Self::Error>` — search for atoms

**AtomRegistry** — extends AtomSource with write operations
(publishing front, lives at the source):

- `claim(ClaimReq) → Result<Czd>` — establish ownership
- `publish(PublishReq) → Result<()>` — publish a version

**AtomStore** — extends AtomSource with local accumulation
(consumption front, lives on the consumer's machine):

- `ingest(dyn AtomSource) → Result<()>` — import from a remote source
- `contains(AtomId) → bool` — check local availability

**[trait-async-io]**: The `AtomSource` and `AtomStore` traits MUST use async methods for operations that MAY involve I/O. Specifically:

- `AtomSource::resolve()` — MUST be async. Implementations MAY need to fetch from remote registries.
- `AtomSource::discover()` — MUST be async. Remote search queries involve network I/O.
- `AtomStore::ingest()` — MUST be async. Ingestion from remote sources involves network transfers.
- `AtomStore::contains()` — MUST be async. Remote store existence checks involve network I/O.

Local implementations (e.g., git-backed stores on the same filesystem) MAY complete these async methods synchronously (no `.await` points). The async boundary exists for consumers that need it (registry sources, peer sources), not to mandate concurrency in all implementations.

Data accessor traits (`AtomEntry`, `AtomVersion`, `Manifest`) MUST remain synchronous — they operate on in-memory data structures with no I/O.

`AtomRegistry::claim()` and `AtomRegistry::publish()` MAY remain synchronous in v1 (registries are local git repos). If remote registry write operations are added in vN, these SHOULD be made async.

`VERIFIED: unverified`

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

**Composite Sources**: An `AtomSource` implementation MAY compose multiple underlying sources into a single interface with priority-based resolution. A composite source tries each underlying source in priority order and returns the first successful resolution.

The canonical composition for an eos daemon:

```
CompositeAtomSource
  Priority 1: LocalGitStore    — cached atoms (instant)
  Priority 2: RegistrySource(s) — remote mirrors (async fetch + ingest)
  Priority 3: PeerSource        — client's AtomStore as AtomSource (store-to-store transfer)
```

This composition is transparent: eos calls `resolve()` on the composite and does not know which underlying source provided the result. The composite ingests fetched atoms into the local store (Priority 1) so that subsequent resolutions for the same atom are cache hits.

Composite sources MUST preserve the accumulation guarantee: after resolving an atom from any priority level, the local store MUST contain that atom for future lookups.

**[composite-source-concurrent]**: A composite source SHOULD resolve multiple atoms concurrently. When a `BuildRequest` contains N atom dependencies, the composite source SHOULD spawn N concurrent resolution tasks (bounded by a configurable concurrency limit). This is the primary performance justification for the async trait requirement (`[trait-async-io]`).
`VERIFIED: unverified`

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
TYPE  Cad         = Vec<u8>                                       (coz-rs — canonical digest)
TYPE  Czd         = Vec<u8>                                       (coz-rs — coz digest)
TYPE  Tmb         = Vec<u8>                                       (coz-rs — key thumbprint)

TYPE  Label       = String  { UAX #31 validated }                 (atom-id)
TYPE  RawVersion  = String  { opaque, unparsed }                  (atom-id)

TYPE  Anchor      = Vec<u8>                                        (atom-id)
  -- Opaque digest. Derivation is backend-specific.
  -- See §Anchor for required properties.

TYPE  AtomId      = { anchor: Anchor, label: Label }              (atom-id)
  -- Protocol-level identity. Determined solely by the source's
  -- anchor and the atom's label. Algorithm-free, permanent.
  -- Two atoms with the same (anchor, label) ARE the same atom.
  -- NOT a hash — this is the abstract identity pair.

TYPE  CharterPayload = {
        alg:    Alg,
        now:    u64,
        owner:  Vec<u8>,   -- opaque identity digest (same abstraction as claims)
        prior:  Czd?,      -- OPTIONAL: czd of the charter this one succeeds
        src:    Vec<u8>,   -- source revision demarking the chartering point
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/charter"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- The founding charter (no `prior`) DEFINES the set: Anchor :=
  -- czd(charter₀). A successor charter (with `prior`) MUST be
  -- authorized by the owner of the charter it succeeds; succession
  -- preserves the anchor. `src` demarks the chartering point in
  -- history — everything before it is unowned by this set unless
  -- claimed after it ("orphaned unless re-claimed").

TYPE  ClaimPayload = {
        alg:    Alg,
        anchor: Anchor,
        label:  Label,
        now:    u64,
        owner:  Vec<u8>,   -- opaque identity digest
        pkg:    String,    -- PURL type (e.g., "cargo", "npm", "pypi")
        src:    Vec<u8>,   -- source revision hash at claim time (temporal floor)
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/claim"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- The `anchor` field IS the chain link to the charter: anchor ==
  -- czd(charter₀). No separate charter field exists or is needed —
  -- exactly as publish chains to claim by `claim: Czd`, claim chains
  -- to charter by `anchor`.
  -- The `src` field cryptographically binds the claim to its temporal
  -- position in history via the signed payload.
  -- The `pkg` field identifies the ecosystem. Implementations SHOULD
  -- use PURL type strings (https://github.com/package-url/purl-spec)
  -- where a matching type exists (e.g., "cargo", "npm", "pypi") for
  -- interoperability with SBOM and supply-chain tooling. Custom type
  -- strings MAY be used for ecosystems not yet in the PURL registry.
  -- Manifest discovery is implicit — the ecosystem adapter infers it
  -- from `pkg` (e.g., "cargo" → Cargo.toml at tree root). The
  -- protocol does not prescribe manifest format or location.

TYPE  PublishPayload = {
        alg:     Alg,
        anchor:  Anchor,
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

**[charter-typ]**: A charter transaction's payload MUST carry
`typ: "atom/charter"`.
`VERIFIED: unverified (pending implementation)`

**[charter-anchor]**: The atom-set's anchor MUST equal the coz digest of
the founding charter: `Anchor == czd(charter₀)`, where the founding
charter is the unique charter in the succession chain carrying no
`prior` field.
`VERIFIED: unverified (models require extension — see Verification note)`

**[claim-chains-charter]**: Every claim's `anchor` field MUST equal the
czd of a verifiable charter (founding, or reachable from the founding
charter via `[charter-succession]`). This is the claim-level analogue of
`[publish-chains-claim]`: charter : claim :: claim : publish.
`VERIFIED: unverified (models require extension)`

**[claim-charter-authorization]**: A claim's signing key MUST be
authorized by the effective charter's `owner`, under the same delegated
semantics as `[owner-authorization-delegated]`. (The effective charter is
the latest valid charter in the succession chain at claim time.) This
replaces unscoped first-come label TOFU with set-governed claiming; open
or delegated claiming is expressible through the owner abstraction's
identity frameworks, not through protocol exceptions.
`VERIFIED: unverified (models require extension)`

**[claim-replacement-authority]**: A claim MAY be replaced by a new
claim carrying `prior: czd(replaced claim)`, under exactly two
authorities, distinguishable by every consumer:

- **owner replacement** — signing key authorized by the replaced
  claim's `owner` (key rotation, identity-framework upgrade): the
  ordinary path, no special marking.
- **governance replacement** — signing key authorized by the effective
  charter's `owner` but NOT by the replaced claim's owner: the
  replacement payload MUST carry `governance: true`. A governance
  replacement is a first-class, visible seizure event; consumers' trust
  policies MUST be able to distinguish it and MAY refuse, warn, or pin
  the prior owner. Silent seizure is structurally unexpressible.

Publishes chained to a replaced claim remain verifiable history;
new publishes MUST chain to the current claim.
`VERIFIED: unverified (models require extension)`

**[charter-ancestry]**: A claim's `src` MUST be a descendant of (or equal
to) the effective charter's `src`. Together with the existing
claim→publish ancestry, the temporal floor becomes
`charter.src ⟶ claim.src ⟶ publish.src`, rooted at a signed object.
History prior to `charter.src` is visible but unowned by the set —
**orphaned unless re-claimed** after the chartering point.
`VERIFIED: unverified (models require extension)`

**[charter-succession]**: A successor charter (carrying `prior`) MUST be
signed by a key authorized by the owner of the charter named in `prior`,
and MUST NOT alter the anchor: the anchor remains `czd(charter₀)` for
the lifetime of the set. Succession is how key rotation and ownership
transfer occur without identity change (preserving
`[identity-stability]`). Orphaning is keyed to ANCHOR change and
therefore occurs only on fork: succession preserves the anchor, so no
claim or publish is orphaned by rotation or transfer. Note further that
merely _adding_ a key usually requires no charter at all — hierarchical
and rooted identity frameworks (`[owner-abstract]`) authorize new keys
under an unchanged owner digest; succession charters are needed only
when the owner identity itself changes.
`VERIFIED: unverified (models require extension)`

**[charter-fork-distinction]**: A charter with no valid succession chain
from another set's founding charter defines a **distinct atom-set**,
regardless of shared source history. Forks are therefore explicit by
construction: a fork cannot share the origin's anchor (it cannot forge
succession), and cross-fork `(anchor, label)` collision is structurally
impossible.
`VERIFIED: unverified (models require extension)`

**[identity-content-addressed]**: An atom's identity (`AtomId`) MUST be
determined solely by the pair `(anchor, label)`. The `AtomId` MUST NOT
depend on any key, signature, signed message, or hash algorithm. Identity
is permanent and content-addressed. The `AtomId` is the abstract pair —
not a hash of it; there is no identity-layer digest type.
`VERIFIED: machine (Alloy)`

**[identity-stability]**: An atom's `AtomId` MUST NOT change across
versions, ownership transfers, or key rotations. The `AtomId` is
determined by `anchor` and `label`, neither of which changes.
(Model §1 olog: identity stability diagram.)
`VERIFIED: machine (TLC)`

**[owner-abstract]**: The `owner` field in `ClaimPayload` MUST be an
opaque byte vector representing a cryptographic identity digest. The
protocol MUST NOT impose any interpretation on its contents — it is
an opaque value whose meaning is determined by the identity framework
in use. Any identity system producing a stable cryptographic digest
MAY be used.

Different identity frameworks offer different **capabilities** along
the owner abstraction:

- **Single-key** (e.g., raw Coz `tmb`): owner = key thumbprint.
  Compromise requires reclaiming all atoms.
- **Hierarchical keys** (e.g., OpenPGP master + subkeys): owner =
  master key fingerprint. Subkeys are authorized via binding
  signatures from the master key. Subkeys can be rotated; compromise
  of a subkey is local, not catastrophic.
- **Rooted identity** (e.g., Cyphr Principal Root): owner = PR
  digest. Supports key rotation, delegation, and sub-identities
  natively. PR identity survives key transitions.

The protocol is agnostic to which tier is in use. The `owner` value
is stable across key rotations, upgrades, and delegation — only the
authorization semantics of "signing key authorized by owner" vary.
`VERIFIED: machine (Alloy)`

**[owner-authorization-delegated]**: The meaning of "signing key
MUST be authorized by the claim's `owner`" (as required by
`[publish-transition]`) is **delegated to the identity framework**.
The protocol defines the requirement but not the mechanism:

- Single-key: authorized iff `publish.tmb == claim.owner`
- Hierarchical: authorized iff the signing subkey has a valid binding
  signature from the master key whose fingerprint matches `claim.owner`
- Rooted identity: authorized iff the signing key is derivable from
  the Principal Root whose digest matches `claim.owner`

This delegation is intentional — it allows the protocol to benefit
from richer identity frameworks without coupling to any specific one.
`VERIFIED: unverified`

**[owner-compatibility]**: Upgrading from a simpler to a richer
identity framework (e.g., raw key → GPG master → Cyphr PR)
MUST NOT alter the `AtomId`. The `AtomId` is derived from
`(anchor, label)`, which is independent of the owner. A claim
replacement (`[claim-replacement-transition]`) MAY update the
`owner` to a new identity system without changing the atom's
identity.
`VERIFIED: unverified`

**[symmetric-payloads]**: Both `ClaimPayload` and `PublishPayload`
MUST carry raw `anchor` and `label` fields. A consumer MUST be able
to reconstruct the `AtomId` from either payload independently by
extracting `(anchor, label)`.
`VERIFIED: rustc (atom-id: both payloads carry anchor + label)`

**[publish-chains-claim]**: The `claim` field in `PublishPayload` MUST
contain the `czd` of a valid claim for the same `(anchor, label)`.
This creates the cryptographic chain from publish back to claim.
`VERIFIED: machine (TLC)`

**[claim-typ]**: The `typ` field of a `ClaimPayload` MUST be the
literal string `"atom/claim"`. The protocol is the authority.
`VERIFIED: rustc (TYP_CLAIM const, verify_claim checks typ)`

**[publish-typ]**: The `typ` field of a `PublishPayload` MUST be the
literal string `"atom/publish"`.
`VERIFIED: rustc (TYP_PUBLISH const, verify_publish checks typ)`

**[sig-over-pay]**: All Coz messages MUST follow Coz v1.0: the
signature (`sig`) is computed over the canonical digest (`cad`) of
the raw `pay` bytes. Payload field ordering MUST be preserved
exactly as constructed.
`VERIFIED: unit-test (verify_claim_roundtrip, verify_publish_roundtrip)`

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
`VERIFIED: rustc (path: String field in PublishPayload)`

**[rawversion-opaque]**: `RawVersion` MUST be treated as an opaque
string by the protocol layer. Semantic interpretation MUST be
deferred to a `VersionScheme` implementor.
`VERIFIED: rustc (RawVersion newtype, no Deref/AsRef/Into)`

**[claim-key-required]**: A claim `CozMessage` MUST include a `key`
field containing the public key used for signing. This enables
trust-on-first-use (TOFU) verification without external key
discovery.
`VERIFIED: unit-test (claim roundtrip uses key in CozMessage)`

**[publish-key-optional]**: A publish `CozMessage` MAY include a
`key` field. It SHOULD be included when the signing key differs
from the claim's key (e.g., after key rotation). It
MAY be omitted when the same key signed both claim and publish.
`VERIFIED: unit-test (publish roundtrip works with/without key)`

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
label. All atoms in a source share the same anchor, so label uniqueness
within a source directly implies `(anchor, label)` pair uniqueness.
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
`VERIFIED: rustc (no URI type in payload structs)`

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
`VERIFIED: cargo-dep (atom-id depends on coz-rs = "0.4")`

**[key-management-deferred]**: The atom protocol MUST NOT define
mechanisms for public key storage, discovery, or trust
establishment. These concerns are deferred to the identity
framework in use (e.g., Cyphr). The atom
verification function MUST accept raw public key bytes as a
parameter.
`VERIFIED: unit-test (verify functions take pub_key: &[u8])`

### Transitions

**[charter-transition]**: An atom-set MAY be chartered by constructing
a `CharterPayload`, signing it, and producing a `CozMessage` that
includes the public key.

- **PRE** (founding): no `prior` field; `src` MUST be a revision that
  exists in the source. The founding charter's czd becomes the
  atom-set's anchor.
- **PRE** (successor): `prior` MUST be the czd of a valid charter in
  this set's succession chain; the signing key MUST be authorized by
  that charter's `owner`; `now` MUST exceed the prior charter's `now`.
- **POST**: The charter message is stored in the source's atom refs,
  enumerable by consumers and retrievable by its czd.
  `VERIFIED: unverified (pending implementation)`

**[claim-transition]**: An atom MAY be claimed by constructing a
`ClaimPayload`, signing it with a Coz-compatible key, and producing
a `CozMessage` that includes the public key.

- **PRE**: `anchor` MUST equal the czd of the set's founding charter,
  with a verifiable succession chain to the effective charter; the
  signing key MUST be authorized by the effective charter's `owner`
  (`[claim-charter-authorization]`); `src` MUST descend from the
  effective charter's `src` (`[charter-ancestry]`).
  `label` MUST pass UAX #31 validation. The signing key MUST be
  valid for the specified `alg`. The `CozMessage` MUST include a
  `key` field with the signing public key.
- **POST**: The claim message (including `key`) MUST be stored in
  a backend-specific location retrievable by `AtomId`.
  `VERIFIED: unit-test (verify_claim_roundtrip)`

**[publish-transition]**: A version MAY be published for a claimed
atom by constructing a `PublishPayload`, signing it, and producing
a `CozMessage`.

**[claim-replacement-transition]**: A claim MAY be replaced per
`[claim-replacement-authority]` — owner replacement unmarked,
governance replacement marked `governance: true` — the replacement
carrying `prior: czd(replaced claim)`. (This defines the transition
previously referenced by `[owner-compatibility]` but never specified.)

- **PRE**: authority per `[claim-replacement-authority]`; `now` MUST
  exceed the replaced claim's `now`; `(anchor, label)` MUST be
  unchanged (replacement never alters identity).
- **POST**: the replacement is stored alongside the replaced claim;
  both remain retrievable (history is never erased).
  `VERIFIED: unverified (pending implementation)`

- **PRE**: `claim` MUST be the `czd` of a valid, non-revoked claim
  for this `(anchor, label)`. `version` MUST be a non-empty string.
  `dig` MUST be the hash of the reproducible atom snapshot. `src`
  MUST be the hash of the source revision. `path` MUST be the
  subdir path. `now` MUST be greater than `claim.now`. The signing
  key MUST be authorized by the claim's `owner`.
- **POST**: The publish transaction is stored associating
  `(AtomId, version)` with the signed `CozMessage`.
  `VERIFIED: unit-test (verify_publish_roundtrip)`

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

**[publish-payload-extensible]**: The publish `CozMessage` payload
MAY contain additional user-defined fields beyond the required set
(`anchor`, `label`, `claim`, `dig`, `src`, `path`, `version`). For
example, a reproducible-build artifact hash MAY be included to
cryptographically tie the final build artifact to the source.
Additional fields are signed as part of the `CozMessage` and
therefore carry cryptographic assurance. Backends MUST preserve
all payload fields, including unknown ones, when storing and
retrieving publish transactions.

**Root-level JSON keys are strictly reserved for current and future
protocol fields.** All ecosystem-specific or user-defined extensions
MUST be nested inside a dedicated `"meta"` object in the payload.
This prevents forward-compatibility collisions if a future protocol
version introduces new required fields.

- **Type**: Safety
  `VERIFIED: unverified`

**[claim-payload-extensible]**: A claim `CozMessage` payload MAY
contain additional user-defined fields beyond the required set
(`alg`, `anchor`, `label`, `now`, `owner`, `pkg`, `src`, `tmb`).
Like publish payloads, all ecosystem-specific or user-defined
extensions MUST be nested inside a dedicated `"meta"` object.
Additional fields are signed as part of the `CozMessage` and
therefore carry cryptographic assurance. Backends MUST preserve
all payload fields, including unknown ones, when storing and
retrieving claim transactions.

Claim `meta` is particularly important for **claim chain
transitions** — when a new claim replaces a previous one. The
`meta` fields on the new claim communicate the intent and severity
of the transition to consumers who may hold the old claim.

- **Type**: Safety
  `VERIFIED: unverified`

**[fs-source-contract]**: An `AtomSource` implementation MAY exist
for filesystem directories (paths without git history). Such a source:

- MUST support `discover` (scan for manifests) and `resolve` (read
  atom metadata)
- MUST NOT support `claim` or `publish` (no VCS history means no
  signed transactions)
- MUST be ingestible into an `AtomStore` for consumption
- MUST use a well-known constant sentinel value as its anchor, so
  that the `AtomId` (the pair `(anchor, label)`) is reconstructible
  for all atoms. The sentinel anchor distinguishes filesystem-sourced
  atoms from git-sourced atoms and prevents them from being confused
  with published atoms. **Note**: the exact byte encoding of the
  `FsSource` sentinel anchor is a protocol-level constant that MUST
  be specified; its value is currently unspecified (SAD §9, gap 2).

This enables local development workflows where atoms are evaluated
from the working tree without requiring publication. The `FsSource`
implementation SHOULD reside in atom-core as a default degradation
target, allowing any storage backend to serve as the `AtomStore` for
ingested filesystem atoms.

- **Type**: Safety
  `VERIFIED: unverified`

## Verification Pipeline

The following defines the normative verification steps for consumers.

### Local Verification (zero network)

A consumer who has the atom snapshot, publish transaction, claim
transaction, and charter chain MUST be able to perform all of the
following locally:

| Step | Check                            | Field(s)                                                             |
| :--- | :------------------------------- | :------------------------------------------------------------------- |
| 1    | Atom snapshot hash matches `dig` | `publish.dig`                                                        |
| 2    | Charter signature(s) valid       | `charter.pay`, `charter.sig`, `charter.key` (each link in the chain) |
| 3    | Charter chain valid              | each successor's `prior` + signer authorized by prior `owner`        |
| 4    | Claim signature valid            | `claim.pay`, `claim.sig`, `claim.key`                                |
| 5    | Publish signature valid          | `publish.pay`, `publish.sig`, key                                    |
| 6    | Key thumbprints match            | `tmb(x.key) == x.pay.tmb` for charter/claim                          |
| 7    | Claim chains to charter          | `claim.anchor == czd(charter₀)`                                      |
| 8    | Publish chains to claim          | `publish.claim == czd(claim)` (current claim per replacement chain)  |
| 9    | Temporal ordering                | `charter.now < claim.now < publish.now`                              |
| 10   | Claim signer authorized          | `claim.tmb` authorized by effective charter `owner`                  |
| 11   | Publish signer authorized        | `publish.tmb` authorized by `claim.owner`                            |
| 12   | Replacement authority (if any)   | per `[claim-replacement-authority]`; `governance` flag surfaced      |
| 13   | AtomId matches payload fields    | extract `(anchor, label)` from payload, compare to expected `AtomId` |

### Provenance Verification (minimal network)

A consumer MAY additionally verify content provenance. The cost is
backend-dependent but MUST be achievable without fetching full file
content or the complete source history:

| Step | Check                                             | Requirement                 |
| :--- | :------------------------------------------------ | :-------------------------- |
| 14   | Fetch source revision metadata at `src`           | Revision metadata only      |
| 15   | Ancestry: `charter.src ⟶ claim.src ⟶ publish.src` | Commit-graph walk only      |
| 16   | Walk source content tree → `path` → subtree       | Content tree structure only |
| 17   | Subtree hash equals atom content tree hash        | Local comparison            |
| 18   | Reconstruct atom snapshot → matches `dig`         | Local computation           |

## Verification

> [!IMPORTANT]
> **Charter amendment re-verification (2026-07-08).** The charter
> transaction changes the trust chain's root and the fork semantics that
> the TLA+ fork-scenario configuration models. All `[charter-*]`,
> `[claim-chains-charter]`, `[claim-charter-authorization]`, and
> `[claim-replacement-authority]` constraints are `unverified` pending
> extension of both formal models; the existing verified rows below
> remain valid for the claim/publish subchain but the fork scenario MUST
> be re-modeled against charter succession before implementation.

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

**Coverage:** 13 formal (TLC/Alloy), 11 rustc, 4 cargo-dep, 6 unit-test, 8 integration-test = **42 total, 0 agent-check**.

> [!NOTE]
> Phase 1 items promoted to **pass** on 2026-02-28 based on atom-id
> implementation review (59 tests, clippy clean).

| Constraint                    | Method           | Result   | Detail                                     | Phase |
| :---------------------------- | :--------------- | :------- | :----------------------------------------- | :---- |
| identity-content-addressed    | machine (Alloy)  | **pass** | Alloy `identity_content_addressed`         | —     |
| identity-stability            | machine (TLC)    | **pass** | TLA+ `IdentityStability` — 2 configs       | —     |
| owner-abstract                | machine (Alloy)  | **pass** | Alloy `ownership_independence`             | —     |
| owner-compatibility           | machine (Alloy)  | **pass** | Alloy `ownership_independence`             | —     |
| owner-authorization-delegated | integration-test | pending  | Signing key auth varies by identity system | 4     |
| symmetric-payloads            | rustc            | **pass** | Both structs have `anchor` + `label`       | 1     |
| publish-chains-claim          | machine (TLC)    | **pass** | TLA+ `PublishChainsClaim` — 2 configs      | —     |
| claim-typ                     | rustc            | **pass** | `TYP_CLAIM` const = `"atom/claim"`         | 1     |
| publish-typ                   | rustc            | **pass** | `TYP_PUBLISH` const = `"atom/publish"`     | 1     |
| sig-over-pay                  | unit-test        | **pass** | sign→verify roundtrip in atom-id tests     | 1     |
| dig-is-atom-snapshot          | unit-test        | pending  | Snapshot hash matches `dig` field          | 4     |
| src-is-source-revision        | integration-test | pending  | Git revision hash matches `src` field      | 4     |
| path-is-subdir                | rustc            | **pass** | `path` field type constrains to subdir     | 1     |
| rawversion-opaque             | rustc            | **pass** | Newtype, no `Deref`/`AsRef`/`Into`         | 1     |
| claim-key-required            | unit-test        | **pass** | CozMessage key — tested in claim roundtrip | 1     |
| publish-key-optional          | unit-test        | **pass** | CozMessage key — optional per Coz format   | 1     |
| crypto-layer-separation       | cargo-dep        | pending  | atom-core Cargo.toml has no coz-rs         | 3     |
| crypto-via-coz                | cargo-dep        | **pass** | atom-id Cargo.toml depends on coz-rs       | 1     |
| key-management-deferred       | cargo-dep        | pending  | No key storage crate in atom workspace     | 3     |
| claim-transition              | unit-test        | **pass** | `verify_claim_roundtrip` sign→verify       | 1     |
| publish-transition            | unit-test        | **pass** | `verify_publish_roundtrip` sign→verify     | 1     |
| session-ordering              | machine (TLC)    | **pass** | TLA+ `SessionOrdering` — 2 configs         | —     |
| no-unclaimed-publish          | machine (TLC)    | **pass** | TLA+ `NoUnclaimedPublish` — 2 configs      | —     |
| no-duplicate-version          | machine (TLC)    | **pass** | TLA+ `NoDuplicateVersion` — 2 configs      | —     |
| no-cross-layer-crypto         | cargo-dep        | pending  | atom-core has zero crypto deps             | 3     |
| no-backdated-publish          | machine (TLC)    | **pass** | TLA+ `NoBackdatedPublish` — 2 configs      | —     |
| verification-local            | integration-test | pending  | Pipeline steps 1–8 offline                 | 4     |
| verification-provenance       | integration-test | pending  | Pipeline steps 9–12 with source access     | 4     |
| atom-snapshot-reproducible    | unit-test        | pending  | Same inputs → same snapshot hash           | 4     |
| ingest-preserves-identity     | machine (Alloy)  | **pass** | Alloy `ingest_preserves_identity`          | —     |
| backend-agnostic-protocol     | rustc            | pending  | Trait sigs use only associated types       | 3     |
| anchor-immutable              | integration-test | pending  | Anchor unchanged across operations         | 4     |
| anchor-content-addressed      | integration-test | pending  | Anchor = hash(genesis content)             | 4     |
| anchor-discoverable           | integration-test | pending  | Anchor derivable from source alone         | 4     |
| manifest-minimal              | machine (Alloy)  | **pass** | Alloy `manifest_properties` fact           | —     |
| backend-bit-perfect           | integration-test | pending  | CozMessage bytes unchanged after store     | 4     |
| atomid-per-source-unique      | machine (TLC)    | **pass** | TLA+ `AtomIdPerSourceUnique` — 2 configs   | —     |
| publish-claim-coherence       | machine (TLC)    | **pass** | TLA+ `PublishClaimCoherence` — 2 configs   | —     |
| atom-detached                 | integration-test | pending  | Atom subtree has no parent refs            | 4     |
| uri-not-metadata              | rustc            | **pass** | URI type absent from payload structs       | 1     |
| trait-signature-pure          | rustc            | pending  | No backend types in trait signatures       | 3     |
| publish-payload-extensible    | unit-test        | pending  | Extra fields in payload round-trip         | 3     |
| fs-source-contract            | integration-test | pending  | FsSource discover+resolve, no claim/pub    | 4     |

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
