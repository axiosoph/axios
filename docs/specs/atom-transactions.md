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
set's **founding charter transaction**: `Anchor := czd(charter)`. This
replaces the earlier backend-specific derivation (git: genesis-commit hash),
and makes anchor derivation **backend-agnostic**: the charter is a coz
object regardless of backend; only the interpretation of its `src` field is
backend-specific. The genesis commit is not lost — a revision hash commits
to its entire ancestry, so `charter.src` transitively pins the genesis; it
simply stops being the identity.

An anchor MUST satisfy the following properties:

1. **Immutable**: The anchor is fixed at chartering and persists for the
   lifetime of the atom-set. The charter is written once and never
   reissued — there is no successor charter
   (`[atom-genesis-and-facts]`, ADR-0007 §3; `docs/glossary.md`
   "Genesis record / genesis-once"). Key rotation and owner-set
   membership change happen as a `fact` layered on the charter's own
   scope, never as a new charter, and therefore cannot change the
   anchor by construction rather than by a rule that must be
   separately checked (`[charter-owner-fact]`).
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
exist in the re-hashed repository. Genesis-once forecloses the answer
this note previously gave (a successor charter chaining to the founding
one): there is no successor charter, and the charter's `src` is a
genesis-payload field, fixed forever, never a target of amendment or
fact. The re-hashed repository therefore has no in-protocol path to the
original anchor at all — establishing an atom-set over it requires a new
charter, which is, by definition, a genuine fork
(`[charter-fork-distinction]`). Continuity between the two anchors — "the
re-hashed history is a continuation of the original project" — is not a
protocol-level guarantee; it is, at most, an out-of-band trust decision a
consumer makes on other evidence (e.g. an attestation `fact` on either
anchor naming the other), never something this specification resolves.

The anchor feeds into identity, but is never identity itself. For a
**claimed** atom, identity is the claim record's own `czd` — the
signed transaction's content-addressed digest (glossary, "Atom
identity"). For an **unclaimed development atom**, identity degrades
to `AtomId`, a deterministic digest of the pre-signature preimage
`(anchor, label)` — the only mechanism anywhere in this protocol
where anchor and label combine as a unit, and only at this one
pre-claim level. Neither case makes `AtomId` or `czd` the fused pair
`(anchor, label)` itself: naming the pair as identity, or as a lookup
key, is glossary-repudiated (2026-07-18) — each component has real,
separate significance, and there is no "hashed atom id." Algorithm
agility for content addressing lives only in the Coz `czd`; the
pre-claim `AtomId` digest is computed the same way but over an
unsigned preimage (`[identity-content-addressed]`).

## Implementor Abstraction

The atom protocol is intentionally agnostic to package formats, versioning
schemes, and build systems. These concerns are the responsibility of
**ecosystem adapters** — internal plugins of the single atom
implementation that read a wrapped ecosystem's conventions (e.g., Cargo
crates, npm packages) for version dialects and fetch enumeration. Atom
sits ABOVE language package managers as the composition-unit layer
(atom-sad.md §1.1); adapters are not peer integrations that ecosystems
implement — they are how the one implementation understands what it
wraps.

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

The formal elaboration of these lists — the algebraic signature, laws,
and typed seam any conforming backend must provide — is
[atom-backend-contract.md](atom-backend-contract.md); the lists below
are the protocol-facing summary.

A backend MUST:

1. Store and retrieve claim `CozMessage`s by `AtomId`.
2. Store and retrieve publish `CozMessage`s by `(AtomId, version)`.
3. Resolve and verify the founding charter from the source, so the
   consumer can check `anchor == czd(charter)` (`[anchor-resolvable]`;
   the anchor is never derived from backend state).
4. Enforce version uniqueness per `(claim czd, version)` pair — `claim
   czd` alone already identifies the claimed atom
   (`[identity-content-addressed]`); pairing it with `AtomId` would be
   redundant, since a claim's own `czd` IS its `AtomId`.
5. Reject a second claim for an already-claimed `label` within its
   `anchor` as void, independent of who signs it — genesis-once
   (`[atom-genesis-and-facts]`, `[publish-claim-coherence]`). _(Re-scoped
   2026-08-05: this item previously required a backend to **support**
   multiple claims for one already-claimed label within an anchor as
   distinct identities — the retired `[claim-replacement-authority]` design's
   governance-seizure path. There is exactly one claim per label, ever;
   a label's current owner changes via `[claim-owner-fact]`, never via
   a second claim record.)_

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

#### [trait-async-io]

The `AtomSource` and `AtomStore` traits MUST use async methods for operations that MAY involve I/O. Specifically:

- `AtomSource::resolve()` — MUST be async. Implementations MAY need to fetch from remote registries.
- `AtomSource::discover()` — MUST be async. Remote search queries involve network I/O.
- `AtomStore::ingest()` — MUST be async. Ingestion from remote sources involves network transfers.
- `AtomStore::contains()` — MUST be async. Remote store existence checks involve network I/O.

Local implementations (e.g., git-backed stores on the same filesystem) MAY complete these async methods synchronously (no `.await` points). The async boundary exists for consumers that need it (registry sources, peer sources), not to mandate concurrency in all implementations.

Data accessor traits (`AtomEntry`, `AtomVersion`, `Manifest`) MUST remain synchronous — they operate on in-memory data structures with no I/O.

`AtomRegistry::claim()` and `AtomRegistry::publish()` MAY remain synchronous in v1 (registries are local git repos). If remote registry write operations are added in vN, these SHOULD be made async.

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

```claim
kind: constraint
evaluator: type
```

#### [composite-source-concurrent]

A composite source SHOULD resolve multiple atoms concurrently. When a `BuildRequest` contains N atom dependencies, the composite source SHOULD spawn N concurrent resolution tasks (bounded by a configurable concurrency limit). This is the primary performance justification for the async trait requirement (`[trait-async-io]`).

No `CompositeAtomSource` implementation exists anywhere in the
codebase yet; this depends on `[trait-async-io]`.

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
3. _(Re-scoped 2026-08-05 — genesis-once: this item previously described
   two claims contesting the same already-claimed label within an anchor
   as forks; that was residue from the retired
   `[claim-replacement-authority]` design — see
   `[publish-claim-coherence]`.)_ Forked sources each charter
   independently (`[charter-fork-distinction]`), so a fork's claims
   live under a *different* anchor by construction, never the same
   already-claimed label within the anchor it forked from — two forks MAY
   publish under the identical human-facing label with no collision,
   since label uniqueness is scoped to a single anchor, never global
   (`docs/glossary.md` "Label"). Each fork's claims therefore carry
   their own distinct `AtomId` trivially, as a consequence of the
   distinct anchor, not because of any contest over one slot; the
   consumer selects which source (and therefore which anchor) to
   trust.
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
  -- Opaque digest. Anchor := czd(charter), backend-agnostic.
  -- See §Anchor for required properties.

TYPE  AtomId      = Vec<u8>                                       (atom-id)
  -- Opaque content-addressed digest — the generic identity/lookup-key
  -- type used across `resolve`/`discover`/`contains` and everywhere
  -- else an atom is referenced. What occupies it changes at the
  -- claim boundary, by design (glossary, "Atom identity"): for an
  -- UNCLAIMED development atom, `AtomId` is a deterministic digest of
  -- the pre-signature preimage `(anchor, label)` — the only mechanism
  -- anywhere in this protocol where anchor and label combine as a
  -- unit, and only at this one pre-claim level; for a CLAIMED atom,
  -- `AtomId` IS the claim record's own `czd`, byte-identical, not a
  -- re-derivation or a wrapping of it. Either way `AtomId` is a
  -- digest, never the fused pair `(anchor, label)` itself and never a
  -- lookup key built from it — both framings are glossary-repudiated
  -- (2026-07-18). Two unclaimed atoms with the same `(anchor, label)`
  -- share the same pre-claim `AtomId` — content-addressing over the
  -- preimage, not identity residing in the preimage.

TYPE  OwnerKind   = "single-key" | "hierarchical" | "rooted-identity"
                                                                   (atom-id)
  -- Required, explicit discriminator on every owner-reference — no
  -- implicit default, not even for "single-key". Names which
  -- external identity framework interprets `OwnerRef.value`; the
  -- VALUE stays opaque regardless of `kind`. See [owner-abstract],
  -- [owner-kind-required].

TYPE  OwnerRef    = { kind: OwnerKind, value: Vec<u8> }           (atom-id)
  -- One kind-tagged, opaque identity digest. `ClaimPayload.owner` is
  -- a single `OwnerRef`; `CharterPayload.owner` is a non-empty set
  -- of them. See [claim-owner-single], [charter-owner-set].

TYPE  CharterPayload = {
        alg:    Alg,
        now:    u64,
        owner:  Vec<OwnerRef>,  -- non-empty set: the principals
                            -- recognized under this anchor
                            -- ([charter-owner-set])
        src:    Vec<u8>,   -- source revision demarking the chartering point
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/charter"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- The charter DEFINES the set: Anchor := czd(charter). Written once,
  -- at chartering, never reissued — genesis-once
  -- ([atom-genesis-and-facts], ADR-0007 §3). No `prior` field exists on
  -- this payload: there is no successor charter to chain to. Owner-set
  -- membership change and key rotation happen as a `fact` extending the
  -- charter's own scope instead ([charter-owner-fact]) — the concrete
  -- fact-record wire shape is not yet specified in this document (Open
  -- Items). `src` demarks the chartering point in history — everything
  -- before it is unowned by this set unless claimed after it
  -- ("orphaned unless re-claimed").

TYPE  ClaimPayload = {
        alg:    Alg,
        anchor: Anchor,
        label:  Label,
        now:    u64,
        owner:  OwnerRef,  -- single owner-reference: the one identity
                            -- accountable for this label
                            -- ([claim-owner-single])
        pkg:    String,    -- PURL type (e.g., "cargo", "npm", "pypi")
        src:    Vec<u8>,   -- source revision hash at claim time (temporal floor)
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/claim"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- The `anchor` field IS the chain link to the charter: anchor ==
  -- czd(charter). No separate charter field exists or is needed —
  -- exactly as publish chains to claim by `claim: Czd`, claim chains
  -- to charter by `anchor`.
  -- Written once, at first claim for this label, never reissued —
  -- genesis-once ([atom-genesis-and-facts], ADR-0007 §3). No `prior` or
  -- `governance` field exists on this payload: there is no replacement
  -- claim to chain to or mark. Label transfer and owner-identity change
  -- happen as a `fact` extending the claim's own scope instead
  -- ([claim-owner-fact]) — the concrete fact-record wire shape is not
  -- yet specified in this document (Open Items).
  -- The `src` field cryptographically binds the claim to its temporal
  -- position in history via the signed payload.
  -- The `pkg` field identifies the ecosystem. Implementations SHOULD
  -- use PURL type strings (https://github.com/package-url/purl-spec)
  -- where a matching type exists (e.g., "cargo", "npm", "pypi") for
  -- interoperability with SBOM and supply-chain tooling. Custom type
  -- strings MAY be used for ecosystems not yet in the PURL registry.
  -- `pkg` names which upstream ecosystem this atom WRAPS: it selects
  -- the version dialect (VersionScheme) and the fetch adapter for the
  -- wrapped ecosystem's lockfile. The atom's own manifest is the atom
  -- manifest; ecosystem files (Cargo.toml, Cargo.lock, ...) are build
  -- inputs inside the atom's content, not protocol surfaces. The
  -- protocol does not prescribe their format or location.

TYPE  PublishPayload = {
        alg:     Alg,
        anchor:  Anchor,
        claim:   Czd,       -- czd of authorizing claim
        content_hash?: Vec<u8>,  -- OPTIONAL: BLAKE3 content tree digest
                            -- ([content-hash-is-tree-digest]; absent = not asserted)
        dig:     Vec<u8>,   -- atom snapshot hash (the published artifact)
        label:   Label,
        mode?:   "reproducible" | "witnessed",  -- reproducibility mode
                            -- ([publish-mode]; absent = "witnessed")
        now:     u64,
        path:    String,    -- subdir in source content tree
        src:     Vec<u8>,   -- source revision hash (provenance)
        tmb:     Tmb,       -- standard Coz: signing key thumbprint
        version: RawVersion,
        typ:     "atom/publish"
      }                                                           (atom-id)
  -- CozMessage MAY include `key` field (convenience for rotated keys).
  -- `content_hash`, when present, is signed exactly like every other
  -- payload field — it carries no separate trust mechanism of its own.
  -- Unlike `dig`, it is never required: a publisher on an
  -- already-collision-resistant backend has no reason to compute it
  -- ([content-hash-obligation]).

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

```claim
kind: constraint
evaluator: none
```

#### [atom-genesis-and-facts]

Charter, claim, and publish are each a
permanent, single genesis record for their scope: written exactly
once, at the moment that scope first comes into existence, and never
again — never reissued, never superseded by a second record of the
same kind for the same scope. The only way to establish a new one is a
genuine fork, a deliberate, out-of-band new anchor
([charter-fork-distinction](#charter-fork-distinction)), never an
in-protocol succession. A
second genesis record for an already-founded scope — a second claim
for an already-claimed label, a second publish for an already-published
`(label, version)` — is void, and every conforming verifier MUST reject
it, independent of any registry's own ref state. This is a fail-safe,
not the primary discipline: a conforming client is expected to check
its own already-fetched history and refuse to construct a duplicate
charter, claim, or publish before ever signing one — that client-side
discipline is not yet itself a specified constraint (Open Items); the
rejection rule above exists for when it does not hold regardless — a
buggy client, a dishonest one, or a compromised key.

```claim
kind: constraint
evaluator: none
depends: [charter-fork-distinction]
```

#### [charter-typ]

A charter transaction's payload MUST carry
`typ: "atom/charter"`.

```claim
kind: constraint
evaluator: none
```

#### [charter-anchor]

The atom-set's anchor MUST equal the coz digest of
the charter: `Anchor == czd(charter)`, where the charter is the
anchor's one and only genesis record — written once, never reissued
(`[atom-genesis-and-facts]`, ADR-0007 §3).

```claim
kind: constraint
evaluator: model-check
```

#### [claim-chains-charter]

Every claim's `anchor` field MUST equal
`czd(charter)` — the charter's own czd, exactly, and nothing else:
there is no successor charter whose czd could be mistaken for it
(`[atom-genesis-and-facts]`, ADR-0007 §3). A `fact` extending the
charter's owner set (`[charter-owner-fact]`) changes who is currently
authorized to act under the anchor; it never produces a new charter
record and therefore never produces a second value this field could
legally hold. This is the claim-level analogue of
`[publish-chains-claim]`: charter : claim :: claim : publish.

```claim
kind: constraint
evaluator: model-check
```

#### [claim-charter-authorization]

_(amended 2026-07-14 — owner-set
membership; re-scoped 2026-08-05 — genesis-once)_: A claim's signing
key MUST be authorized by membership in the charter's **current
owner-fold** — some entry authorizes the signer, under that entry's
own `kind` (`[owner-authorization-delegated]`'s set composition rule).
The charter's current owner-fold is its genesis `owner` set as amended
by every `[charter-owner-fact]` signed since (an add, a remove, never a
new charter — there is exactly one charter, permanently); "the
effective charter" from the pre-genesis-once wording named the wrong
thing; it is the fold that evolves, never the charter record. This
replaces unscoped first-come label TOFU with set-governed claiming;
open or delegated claiming is expressible through the owner
abstraction's identity frameworks, not through protocol exceptions.

```claim
kind: constraint
evaluator: model-check
```

#### [claim-replacement-authority] — RETIRED 2026-08-05

**Retired, not amended.** This constraint specified a **replacement
claim** — a second `ClaimPayload` carrying `prior: czd(replaced claim)`
— as the mechanism for a label's owner to change. ADR-0007 §3 abolishes
that shape everywhere: claim is one of the three genesis record kinds
(charter, claim, publish) that are "each a permanent, single genesis
record for their scope — never reissued, never superseded by a record
of the same kind" (`[atom-genesis-and-facts]`). A `ClaimPayload` with a
`prior` field pointing at an earlier claim is not an amended design —
it is the shape this ADR names and forecloses. There is no replacement
claim, marked or unmarked, owner-authorized or governance-authorized:
`ClaimPayload` no longer carries `prior` or `governance` fields at all
(Type Declarations, above).

What this constraint protected — that a label's controlling identity
can change, and that a change forced by the wider charter-owner-fold
against the current holder's wishes is a first-class, marked, fork-
detectable event, never a silent, unmarked seizure — is real and still
required. `[claim-owner-fact]`, below, restates it as a `fact`
extending the claim's own scope, per ADR-0007 §3's "Label transfer and
governance amendments are `fact`s on the label's record-log entries,
the same discipline as charter." That the two authorization tiers this
constraint drew (ordinary owner-authorized change vs.
charter-owner-fold-authorized seizure) survive unaffected is not a
coincidence: ADR-0007 §3's own fold-based authorization rule for a
label-scope fact — "authorized by the current fold of the label's own
history, cross-checked that the signer is currently in the *claiming
anchor's* owner-fold" — is this exact two-tier structure, independently
arrived at from the genesis-once side.

#### [claim-owner-fact]

A label's currently-effective owner MAY change via a `fact` extending
the claim's own scope — never a new claim record; the claim itself is
permanent and genesis-once (`[atom-genesis-and-facts]`, `[identity-
stability]`). Every publish continues to chain to the one, unchanging
claim czd regardless of how many owner-facts have since landed
(`[publish-chains-claim]`); an owner-fact changes who currently holds
the label, never which record the label *is*. Authorized under exactly
two tiers, distinguishable by every consumer — carried forward from
`[claim-replacement-authority]`'s owner-replacement/governance-
replacement split, now expressed as ADR-0007 §3's fold-based fact rule:

- **owner fact** — signed by the label's own currently-effective owner
  (`[claim-owner-single]`'s single-value semantics: key rotation,
  identity-framework upgrade). The ordinary path, no special marking.
  Unaffected by the charter-owner-set generalization — a claim's own
  effective owner stays single-valued at every point in its history.
- **governance fact** — signed by a member of the charter's *current
  owner-fold* (`[owner-authorization-delegated]`'s set composition
  rule) who is NOT the label's currently-effective owner: a first-class,
  visible seizure event, distinguishably marked so a consumer's trust
  policy MAY refuse, warn, or pin the prior owner. Seizure is
  unmarked-and-invisible to no one — visible to every consumer who
  observes the newer state, with rollback below any consumer's recorded
  state detectable (`[chain-monotonicity]`); a consumer who has never
  seen the newer state makes a TOFU judgment, as at all first contact.
  Absolute freshness without a transport of record is not claimed —
  monotonic non-regression plus mandatory marking is.

  **Widened by the owner-set generalization, mitigated the same way
  charter-owner-fact divergence is.** Under a single-valued charter
  owner, only that one key could force a governance fact. Under
  `[charter-owner-set]`, ANY current charter-fold member can,
  unilaterally — the same flat, undifferentiated authority
  `[charter-owner-fact-linear]` grants for a charter-owner fact now
  extends to seizing any claim under the anchor. This constraint
  extends `[charter-owner-fact-linear]`'s fail-closed divergence rule
  one level down, mirroring it exactly: **a claim has at most one
  effective governance fact per divergence point.** Nothing can prevent
  a charter-fold member from *signing* two divergent governance facts
  against the same claim; the constraint therefore binds consumers —
  observing two DIVERGENT governance facts, signed by different
  charter-fold members, is a **claim-authority fork**, and a consumer
  MUST fail closed for any authority decision downstream of the
  divergence point — neither branch is effective — surfacing the
  divergence for an out-of-band trust decision, exactly as
  `[charter-owner-fact-linear]` requires for an owner-fold fork. A
  consumer's previously recorded chain head (`[chain-monotonicity]`)
  remains valid for decisions at or below that head. This rule is
  scoped to governance facts specifically — two divergent OWNER facts
  against the same claim are outside this constraint's scope,
  unaffected by the owner-set widening this rule mitigates. Exactly as
  for the charter, safety here comes from fork-detection, not from
  unilateral governance-fact authority being intrinsically safe; the
  same named future extension (human-configurable per-charter
  governance policy, `[charter-owner-fact-linear]`) is where
  quorum-style protection against unilateral claim seizure would
  eventually land — this rule is not a complete answer to unilateral
  capture, only to divergent seizure attempts going undetected.

The concrete fact-record wire shape (chaining field, discriminator
naming the fact as owner- vs. governance-authorized) is not yet
specified in this document — Open Items; scoping it is a
git-storage-format.md / atom-backend-contract.md concern, not settled
by re-grading this constraint alone.

**Historical model coverage of the retired design (superseded, not
evidence for this constraint).** `docs/specs/tla/AtomCharter.tla`'s
`ReplacementAuthority` invariant machine-verifies the *replacement-claim*
shape — the `prior`-chained structure and the governance flag's
correctness — that this section retires; that coverage is real, for the
design it checks, and even there it is weaker than it first reads:
`ReplacementAuthority`'s anchor/label conjuncts hold at every state only
because `ClaimReplace` hardcodes them into the replacement record — a
regression guard on the construction, not a state-space proof that
identity persists, since no reachable state could ever violate a
hardcoded field. And the claim-authority fail-closed rule stated above
was never covered at all: `AtomCharter.tla`'s `Divergent`/`ForkFailClosed`
machinery is charter-scoped only, so nothing in the model prevented two
governance replacements from naming the same `prior` — a gap in the
retired design's own coverage, not merely staleness.

**None of the above bears on `[claim-owner-fact]`, the fact-based
restatement.** A green `ReplacementAuthority` result is evidence the
superseded replacement-claim design was internally consistent (with the
vacuity above noted), never evidence for this constraint. No model checks
the fact-based claim-owner-change rule anywhere in this repository.

```claim
kind: constraint
evaluator: none
```

#### [charter-ancestry]

A claim's `src` MUST be a descendant of (or equal
to) the charter's `src`. Together with the existing claim→publish
ancestry, the temporal floor becomes
`charter.src ⟶ claim.src ⟶ publish.src`, rooted at a signed object.
History prior to `charter.src` is visible but unowned by the set —
**orphaned unless re-claimed** after the chartering point. This is a
consumer obligation, not narrative: a resolver encountering a claim
whose `src` does not descend from the charter's `src` MUST
treat it as unowned by this set — neither silently valid nor silently
dropped, but surfaced as pre-charter state awaiting re-claim.
(There is one charter per anchor, genesis-once —
`[atom-genesis-and-facts]` — so "the charter's `src`" names a fixed
value, never a value that shifts with succession.)

```claim
kind: constraint
evaluator: model-check
```

#### [charter-succession] and [charter-succession-linear] — RETIRED 2026-08-05

**Both retired, not amended — together, since they specified one
mechanism.** These constraints specified a **successor charter** — a
second `CharterPayload` carrying `prior: czd(prior charter)` — as the
mechanism for owner-set membership change and key rotation
(`[charter-succession]`), plus the linearity/fork-detection rule
governing it (`[charter-succession-linear]`). ADR-0007 §3 forecloses
this shape by the same rule that retires `[claim-replacement-
authority]`, above: the charter is one of the three genesis record
kinds "never reissued, never superseded by a record of the same kind"
(`[atom-genesis-and-facts]`) — the anchor's first and only charter,
full stop, glossary-confirmed ("There is no charter succession and no
claim replacement; the only way to a 'new charter' is a genuine
fork"). `CharterPayload` no longer carries a `prior` field (Type
Declarations, above).

What this constraint protected — membership change and key rotation
without identity change, and the "two independent layers" distinction
(a principal rotating keys within its own identity framework needs no
charter action; only the *set of principals* changing does) — is real
and still required. `[charter-owner-fact]`, below, restates both as a
`fact` extending the charter's own scope, per ADR-0007 §3's own
statement of this exact rule: "Everything that used to require a new
charter record — owner-set rotation, threshold changes, governance
amendments — is now a `fact` on the anchor's own record-log entries."

#### [charter-owner-fact]

Owner-set membership change and key rotation happen as a `fact`
extending the charter's own scope — never a new charter record; the
charter itself is permanent and genesis-once (`[atom-genesis-and-
facts]`). A charter-owner fact MUST be signed by a key authorized under
`[owner-authorization-delegated]`'s set composition rule by the
charter's *current* owner-fold (the genesis `owner` set as amended by
every charter-owner fact signed since), and MUST NOT alter the anchor:
the anchor remains `czd(charter)` for the lifetime of the set
(preserving `[identity-stability]`). Orphaning is keyed to ANCHOR
change and therefore occurs only on fork: a charter-owner fact
preserves the anchor, so no claim or publish is orphaned by a
membership change.

**Two independent layers, not one** — the confirmed non-conflict
(`[charter-owner-set]` does not disturb this): each `OwnerRef` entry
represents one coherent principal; that principal growing or rotating
keys WITHIN its own identity framework (a hierarchical master
authorizing a new subkey, a rooted identity deriving a new key)
requires no charter-owner fact at all, because the entry's `value` —
the principal's own digest — is unchanged
(`[owner-authorization-delegated]`). A charter-owner fact is needed
only when the *set of principals itself* changes: a member added,
removed, or replaced. Single-key-tier entries are the degenerate case
— a `"single-key"` principal has no internal growth mechanism of its
own, so it cannot add a key without a membership change (a new
`OwnerRef` entry, or replacing its own).

```claim
kind: constraint
evaluator: none
```

#### [charter-owner-fact-linear]

The charter's owner-fold has, at any point in its history, at most one
valid current state. Nothing can prevent a current-fold member from
_signing_ two divergent charter-owner facts against the same prior
fold state; the constraint therefore binds consumers: observing
divergent facts is a **set-authority fork**, and a consumer MUST fail
closed for any authority decision downstream of the divergence point —
neither branch is effective — surfacing the divergence for an
out-of-band trust decision. A consumer's previously recorded chain
head (`[chain-monotonicity]`) remains valid for decisions at or below
that head. The current owner-fold is the fold reached by the unique
valid sequence of charter-owner facts, ordered by **chain position**,
never by `now`: the `now` field is untrusted for authority ordering
(it feeds only the temporal-floor checks).

**Adding a principal** to the owner set requires that principal's own
possession-proof, mirroring the single-owner transfer mechanism
exactly: the charter-owner fact naming the enlarged set is signed by a
key authorized under the current owner-fold's set composition rule,
and a SECOND, independently-signed fact MUST follow it, chained onto
the first fact's own czd and signed by the INCOMING principal's key
(proof of possession) — the same append-only chaining this constraint
already uses for full transfer, applied one link further, for the same
reason: a coz message carries exactly one signature (`czd` is the
digest of a single `{cad, sig}` pair — Coz `README.md` "Canon"), so
proof of possession cannot be a second signature embedded in the first
fact's own message; it is the next link in the chain. A unilateral
addition naming an unwitting recipient — an enlarged set with no such
chained possession-proof for the new entry — is invalid.

**Removing a principal** from the owner set requires only an existing
REMAINING member's signature — no consent or proof from the removed
principal. Rationale: a compromised or malicious member MUST NOT be
able to block its own removal by withholding cooperation.

**This removal rule adds no NEW weakness, but the reason must be
stated, not assumed.** Flat fold authority already lets any single
current member sign an arbitrary charter-owner fact — including one
that removes every other member. The actual protection against a
malicious or compromised member racing a legitimate removal is NOT the
removal rule itself; it is this constraint's own fail-closed divergence
rule, already stated above: two conflicting facts against the same
prior fold state (a mutual-ejection race is one instance) fork set
authority, and a consumer MUST fail closed for any decision downstream
of that divergence. A reader MUST NOT conclude "no consent needed for
removal" implies "removal is safe by construction" — safety here comes
from fork-detection, not from the removal rule.

A charter-owner fact whose resulting owner set would be empty MUST be
rejected (`[charter-owner-set-non-empty]`) — the empty-set rejection
applies to a removal-driven fact exactly as it does to any other path
that could reach an empty set.

**Named future extension, explicitly out of MVP scope**:
human-configurable per-charter governance policy — e.g., a project
declaring "changes to this charter's owner set require 3-of-5 member
signatures" rather than the flat "any current member" default this
constraint specifies — a distinct mechanism from the declared
multi-signature threshold `[charter-owner-threshold]` specifies below
for *arbitrary* owner-authoritative facts, which no implementation
incorporates yet. This is specifically where quorum-style protection
against unilateral capture (a single compromised member signing an
arbitrary, damaging fact) would eventually land. Naming the extension
point here keeps the flat MVP default honest about what it does and
does not protect against: it is not a complete answer to unilateral
capture, only to a removed member blocking its own removal.

**Not machine-verified as stated.** `AtomCharter.tla`'s "succession"
config checks this rule's *predecessor* — the successor-charter shape
`[charter-succession-linear]` retired above — never a fact-based
owner-fold history; per the same reasoning `[claim-owner-fact]` states,
a green result there evidences the superseded design's internal
consistency, not this restatement's correctness.

```claim
kind: constraint
evaluator: none
```

#### [charter-owner-threshold]

The charter MAY declare a signing threshold
for owner-authoritative actions — for example, requiring 2 of 3
current owners to agree before an owner-rotation fact takes effect.
No native multisig primitive is needed: every additional required
signature is its own record. A threshold-gated fact is signed once by
its primary signer, exactly like any other
[charter-owner-fact](#charter-owner-fact); each additional required
signer produces their own, independently
signed co-sign fact naming the primary record's own czd, authorized by
the same fold-check the target itself uses. Verification counts
distinct current-owner-fold members among the primary signer plus
every co-signer; the action takes effect only once that count meets
the charter's declared threshold. A record short of threshold is not
invalid or malformed — it is simply not yet effective. Once a primary
record has, at any point while walking the history forward,
accumulated threshold-many distinct valid co-signers, it is
permanently effective; a later retraction of one co-sign fact does not
undo an action that already took effect — retraction only has power
before threshold is crossed, where it may delay or prevent a
not-yet-effective action from ever reaching threshold. Not yet
incorporated by any implementation (the flat, single-signer
`[charter-owner-fact]` default is what conforming implementations
build against today); this constraint specifies the optional
extension, not a present obligation on every charter.

```claim
kind: constraint
evaluator: none
depends: [charter-owner-fact]
```

#### [publish-delegate-role]

An owner-fold MAY declare a lower-privilege
`publish-delegate` role, authorizing a distinct key to sign routine
publish records without holding full owner-governance authority — the
same declared-role discipline `attesters`/`witnesses` roles already
use for third-party fact authorization. Optional, not required: a
project with no declared publish-delegate authorizes publish through
the ordinary owner-fold, exactly as
[claim-charter-authorization](#claim-charter-authorization) already
specifies. Declaring one trades exposing the full
owner-governance key on every routine publish for a narrower,
revocable signing surface — routine, frequent signing should not need
to expose the same key that governs owner rotation, label transfer,
and threshold membership. Not yet incorporated by any implementation;
this constraint specifies the optional extension, not a present
obligation.

```claim
kind: constraint
evaluator: none
depends: [claim-charter-authorization]
```

#### [chain-monotonicity]

_(re-scoped 2026-08-05 — genesis-once: "chain head" now names a
scope's fact history, never a succession chain)_: Consumers MUST record
the czd of the head of the charter's own fact history (and SHOULD
record the claim czds) under which they acted, and MUST refuse any
served history that regresses below a recorded head — a prefix of
previously observed state is a detected rollback, not an alternative.
First contact with a set is a TOFU decision, as all first contact is.
Locks participate: a resolved lock pins the charter-fact head its
resolution consulted (a follow-up field in the lock schema). The
charter record itself never needs a head distinct from its own czd —
it is fixed forever — the head that can regress is the *fold* of
whatever charter-owner facts (`[charter-owner-fact]`) or claim-owner
facts (`[claim-owner-fact]`) have landed since genesis.

```claim
kind: constraint
evaluator: model-check
```

#### [charter-fork-distinction]

_(simplified 2026-08-05 — genesis-once removes the succession-chain
comparison this constraint used to require)_: Two charter records with
different czds are, by construction, two distinct anchors
(`Anchor := czd(charter)`), regardless of shared source history — this
now follows directly from `[charter-anchor]` and ordinary digest
collision-resistance, with no separate chain-validity check needed: a
fork was never anything other than "a different signed charter," and
genesis-once makes that the only way any two anchors can ever relate.
Forks are therefore explicit by construction: a second charter cannot
retroactively become "the same anchor" as an existing one — there is
no succession mechanism through which it could — and a cross-fork collision on the same label within the same anchor is
structurally impossible.

```claim
kind: constraint
evaluator: model-check
```

**Alloy assertion consolidated, not dropped (2026-08-05).**
`atom_structure.als` carried a dedicated `charter_fork_distinction`
assertion for this constraint until the succession-abolition pass on that
file (same date). With the founding/successor anchor-derivation split
retired, `Anchor` is the charter's own `czd` directly, so "distinct
charters have distinct anchors" restated in the file's terms is `all disj
c1, c2: Charter | c1.czd != c2.czd` — the model's `charter_properties`
injectivity fact verbatim, not an independent claim to check. This is
exactly the "no separate chain-validity check needed" already stated
above, now true of the model as well as the prose. The dedicated
assertion was retired, marked in place with the reasoning above, not
silently deleted; the property remains machine-verified, folded into
`anchor_content_addressed`'s uniqueness conjunct ("every anchor resolves
to **at most one** charter" is this constraint read from the other
direction) — the same assertion-consolidation pattern `[charter-anchor]`
already uses (no dedicated `charter_anchor` assertion exists either; it
is covered the same way). `evaluator: model-check` is left as-is: the
property is verified by `docs/specs/run_model_check.sh`, just via
`anchor_content_addressed` rather than its own assertion.

#### [identity-content-addressed]

An **unclaimed** atom's identity
(`AtomId`) MUST be a deterministic digest of the pair `(anchor,
label)` — content-addressed, collision-free (same inputs always
produce the same digest, distinct inputs always produce distinct
digests), and independent of any key, signature, or signed message.
The pair is the digest's INPUT, never the digest itself: it MUST NOT
be observable in, or reconstructible from, an `AtomId` value, and MUST
NOT be used as a lookup key in its own right (glossary-repudiated,
2026-07-18, both framings). Once the atom is **claimed**, this
pre-signature digest is superseded — the atom's permanent identity
becomes the claim record's own `czd` instead (`[identity-stability]`),
never a hash or wrapping of the pair.

```claim
kind: constraint
evaluator: model-check
```

#### [identity-stability]

_(simplified 2026-08-05 — genesis-once removes the replacement-claim
case this constraint used to distinguish)_: Once claimed, an atom's
identity (the claim record's `czd`) MUST NOT change as new versions are
published against it — a publish's `claim` reference names one fixed
czd for the atom's entire publish history. Identity MUST ALSO survive
ownership transfers and key rotations: because claim is genesis-once
(`[atom-genesis-and-facts]`), there is exactly one claim record per
label, ever, so "the atom's permanent identity" and "the claim's own
`czd`" are the same statement, not two that happen to coincide — the
distinct **label-scope genesis claim** this constraint used to carve
out from later replacement claims no longer needs carving out, because
no replacement claim exists to confuse it with
(`[claim-owner-fact]`'s owner- and governance-facts change who
currently holds the label; they never mint a second claim record or a
second `czd`). Before any claim exists, the unclaimed atom's `AtomId`
(`[identity-content-addressed]`) is likewise fixed by `(anchor,
label)`, neither of which changes pre-claim.
(Model §1 olog: identity stability diagram.)

**Re-graded 2026-08-05, not left at the stale grade.** The Pair-identity
re-derivation note (Verification, below) already established that no
currently-checked TLA+ invariant covers this claim — it was retired
from both `.cfg` files' checked lists. This amendment does not restore
coverage: `AtomTransactions.tla` has no model of genesis-once
void-rejection (a second claim record for an already-claimed label
being structurally invalid, `[publish-claim-coherence]`) any more than
it modeled claim replacement — a different, still-open modeling gap,
not a smaller version of the old one.

```claim
kind: constraint
evaluator: none
```

#### [owner-abstract]

_(amended 2026-07-14 — owner_kind, charter-as-set)_:
An `OwnerRef = { kind: OwnerKind, value: Vec<u8> }` is the protocol's
unit of identity. `value` MUST be an opaque byte vector representing
a cryptographic identity digest; the protocol MUST NOT impose any
interpretation on `value`'s contents — its meaning is determined
entirely by the identity framework `kind` names. Any identity system
producing a stable cryptographic digest MAY be used. `kind` itself IS
protocol-interpreted (`[owner-kind-required]`): it is the only part
of an `OwnerRef` the protocol reads, and it says nothing about the
digest's contents — only which external system a consumer must ask
to resolve authorization. This narrows, but does not contradict, the
original opacity guarantee: the *value* stays opaque; only
*which-system-to-ask* becomes explicit.

`ClaimPayload.owner` is a single `OwnerRef` (`[claim-owner-single]`);
`CharterPayload.owner` is a non-empty set of them
(`[charter-owner-set]`) — the two payloads share the SAME `OwnerRef`
shape at different cardinalities, not different owner concepts.

Different identity frameworks offer different **capabilities** along
the owner abstraction, named by `OwnerKind`:

- **`"single-key"`** (e.g., raw Coz `tmb`): `value` = key thumbprint.
  Compromise requires reclaiming all atoms. The only tier with a
  working evaluator today.
- **`"hierarchical"`** (e.g., OpenPGP master + subkeys): `value` =
  master key fingerprint. Subkeys are authorized via binding
  signatures from the master key. Subkeys can be rotated; compromise
  of a subkey is local, not catastrophic. Named and reserved — not
  yet implemented.
- **`"rooted-identity"`** (e.g., Cyphr Principal Root): `value` = PR
  digest. Supports key rotation, delegation, and sub-identities
  natively. PR identity survives key transitions. Named and
  reserved — not yet implemented.

The protocol is agnostic to which tier is in use beyond dispatching
on `kind`. An `OwnerRef`'s `value` is stable across key rotations,
upgrades, and delegation — only the authorization semantics of
"signing key authorized by this `OwnerRef`" vary, per
`[owner-authorization-delegated]`.

```claim
kind: constraint
evaluator: model-check
```

#### [owner-kind-required]

Every `OwnerRef`'s `kind` field MUST be
present and MUST be one of `OwnerKind`'s three named values — there
is no implicit default, not even for `"single-key"`; a producer MUST
tag every owner-reference explicitly. A consumer encountering
`"hierarchical"` or `"rooted-identity"` MUST reject cleanly (treat
the `OwnerRef` as unauthorizable) rather than attempt to interpret
`value` — those tiers are named and reserved, not implemented; only
`"single-key"` carries a working evaluator
(`[owner-authorization-delegated]`).

```claim
kind: constraint
evaluator: test
```

#### [claim-owner-single]

`ClaimPayload.owner` MUST be a single
`OwnerRef`. A claim represents accountability: within an
organization, exactly one identity is responsible for a specific
label. This is unchanged by the charter-level set generalization
below (`[charter-owner-set]`) — the two fields share `OwnerRef`'s
shape, not its cardinality.

```claim
kind: constraint
evaluator: type
```

#### [charter-owner-set]

`CharterPayload.owner` MUST be a non-empty
set of `OwnerRef` values (`Vec<OwnerRef>`; membership only — order
carries no semantic meaning). A charter declares who is on the team
under this anchor, not who owns one thing: team membership is
naturally plural, and every listed principal has full, equal,
undifferentiated authority — no per-entry roles or restricted
identities exist (there is no concrete use case for a restricted
identity distinct from "not on the team"). A charter whose `owner` set
is empty — at genesis, or as reduced by a `[charter-owner-fact]` — is a
charter nobody could ever claim under; `[charter-owner-set-non-empty]`
makes rejecting it explicit.

```claim
kind: constraint
evaluator: test
```

#### [charter-owner-set-non-empty]

A charter transaction, or a
`[charter-owner-fact]` amending its owner set, whose resulting `owner`
set would contain zero entries MUST be rejected. This is a direct
consequence of `[charter-owner-set]`'s non-empty requirement, restated
as its own constraint because it is exactly the failure mode a
removal-only owner-fact path (`[charter-owner-fact-linear]`) could
otherwise reach silently at implementation time.

```claim
kind: constraint
evaluator: test
```

#### [owner-authorization-delegated]

_(amended 2026-07-14 — owner_kind,
set composition)_: The meaning of "signing key MUST be authorized by
owner-reference `o`" is **delegated to `o.kind`**. The protocol
defines the requirement but not the mechanism:

- `"single-key"`: authorized iff `signer.tmb == o.value`
- `"hierarchical"`: authorized iff the signing subkey has a valid
  binding signature from the master key whose fingerprint matches
  `o.value`
- `"rooted-identity"`: authorized iff the signing key is derivable
  from the Principal Root whose digest matches `o.value`

This delegation is intentional — it allows the protocol to benefit
from richer identity frameworks without coupling to any specific one.

**Set composition** (charter-owner case): where `owner` is a set
(`[charter-owner-set]`), a signer is authorized iff authorized by ANY
entry in the set, evaluated under that entry's own `kind` per the
per-value rule above — set membership is a disjunction over
single-valued authorization, never a distinct mechanism of its own.
Where `owner` is single-valued (`[claim-owner-single]`), the
per-value rule above applies directly and unchanged; there is no
composition step.

```claim
kind: constraint
evaluator: test
```

#### [owner-compatibility]

_(re-scoped 2026-08-05 — genesis-once)_: Upgrading from a simpler to a
richer identity framework (e.g., raw key → GPG master → Cyphr PR) MUST
NOT alter the atom's identity. An owner-identity-framework upgrade is
an owner fact extending the claim's own scope (`[claim-owner-fact]`),
never a new claim record: the claim's own `czd` never changes, so
"which owner is active" and "which czd is identity" are not even
separable questions to keep straight the way they were under claim
replacement — there is only ever the one claim, the one `czd`
(`[identity-stability]`).

```claim
kind: constraint
evaluator: none
```

#### [symmetric-payloads]

`ClaimPayload` and a publish's **base
payload** (`[amendment-field-classification]` class 1 — the first tag
in an update chain, the one targeting the atom commit) MUST carry raw
`anchor` and `label` fields. A consumer holding either payload MUST be
able to verify its `anchor` and `label` fields independently — without
needing the other payload present — and cross-check them for
consistency once both are available (verification step 13). This is
a coherence check between two payloads, never an identity
reconstruction: the payload's identity is its own `czd`
(`[identity-content-addressed]`, `[identity-stability]`), not a value
computed by extracting and re-hashing `(anchor, label)`.

This constraint does NOT extend to a non-base **amendment payload**
(`[publish-update-transition]`, git-storage-format.md): an amendment
payload structurally lacks every identity-class field — `anchor` and
`label` included — by the same class-1 mechanism that already governs
`dig`/`src`/`path`/`version`/`content_hash`/`claim`. Re-adding
`anchor`+`label` to an amendment payload alone, while every other
identity field stays absent, would be an ad hoc asymmetry the field
classification does not otherwise support; an amendment payload was
never symmetric with `ClaimPayload` under the duplicate-then-compare
model this constraint originally described. Verifying `anchor`/`label`
from an amendment payload in isolation is NOT possible by design — a
consumer MUST walk the chain to the base tag
(`[tag-chain-semantic-immutable]`) to recover them, the same
obligation the resolver already has for every other identity-class
field.

`ClaimPayload`/base-`PublishPayload` carrying `anchor`+`label` is
rustc-verified today; the amendment-payload exclusion is new protocol
design — the base/amendment Rust type split
(`[amendment-field-classification]`) is not yet implemented.

```claim
kind: constraint
evaluator: type
```

#### [publish-chains-claim]

The `claim` field in a publish's **base
payload** (`[amendment-field-classification]` class 1) MUST contain
the `czd` of a valid claim for the same `label` within the same
`anchor`. This creates the cryptographic chain from publish back to claim. A non-base
amendment payload carries no `claim` field — `claim` is
identity-class, structurally absent from every tag but the base
(`[amendment-field-classification]`) — so an amendment's membership in
this same chain, and therefore this same claim, is established by
chain position (`[tag-chain-semantic-immutable]`), never by restating
this field.

The base-payload case is machine-verified (TLC); the amendment-payload
exclusion is unverified — new protocol design, no TLA+ model of the
publish tag chain exists yet.

```claim
kind: constraint
evaluator: model-check
```

#### [claim-typ]

The `typ` field of a `ClaimPayload` MUST be the
literal string `"atom/claim"`. The protocol is the authority.

```claim
kind: constraint
evaluator: type
```

#### [publish-typ]

The `typ` field of a `PublishPayload` MUST be the
literal string `"atom/publish"`.

```claim
kind: constraint
evaluator: type
```

#### [sig-over-pay]

All Coz messages MUST follow Coz v1.0: the
signature (`sig`) is computed over the canonical digest (`cad`) of
the raw `pay` bytes. Payload field ordering MUST be preserved
exactly as constructed.

```claim
kind: constraint
evaluator: test
```

#### [dig-is-atom-snapshot]

The `dig` field in `PublishPayload` MUST
be the content-addressed hash of the atom snapshot — the
reproducible, detached artifact produced by the publisher. The atom
snapshot uses deterministic metadata (e.g., constant timestamps and
authorship) to ensure reproducibility across backends.

```claim
kind: constraint
evaluator: none
```

#### [src-is-source-revision]

The `src` field in `PublishPayload`
MUST be the content-addressed hash of the source revision from
which the atom was extracted.

```claim
kind: constraint
evaluator: test
```

#### [path-is-subdir]

The `path` field in `PublishPayload` MUST be
the subdirectory path within the source content tree where the
atom's content resides. This MUST be the exact path needed to
navigate from the source revision's root to the atom's subtree.

```claim
kind: constraint
evaluator: none
```

#### [publish-mode]

The OPTIONAL root-level `mode` field of a
`PublishPayload` declares the atom's reproducibility mode
(atom-model.md §6): `"reproducible"` asserts that every action the
publish denotes yields `record_core`-equal records at fixed
`action_id`; `"witnessed"` (the default — an absent field MUST be
read as `"witnessed"`) asserts nothing beyond witness accumulation.
The mode is a protocol field and MUST NOT be nested under `meta`
(root keys are reserved for protocol fields,
`[publish-payload-extensible]`). Mode transitions — promotion or
demotion, both signed — occur ONLY as new tags appended to the
existing publish chain (`[publish-update-transition]` in the
backend spec), NEVER as a new version: a mode transition changes
no immutable payload field, so `[no-duplicate-version]`'s
same-version rejection does not apply to it and MUST NOT be
weakened to permit it — the chain append is the lawful path.

```claim
kind: constraint
evaluator: test
```

#### [content-hash-is-tree-digest]

The OPTIONAL root-level
`content_hash` field of a `PublishPayload` is a BLAKE3 digest over the
atom's content entries — the same `(path, data-or-target, executable)`
set a backend's own content-tree construction consumes — computed and
included by the publisher before signing (never derived after the
fact by a backend or an ingesting party: a value added post-signature
would carry no cryptographic assurance and is not this field). Where
present, it is a stronger, backend-independent restatement of the
same content identity `dig` already asserts through the backend's own
object hash; where absent, no claim beyond `dig` is made. The exact
algorithm is `[content-hash-algorithm]`; the exact obligation level is
`[content-hash-obligation]`.

```claim
kind: constraint
evaluator: test
```

#### [content-hash-algorithm]

Where present, `content_hash` MUST be
computed by the canonical recursion below, which mirrors — deliberately,
not coincidentally — the same recursive structure a backend's own
canonical content-tree construction already builds (git:
`[snapshot-deterministic]`'s tree; the entry sort rule below is
git-storage-format.md's own "Tree construction" implementation
guidance, restated precisely here), substituting BLAKE3 for the
backend's object hash at every level. This substitution is at the
level of recursive STRUCTURE only, not byte framing: the numbered
steps below deliberately omit git's own object headers (`blob
<len>\0`, `tree <len>\0`) — an implementer reusing a backend's real
object-hashing code path, which adds those headers, MUST NOT do so
here; follow the numbered steps literally, not the backend's own
hashing routine. Two independent implementations given the same
content entries and this algorithm MUST produce byte-identical
output:

1. **Leaf digest.** A regular file's leaf digest is `BLAKE3(data)` —
   the raw file bytes, with no length or type prefix. A symlink's leaf
   digest is `BLAKE3(target)` — the raw target-path bytes. A directory
   has no leaf digest of its own; it is defined entirely by step 2.
2. **Per-directory digest.** For a directory (including the content
   root, treated as the directory at the empty path), collect its
   immediate children — regular files, symlinks, and subdirectories
   exactly one path-segment below it — as `(mode, filename,
   child_digest)` triples. `mode` MUST be the exact ASCII decimal
   digit string a git tree object would record for that entry's kind:
   `100644` (regular file), `100755` (executable regular file),
   `120000` (symlink), or `40000` (directory — five digits, not six;
   this is git's own canonical tree-mode encoding, reused verbatim,
   not reinvented). `child_digest` is that child's leaf digest (step
   1) or, for a subdirectory, its own per-directory digest computed
   recursively, children before parents. Sort the triples using the
   same tie-break rule git's own canonical tree-entry order uses:
   comparing two entries, let `n` be the length of the shorter
   filename; compare the first `n` bytes of each filename byte-wise —
   if they differ, that decides the order. If equal, each entry's
   *tie-break byte* is: its `(n+1)`-th filename byte, if its filename
   is longer than `n`; otherwise `0x2F` (`/`) if the entry is a
   directory; otherwise the entry has no tie-break byte at all, and an
   entry with no tie-break byte sorts before one that has any. (Plain
   filename-only comparison, without this rule, can order a directory
   relative to a same-prefixed sibling file differently than git's own
   tree does — this rule exists precisely to make the two agree, and
   is why it is reused rather than replaced with a simpler comparison.)
   A filename containing a NUL byte (`0x00`) is a forbidden input to
   this algorithm; a producer MUST reject such content before hashing.
   Serialize the sorted triples by concatenating, for each: the `mode`
   digits, one ASCII space (`0x20`), the filename bytes, one NUL byte
   (`0x00`), and the child digest's raw 32 bytes. The directory's
   digest is `BLAKE3` of that concatenation. A directory with zero
   children serializes to the empty byte string; its digest is
   `BLAKE3("")`.
3. **Result.** `content_hash` is the per-directory digest (step 2) of
   the content root: 32 raw bytes, carried as `Vec<u8>` exactly like
   `dig`, `src`, and `anchor` — never hex-encoded at the protocol
   layer. This is a distinct field computed by a distinct algorithm
   from the git backend's own object hash: `content_hash`'s length,
   if ever rendered as hex, coincides with a SHA-256 `src` value's hex
   length, which is exactly why the git backend's own `src`-header
   disambiguation (`[src-hash-kind-disambiguated]`,
   git-storage-format.md) is scoped to the `src` header's own field
   family and MUST NOT be extended to infer or cross-check
   `content_hash` by length or position — they are unrelated fields.

The same canonical-serialization argument that makes a backend's own
content tree injective (distinct content never shares an object hash)
applies here unchanged: the explicit mode digits and NUL-delimited
framing make the per-level serialization unambiguous independent of
which total order is used to sort siblings, and the git tie-break rule
above supplies that order.

```claim
kind: constraint
evaluator: test
```

#### [content-hash-obligation]

`content_hash`'s obligation is
three-tiered, and each tier MUST be read independently — none of the
three follows from either of the others:

1. **Schema level: OPTIONAL.** `content_hash` is not a required
   `PublishPayload` field. A publisher on an already
   collision-resistant backend has no integrity reason to compute it —
   `dig` alone already carries the guarantee this field exists to add.
2. **Consumer level: MUST-verify-when-present.** A consumer that
   receives a `PublishPayload` carrying `content_hash` MUST recompute
   it per `[content-hash-algorithm]` over the resolved content and
   MUST reject the publish on mismatch. Because the field is inside
   the signed payload, an attacker cannot strip it to force a consumer
   back to weak-only verification without invalidating the signature
   entirely — so this obligation costs nothing extra and closes off
   selectively targeting non-checking consumers, which a mere
   SHOULD-prefer would leave open.
3. **Publisher level: SHOULD-include when the backend's own hash is
   weak.** A recommendation, not a schema requirement: publishers on a
   backend whose own object hash is not collision-resistant (git's
   default SHA-1) SHOULD include `content_hash`; publishers on an
   already collision-resistant backend have no obligation to.

**Known limit — `src` is NOT hardened by this field.** `content_hash`
strengthens content identity (the `dig`-equivalent question: "is this
the content the publisher signed"). It does nothing for `src`'s
ancestry and temporal verification (`[charter-ancestry]`,
`[no-backdated-publish]`): those checks depend on walking the actual
git commit-parent graph of the mainline source repository, whose
identity and links are the backend's own commit hashes — a property of
graph structure, not of any single signed value, and therefore not
substitutable by a signed digest the way `dig` is. Hardening `src`'s
ancestry guarantee is irreducible without migrating the mainline
repository itself to a stronger hash; this specification does not
attempt it and no such mechanism should be inferred from
`content_hash`'s existence.

```claim
kind: constraint
evaluator: test
```

#### [symlink-escape-reported]

A symlink in a publish's content whose target — resolved relative to its
own location in the content tree — does not resolve to a path within the
content root MUST be detected at publish time and MUST be reported to
the publisher. Detection MUST NOT, on its own, cause the publish to be
rejected.

This is not a relaxed version of a rejection rule; it is the correct
behavior given what an atom is. Atoms are composed
([Composition Model](../models/composition-model.md)), materialized by
HTC into an FHS view alongside other atoms in the same composition, and
a symlink escaping its own atom's tree is frequently *correct*: its
target may be an FHS location, or a path a sibling atom in the same
composition materializes — resolvable only once composition supplies
it. Rejecting such a publish would encode the assumption that an atom is
a standalone artifact, which contradicts the composition model this
specification is built against. (This replaces the mechanism [ADR-0007
§6](../adr/0007-atom-version-integrity-system.md) once used for the same
case — declaring the symlink's target as its own independently-verified
subject; that mechanism is retired, and an atom is a single content tree,
above.)

The report MUST state a fact, never a verdict: that the link does not
resolve within the atom's own content tree, and will resolve only if the
consuming composition supplies its target. It MUST NOT characterize this
as an error or a defect — whether it is one is knowable only to the
publisher.

```claim
kind: constraint
evaluator: test
```

#### [amendment-field-classification]

Every `PublishPayload` field is
classified into exactly one of three kinds. The classification is a
fixed protocol table, not a per-message choice, and covers every field
in the `PublishPayload` type declaration (above) without exception:

1. **Identity (write-once).** `label`, `version`, `dig`, `src`, `path`,
   `content_hash`, `anchor`, `claim`. MUST be set only by the **base
   publish payload** — the tag that targets the atom commit directly,
   the first tag in a publish's update chain. Every other tag in the
   chain carries an **amendment payload** whose shape has no slot for
   an identity-class field at all — the field is structurally absent,
   not present and checked equal to the base tag's value. An amendment
   cannot change which claim it chains to (`claim`) or which anchor
   it's under (`anchor`), exactly as it cannot change `label` or `dig`;
   chain membership for a non-base tag is established by **chain
   position** (its git tag object targets the previous tag, ultimately
   reaching the base tag), never by restating these fields.
   `[tag-chain-semantic-immutable]` (git-storage-format.md) is the
   storage-side statement of this rule; this constraint is its
   protocol-level source.

   `typ` is identity-class in the same VALUE sense — an amendment
   cannot change which transaction type it is — but is the one
   exception to class 1's structural-absence mechanism: it MUST be
   present, unchanged, on every tag, base or amendment alike
   (`[publish-typ]`), because it is the `CozMessage` payload-family
   discriminator needed to parse the message at all, not per-chain
   data a resolver accumulates from the base tag. No other field in
   this classification carries a parsing role, so no other field needs
   this exception.
2. **Overwrite (latest-wins).** `mode`, `meta`, the signing identity
   (`tmb`, the envelope `key`, and `alg`), `now`. MAY be set by the
   base payload or by any amendment payload. The current value of an
   overwrite-class field is the value set by the last tag, walking the
   chain base to tip, that set it; a tag that omits an overwrite-class
   field leaves an earlier tag's value for it unchanged. `alg` groups
   with the signing identity fields rather than sitting alone: a key
   rotation (`tmb`/`key` changing) MAY plausibly carry an algorithm
   change with it (e.g. moving from an EC curve to Ed25519), and `alg`
   has no independent existence apart from the key it describes — it
   is not classified identity, since nothing about the payload's
   *content* identity depends on which signature algorithm secured it.
3. **Append (accumulate, never overwrite).** Fact-kind entries
   (`[fact-kind-table]`, below — `meta` is NOT the carrier for these).
   MAY be set by the base payload or by any amendment payload. The
   current state of the append-class fields is the union of every
   entry seen walking the whole chain; a later tag MUST NOT cause an
   earlier append-class entry to be removed or replaced.

A conforming resolver walks a publish's update chain exactly once,
base to tip, and produces one accumulated view: identity fields read
from the base tag only (`typ` read from any tag — it is identical on
all of them, so no accumulation is needed); the latest-setter value of
every overwrite field; the full accumulated set of every append entry.
This is the sole resolution algorithm for a publish chain — pairwise
inter-tag comparison MUST NOT be used to enforce identity-field
immutability; class 1's payload shape enforces it structurally, so
there is nothing to compare.

```claim
kind: constraint
evaluator: none
```

#### [fact-kind-table]

An append-class entry (`[amendment-field-
classification]` class 3) MUST carry one of the following fact kinds.
The species column is atom-model.md §4's own partition: a **derived**
fact is produced only by building or independently inspecting the
published artifact; an **asserted** fact is a keyed, post-hoc
assertion no build produces. The tier column is this entry's
authorization tier — **open** (any anchored signer with the fact's
`SignerRole`, `[trust-role-authorization]`, trust-model.md, governs
acceptance alone) or **claim-owner-gated** (additionally requires
`[fact-claim-owner-gated]`, below):

| Kind                 | Species  | Tier              | Retired `meta` field |
| :------------------- | :------- | :---------------- | :------------------- |
| `build-record`       | derived  | open              | `meta.build-hash`    |
| `interface-manifest` | derived  | open              | —                    |
| `observation-record` | derived  | open              | —                    |
| `trial-attestation`  | derived  | open              | —                    |
| `advisory`           | asserted | open              | `meta.security`      |
| `deprecated`         | asserted | claim-owner-gated | `meta.deprecated`    |
| `yanked`             | asserted | claim-owner-gated | `meta.broken`        |
| `superseded-by`      | asserted | claim-owner-gated | `meta.superseded-by` |
| `runtime-requires`   | asserted | claim-owner-gated | —                    |

The four derived kinds are open per atom-model.md §4's own language
("signed by their deriving executor, generally not the claim owner") —
requiring claim-owner authorization for a build record would break the
builder≠owner obligation the model already establishes. `advisory` is
open because independent security-research value depends on
non-project-controlled signers being admissible; gating it to the
claim owner would let a compromised or uncooperative owner suppress
advisories about their own package. `deprecated`/`yanked`/
`superseded-by` are claim-owner-gated because a lifecycle marker on
one label is that label's claim owner's call, not any other anchored
signer's. `runtime-requires` is claim-owner-gated for the same reason,
though it is not a lifecycle marker: it is an assertion about the
package's own technical properties (its declared runtime dependency),
for which the claim owner — the accountable identity for this label —
is the authoritative source, exactly as for the lifecycle markers.

No kind draws its tier from charter-set membership rather than
claim-single-owner. Charter-set membership governs who MAY hold a
claim under the anchor (`[claim-charter-authorization]`); it says
nothing about who is accountable for a SPECIFIC already-held claim's
technical or lifecycle assertions. A charter-set member with no stake
in this particular label would, under charter-set gating, be able to
yank, deprecate, or assert runtime requirements for a package they do
not own — defeating the isolation the claim-owner model exists to
provide. Every claim-owner-gated kind therefore stays scoped to the
narrower claim-single-owner selector, never the broader charter set.

A fact is simply an append-class amendment tag: `typ` remains the
literal `"atom/publish"` (`[publish-typ]`) — a fact does NOT introduce
a distinct `typ` value — carrying an append-class entry naming one of
the kinds above, in place of or alongside an overwrite-class field.
This is why no separate ref family or walk-compatibility branch is
needed for facts: a resolver never special-cases by `typ`; it
classifies every field it encounters by kind
(`[amendment-field-classification]`), uniformly, for every tag in the
chain.

```claim
kind: constraint
evaluator: none
```

Verification will round-trip fact entries and confirm the kind name
is drawn from the reserved table.

#### [fact-claim-owner-gated]

For an append-class entry of a
claim-owner-gated fact kind (`yanked`, `deprecated`, `superseded-by`,
`runtime-requires`; `[fact-kind-table]` tier column), the
`assertor`-role authorization that `[trust-role-authorization]`
(trust-model.md) grants an anchored `assertor` signer is NECESSARY BUT
NOT SUFFICIENT: a conforming reader's acceptance procedure MUST
additionally require the signer to match the atom's EFFECTIVE CLAIM's
single owner at the entry's chain position —
`[trust-owner-selector]` (trust-model.md), disambiguated to mean
specifically `ClaimPayload.owner` (`[claim-owner-single]`), never
`CharterPayload.owner` set membership — before assigning verdict
`fact` (`[trust-acceptance-procedure]`, trust-model.md) to a
claim-owner-gated entry. A claim-owner-gated entry from an `assertor`-
anchored signer that does not match the claim owner MUST receive
verdict `evidence` — a role-adequate signer failing this constraint's
additional owner-match gate, falling back to the same `evidence`
verdict `[trust-role-authorization]` assigns a role-inadequate signer,
though for a distinct reason (owner mismatch, not role mismatch); this
constraint introduces no new verdict and no new `SignerRole`. Open-tier
kinds (the four derived kinds, plus `advisory`) are governed by
`[trust-role-authorization]` alone, with no additional owner
requirement.

This is a consumer-policy-layer requirement — it governs which verdict
a consumer assigns a structurally valid fact entry, not whether the
entry may exist on the chain at all (`[trust-signer-relative]`,
trust-model.md: verdicts are per-consumer, fact-hood is
signer-relative). It is therefore distinct from, and does not
duplicate, `[publish-update-transition]`'s protocol-level amendment
authorization (git-storage-format.md) — the PRE clause there governs
transaction VALIDITY (an append-class entry from any signer is a
structurally valid amendment); this constraint governs TRUST (which
verdict a specific claim-owner-gated entry earns from a given
consumer).

```claim
kind: constraint
evaluator: none
```

#### [rawversion-opaque]

`RawVersion` MUST be treated as an opaque
string by the protocol layer. Semantic interpretation MUST be
deferred to a `VersionScheme` implementor.

```claim
kind: constraint
evaluator: type
```

Verification will confirm `RawVersion` is a newtype with no
`Deref`/`AsRef`/`Into` impl.

#### [claim-key-required]

A claim `CozMessage` MUST include a `key`
field containing the public key used for signing. This enables
trust-on-first-use (TOFU) verification without external key
discovery.

```claim
kind: constraint
evaluator: test
```

Verification is tested in the claim roundtrip.

#### [publish-key-optional]

A publish `CozMessage` MAY include a
`key` field. It SHOULD be included when the signing key differs
from the claim's key (e.g., after key rotation). It
MAY be omitted when the same key signed both claim and publish.

```claim
kind: constraint
evaluator: test
```

#### [anchor-immutable]

_(amended 2026-07-08; re-scoped 2026-08-05 — genesis-once)_: An anchor
MUST NOT change over the lifetime of its atom-set: it is `czd(charter)`
permanently. There is no successor charter to alter it — owner-set
membership change and key rotation are facts layered on the charter's
scope, which never touch the charter record itself
(`[charter-owner-fact]`).

```claim
kind: constraint
evaluator: model-check
```

#### [anchor-content-addressed]

_(amended 2026-07-08)_: An anchor MUST
be the coz digest of the signed founding charter — content-addressed
over an _owned_ payload, never derived from unowned or mutable source
metadata (the pre-charter genesis-hash derivation is retired).

```claim
kind: constraint
evaluator: model-check
```

#### [anchor-resolvable]

_(supersedes [anchor-discoverable],
2026-07-08)_: Given a source, any party MUST be able to enumerate
candidate charters and verify a given anchor against its founding
charter without trusting the publisher. _Selecting_ among candidate
anchors is a recorded consumer trust decision (in locks and URIs), not
a derivation — the anchor is given, then verified; it is no longer
derivable from source content alone.

```claim
kind: constraint
evaluator: none
```

#### [manifest-minimal]

The `Manifest` trait MUST require exactly
`label` and `version`. All other metadata is ecosystem-specific
and MUST NOT be required by the protocol.

```claim
kind: constraint
evaluator: none
```

#### [backend-bit-perfect]

A backend MUST NOT alter the content
of stored `CozMessage`s. Signed messages are immutable binary
blobs (cf. Coz bit-perfect preservation).

```claim
kind: constraint
evaluator: none
```

#### [atomid-per-source-unique]

Within a single source, a label MUST
identify at most one atom — no two atoms in the same source MAY
share the same label. This is a source-content constraint, not a
claim-identity one: it holds prior to and independent of claiming.
_(Re-scoped 2026-08-05 — genesis-once removes the "competing claims"
case this constraint used to defer to `[publish-claim-coherence]`:
that constraint no longer describes a legitimate coexistence, only a
void second-genesis attempt — see below.)_ All atoms in a source share
the same anchor, so label uniqueness within a source directly implies
the pre-signature preimage `(anchor, label)` is unique per atom there
too — the unclaimed `AtomId` this constraint is named for
(`[identity-content-addressed]`). This prevents ambiguous atom
references within a source.

```claim
kind: constraint
evaluator: model-check
```

#### [publish-claim-coherence]

_(re-scoped 2026-08-05 — genesis-once)_: A publish's `claim` field MUST
reference the `czd` of the valid claim for the publish's `label` within
its `anchor` — there is at most one, ever: claim is genesis-once
(`[atom-genesis-and-facts]`), so a given label within its anchor has
exactly one valid claim once claimed, none before. A second claim record
naming an already-claimed label within that anchor is **void**, independent of
who signed it or under what authority, and MUST be rejected by every
conforming verifier — the genesis-once verification rule ADR-0007 §10
states generally, applied here: "the *canonical* genesis is the first
such leaf encountered... any later leaf declaring the same genesis
scope... is void." This retires the prior wording's "multiple claims
... MAY coexist (e.g., forks sharing the same anchor)" — that
described the claim-authority-fork residue `[claim-owner-fact]`'s
governance-fact divergence rule now covers (two *facts* against the
one claim disagreeing about who currently holds it), never two
legitimate claim *records*; conflating the two was itself residue from
the retired `[claim-replacement-authority]` design (`[charter-
fork-distinction]` already establishes that a genuine fork carries a
*different* anchor, so "forks sharing the same anchor" was never a
coherent phrase to begin with). The publish→claim chain is the sole
mechanism for binding a publish to a specific claim. A publish
referencing a void second claim is itself invalid.

```claim
kind: constraint
evaluator: model-check
```

#### [atom-detached]

An atom MUST be a self-contained, detached
subtree. It MUST NOT carry source history. Provenance is recorded
in the publish payload (`src`, `path`) and verified separately.

```claim
kind: constraint
evaluator: none
```

#### [uri-not-metadata]

Atom URIs MUST NOT appear in signed
transaction payloads, persisted protocol state, or any metadata
that participates in cryptographic operations. URIs are a user
convenience — all persistent references MUST use canonical forms.

```claim
kind: constraint
evaluator: type
```

Verification will audit the payload structs and confirm no URI type
is present.

#### [trait-signature-pure]

Protocol trait signatures (`AtomSource`,
`AtomRegistry`, `AtomStore`) MUST NOT contain backend-specific types.
Backend specifics MUST be expressed exclusively through associated
types on the traits.

```claim
kind: constraint
evaluator: review
```

#### [crypto-layer-separation]

Within the atom crate hierarchy,
cryptographic operations (hashing, signing, verification) MUST be
owned by atom-id. atom-core MUST NOT have any direct dependency
on cryptographic libraries — all crypto MUST flow through atom-id.

```claim
kind: constraint
evaluator: none
```

Verification will audit atom-core's `Cargo.toml` and confirm it has
no `coz-rs` dependency.

#### [crypto-via-coz]

All cryptographic operations MUST conform to
the Coz specification semantics. The atom protocol relies on Coz
for signing, verification, digest computation, and key thumbprint
derivation.

```claim
kind: constraint
evaluator: type
```

Verification confirms atom-id's `Cargo.toml` depends on `coz-rs`.

#### [key-management-deferred]

The atom protocol MUST NOT define
mechanisms for public key storage, discovery, or trust
establishment. These concerns are deferred to the identity
framework in use (e.g., Cyphr). The atom
verification function MUST accept raw public key bytes as a
parameter.

Verification will audit the atom workspace and confirm no key
storage crate exists.

### Transitions

```claim
kind: constraint
evaluator: review
```

#### [charter-transition]

An atom-set MAY be chartered by constructing
a `CharterPayload`, signing it, and producing a `CozMessage` that
includes the public key. _(Simplified 2026-08-05 — genesis-once:
there is no successor charter, so there is only one PRE case, not
two.)_

- **PRE**: `src` MUST be a revision that exists in the source. The
  charter's czd becomes the atom-set's anchor. **Bootstrap gate**: if
  the source already carries claims predating any charter, the charter
  MUST be authorized by the owner of the earliest such claim —
  chartering over a live, claimed set is a migration act reserved to
  its incumbent, not a race open to strangers. A virgin source is
  first-to-charter by design (that is `[charter-fork-distinction]`
  working).
- **POST**: The charter message is stored in the source's atom refs,
  enumerable by consumers and retrievable by its czd.

Construction and signature correctness are tested
(`atom/atom-id/tests/charter/construction.rs`), but the PRE
bootstrap-gate authorization check has no implementation to call:
`bootstrap_gate.rs`'s own red test states "no bootstrap-gate
authorization check exists yet"; the POST storage-in-atom-refs
requirement has no atom-git charter storage implementation either.

```claim
kind: constraint
evaluator: test
```

#### [claim-transition]

An atom MAY be claimed by constructing a
`ClaimPayload`, signing it with a Coz-compatible key, and producing
a `CozMessage` that includes the public key.

- **PRE**: `anchor` MUST equal the czd of the set's one charter
  (`[claim-chains-charter]`); the signing key MUST be authorized by
  membership in the charter's current owner-fold
  (`[claim-charter-authorization]`); `src` MUST descend from the
  charter's `src` (`[charter-ancestry]`).
  `label` MUST pass UAX #31 validation. The signing key MUST be
  valid for the specified `alg`. The `CozMessage` MUST include a
  `key` field with the signing public key.
- **POST**: The claim message (including `key`) MUST be stored in
  a backend-specific location retrievable by `AtomId`.

```claim
kind: constraint
evaluator: test
```

Verification is `verify_claim_roundtrip`: sign, then verify.

#### [publish-transition]

A version MAY be published for a claimed
atom by constructing a `PublishPayload`, signing it, and producing
a `CozMessage`. _(PRE/POST restored 2026-08-05: this transition's own
clauses had drifted under the now-retired `[claim-replacement-
transition]` section below it, with no heading of their own —
corrected in place, not new content.)_

- **PRE**: `claim` MUST be the `czd` of a valid, non-revoked claim
  for this `label` within this `anchor`. `version` MUST be a non-empty string.
  `dig` MUST be the hash of the reproducible atom snapshot. `src`
  MUST be the hash of the source revision. `path` MUST be the
  subdir path. `now` MUST be greater than `claim.now`. The signing
  key MUST be authorized by the claim's `owner`.
- **POST**: The publish transaction is stored associating
  `(AtomId, version)` with the signed `CozMessage`.

```claim
kind: constraint
evaluator: test
```

Verification is `verify_publish_roundtrip`: sign, then verify.

#### [claim-replacement-transition] — RETIRED 2026-08-05

**Retired, not amended.** This transition constructed a **replacement
claim** — the shape `[claim-replacement-authority]` specified and
ADR-0007 §3 forecloses (see that constraint's retirement note, above).
There is no replacement claim to construct: `ClaimPayload` carries no
`prior`/`governance` fields (Type Declarations). What this transition
was for — changing who currently holds a label, with the owner-fact/
governance-fact authorization split — is `[claim-owner-fact]`'s
concern now; that constraint's own note already states the concrete
fact-record construction (chaining field, discriminator) is not yet
specified in this document (Open Items), so no replacement PRE/POST
transition is authored here to take this one's place. Its own test
coverage, `construction.rs::claim_replacement_transactions_verify`,
exercised the retired replacement-claim shape specifically (`prior`
linkage, `governance` marking) and verifies nothing about the fact-
based restatement.

#### [session-ordering]

Claim MUST precede publish (model §3.1).
Enforced by:
(a) data flow — `publish` requires `claim` czd, which can only be
obtained from a completed claim; (b) temporal ordering —
`publish.now > claim.now` MUST hold.

### Forbidden States

```claim
kind: constraint
evaluator: model-check
```

#### [no-unclaimed-publish]

A publish transaction MUST NOT exist for
an `AtomId` that has no corresponding claim. If a backend discovers
a publish without a verifiable claim, it MUST treat the publish as
invalid.

```claim
kind: constraint
evaluator: model-check
```

#### [no-duplicate-version]

For a given `AtomId`, a backend MUST NOT
store two publish transactions with the same `version` string.
Republishing the same version MUST be rejected. Version
immutability — once published, a version is sealed.

```claim
kind: constraint
evaluator: model-check
```

#### [no-cross-layer-crypto]

atom-core MUST NOT import, depend on,
or transitively require any cryptographic crate. All crypto flows
through atom-id via Coz.

```claim
kind: constraint
evaluator: none
```

#### [no-backdated-publish]

A publish's **base payload** — the first
tag in the chain (`[amendment-field-classification]` class 1) — with
`now` less than or equal to the `now` of the referenced claim MUST be
rejected. Publishes MUST be temporally ordered after their authorizing
claim. This check anchors the temporal floor once, at base-publish
time; it is NOT re-evaluated against a later amendment payload's own
`now` value. `now` is overwrite-class (`[amendment-field-
classification]`) — every tag, base or amendment, necessarily carries
its own distinct `now` — but only the base payload's `now`
participates in this temporal-floor check; an amendment's `now`
records when the amendment was signed, not a new temporal floor for
the publish event.

### Behavioral Properties

```claim
kind: constraint
evaluator: model-check
```

#### [verification-local]

Given an atom snapshot, its publish
`CozMessage`, and the corresponding claim `CozMessage`, a consumer
MUST be able to verify artifact integrity, signature validity,
claim chain, temporal ordering, and identity derivation using
only local computation — zero network round-trips.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [verification-provenance]

Given the additional ability to fetch
individual source objects (revision metadata and content tree
structure, without full content), a consumer MUST be able to verify
that the atom's content tree is contained within the source content
tree at the claimed `path` from the revision at `src`. This deeper
verification MUST NOT require fetching full file content or the
complete source history.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [atom-snapshot-reproducible]

The atom snapshot at `dig` MUST be
reproducible: given the same source content at `src`/`path` and
the same deterministic metadata, any party MUST be able to
independently construct the atom snapshot and produce the same
hash.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [ingest-preserves-identity]

When an atom is ingested from one
`AtomSource` into an `AtomStore`, its `AtomId` MUST be preserved.
(Model §2.3 — ingest ⊇ condition.)

- **Type**: Safety

```claim
kind: constraint
evaluator: model-check
```

#### [backend-agnostic-protocol]

All protocol-level types (`AtomId`,
`ClaimPayload`, `PublishPayload`, `RawVersion`) MUST be
backend-agnostic. Backend-specific concerns are expressed through
associated types on traits.

- **Type**: Safety

```claim
kind: constraint
evaluator: review
```

#### [publish-payload-extensible]

The publish `CozMessage` payload
MAY contain additional user-defined fields beyond the required set
(`anchor`, `label`, `claim`, `dig`, `src`, `path`, `version`) and
the optional protocol field `mode` (`[publish-mode]`). For
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

```claim
kind: constraint
evaluator: test
```

#### [claim-payload-extensible]

A claim `CozMessage` payload MAY
contain additional user-defined fields beyond the required set
(`alg`, `anchor`, `label`, `now`, `owner`, `pkg`, `src`, `tmb`).
Like publish payloads, all ecosystem-specific or user-defined
extensions MUST be nested inside a dedicated `"meta"` object.
Additional fields are signed as part of the `CozMessage` and
therefore carry cryptographic assurance. Backends MUST preserve
all payload fields, including unknown ones, when storing and
retrieving claim transactions.

_(Re-scoped 2026-08-05 — genesis-once: there is no new claim replacing
a previous one; the paragraph below now describes a fact, not a claim.)_
Claim `meta` remains available on the one, permanent claim payload for
ecosystem extension; the transition-intent-and-severity communication
this note originally described for a **replacement claim** belongs to
the fact-record extending the claim's scope instead
(`[claim-owner-fact]`) — that record's own `meta`-equivalent field, not
this document's `ClaimPayload.meta`, once the fact-record wire shape is
specified (Open Items).

- **Type**: Safety

`ClaimPayload` (`atom/atom-id/src/lib.rs`) has a fixed field set with
no `meta` field or unknown-field-preservation mechanism; default
serde deserialize silently drops fields not in the struct rather than
preserving them, so this constraint is not yet satisfied by the
landed type, let alone verified.

```claim
kind: constraint
evaluator: test
```

#### [fs-source-contract]

An `AtomSource` implementation MAY exist
for filesystem directories (paths without git history). Such a source:

- MUST support `discover` (scan for manifests) and `resolve` (read
  atom metadata)
- MUST NOT support `claim` or `publish` (no VCS history means no
  signed transactions)
- MUST be ingestible into an `AtomStore` for consumption
- MUST use a well-known constant sentinel value as its anchor, so
  that the `AtomId` (a deterministic digest of the pair `(anchor,
  label)`, `[identity-content-addressed]`) is derivable for all atoms.
  The sentinel anchor distinguishes filesystem-sourced
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

### Extensibility & Evolution

The lock format already establishes two independent extension
disciplines: version rejection (`[lock-schema-version]`,
lock-file-schema.md) and a plugin mechanism for new entry types
(`[lock-type-extension-mechanism]`, lock-file-schema.md, ion-owned —
its mechanics live in ion's spec territory and are not restated here).
This subsection generalizes the first discipline, and the
unknown-field-preservation half of `[publish-payload-extensible]`
(above), into cross-format design principles that any future
HTC-plane persisted format (a composition, a build record, an
interface manifest) SHOULD follow, and seeds the reserved-name
vocabulary those principles govern.

```claim
kind: constraint
evaluator: none
```

#### [format-version-discriminator]

Any new persisted wire format — a
distinct serialized object type intended for storage, exchange, or
retention, such as a lock, a composition, a build record, or an
interface manifest — SHOULD carry a top-level version/schema
discriminator field. Where such a field exists, consumers MUST refuse
to interpret an instance whose discriminator value they do not
implement. This generalizes `[lock-schema-version]`'s pattern
(lock-file-schema.md:128-132) as a design principle for future
formats; it is prospective only and does NOT retroactively require a
discriminator on an already-shipped format that lacks one. (As of this
writing, htc-sad.md §2.1's `Composition` object carries a literal
`version: 0` field with no stated consumer-side rejection rule, and
§2.3's `BuildRecord` carries no version field at all — both are known,
pre-existing gaps this principle does not retroactively close; they
are noted here for the record, not addressed by this document.)

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [format-unknown-field-tolerance]

Any persisted format that admits
third-party or ecosystem-specific extension MUST preserve fields it
does not itself define — nested under that format's designated
extension namespace (e.g. a `meta` object) — rather than silently
dropping them. This generalizes `[publish-payload-extensible]`'s
unknown-field-preservation rule (above) as a cross-format norm; a
format that does not support extension at all is simply out of this
rule's scope. (The claim side of this same file already has a landed
counter-example tracked as residue: `[claim-payload-extensible]`'s
`RESIDUE` note above records that `ClaimPayload`'s current
implementation has no `meta` field and drops unknown fields on
deserialization — this principle names the target state that residue
is measured against, it does not resolve it.)

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [format-reserved-names]

The following namespace, field, and kind
identifiers are already in live protocol use and are reserved bare
names — a third-party or ecosystem-specific extension MUST NOT
redefine any of them with incompatible meaning. Interface-analyzer
namespaces (`[htc-manifest-binding-free]`, htc-sad.md:233-234, an open
plugin set per §6.1): `elf-soname`, `python-module`, `cli-name`,
`pkgconfig`. Publish-side overwrite-class `meta` field
(git-storage-format.md, "Publish tag metadata"): `meta.min-compatible`.
Publish-side append-class fact kinds (`[fact-kind-table]`, above):
`build-record`, `interface-manifest`, `observation-record`,
`trial-attestation`, `advisory`, `deprecated`, `yanked`,
`superseded-by`, `runtime-requires`. Claim-side transition `meta`
fields (git-storage-format.md:883-895): `meta.supersedes`,
`meta.announcement`, `meta.effective-after`. A new namespace, `meta`
field, or fact kind introduced by a third-party or ecosystem-specific
extension MUST use a name distinguishable from this reserved set — a
lightweight prefix convention (for example, reverse-DNS-style
qualification such as `com.example.my-field`) MAY be used.
Distinguishing reserved from qualified names is a naming convention
enforced through ordinary review of new namespace/field/kind
additions, mirroring in spirit — not in mechanism —
`[lock-type-extension-mechanism]`'s plugin discipline; no separate
submission process governs it.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

## Verification Pipeline

The following defines the normative verification steps for consumers.

### Local Verification (zero network)

A consumer who has the atom snapshot, publish transaction, claim
transaction, and the charter (with any owner-facts signed since
genesis) MUST be able to perform all of the following locally:

| Step | Check                            | Field(s)                                                             |
| :--- | :------------------------------- | :------------------------------------------------------------------- |
| 1    | Atom snapshot hash matches `dig` | `publish.dig`                                                        |
| 2    | Charter signature valid          | `charter.pay`, `charter.sig`, `charter.key` (the one genesis record — genesis-once, `[atom-genesis-and-facts]`) |
| 3    | Charter owner-fold valid         | step 2's signature, plus every `[charter-owner-fact]` signed since genesis, each fold-authorized at signing time — never a chain of successor charters |
| 4    | Claim signature valid            | `claim.pay`, `claim.sig`, `claim.key`                                |
| 5    | Publish signature valid          | `publish.pay`, `publish.sig`, key                                    |
| 6    | Key thumbprints match            | `tmb(x.key) == x.pay.tmb` for charter/claim/BASE-publish             |
| 7    | Claim chains to charter          | `claim.anchor == czd(charter)`                                      |
| 8    | Publish chains to claim          | BASE-`publish.claim == czd(claim)` (the one, permanent claim — genesis-once, `[atom-genesis-and-facts]`; `claim` is identity-class, present only on the base tag, `[amendment-field-classification]`) |
| 9    | Temporal ordering                | `charter.now < claim.now < BASE-publish.now` (an amendment tag's own `now` is overwrite-class and does not re-anchor this floor, `[no-backdated-publish]`) |
| 10   | Claim signer authorized          | `claim.tmb` authorized by the charter's current owner-fold (step 3) |
| 11   | Publish signer authorized        | `publish.tmb` (already bound to the signing key by step 6) authorized by `claim.owner` as amended by any `[claim-owner-fact]` since genesis (single-valued at every point, per its `kind`) |
| 12   | Claim-owner fact fold (if any)   | per `[claim-owner-fact]`; owner-fact vs. governance-fact distinguished; a divergent governance fact fails this step closed |
| 13   | Symmetric-payload consistency    | BASE-`publish.anchor` == `claim.anchor` and `publish.label` == `claim.label` (`[symmetric-payloads]`; `anchor`/`label` are identity-class, present only on the base tag, `[amendment-field-classification]`) |

This pipeline verifies a single base transaction set (the charter and
its owner-fact history, one claim, one publish's base tag). Verifying
a publish's full update
chain — walking amendment tags and applying
`[amendment-field-classification]`'s accumulation algorithm, and
checking `[publish-update-transition]`'s per-class amendment
authorization — is additional to these 18 steps, not modeled as its
own numbered sequence here; the two governing constraints are
normative on their own.

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

> [!NOTE]
> **Genesis-once retirement (2026-08-05) — read this before trusting any
> `machine (TLC)`/`machine (Alloy)` pass below on a succession- or
> replacement-shaped claim.** ADR-0007 §3 abolished the successor-charter
> and replacement-claim record shapes; `[charter-succession]`,
> `[charter-succession-linear]`, and `[claim-replacement-authority]`
> are RETIRED (see their own entries under Invariants). `AtomCharter.tla`'s
> "succession" config, cited below, verifies that retired design — a
> green result there is evidence the superseded model was internally
> consistent, never evidence for `[charter-owner-fact]`,
> `[charter-owner-fact-linear]`, or `[claim-owner-fact]`, the fact-based
> constraints that replace them. Those three are graded `evaluator: none`
> at their own claim blocks. `atom_structure.als`'s own Charter succession
> chain (`prior`, the founding/successor anchor-derivation split, its
> `charter_fork_distinction` assertion) was abolished the same day, not
> merely flagged: the file's Charter modeling is now genesis-once and
> current, no longer part of what this caveat warns about — see the Alloy
> model paragraph below for its post-abolition state. The historical
> record below is otherwise left intact — it accurately describes what
> was checked, for the design that was checked, as of the dates given.

> [!NOTE]
> **Charter amendment verification (2026-07-08, discharged).** The charter
> transaction changed the trust chain's root and the fork semantics. The
> fork scenario has been re-modeled against charter succession in a new
> module, `docs/specs/tla/AtomCharter.tla`, and the 13 amendment constraints
> are discharged: 8 by TLC (charter/claim transition-system safety), 3 by
> Alloy (static charter/anchor structure), and 2 via a **review residue**
> (procedural, not a state-space property — see below). The existing
> claim/publish rows remain valid.

**TLA+ models**: verified by TLC (pinned toolchain, reproducible via
`docs/specs/run_model_check.sh`).

- `docs/specs/tla/AtomTransactions.tla` — claim/publish subchain, two
  configs, both re-derived 2026-08-05 over the corrected identity (a
  claim's own `czd`, not the `(anchor, label)` pair): fork and
  distinct-anchor configs each now generate 24,041 states (12,521
  distinct), 0 errors. `AtomIdPerSourceUnique` and `IdentityStability`
  were RETIRED from both configs' checked invariants — see the note
  below; this repo's formal models do not model claim replacement, so
  neither invariant is re-expressible without either new modeling
  scope or genuine violation (forks legitimately differ in identity
  now). `PublishClaimCoherence`, `NoUnclaimedPublish`, and
  `PublishChainsClaim` are now textually identical formulas under the
  corrected identity — a pre-existing three-way naming redundancy the
  fix surfaces rather than introduces.
- `docs/specs/tla/AtomCharter.tla` — charter/authorization/anchor layer,
  two configs, re-verified 2026-08-05 (identity-doctrine comment fixes
  only, no formula changes — see below): succession now measures
  4,115,091 states generated / 2,090,615 distinct, depth 6; rotation
  649,144 / 328,807, depth 5 — both re-measured identically before and
  after the comment fixes, confirming no behavior change. These figures
  differ from the previously-recorded 1,409,951/480,096; not
  investigated further here (out of this fix's scope), flagged rather
  than silently overwritten. All 10 safety invariants plus the
  `MonotonicHead` and `ForkFailClosed` temporal properties pass, 0
  errors. Non-vacuity is established by 5 reachability witnesses and an
  8-mutant guard battery (each disabled guard yields the expected
  counterexample). Three identity-doctrine comments were corrected
  (`ClaimReplace`, `ReplacementAuthority`, `[anchor-immutable]`) — see
  the note below; no invariant's formula changed, since the underlying
  facts they check (anchor/label consistency, authorization
  correctness) were already true, only mislabeled as "identity."

**Alloy model**: `docs/specs/alloy/atom_structure.als` verified by Alloy
Analyzer 5.1.0 (pinned nixpkgs) at scope 4, headless via `SimpleCLI`
(SAT4J). All 6 remaining structural assertions pass (UNSAT): five
re-derived 2026-08-05 over the corrected identity — `identity_content_addressed`
(re-scoped to the unclaimed-atom preimage digest), `ingest_preserves_identity`,
`anchor_set_coherence`, `verification_chain` (re-derived from
`[symmetric-payloads]` directly, no longer via a removed `atomId` field on
`Publish`), `anchor_content_addressed` — plus `claim_chains_charter`. Both
`fork_scenario` and the charter-rooted `charter_rooted_fork` are
satisfiable (SAT), confirming the charter facts are consistent.
`ownership_independence` was REMOVED, not re-derived — see the note below.

**Charter succession abolished, same day (2026-08-05).** The file's
`Charter { prior: lone Charter }` succession chain, its recursive
founding/successor anchor-derivation fact, the `founders` function, and
the `charter_fork_distinction` assertion were retired, not repaired —
ADR-0007 §3 forecloses the successor-charter shape they modeled (the same
abolition `[charter-succession]`/`[charter-succession-linear]` record
above). `Charter` is now `{ czd: one Czd }` alone: genesis-once,
`Anchor := czd(charter)` directly, no successor field to carry.
`charter_fork_distinction` was retired as a dedicated assertion not
because it modeled a stale design, but because it became a direct
syntactic restatement of `charter_properties`'s czd-injectivity fact once
the founding/successor split it depended on was gone (full argument at
`[charter-fork-distinction]`'s own claim block, above) — its content
remains machine-verified inside `anchor_content_addressed`'s uniqueness
conjunct. `anchor_content_addressed` and `claim_chains_charter` both
dropped their `founders`-quantified form for a direct `Charter`
quantification with no change in meaning: genesis-once removes the
founding/non-founding distinction those functions existed to draw. This
file's Charter modeling is current, not superseded — unlike
`AtomCharter.tla`'s "succession" config below, it no longer checks an
abolished design.

**Verification methods:**

- `machine (TLC)` / `machine (Alloy)` — formal model checker
- `review-residue` — a constraint whose nature is NOT a state-space property
  (a payload schema literal, or a procedural capability); discharged by a
  decorrelated review of its classification and of the mechanism that will
  eventually check it, not by a model checker, for which a machine discharge
  would be vacuous
- `rustc` — Rust type system; if code compiles, constraint holds
- `cargo-dep` — Cargo.toml dependency audit; verified by `cargo check`
- `unit-test` — deterministic test in isolation
- `integration-test` — end-to-end test requiring git backend

**Review-residue justifications (2026-07-08):**

- `[charter-typ]` — a payload carries `typ: "atom/charter"`: a serialization
  schema literal, identical in kind to `[claim-typ]`/`[publish-typ]`
  (both discharged by `rustc`). In a structural model the message type is already
  carried by the sig/record, so a model-checker "discharge" would be a
  vacuous restatement. Deferred to `rustc` (a `TYP_CHARTER` const check) at
  implementation, on the established typ-literal precedent.
- `[anchor-resolvable]` — any party can enumerate candidate charters and
  verify a given anchor against its founding charter: an existence-of-
  enumeration capability, not a state-space invariant. It rests on
  `[charter-transition]` POST (the charter is stored in the source's atom
  refs, enumerable and retrievable by czd) and the local verification
  pipeline (steps 2–3, 7); selection among candidates is a recorded consumer
  trust decision, not a derivation. Discharged by decorrelated review.

**Coverage:** the 2026-07-08 charter amendment adds 13 constraints — 8
`machine (TLC)`, 3 `machine (Alloy)`, 2 `review-residue` — all discharged
(rows below). Combined with the pre-amendment table: 24 formal (TLC/Alloy),
11 rustc, 4 cargo-dep, 6 unit-test, 8 integration-test, 2 review-residue.
_The rows added or amended for the charter constraints are marked (amended)._

> [!NOTE]
> Phase 1 items promoted to **pass** on 2026-02-28 based on atom-id
> implementation review (59 tests, clippy clean).

> [!NOTE]
> A `machine (TLC)`/`machine (Alloy)` **pass** on a charter constraint means
> the abstract protocol satisfies that property under model checking — it
> does NOT mean a Rust-level validator exists yet. As of this table's
> current state, no chain/succession/ancestry/authorization validator is
> implemented anywhere in the codebase for any charter constraint
> (`verify_succession_chain`, `verify_claim_replacement`, and `CharterStore`
> are all explicit Phase 1 stubs). See `atom/atom-id/tests/charter/` for the
> corresponding red-test inventory tracking that implementation gap.

> [!NOTE]
> **Owner-set amendment (2026-07-14) outpaces `AtomCharter.tla`.** This
> revision generalizes `CharterPayload.owner` from a single value to a
> non-empty `Vec<OwnerRef>` (`[charter-owner-set]`) and reworked the
> succession-era authorization semantics to set membership.
> `AtomCharter.tla` still encodes the pre-amendment single-owner model
> (`CharterSucceed`'s rotation guard, `CharterTransfer`'s transfer
> dichotomy assume exactly one owner per charter) and has not been
> re-run against set-valued ownership. The `machine (TLC)` **pass**
> recorded below for `[charter-anchor]`, `[claim-charter-authorization]`,
> `[charter-ancestry]`, `[chain-monotonicity]`, and `[anchor-immutable]`
> therefore reflects the single-owner model, not this amendment's set
> semantics. An `AtomCharter.tla` rework covering add/remove-membership
> operations is registered follow-on work, not yet landed; until it
> lands and re-discharges these rows, their **pass** status is
> inherited from the prior model, not fresh coverage of the set case.
>
> **Genesis-once retirement (2026-08-05) removes three rows from this
> note's scope, not just staleness from it.** `[charter-succession]`,
> `[charter-succession-linear]`, and `[claim-replacement-authority]`
> are RETIRED (ADR-0007 §3 forecloses the successor-charter and
> replacement-claim shapes both modeled); `AtomCharter.tla`'s
> "succession" config and its checked invariants continue to describe
> that retired design faithfully, but neither their historical
> `machine (TLC)` **pass** nor this note's staleness caveat carries
> forward to the fact-based replacements. `[charter-owner-fact]`,
> `[charter-owner-fact-linear]`, and `[claim-owner-fact]` are graded
> `evaluator: none` on their own claim blocks, above — ungraded, not
> stale-graded, because nothing in this repository checks them yet.

> [!NOTE]
> **Pair-identity re-derivation (2026-08-05) leaves four claims without
> machine coverage, plus a fifth found by a property-phrasing sweep.**
> Correcting identity from the `(anchor, label)` pair to the claim's own
> `czd` (glossary-repudiated 2026-07-18) surfaced that four
> `machine (TLC)`/`machine (Alloy)` claims were never really
> state-space-verified in the first place, and cannot be re-derived
> without new modeling scope. A follow-up sweep for the same doctrine
> phrased WITHOUT naming the pair — properties merely asserting
> "identity is stable" / "never altered" — additionally found
> `AtomCharter.tla` defective in three comments, though not vacuous in
> the same way:
>
> - `[atomid-per-source-unique]` and `[identity-stability]`
>   (`AtomTransactions.tla`): under the old pair identity these held
>   VACUOUSLY — `AtomId` was a pure function of the very fields being
>   compared, so TLC needed no reachable-state exploration to "prove"
>   them. Substituting the corrected identity makes the literal
>   invariant FALSE (TLC finds a 2-state counterexample: the same
>   owner re-claiming a source+label at a later clock tick produces a
>   second, distinct `czd`). The doctrinally-correct restatement —
>   identity is stable across ownership transfer via the label-scope
>   genesis claim's `czd` — cannot be checked because no TLA+ module
>   models genesis-once void-rejection (a second claim record for an
>   already-claimed label being structurally invalid,
>   `[publish-claim-coherence]`) anywhere in this repo. Both invariants
>   were retired from the checked list in both `.cfg` files rather
>   than left green-by-construction; see `AtomTransactions.tla` for
>   the full reasoning.
> - `[owner-abstract]` and `[owner-compatibility]`
>   (`atom_structure.als`): their backing assertion,
>   `ownership_independence`, was a logical tautology independent of
>   the identity question (`(x = a ∧ y = a) → x = y` holds for any
>   relation) — it never tested anything. Its intended content
>   ("upgrading ownership doesn't alter identity") hits the identical
>   modeling gap as above: `atom_structure.als` has no owner-fact
>   mechanism at all, so an owner-identity-framework upgrade isn't
>   modeled. The assertion was REMOVED rather than repaired in place.
> - `AtomCharter.tla` DOES model claim replacement (`ClaimReplace`,
>   `prior` field, `ReplacementAuthority`) — the one formal model in
>   this repo that does. But its `ReplacementAuthority` conjunct
>   `cl.anchor = old.anchor` (a comment there read "identity never
>   altered") holds at every state only because `ClaimReplace` hardcodes
>   anchor/label to be copied unchanged — a regression guard, not a
>   state-space proof. Genesis-czd persistence there is definitional
>   (an append-only chain whose czds are never mutated or reissued), the
>   same `review-residue` category as `[charter-typ]`, not something a
>   model check could non-vacuously establish without a construction
>   where anchor/label COULD drift. Three misleading comments were
>   corrected (`ClaimReplace`, `ReplacementAuthority`,
>   `[anchor-immutable]` — the last conflated ANCHOR/atom-set stability
>   with individual ATOM identity, a related but separate claim); no
>   invariant formula changed, and both configs re-verified with
>   identical state counts before and after (4,115,091/2,090,615
>   succession, 649,144/328,807 rotation, 0 errors both).
>
> **Genesis-once retirement (2026-08-05) resolves the "cannot be
> re-derived without new modeling scope" framing above for two of these
> four, not by adding coverage but by removing the premise — and
> supersedes a competing claim below, not just a phrasing choice.** The
> original wording attributed the gap to "no TLA+ module models claim
> replacement" — as if extending `AtomTransactions.tla` to cover
> replacement would eventually close it. There is no replacement to
> model (`[claim-replacement-authority]` retired, above); the honest
> gap is the different one restated in the two bullets above. Per this
> amendment's own instruction to grade what it touches rather than
> defer, `[identity-stability]` and `[owner-compatibility]` are
> re-graded `evaluator: none` at their own claim blocks, above — not
> left at the stale `model-check` grade pending disposition.
> `[atomid-per-source-unique]` and `[owner-abstract]` are untouched by
> this amendment; their pair-identity-specific coverage gap remains the
> architect's to disposition, unchanged by anything above.
>
> A separately-landed pass through this same note (concurrent with this
> amendment, corrected `AtomCharter.tla`'s misleading comments — see the
> bullet immediately above) concluded `[claim-replacement-authority]`
> "was already correct and remain[s] `machine (TLC)`-verified without
> change," needing only a comment fix. That conclusion is accurate about
> the comments and the retired model, and irrelevant to this document as
> of this amendment: `[claim-replacement-authority]` is retired, not
> merely re-verified, and a correct account of what a stale model checks
> is not evidence the constraint it checks survives (this amendment's
> own governing instruction — a worker was reversed today for treating a
> green model check as exactly that kind of evidence). Superseded here,
> not silently dropped: the finding about `AtomCharter.tla`'s comments
> stands as the accurate historical record it is; the inference that
> `[claim-replacement-authority]` therefore remains a live claim does
> not.

## Implications

### Scope Boundaries

This specification explicitly does NOT define:

- **Manifest schemas**: `Cargo.toml`, `package.json`, `recipe.ion`, etc.
  are ecosystem concerns.
- **Dependency resolution**: algorithms for resolving version constraints.
- **Build integration**: how atoms are consumed by build systems.
- **Network transport**: HTTP, SSH, native protocols — implementation details.
- **Key/identity management**: deferred to Cyphr.
- **Charter `src` interpretation**: the founding charter's derivation
  (`Anchor := czd(charter)`) is now fixed and backend-agnostic; only
  how a backend interprets the charter's `src` field remains
  backend-specific.
- **Fact-append mechanics**: atom-model.md §4 states the governing
  laws for post-publish facts on the metadata chain — builder≠owner
  signer authorization for appended facts, and the fact-append
  carve-out (routine fact appends must not present as
  ownership-relevant events). The concrete fact-kind encoding and
  authorization mechanism are now defined —
  `[amendment-field-classification]`, `[fact-kind-table]`, and
  `[fact-claim-owner-gated]` above — closing atom-sad.md §9 gap 5
  at the specification level. The Rust implementation of this
  mechanism is separate, not-yet-landed work.
- **Charter-scope and claim-scope fact wire shape (2026-08-05, new
  gap)**: `[charter-owner-fact]`, `[charter-owner-fact-linear]`, and
  `[claim-owner-fact]` require owner-set membership change, key
  rotation, and label-owner transfer to be expressed as a `fact`
  extending the charter's or claim's own scope, per ADR-0007 §3 —
  replacing the retired successor-charter and replacement-claim record
  shapes. This document states the authorization rules those facts
  MUST satisfy but does NOT define their concrete encoding (a chaining
  field analogous to `pay.prior`, a discriminator distinguishing an
  owner-fact from a governance-fact, storage/ref placement). That is
  deliberately left to `git-storage-format.md` and
  `atom-backend-contract.md` — both separately obligated by ADR-0007 to
  amend for the same reason, and the natural home for a wire-format
  decision that must cohere with those documents' own backend
  representation, not be pre-empted here in isolation.
