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
   consumer can check `anchor == czd(charter₀)` (`[anchor-resolvable]`;
   the anchor is never derived from backend state).
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

`VERIFIED: type (atom/atom-core/src/lib.rs -- AtomSource::resolve/discover are async fn (lines 124, 130); AtomStore::ingest/contains are async fn (lines 250, 253); the accessor traits AtomEntry/AtomVersion/Manifest are sync fn (lines 70, 87, 54); AtomRegistry::claim/publish are sync fn (lines 210, 223) -- the async/sync split is type-enforced: six real implementations compile against these exact signatures (atom-git's GitSource, GitRegistry, GitStore; eos-daemon/src/scheduler.rs's test-only RecordingSource), and an impl violating the split would fail to compile)`

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
`RESIDUE: Phase 1/2 -- no CompositeAtomSource implementation exists anywhere in the codebase yet; depends on [trait-async-io]`

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
  -- Opaque digest. Anchor := czd(charter₀), backend-agnostic.
  -- See §Anchor for required properties.

TYPE  AtomId      = { anchor: Anchor, label: Label }              (atom-id)
  -- Protocol-level identity. Determined solely by the source's
  -- anchor and the atom's label. Algorithm-free, permanent.
  -- Two atoms with the same (anchor, label) ARE the same atom.
  -- NOT a hash — this is the abstract identity pair.

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
        prior:  Czd?,      -- OPTIONAL: czd of the charter this one succeeds
        src:    Vec<u8>,   -- source revision demarking the chartering point
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/charter"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- The founding charter (no `prior`) DEFINES the set: Anchor :=
  -- czd(charter₀). A successor charter (with `prior`) MUST be
  -- authorized per [charter-succession]/[charter-succession-linear];
  -- succession preserves the anchor. `src` demarks the chartering
  -- point in history — everything before it is unowned by this set
  -- unless claimed after it ("orphaned unless re-claimed").

TYPE  ClaimPayload = {
        alg:    Alg,
        anchor: Anchor,
        label:  Label,
        now:    u64,
        owner:  OwnerRef,  -- single owner-reference: the one identity
                            -- accountable for this label
                            -- ([claim-owner-single])
        pkg:    String,    -- PURL type (e.g., "cargo", "npm", "pypi")
        prior:  Czd?,      -- OPTIONAL: czd of a replaced claim ([claim-replacement-authority])
        governance: bool?, -- OPTIONAL: MUST be true on governance replacement; absent otherwise
        src:    Vec<u8>,   -- source revision hash at claim time (temporal floor)
        tmb:    Tmb,       -- standard Coz: signing key thumbprint
        typ:    "atom/claim"
      }                                                           (atom-id)
  -- CozMessage MUST include `key` field (public key for TOFU).
  -- `prior` and `governance` are root-level PROTOCOL fields (declared
  -- here precisely so the reserved-root-keys rule is satisfied).
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

**[charter-typ]**: A charter transaction's payload MUST carry
`typ: "atom/charter"`.
`VERIFIED: review-residue (payload schema literal; rustc at impl — cf. [claim-typ]/[publish-typ])`

**[charter-anchor]**: The atom-set's anchor MUST equal the coz digest of
the founding charter: `Anchor == czd(charter₀)`, where the founding
charter is the unique charter in the succession chain carrying no
`prior` field.
`VERIFIED: machine (TLC)`

**[claim-chains-charter]**: Every claim's `anchor` field MUST equal
`czd(charter₀)` — the founding charter's czd, exactly. Succession
governs _authorization_, never the anchor value (a successor's czd is
not an anchor and MUST NOT appear as one). This is the claim-level
analogue of
`[publish-chains-claim]`: charter : claim :: claim : publish.
`VERIFIED: machine (Alloy)`

**[claim-charter-authorization]** _(amended 2026-07-14 — owner-set
membership)_: A claim's signing key MUST be authorized by membership
in the effective charter's `owner` set — some entry in the set
authorizes the signer, under that entry's own `kind`
(`[owner-authorization-delegated]`'s set composition rule). (The
effective charter is the latest valid charter in the succession chain
at claim time.) This replaces unscoped first-come label TOFU with
set-governed claiming; open or delegated claiming is expressible
through the owner abstraction's identity frameworks, not through
protocol exceptions.
`VERIFIED: machine (TLC)`

**[claim-replacement-authority]** _(amended 2026-07-14 — governance
path reworded for owner-set membership)_: A claim MAY be replaced by
a new claim carrying `prior: czd(replaced claim)`, under exactly two
authorities, distinguishable by every consumer:

- **owner replacement** — signing key authorized by the replaced
  claim's `owner` (key rotation, identity-framework upgrade), under
  `[claim-owner-single]`'s single-value semantics: the ordinary path,
  no special marking. Unaffected by the charter-owner-set
  generalization — a claim's own `owner` stays single-valued.
- **governance replacement** — signing key authorized by MEMBERSHIP
  in the effective charter's `owner` set (`[owner-authorization-delegated]`'s
  set composition rule) but NOT by the replaced claim's `owner`: the
  replacement payload MUST carry `governance: true`. A governance
  replacement is a first-class, visible seizure event; consumers' trust
  policies MUST be able to distinguish it and MAY refuse, warn, or pin
  the prior owner. The honest strength of this guarantee: seizure is
  unmarked-and-invisible to no one — it is visible to every consumer
  who observes the newer state, and rollback below any consumer's
  recorded state is detectable (`[chain-monotonicity]`); a consumer who
  has never seen the newer state makes a TOFU judgment, as at all first
  contact. Absolute freshness without a transport of record is not
  claimed — monotonic non-regression plus mandatory marking is.

  **Widened by the owner-set generalization (named here, not
  mitigated).** Under a single-valued charter owner, only that one key
  could force a governance replacement. Under `[charter-owner-set]`,
  ANY existing charter-set member can, unilaterally — the same flat,
  undifferentiated authority `[charter-succession-linear]` grants for
  charter succession now extends to seizing any claim under the
  anchor. Charter succession has an explicit, already-specified
  mitigation for unilateral action (`[charter-succession-linear]`'s
  fail-closed rule on conflicting successors); THIS constraint does
  NOT define an analogous fail-closed rule for two CONFLICTING
  governance replacements of the same claim signed by different
  charter-set members, and `[charter-succession-linear]`'s named
  governance-policy extension point is scoped to charter succession,
  not claim-level governance replacement. This is a genuine widening
  of unilateral-seizure authority introduced by the owner-set
  amendment; it is named explicitly rather than left implicit, and
  resolving it (a claim-level fork/divergence rule, or folding claim
  governance under the same future per-charter policy mechanism) is
  out of this node's scope.

Publishes chained to a replaced claim remain verifiable history;
new publishes MUST chain to the current claim.
`VERIFIED: machine (TLC)`

**[charter-ancestry]**: A claim's `src` MUST be a descendant of (or equal
to) the effective charter's `src`. Together with the existing
claim→publish ancestry, the temporal floor becomes
`charter.src ⟶ claim.src ⟶ publish.src`, rooted at a signed object.
History prior to `charter.src` is visible but unowned by the set —
**orphaned unless re-claimed** after the chartering point. This is a
consumer obligation, not narrative: a resolver encountering a claim
whose `src` does not descend from the effective charter's `src` MUST
treat it as unowned by this set — neither silently valid nor silently
dropped, but surfaced as pre-charter state awaiting re-claim.
`VERIFIED: machine (TLC)`

**[charter-succession]** _(amended 2026-07-14 — owner-set
membership)_: A successor charter (carrying `prior`) MUST be signed
by a key authorized under `[owner-authorization-delegated]`'s set
composition rule by the charter named in `prior`'s `owner` set, and
MUST NOT alter the anchor: the anchor remains `czd(charter₀)` for the
lifetime of the set. Succession is how membership change and key
rotation occur without identity change (preserving
`[identity-stability]`). Orphaning is keyed to ANCHOR change and
therefore occurs only on fork: succession preserves the anchor, so no
claim or publish is orphaned by a membership change.

**Two independent layers, not one** — the confirmed non-conflict
(`[charter-owner-set]` does not disturb this): each `OwnerRef` entry
represents one coherent principal; that principal growing or rotating
keys WITHIN its own identity framework (a hierarchical master
authorizing a new subkey, a rooted identity deriving a new key)
requires no charter at all, because the entry's `value` — the
principal's own digest — is unchanged (`[owner-authorization-delegated]`).
A successor charter is needed only when the *set of principals
itself* changes: a member added, removed, or replaced.
Single-key-tier entries are the degenerate case — a `"single-key"`
principal has no internal growth mechanism of its own, so it cannot
add a key without a membership change (a new `OwnerRef` entry, or
replacing its own).
`VERIFIED: machine (TLC)`

**[charter-succession-linear]** _(amended 2026-07-14 — owner-set
add/remove semantics)_: A charter has at most one valid successor.
Nothing can prevent a key from _signing_ two successors naming the
same `prior`; the constraint therefore binds consumers: observing
divergent successors is a **set-authority fork**, and a consumer MUST
fail closed for any authority decision downstream of the divergence
point — neither branch is effective — surfacing the divergence for an
out-of-band trust decision. A consumer's previously recorded chain
head (`[chain-monotonicity]`) remains valid for decisions at or below
that head. The effective charter is the head of the unique valid
chain, ordered by **chain position** (`prior` links), never by `now`:
the `now` field is untrusted for authority ordering (it feeds only
the temporal-floor checks).

**Adding a principal** to the owner set requires that principal's own
possession-proof, mirroring the single-owner transfer mechanism
exactly: the successor charter (naming the enlarged set) is signed by
a key authorized under the prior charter's `owner` set
(`[owner-authorization-delegated]`'s set composition rule), and a
SECOND, independently-signed charter MUST follow it, chained via
`prior` onto the successor's own `czd` and signed by the INCOMING
principal's key (proof of possession) — the same succession-chain
mechanism this constraint already uses for full transfer, applied one
link further, for the same reason: a coz message carries exactly one
signature (`czd` is the digest of a single `{cad, sig}` pair — Coz
`README.md` "Canon"), so proof of possession cannot be a second
signature embedded in the successor's own message; it is the next
link in the chain. A unilateral addition naming an unwitting
recipient — an enlarged set with no such chained possession-proof for
the new entry — is invalid.

**Removing a principal** from the owner set requires only an existing
REMAINING member's signature — no consent or proof from the removed
principal. Rationale: a compromised or malicious member MUST NOT be
able to block its own removal by withholding cooperation.

**This removal rule adds no NEW weakness, but the reason must be
stated, not assumed.** Flat set authority already lets any single
existing member sign an arbitrary successor — including one that
removes every other member. The actual protection against a
malicious or compromised member racing a legitimate removal is NOT
the removal rule itself; it is this constraint's own fail-closed
divergence rule, already stated above: two conflicting successors
naming the same `prior` (a mutual-ejection race is one instance) fork
set authority, and a consumer MUST fail closed for any decision
downstream of that divergence. A reader MUST NOT conclude "no consent
needed for removal" implies "removal is safe by construction" — safety
here comes from fork-detection, not from the removal rule.

A successor charter whose resulting owner set would be empty MUST be
rejected (`[charter-owner-set-non-empty]`) — the empty-set rejection
applies to a removal-driven successor exactly as it does to any other
path that could reach an empty set.

**Named future extension, explicitly out of MVP scope**:
human-configurable per-charter governance policy — e.g., a project
declaring "changes to this charter require 3-of-5 member signatures"
rather than the flat "any existing member" default this constraint
specifies. This is specifically where quorum-style protection against
unilateral capture (a single compromised member signing an
arbitrary, damaging successor) would eventually land. Naming the
extension point here keeps the flat MVP default honest about what it
does and does not protect against: it is not a complete answer to
unilateral capture, only to a removed member blocking its own
removal.
`VERIFIED: machine (TLC)`

**[chain-monotonicity]**: Consumers MUST record the czd of the charter
chain head (and SHOULD record the claim czds) under which they acted,
and MUST refuse any served chain that regresses below a recorded head
— a prefix of previously observed state is a detected rollback, not an
alternative. First contact with a set is a TOFU decision, as all first
contact is. Locks participate: a resolved lock pins the charter head
its resolution consulted (a follow-up field in the lock schema).
`VERIFIED: machine (TLC)`

**[charter-fork-distinction]**: A charter with no valid succession chain
from another set's founding charter defines a **distinct atom-set**,
regardless of shared source history. Forks are therefore explicit by
construction: a fork cannot share the origin's anchor (it cannot forge
succession), and cross-fork `(anchor, label)` collision is structurally
impossible.
`VERIFIED: machine (Alloy)`

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

**[owner-abstract]** _(amended 2026-07-14 — owner_kind, charter-as-set)_:
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
`VERIFIED: machine (Alloy)`

**[owner-kind-required]**: Every `OwnerRef`'s `kind` field MUST be
present and MUST be one of `OwnerKind`'s three named values — there
is no implicit default, not even for `"single-key"`; a producer MUST
tag every owner-reference explicitly. A consumer encountering
`"hierarchical"` or `"rooted-identity"` MUST reject cleanly (treat
the `OwnerRef` as unauthorizable) rather than attempt to interpret
`value` — those tiers are named and reserved, not implemented; only
`"single-key"` carries a working evaluator
(`[owner-authorization-delegated]`).
`VERIFIED: unverified`

**[claim-owner-single]**: `ClaimPayload.owner` MUST be a single
`OwnerRef`. A claim represents accountability: within an
organization, exactly one identity is responsible for a specific
label. This is unchanged by the charter-level set generalization
below (`[charter-owner-set]`) — the two fields share `OwnerRef`'s
shape, not its cardinality.
`VERIFIED: unverified`

**[charter-owner-set]**: `CharterPayload.owner` MUST be a non-empty
set of `OwnerRef` values (`Vec<OwnerRef>`; membership only — order
carries no semantic meaning). A charter declares who is on the team
under this anchor, not who owns one thing: team membership is
naturally plural, and every listed principal has full, equal,
undifferentiated authority — no per-entry roles or restricted
identities exist (there is no concrete use case for a restricted
identity distinct from "not on the team"). A charter (founding or
successor) whose `owner` set is empty is a charter nobody could ever
claim under; `[charter-owner-set-non-empty]` makes rejecting it
explicit.
`VERIFIED: unverified`

**[charter-owner-set-non-empty]**: A charter transaction (founding or
successor) whose `owner` set would contain zero entries MUST be
rejected. This is a direct consequence of `[charter-owner-set]`'s
non-empty requirement, restated as its own constraint because it is
exactly the failure mode a removal-only transfer path
(`[charter-succession-linear]`) could otherwise reach silently at
implementation time.
`VERIFIED: unverified`

**[owner-authorization-delegated]** _(amended 2026-07-14 — owner_kind,
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

**[publish-mode]**: The OPTIONAL root-level `mode` field of a
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
`VERIFIED: unverified`

**[content-hash-is-tree-digest]**: The OPTIONAL root-level
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
`VERIFIED: unverified`

**[content-hash-algorithm]**: Where present, `content_hash` MUST be
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
`VERIFIED: unverified`

**[content-hash-obligation]**: `content_hash`'s obligation is
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
`VERIFIED: unverified`

**[amendment-field-classification]**: Every `PublishPayload` field is
classified into exactly one of three kinds. The classification is a
fixed protocol table, not a per-message choice:

1. **Identity (write-once).** `label`, `version`, `dig`, `src`, `path`,
   `content_hash`. MUST be set only by the **base publish payload** —
   the tag that targets the atom commit directly, the first tag in a
   publish's update chain. Every other tag in the chain carries an
   **amendment payload** whose shape has no slot for an identity-class
   field at all — the field is structurally absent, not present and
   checked equal to the base tag's value. `[tag-chain-semantic-
   immutable]` (git-storage-format.md) is the storage-side statement
   of this rule; this constraint is its protocol-level source.
2. **Overwrite (latest-wins).** `mode`, `meta`, the signing identity
   (`tmb` and the envelope `key`), `now`. MAY be set by the base
   payload or by any amendment payload. The current value of an
   overwrite-class field is the value set by the last tag, walking the
   chain base to tip, that set it; a tag that omits an overwrite-class
   field leaves an earlier tag's value for it unchanged.
3. **Append (accumulate, never overwrite).** Fact-kind entries
   (`[fact-kind-table]`, below — `meta` is NOT the carrier for these).
   MAY be set by the base payload or by any amendment payload. The
   current state of the append-class fields is the union of every
   entry seen walking the whole chain; a later tag MUST NOT cause an
   earlier append-class entry to be removed or replaced.

A conforming resolver walks a publish's update chain exactly once,
base to tip, and produces one accumulated view: identity fields read
from the base tag only; the latest-setter value of every overwrite
field; the full accumulated set of every append entry. This is the
sole resolution algorithm for a publish chain — pairwise inter-tag
comparison MUST NOT be used to enforce identity-field immutability;
class 1's payload shape enforces it structurally, so there is nothing
to compare.
`VERIFIED: unverified`

**[fact-kind-table]**: An append-class entry (`[amendment-field-
classification]` class 3) MUST carry one of the following fact kinds.
The species column is atom-model.md §4's own partition: a **derived**
fact is produced only by building or independently inspecting the
published artifact; an **asserted** fact is a keyed, post-hoc
assertion no build produces.

| Kind                 | Species  | Retired `meta` field  |
| :------------------- | :------- | :--------------------- |
| `build-record`        | derived  | `meta.build-hash`       |
| `interface-manifest`  | derived  | —                        |
| `observation-record`  | derived  | —                        |
| `trial-attestation`   | derived  | —                        |
| `advisory`             | asserted | `meta.security`          |
| `deprecated`           | asserted | `meta.deprecated`        |
| `yanked`               | asserted | `meta.broken`            |
| `superseded-by`        | asserted | `meta.superseded-by`     |
| `runtime-requires`     | asserted | —                        |

A fact is simply an append-class amendment tag: `typ` remains the
literal `"atom/publish"` (`[publish-typ]`) — a fact does NOT introduce
a distinct `typ` value — carrying an append-class entry naming one of
the kinds above, in place of or alongside an overwrite-class field.
This is why no separate ref family or walk-compatibility branch is
needed for facts: a resolver never special-cases by `typ`; it
classifies every field it encounters by kind
(`[amendment-field-classification]`), uniformly, for every tag in the
chain.
`VERIFIED: unverified`

**[fact-lifecycle-owner-gated]**: For an append-class entry of a
lifecycle fact kind (`yanked`, `deprecated`, `superseded-by`;
`[fact-kind-table]`), the `assertor`-role authorization that
`[trust-role-authorization]` (trust-model.md) grants an anchored
`assertor` signer is NECESSARY BUT NOT SUFFICIENT: a conforming
reader's acceptance procedure MUST additionally require the signer to
match the `owner` `SignerRef` at the entry's chain position
(`[trust-owner-selector]`, trust-model.md) before assigning verdict
`fact` (`[trust-acceptance-procedure]`, trust-model.md) to a
lifecycle-kind entry. A lifecycle-kind entry from an `assertor`-
anchored signer that does not match `owner` MUST receive verdict
`evidence` — a role-adequate signer failing this constraint's
additional owner-match gate, falling back to the same `evidence`
verdict `[trust-role-authorization]` assigns a role-inadequate signer,
though for a distinct reason (owner mismatch, not role mismatch); this
constraint introduces no new verdict, no new `SignerRole`, and no
chain-level
rejection (`[trust-signer-relative]`, trust-model.md) — the gate is
evaluated entirely in the consumer's policy layer, using machinery
`[trust-owner-selector]` already defines. Non-lifecycle asserted kinds
(`advisory`, `runtime-requires`) and every derived kind are governed
by `[trust-role-authorization]` alone, with no additional owner
requirement.
`VERIFIED: unverified`

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

**[anchor-immutable]** _(amended 2026-07-08)_: An anchor MUST NOT
change over the lifetime of its atom-set: it is `czd(charter₀)`
permanently; succession never alters it (`[charter-succession]`).
`VERIFIED: machine (TLC)`

**[anchor-content-addressed]** _(amended 2026-07-08)_: An anchor MUST
be the coz digest of the signed founding charter — content-addressed
over an _owned_ payload, never derived from unowned or mutable source
metadata (the pre-charter genesis-hash derivation is retired).
`VERIFIED: machine (Alloy)`

**[anchor-resolvable]** _(supersedes [anchor-discoverable],
2026-07-08)_: Given a source, any party MUST be able to enumerate
candidate charters and verify a given anchor against its founding
charter without trusting the publisher. _Selecting_ among candidate
anchors is a recorded consumer trust decision (in locks and URIs), not
a derivation — the anchor is given, then verified; it is no longer
derivable from source content alone.
`VERIFIED: review-residue (procedural: enumeration + local verification capability — see Verification note)`

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
  atom-set's anchor. **Bootstrap gate**: if the source already carries
  claims predating any charter, the founding charter MUST be authorized
  by the owner of the earliest such claim — chartering over a live,
  claimed set is a migration act reserved to its incumbent, not a race
  open to strangers. A virgin source is first-to-charter by design
  (that is `[charter-fork-distinction]` working).
- **PRE** (successor): `prior` MUST be the czd of a valid charter in
  this set's succession chain; the signing key MUST be authorized by
  membership in that charter's `owner` set
  (`[owner-authorization-delegated]`'s set composition rule); `now`
  MUST exceed the prior charter's `now`.
- **POST**: The charter message is stored in the source's atom refs,
  enumerable by consumers and retrievable by its czd.
  `VERIFIED: unverified (pending implementation)`
  `RESIDUE: Phase 1 -- construction/signature correctness is tested (atom/atom-id/tests/charter/construction.rs), but the PRE bootstrap-gate authorization check has no implementation to call: bootstrap_gate.rs's own red test states "no bootstrap-gate authorization check exists yet"; the POST storage-in-atom-refs requirement has no atom-git charter storage implementation either`

**[claim-transition]**: An atom MAY be claimed by constructing a
`ClaimPayload`, signing it with a Coz-compatible key, and producing
a `CozMessage` that includes the public key.

- **PRE**: `anchor` MUST equal the czd of the set's founding charter,
  with a verifiable succession chain to the effective charter; the
  signing key MUST be authorized by membership in the effective
  charter's `owner` set (`[claim-charter-authorization]`); `src`
  MUST descend from the effective charter's `src`
  (`[charter-ancestry]`).
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
  `RESIDUE: Phase 1 -- construction.rs::claim_replacement_transactions_verify tests replacement shape (prior linkage, governance marking, distinct signing keys) and signature validity, but its own module docstring is explicit: "construction correctness only -- no ... authorization validation runs anywhere in this corpus; that is Phase 1". No storage backend exists either.`

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
  `RESIDUE: Phase 1/2 -- ClaimPayload (atom/atom-id/src/lib.rs) has a fixed field set with no "meta" field or unknown-field-preservation mechanism; default serde deserialize silently drops fields not in the struct rather than preserving them, so this constraint is not yet satisfied by the landed type, let alone verified`

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

**[format-version-discriminator]**: Any new persisted wire format — a
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
  `VERIFIED: unverified`

**[format-unknown-field-tolerance]**: Any persisted format that admits
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
  `VERIFIED: unverified`

**[format-reserved-names]**: The following namespace, field, and kind
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
| 3    | Charter chain valid              | each successor's `prior` + signer authorized by membership in prior `owner` set |
| 4    | Claim signature valid            | `claim.pay`, `claim.sig`, `claim.key`                                |
| 5    | Publish signature valid          | `publish.pay`, `publish.sig`, key                                    |
| 6    | Key thumbprints match            | `tmb(x.key) == x.pay.tmb` for charter/claim                          |
| 7    | Claim chains to charter          | `claim.anchor == czd(charter₀)`                                      |
| 8    | Publish chains to claim          | `publish.claim == czd(claim)` (current claim per replacement chain)  |
| 9    | Temporal ordering                | `charter.now < claim.now < publish.now`                              |
| 10   | Claim signer authorized          | `claim.tmb` authorized by membership in effective charter `owner` set |
| 11   | Publish signer authorized        | `publish.tmb` authorized by `claim.owner` (single-valued, per its `kind`) |
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
  configs (fork: 31,593 states; distinct-anchor: 27,817). Unchanged.
- `docs/specs/tla/AtomCharter.tla` — charter/authorization/anchor layer,
  two configs (succession: 1,409,951 states, depth 6; rotation: 480,096
  states). All 9 safety invariants plus the `MonotonicHead` and
  `ForkFailClosed` temporal properties pass, 0 errors. Non-vacuity is
  established by 5 reachability witnesses and an 8-mutant guard battery
  (each disabled guard yields the expected counterexample).

**Alloy model**: `docs/specs/alloy/atom_structure.als` verified by Alloy
Analyzer 5.1.0 (pinned nixpkgs) at scope 4, headless via `SimpleCLI`
(SAT4J). All 8 structural assertions pass (UNSAT) — the 5 original plus
`anchor_content_addressed`, `claim_chains_charter`, `charter_fork_distinction`.
Both `fork_scenario` and the charter-rooted `charter_rooted_fork` are
satisfiable (SAT), confirming the charter facts are consistent.

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
  schema literal, identical in kind to `[claim-typ]`/`[publish-typ]` (both
  `VERIFIED: rustc`). In a structural model the message type is already
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
> non-empty `Vec<OwnerRef>` (`[charter-owner-set]`) and reworks
> `[charter-succession]`/`[charter-succession-linear]`'s authorization
> semantics to set membership. `AtomCharter.tla` still encodes the
> pre-amendment single-owner model (`CharterSucceed`'s rotation guard,
> `CharterTransfer`'s transfer dichotomy assume exactly one owner per
> charter) and has not been re-run against set-valued ownership. The
> `machine (TLC)` **pass** recorded below for `[charter-anchor]`,
> `[claim-charter-authorization]`, `[charter-ancestry]`,
> `[charter-succession]`, `[charter-succession-linear]`,
> `[chain-monotonicity]`, `[claim-replacement-authority]`, and
> `[anchor-immutable]` therefore reflects the single-owner model, not
> this amendment's set semantics. An `AtomCharter.tla` rework covering
> add/remove-membership operations is registered follow-on work, not
> yet landed; until it lands and re-discharges these eight rows, their
> **pass** status is inherited from the prior model, not fresh
> coverage of the set case. `atom_structure.als`'s structural
> assertions (`ownership_independence` and others) are unaffected —
> that model carries no `owner` field at all, so `[owner-abstract]`'s
> `machine (Alloy)` status stands unchanged.

| Constraint                    | Method           | Result   | Detail                                                                    | Phase |
| :---------------------------- | :--------------- | :------- | :------------------------------------------------------------------------ | :---- |
| identity-content-addressed    | machine (Alloy)  | **pass** | Alloy `identity_content_addressed`                                        | —     |
| identity-stability            | machine (TLC)    | **pass** | TLA+ `IdentityStability` — 2 configs                                      | —     |
| owner-abstract                | machine (Alloy)  | **pass** | Alloy `ownership_independence`                                            | —     |
| owner-compatibility           | machine (Alloy)  | **pass** | Alloy `ownership_independence`                                            | —     |
| owner-authorization-delegated | integration-test | pending  | Signing key auth varies by identity system; set composition adds a disjunction over per-value checks | 4     |
| owner-kind-required           | unit-test        | pending  | `kind` required, no default; `hierarchical`/`rooted-identity` rejected cleanly | 4     |
| claim-owner-single             | rustc            | pending  | `ClaimPayload.owner: OwnerRef` (not a collection)                          | 4     |
| charter-owner-set             | unit-test        | pending  | `CharterPayload.owner: Vec<OwnerRef>`; empty set rejected                  | 4     |
| charter-owner-set-non-empty   | unit-test        | pending  | Founding/successor charter with empty resulting `owner` set rejected      | 4     |
| symmetric-payloads            | rustc            | **pass** | Both structs have `anchor` + `label`                                      | 1     |
| publish-chains-claim          | machine (TLC)    | **pass** | TLA+ `PublishChainsClaim` — 2 configs                                     | —     |
| claim-typ                     | rustc            | **pass** | `TYP_CLAIM` const = `"atom/claim"`                                        | 1     |
| publish-typ                   | rustc            | **pass** | `TYP_PUBLISH` const = `"atom/publish"`                                    | 1     |
| sig-over-pay                  | unit-test        | **pass** | sign→verify roundtrip in atom-id tests                                    | 1     |
| dig-is-atom-snapshot          | unit-test        | pending  | Snapshot hash matches `dig` field                                         | 4     |
| src-is-source-revision        | integration-test | pending  | Git revision hash matches `src` field                                     | 4     |
| content-hash-is-tree-digest   | unit-test        | pending  | Present `content_hash` is BLAKE3 over content entries, publisher-signed   | 4     |
| content-hash-algorithm        | unit-test        | pending  | Two independent inputs of same content → byte-identical digest            | 4     |
| content-hash-obligation       | integration-test | pending  | Optional schema; present ⇒ consumer verifies or rejects; SHOULD on weak backend | 4 |
| amendment-field-classification | unit-test       | pending  | Amendment payload has no identity-field slot; base tag is sole source     | 4     |
| fact-kind-table               | unit-test        | pending  | Fact entries round-trip; kind name drawn from the reserved table          | 4     |
| fact-lifecycle-owner-gated    | policy-test       | pending  | Lifecycle fact from non-owner assertor → evidence, never fact             | 4     |
| path-is-subdir                | rustc            | **pass** | `path` field type constrains to subdir                                    | 1     |
| rawversion-opaque             | rustc            | **pass** | Newtype, no `Deref`/`AsRef`/`Into`                                        | 1     |
| claim-key-required            | unit-test        | **pass** | CozMessage key — tested in claim roundtrip                                | 1     |
| publish-key-optional          | unit-test        | **pass** | CozMessage key — optional per Coz format                                  | 1     |
| crypto-layer-separation       | cargo-dep        | pending  | atom-core Cargo.toml has no coz-rs                                        | 3     |
| crypto-via-coz                | cargo-dep        | **pass** | atom-id Cargo.toml depends on coz-rs                                      | 1     |
| key-management-deferred       | cargo-dep        | pending  | No key storage crate in atom workspace                                    | 3     |
| claim-transition              | unit-test        | **pass** | `verify_claim_roundtrip` sign→verify                                      | 1     |
| publish-transition            | unit-test        | **pass** | `verify_publish_roundtrip` sign→verify                                    | 1     |
| session-ordering              | machine (TLC)    | **pass** | TLA+ `SessionOrdering` — 2 configs                                        | —     |
| no-unclaimed-publish          | machine (TLC)    | **pass** | TLA+ `NoUnclaimedPublish` — 2 configs                                     | —     |
| no-duplicate-version          | machine (TLC)    | **pass** | TLA+ `NoDuplicateVersion` — 2 configs                                     | —     |
| no-cross-layer-crypto         | cargo-dep        | pending  | atom-core has zero crypto deps                                            | 3     |
| no-backdated-publish          | machine (TLC)    | **pass** | TLA+ `NoBackdatedPublish` — 2 configs                                     | —     |
| verification-local            | integration-test | pending  | Pipeline steps 1–13 offline                                               | 4     |
| verification-provenance       | integration-test | pending  | Pipeline steps 14–18 with source access                                   | 4     |
| atom-snapshot-reproducible    | unit-test        | pending  | Same inputs → same snapshot hash                                          | 4     |
| ingest-preserves-identity     | machine (Alloy)  | **pass** | Alloy `ingest_preserves_identity`                                         | —     |
| backend-agnostic-protocol     | rustc            | pending  | Trait sigs use only associated types                                      | 3     |
| charter-typ                   | review-residue   | **pass** | Schema literal; rustc at impl (cf. claim-typ)                             | —     |
| charter-anchor                | machine (TLC)    | **pass** | AtomCharter `AnchorIsFoundingCzd`/`FoundingUnique`                        | —     |
| claim-chains-charter          | machine (Alloy)  | **pass** | Alloy `claim_chains_charter`                                              | —     |
| claim-charter-authorization   | machine (TLC)    | **pass** | AtomCharter `ClaimAuthorized`                                             | —     |
| claim-replacement-authority   | machine (TLC)    | **pass** | AtomCharter `ReplacementAuthority`                                        | —     |
| charter-ancestry              | machine (TLC)    | **pass** | AtomCharter `ClaimAncestry`                                               | —     |
| charter-succession            | machine (TLC)    | **pass** | AtomCharter `SuccessionAuthorized`                                        | —     |
| charter-succession-linear     | machine (TLC)    | **pass** | AtomCharter `TransferDualSigned`/`ForkFailClosed`                         | —     |
| chain-monotonicity            | machine (TLC)    | **pass** | AtomCharter `MonotonicHead` property                                      | —     |
| charter-fork-distinction      | machine (Alloy)  | **pass** | Alloy `charter_fork_distinction`                                          | —     |
| anchor-immutable              | machine (TLC)    | **pass** | AtomCharter `AnchorIsFoundingCzd` — succession preserves anchor (amended) | —     |
| anchor-content-addressed      | machine (Alloy)  | **pass** | Alloy `anchor_content_addressed` (amended)                                | —     |
| anchor-resolvable             | review-residue   | **pass** | Procedural; enumeration + local verify (amended)                          | —     |
| manifest-minimal              | machine (Alloy)  | **pass** | Alloy `manifest_properties` fact                                          | —     |
| backend-bit-perfect           | integration-test | pending  | CozMessage bytes unchanged after store                                    | 4     |
| atomid-per-source-unique      | machine (TLC)    | **pass** | TLA+ `AtomIdPerSourceUnique` — 2 configs                                  | —     |
| publish-claim-coherence       | machine (TLC)    | **pass** | TLA+ `PublishClaimCoherence` — 2 configs                                  | —     |
| atom-detached                 | integration-test | pending  | Atom subtree has no parent refs                                           | 4     |
| uri-not-metadata              | rustc            | **pass** | URI type absent from payload structs                                      | 1     |
| trait-signature-pure          | rustc            | pending  | No backend types in trait signatures                                      | 3     |
| publish-payload-extensible    | unit-test        | pending  | Extra fields in payload round-trip                                        | 3     |
| publish-mode                  | unit-test        | pending  | Absent mode reads witnessed; transition = chain append, never new version | 3     |
| fs-source-contract            | integration-test | pending  | FsSource discover+resolve, no claim/pub                                   | 4     |

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
  (`Anchor := czd(charter₀)`) is now fixed and backend-agnostic; only
  how a backend interprets the charter's `src` field remains
  backend-specific.
- **Fact-append mechanics**: atom-model.md §4 states the governing
  laws for post-publish facts on the metadata chain — builder≠owner
  signer authorization for appended facts, and the fact-append
  carve-out (routine fact appends must not present as
  ownership-relevant events). The concrete fact-kind encoding and
  authorization mechanism are now defined —
  `[amendment-field-classification]`, `[fact-kind-table]`, and
  `[fact-lifecycle-owner-gated]` above — closing atom-sad.md §9 gap 5
  at the specification level. The Rust implementation of this
  mechanism is separate, not-yet-landed work.
