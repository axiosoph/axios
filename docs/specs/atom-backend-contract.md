# SPEC: Atom Backend Contract

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

**Problem Domain:** The atom protocol is backend-agnostic by design:
its transactions (charter, claim, publish) are coz objects, its
identity is the abstract `(anchor, label)` pair, and its verification
pipeline is local computation over signed payloads
([atom-transactions](atom-transactions.md)). Git is the reference
backend, specified concretely in
[git-storage-format](git-storage-format.md). What has never been
stated is the contract between them: **which formal properties of git
does the protocol actually rely on, such that any other
content-addressed versioned store (a pijul-, jj-, or fossil-class
system) could host atoms by providing the same properties?** This
specification states that contract. It axiomatizes the **intent
store** — the substrate's second storage primitive, deliberately
distinct from the artifact store
([Storage Model](../models/storage-model.md) §5: "the two stores share
the content-addressing idea, not a namespace, an implementation, or an
identifier type") — the way the storage model axiomatizes the first.

**Model Reference:**
[atom-model.md](../models/atom-model.md) — the protocol plane this
contract carries (§5 anchoring law, §4 metadata chain, §9 naming this
spec as companion);
[publishing-stack-layers.md](../models/publishing-stack-layers.md) —
§2.1–2.3 (the coalgebras a backend implements);
[storage-model.md](../models/storage-model.md) — the artifact-side
axioms this contract mirrors.

**Parent Specification:**
[atom-transactions.md](atom-transactions.md) — this contract is the
formal elaboration of its §Backend requirements ("A backend MUST:
… A backend MUST NOT: …"). All protocol constraints apply
unconditionally.
[git-storage-format.md](git-storage-format.md) is this contract's
**reference instantiation**: Appendix A maps every obligation here to
the git constraint that discharges it, and names the rows nothing
discharges yet.

**Criticality Tier:** High — supply chain protocol substrate. The two
identity-conflation defects that landed during implementation (both
fixed; four code sites, one root cause — a protocol content-address
compared with or fabricated from a backend object id) are exactly the
class this contract's seam law makes unwritable. The existing formal
models could not express that bug class at all: they carry no
backend-identifier sort (see §Verification).

## Overview

A backend is not the protocol. The protocol's trust chain is carried
entirely by signed coz objects and verified by local computation; the
backend contributes **carriage, ancestry, and names** — it stores
bytes, witnesses history, and maps mutable names to immutable state.
This contract states the minimum lawful behavior of those three
services, as one typed signature:

```
BACKEND = ⟨ Store, Rev/⊑, Refs ⟩  over sorts  OID, RefName
          + the seam law binding sort OID against sort Czd
```

- **Store** — immutable Merkle content storage, injective under
  canonical serialization. This is the intent-store analogue of the
  artifact store's A1–A3 / P10
  ([Storage Model](../models/storage-model.md) §2, §6).
- **Rev/⊑** — a Merkle DAG of revisions and, **as a distinct object**,
  the ancestry partial order over them. The protocol's entire
  temporal-security argument (`charter.src ⟶ claim.src ⟶ publish.src`,
  [atom-transactions](atom-transactions.md) `[charter-ancestry]`,
  [git-storage-format](git-storage-format.md) `[temporal-vector]`)
  quantifies over ancestry soundness — a law every document has so far
  assumed and none has stated. It is stated here (P15).
- **Refs** — not merely cheap-to-read names: a per-name
  **linearizable register with compare-and-swap**, plus **multi-key
  atomic commit**. The protocol's write transitions depend on the
  write side ([git-storage-format](git-storage-format.md)
  `[publish-transition-git]` POST's CAS; its Atomicity guidance's
  transactional multi-ref requirement), not only on cheap reads.
- **The seam law** — protocol content-addresses (`Czd`) and backend
  object identifiers (`OID`) are **disjoint sorts**. Exactly the `dig`
  and `src` fields of transaction payloads are OID-sorted; every other
  protocol identity is coz-sorted or opaque. Any comparison of a
  czd/anchor against an OID is ill-typed. This is the atom-side
  counterpart of the storage model's §5 artifact-side statement, and
  the type-level kill for the landed bug class.

What a backend never provides: **trust**. Verification is re-hashing
and signature checking over carried bytes
([atom-transactions](atom-transactions.md) §Verification Pipeline);
a conforming backend makes verification *possible* (bit-perfect
carriage, enumeration) and *efficient* (ancestry queries without
content), never *unnecessary*.

## Constraints

### Sorts and Signature

```
SORT  Czd      -- protocol content-address: coz digest (multihash, signed payloads)
SORT  OID      -- backend object identifier (backend-chosen hash, backend-chosen encoding)
SORT  RefName  -- mutable name in the backend's namespace (e.g. "refs/atom/...")

      Czd ∩ OID = ∅                        -- [backend-seam-typed]

Store : put     : Bytes|Tree → OID          -- immutable ingestion
        get     : OID ⇀ Bytes|Tree          -- retrieval
Rev   : parents : OID → Seq<OID>            -- the revision Merkle DAG
⊑     : OID × OID → Bool                    -- ancestry: reflexive-transitive
                                            -- closure of parents (derived,
                                            -- NOT an independent input)
Refs  : read    : RefName ⇀ OID
        cas     : RefName × Option<OID> × OID → Bool     -- linearizable per name
        txn     : Set<(RefName, Option<OID>, OID)> → Bool -- all-or-nothing
```

The Merkle DAG (`parents`, an object-level structure) and the ancestry
relation (`⊑`, an order) are **distinct objects**: the contract binds
the second to the first. A backend that stores a well-formed DAG but
answers ancestry queries from an unrelated index does not conform.

### Invariants

**[backend-store-immutable]**: `get(put(x)) = x`, and a stored object
MUST NOT be mutable under its OID: `get` is a partial function of the
OID alone, stable across time and replicas. (The artifact-store A1/A2
analogue; every protocol claim of the form "the ref moved but the old
object persists" rests on it.)
`VERIFIED: unverified`

**[backend-store-injective]**: The backend's content serialization
MUST be canonical: one content value, one byte serialization, one OID;
two distinct content values MUST NOT share an OID (up to collision
resistance of the backend hash). For atom content trees this is
load-bearing for `[snapshot-deterministic]`-class reproducibility:
any party constructing the same atom content MUST obtain the same
carrier identity. This is P16, the intent-store analogue of the
artifact store's P10.
`VERIFIED: unverified`

**[backend-ancestry-sound]**: The ancestry relation `⊑` MUST be sound
with respect to the backend's content-addressing: `a ⊑ b` MUST hold
iff `a` is reachable from `b` through `parents`, and a revision
identifier MUST commit to its entire ancestry — `parents` participates
in the preimage of every revision OID, transitively, so that asserting
false ancestry requires a hash collision. A backend whose revision
identifiers do not commit to history (e.g. one that stores parent
links in mutable side tables) CANNOT host atoms, whatever its other
properties. This is P15; the protocol properties that quantify over it
are `[charter-ancestry]`, `[temporal-vector]`, `[no-backdated-src]`,
and the claim that "the charter's `src` transitively pins the
genesis."
`VERIFIED: unverified`

**[backend-ancestry-queryable]**: A conforming backend MUST support
ancestry queries (`a ⊑ b`) without transferring or materializing full
content — revision metadata alone suffices (git discharges this with
treeless `tree:0` fetching, `[temporal-vector]`). Provenance
verification is REQUIRED to be cheap
([atom-transactions](atom-transactions.md) §Provenance Verification:
"commit-graph walk only"); a backend where ancestry costs content
download fails that requirement structurally.
`VERIFIED: unverified`

**[backend-refs-linearizable]**: Each ref MUST behave as a
linearizable register per name: reads observe the latest committed
write, and `cas(name, expected, new)` MUST atomically compare and
swap. Protocol write transitions REQUIRE it — e.g. the
claim-not-replaced-since-payload-construction check at publish time
([git-storage-format](git-storage-format.md)
`[publish-transition-git]` POST).
`VERIFIED: unverified`

**[backend-refs-atomic-multi]**: The backend MUST support atomic
multi-ref transactions: a protocol transition that writes several
names (claim ref + protective src ref; version ref + src ref;
ingestion of many versions with claims) MUST be all-or-nothing, both
locally and on any push/replication path. Torn states are forbidden
states of every transition that writes more than one name.
(Reference instantiation: `gix::refs::Transaction` + atomic push,
[git-storage-format](git-storage-format.md) §Implementation Guidance,
Atomicity — promoted here from guidance to law.)
`VERIFIED: unverified`

**[backend-seam-typed]**: `Czd` and `OID` MUST be disjoint sorts, in
the specification and in implementations (distinct types, no implicit
conversion). For any stored `CozMessage` `m`, `czd(m)` is computed by
the coz layer over `m`'s payload bytes and MUST be independent of
`oid(carrier(m))` — the backend object that happens to carry `m`.
Exactly the `dig` and `src` fields of transaction payloads (and the
carrier-level `src` extra header of an atom snapshot) are OID-sorted;
`anchor`, `czd`, `publish_czd`, `claim`, `tmb`, `owner` are NEVER
OID-sorted. Any comparison, assignment, or fabrication across the two
sorts is ill-typed and MUST be rejected at the type level where the
implementation language permits. (The two landed identity-conflation
defects were both violations of this law; see §Verification for why
the existing models could not catch them.)
`VERIFIED: unverified`

**[backend-carriage-bit-perfect]**: The backend MUST carry protocol
payloads byte-exactly: no re-serialization, reformatting, or
normalization of `CozMessage` content, ever — verification is
re-hashing, so a single altered byte severs the trust chain. This
generalizes [git-storage-format](git-storage-format.md)
`[coz-bit-perfect]` and satisfies
[atom-transactions](atom-transactions.md) `[backend-bit-perfect]` at
the contract level: it binds every backend, not the git encoding.
`VERIFIED: unverified`

**[backend-chain-append]**: The backend MUST realize each atom's
transaction and fact chains as **append-only** structures: appending a
new chain element (claim replacement, publish update, fact append —
[atom-model](../models/atom-model.md) §4) MUST NOT destroy, alter, or
unreference prior elements. Old chain state persists as the immutable
audit trail ([git-storage-format](git-storage-format.md)
`[tag-chain-immutable]`, generalized). Monotone history is
load-bearing for the plane's laws (atom-model §6, "monotone history is
the structural cost").
`VERIFIED: unverified`

**[backend-enumeration]**: Given a source, a zero-trust consumer MUST
be able to enumerate: candidate charters
([atom-transactions](atom-transactions.md) `[anchor-resolvable]`),
the labels claimed there, the versions published per label, and each
chain's current tip — without downloading content and without trusting
the publisher (enumeration yields candidates; verification is local).
Version discovery MUST be object-free where the transport allows
(git: `ls-refs` advertisement, atom-sad §6.8). The backend provides
carriage and enumeration, never trust.
`VERIFIED: unverified`

**[backend-liveness-protection]**: State reachable from protocol refs
MUST be protected from the backend's garbage collection: an object the
protocol can still name (a locked `publish_czd`'s chain, a claim
referenced by surviving publishes, a `src` revision underpinning
ancestry checks) MUST NOT be collected while so named. Where the
backend's native GC cannot see protocol reachability, the
instantiation MUST write protective names (git: `refs/atom/src/{oid}`,
`[store-claim-ref]`'s GC-protection purpose).
`VERIFIED: unverified`

**[backend-hash-strength]**: The `dig` and `src` protocol fields
inherit the **backend's** hash strength, not the coz layer's: a
backend whose object hash is not collision-resistant (git's default
SHA-1) makes `dig` forgeable in principle, independent of every
signature in the chain. A conforming backend MUST document its object
hash and its collision-resistance status. Where the hash is weak,
implementations SHOULD re-anchor: record the ingested source's
artifact-store digest (blake3) in atom metadata at the ingest seam —
the cheap integrity upgrade the storage model already notes
([Storage Model](../models/storage-model.md) §5). The anchor and all
czd-sorted identities are unaffected by construction
(`[anchor-hash-agile]`).
`VERIFIED: unverified`

### Behavioral Properties

**[backend-substitutable]**: Two conforming backends carrying the same
protocol state MUST be interchangeable to every consumer: the
coalgebras a backend implements (`AtomSource`, `AtomRegistry`,
`AtomStore` — [publishing-stack-layers](../models/publishing-stack-layers.md)
§2.1–2.3) are the observation surface, and bisimilar backends are
equal. This is what makes the contract a portability statement rather
than documentation: a non-git backend that discharges every invariant
above hosts the same atoms with the same trust semantics.

- **Type**: Safety
  `VERIFIED: unverified`

**[backend-verification-carried]**: A conforming backend MUST make the
full local verification pipeline
([atom-transactions](atom-transactions.md) §Local Verification, steps
1–13) executable from carried state alone — every transaction of every
chain retrievable bit-perfect, every ancestry premise queryable — with
zero writes and zero further network round-trips once state is local.

- **Type**: Safety
  `VERIFIED: unverified`

## Proof Obligations

Continuing the substrate-wide P-numbering (P1–P11 in the substrate
models; P12–P14 in [atom-model](../models/atom-model.md) §10):

- **P15 — ancestry soundness.** `[backend-ancestry-sound]` holds in
  the instantiation: the backend's revision identifiers commit to
  their full ancestry, and the implementation answers `⊑` only from
  hash-committed structure. Per-backend argument obligation (for git:
  commit objects embed parent OIDs in the hashed preimage — an audit
  note, not new mathematics), plus the model row in the seam Alloy
  model below.
- **P16 — intent-store canonical injectivity.** The P10 analogue:
  atom content trees have one canonical serialization, entry order
  included; distinct values never share an OID; snapshot construction
  is deterministic across parties. Audit obligation with a checkable
  inventory (git: canonical tree sorting + the
  `[snapshot-deterministic]` fixture battery).

## Verification

**The owed evaluator, named precisely.** The seam law's machine check
is an Alloy model (planned home: `docs/specs/alloy/atom_backend_seam.als`)
carrying a **genuine `OID` sort disjoint from `Czd`**, modeling: czd
computation independent of carrier OID, the dig/src-only OID-typing of
payload fields, and ancestry as reachability over hash-committed
parent links (P15's abstract row). This model is REQUIRED work, not
optional hardening, because the existing models are structurally blind
to the bug class this contract exists to kill: `atom_structure.als`
and the TLA modules model czd as a perfect injective tuple with **no
backend-identifier sort at all** — they cannot even express "a czd was
compared to an OID," which is precisely why two real instances of that
comparison survived model-checked development. Until the model lands,
`[backend-seam-typed]`'s discharge is the rustc row below plus review.

| Constraint                   | Method           | Result  | Detail                                                              |
| :--------------------------- | :--------------- | :------ | :------------------------------------------------------------------ |
| backend-store-immutable      | integration-test | pending | store→get round-trip; mutation attempts fail                        |
| backend-store-injective      | unit-test (P16)  | pending | canonical serialization audit + fixture battery                     |
| backend-ancestry-sound       | review + Alloy (P15) | pending | per-backend Merkle argument; abstract row in seam model         |
| backend-ancestry-queryable   | integration-test | pending | ancestry check with content transfer disabled                       |
| backend-refs-linearizable    | integration-test | pending | concurrent CAS battery on one ref                                   |
| backend-refs-atomic-multi    | integration-test | pending | multi-ref txn: crash/interrupt yields all-or-nothing                |
| backend-seam-typed           | rustc + machine (Alloy) | pending | disjoint newtypes, no cross-construction outside dig/src sites; seam model |
| backend-carriage-bit-perfect | integration-test | pending | store → retrieve → byte-compare (extends coz-bit-perfect)           |
| backend-chain-append         | integration-test | pending | append preserves prior chain objects and their retrievability       |
| backend-enumeration          | integration-test | pending | charters/labels/versions/tips enumerable object-free                |
| backend-liveness-protection  | integration-test | pending | GC pass collects nothing protocol-reachable                         |
| backend-hash-strength        | review-residue   | pending | documentation obligation + SHOULD-grade re-anchor hardening         |
| backend-substitutable        | integration-test | pending | bisimulation battery across two conforming backends                 |
| backend-verification-carried | integration-test | pending | pipeline steps 1–13 offline against carried state                   |

## Appendix A: The Git Instantiation

Obligation-by-obligation discharge map against
[git-storage-format](git-storage-format.md) (and, where noted, the
protocol spec). **GAP rows are honest**: they name obligations the git
spec relies on today without stating — each is an amendment item
(Appendix B).

| Contract obligation          | Git discharge point                                                                 | Status |
| :--------------------------- | :----------------------------------------------------------------------------------- | :----- |
| backend-store-immutable      | git object database semantics; `[tag-chain-immutable]` relies on it                 | **GAP (implicit)** — relied on throughout, stated nowhere |
| backend-store-injective      | `[snapshot-deterministic]` + §Implementation Guidance "Tree construction" (canonical byte-order sorting) | Stated; verification pending |
| backend-ancestry-sound       | — (used by `[temporal-vector]`, `[no-backdated-src]`, `[charter-ancestry]`; never stated) | **GAP** — the contract's founding finding |
| backend-ancestry-queryable   | `[temporal-vector]` per-publish verification: treeless `tree:0` fetch               | Discharged |
| backend-refs-linearizable    | `[publish-transition-git]` POST: CAS on the claim ref (SHOULD, one site)             | **PARTIAL** — one SHOULD-grade site; general law unstated |
| backend-refs-atomic-multi    | §Implementation Guidance, Atomicity: `gix::refs::Transaction`, atomic push           | **PARTIAL** — guidance, not a tagged constraint |
| backend-seam-typed           | `[anchor-hash-agile]` (anchor is not an ObjectId) is one instance                    | **GAP** — no general law; the landed bug class lived here |
| backend-carriage-bit-perfect | `[coz-bit-perfect]`                                                                  | Discharged |
| backend-chain-append         | `[tag-chain-immutable]`, `[claim-detached]` chains, `[publish-update-transition]`    | Discharged for claim/publish chains; **GAP** for charter encoding (git spec Open Questions #6) |
| backend-enumeration          | atom-sad §6.8 (object-free `ls-refs` discovery), `[ingestion-portable]`, `[anchor-resolvable]` (protocol) | Discharged except charter enumeration (Open Questions #6) |
| backend-liveness-protection  | `refs/atom/src/{oid}` protective refs; `[store-claim-ref]` GC protection             | Discharged |
| backend-hash-strength        | `[anchor-hash-agile]` covers the czd side only                                       | **GAP** — dig/src SHA-1 inheritance unacknowledged; re-anchor hardening unregistered |

## Appendix B: Doc Amendments This Contract Obligates

Recorded for the reconciliation sweep; none performed here:

- **git-storage-format.md**: a header note declaring the spec this
  contract's reference instantiation; state `[backend-store-immutable]`
  and `[backend-ancestry-sound]`'s git argument (parent OIDs are in
  the hashed commit preimage) explicitly; promote the Atomicity
  implementation guidance to a tagged constraint discharging
  `[backend-refs-atomic-multi]`, and generalize the publish-time CAS
  from one SHOULD site to the `[backend-refs-linearizable]` law; state
  the seam law's git instance (exactly `dig`/`src` and the snapshot
  `src` header are ObjectId-valued; every other protocol identity is
  never an ObjectId); register the SHA-1 `dig`/`src` inheritance and
  the blake3 re-anchor hardening (`[backend-hash-strength]`); charter
  encoding remains Open Questions #6, now doubly registered (chain +
  enumeration rows).
- **atom-transactions.md §Backend**: cite this contract as the formal
  elaboration of the backend MUST/MUST NOT lists.
- **atom-model.md §9**: the companion-spec sentence gains a live link
  (this file exists now).
- **docs/specs/alloy/**: the owed `atom_backend_seam.als` model
  (§Verification) — an OID sort disjoint from Czd; checks
  `[backend-seam-typed]` and P15's abstract row.

## Implications

### Scope Boundaries

This contract explicitly does NOT define:

- **The git encoding** — object formats, ref layouts, transitions:
  [git-storage-format](git-storage-format.md), the reference
  instantiation.
- **The protocol** — payloads, signatures, verification, identity:
  [atom-transactions](atom-transactions.md).
- **Trust** — acceptance policy, anchors, signer judgment: the trust
  layer ([atom-model](../models/atom-model.md) §5–§6; Execution Model
  §3.4).
- **The artifact store** — blake3 CAS for build outputs:
  [Storage Model](../models/storage-model.md). The two stores meet at
  exactly one seam (source ingestion), where
  `[backend-hash-strength]`'s re-anchor hardening lives.
- **Key management** — Coz/Cyphr, below the plane.
