# SPEC: Git Storage Format

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

**Problem Domain:** The atom protocol defines abstract behavioral contracts
(AtomSource, AtomRegistry, AtomStore) but explicitly defers anchor
derivation, storage format, and backend-specific machinery. This
specification defines the git backend's contract — how atoms are represented,
stored, discovered, and transferred in git repositories using the gitoxide
(`gix`) library. It fills the "Anchor derivation" scope gap declared in
`atom-transactions.md § Implications`.

**Model Reference:**
[publishing-stack-layers.md](../models/publishing-stack-layers.md) — §1
(olog: Anchor, Atom-set, Revision), §2.1–2.3 (coalgebras: AtomSource,
AtomRegistry, AtomStore), §3.1 (PublishSession), §3.3 (PopulateSession).

**Parent Specification:**
[atom-transactions.md](atom-transactions.md) — this spec is a refinement,
not a replacement. All constraints in atom-transactions.md apply
unconditionally. This specification adds git-specific constraints and
MUST NOT contradict the parent.

**Criticality Tier:** Medium — the git backend is the reference storage
implementation. Correctness failures affect supply chain integrity, but
the protocol-level cryptographic guarantees (in atom-transactions.md) are
the primary defense layer. This spec constrains the storage layer beneath
those guarantees.

## Overview

### Registry vs Store

The git backend serves two distinct roles with different addressing
schemes, ref layouts, and invariants:

| Property       | Registry                        | Store                                     |
| :------------- | :------------------------------ | :---------------------------------------- |
| Purpose        | Source-side: claims + publishes | Consumer-side: aggregation + consumption  |
| Addressing     | Human-readable by label         | Machine-addressed by claim czd            |
| Scope          | Single source repository        | Many sources, many registries             |
| Collision-free | Labels unique per registry      | Claim czd unique globally (crypto-unique) |
| Claim per atom | Exactly one active              | Multiple (from different sources/forks)   |
| Anchor         | One anchor (genesis commit)     | Many anchors (one per ingested source)    |
| Ref prefix     | `refs/atom/{pub,claims/...}/..` | `refs/atom/{d,dev,claims/d}/...`          |

A repository MAY serve as both a registry and a store simultaneously
(e.g., a project that publishes its own atoms and ingests dependencies).

### Git Object Architecture

The git backend uses four categories of git objects:

1. **Genesis commit** — the oldest parentless commit in the
   repository's history (by committer timestamp). Its ObjectId is the
   anchor. Discoverable by walking the commit graph to find all
   parentless commits and selecting the oldest. If multiple parentless
   commits exist (e.g., merged independent histories, orphan branches),
   the oldest is authoritative.

2. **Claim commits** — parentless, detached commits with an empty tree
   whose `message` field contains a claim `CozMessage` (JSON). The
   claim payload's `src` field records the source revision at claim
   time. Like atom commits, claims are standalone objects — they are
   NOT in the main branch history. Their identity is the claim `czd`.

3. **Atom commits** — parentless commits whose tree contains the atom's
   content subtree and whose `src` extra header records the source
   revision. These are the atom snapshots referenced by `dig` in
   publish transactions. Their hash is reproducible given the same
   (tree, src) pair.

4. **Publish tags** — annotated tag objects pointing at atom commits
   (initial publish) or at a previous publish tag (updates). The tag's
   `message` field contains the publish `CozMessage` (JSON). The
   `claim` czd in the publish payload identifies the authorizing claim.
   Update tags form a chain: new → old → ... → atom commit.
   Git's tag-peeling resolves the chain to the underlying atom commit.

### Filesystem Source Ingestion

The abstract `AtomSource` contract and `FsSource` implementation are
protocol-level concerns defined in atom-transactions.md. This section
specifies only the **git-specific ingestion behavior** when a
filesystem source is consumed by a git `AtomStore`.

When a filesystem source is ingested into a git store, the store
MUST create an atom commit from the directory content. Because no
claim or publish transactions exist, the ingested atoms are
**unsigned** — they are available for build consumption but do not
carry provenance guarantees. The store SHOULD mark such atoms as
unsigned to distinguish them from atoms ingested from registries.
No publish tags or claim refs are created for unsigned atoms.
Unsigned dev atoms are stored under `refs/atom/dev/{atom_digest}`
(see Ref Layout).

## Constraints

### Type Declarations

```
TYPE  Anchor         = Vec<u8>                    -- opaque bytes from genesis ObjectId
TYPE  Root           = ObjectId                   -- newtype: genesis commit's ObjectId
TYPE  ClaimCommit    = ObjectId                   -- commit with CozMessage in message
TYPE  AtomCommit     = ObjectId                   -- deterministic, parentless commit
TYPE  PublishTag     = ObjectId                   -- annotated tag → AtomCommit or prev tag
TYPE  RefPath        = String                     -- e.g. "refs/atom/pub/mylib/0.1.0"

TYPE  AtomSnapshot = {
        tree:          TreeId,                    -- content subtree (the atom's files)
        author:        ATOM_AUTHOR,               -- constant: blank identity
        committer:     ATOM_AUTHOR,               -- constant: same as author
        timestamp:     0,                         -- Unix epoch zero, UTC
        message:       "",                        -- empty
        extra_headers: { "src": ObjectId },       -- source revision commit hash
      }
      -- The hash of this commit object is the `dig` in PublishPayload.
      -- Including `src` cryptographically binds the atom to its source
      -- revision, enabling a quick verification that the publish payload's
      -- `src` field matches the atom commit's extra header.

TYPE  ClaimCommitFormat = {
        tree:          EMPTY_TREE,                  -- well-known empty tree hash
        parent:        ∅,                           -- parentless (detached)
        author:        ATOM_AUTHOR,                 -- constant: blank identity
        committer:     ATOM_AUTHOR,                 -- constant: same as author
        timestamp:     ATOM_TIMESTAMP,              -- epoch zero
        message:       CozMessage(ClaimPayload),    -- the signed claim, JSON
      }
      -- Claim commits are detached and lightweight by design: no tree,
      -- no parents, no extra headers. The `src` field in the signed
      -- ClaimPayload (the commit message) ties the claim to its source
      -- revision at both the cryptographic AND object hash level — the
      -- message bytes contribute to the commit hash.

TYPE  PublishTagFormat = {
        target:        AtomCommit | PublishTag,   -- atom commit (initial) or prev tag (update)
        tag:           String,                    -- git tag object `tag` header (metadata)
        tagger:        <real tagger>,             -- the publisher
        timestamp:     <real timestamp>,          -- time of publish
        message:       CozMessage(PublishPayload),         -- the signed publish, JSON
      }
      -- The `claim` czd in the publish payload provides the cryptographic
      -- binding to the authorizing claim. Claims are looked up via
      -- `refs/atom/claims/d/{czd}` or `refs/atom/claims/pub/{label}`.
      --
      -- Publish payloads MAY include additional user-defined fields beyond
      -- the required set. For example, a reproducible-build artifact hash
      -- can be signed into the payload to cryptographically tie the final
      -- artifact to the source. This extensibility is a property of the
      -- Coz message format and the protocol's payload structure.
```

### Constants

```
ATOM_AUTHOR    = "" <> 0 +0000             -- blank identity, epoch zero (matching POC)
ATOM_TIMESTAMP = 0                      -- Unix epoch zero, UTC offset +0000
ATOM_MESSAGE   = ""                     -- empty commit message
EMPTY_TREE     = 4b825dc...             -- well-known empty tree ObjectId
-- Note: ATOM_AUTHOR/ATOM_TIMESTAMP apply to both atom commits AND claim
-- commits. gix creates objects at the byte level, bypassing the git CLI's
-- identity validation. See Open Question #4 for alternatives.
```

### Invariants

**[anchor-is-genesis]**: The anchor for a git-backed atom-set MUST be the
raw bytes of the repository's genesis commit ObjectId. The genesis commit
is the unique commit with no parents reachable from the current history.

**Discovery algorithm:** To derive the anchor, walk the commit graph from
the claim's payload `src` commit. The anchor must be the root
of the claim's specific lineage, since a repository may have divergent
branches with different roots. Follow all parent edges to find all
parentless commits reachable from the claim's `src`. If multiple exist,
the oldest by committer timestamp is authoritative per
`[anchor-oldest-root]`. The ObjectId's byte representation (20 bytes
for SHA-1, 32 bytes for SHA-256) is used directly as the `Anchor` value.

**Verification frequency:** Anchor discovery MUST be performed per-claim,
not cached per-registry. There is no cryptographic guarantee that all
claims share the same anchor (e.g., grafted or replaced histories on
specific branches). Per-claim verification is cheap with treeless
fetching (`tree:0`) — if anchors are genuinely identical, the commit
objects are already in the ODB from the first verification.
`VERIFIED: unverified`

**[anchor-hash-agile]**: The anchor MUST carry the bytes produced by
whatever object hash algorithm the git repository uses. The backend
MUST NOT rehash, truncate, or transform the ObjectId bytes. Hash
algorithm agility is handled by git itself (SHA-1 → SHA-256 transition);
the atom protocol treats the anchor as opaque bytes.
`VERIFIED: unverified`

**[snapshot-deterministic]**: An atom commit MUST be deterministic given
the same inputs. Given the same content tree AND the same source revision
(`src`), any party MUST produce an identical commit object (and therefore
identical ObjectId). This is achieved by fixing: (1) author and committer
to `ATOM_AUTHOR`, (2) timestamps to `ATOM_TIMESTAMP`, (3) commit message
to `ATOM_MESSAGE`, (4) no parent commits, (5) exactly one extra header
(`src`) containing the source revision ObjectId, (6) no GPG signatures.
The commit hash is the `dig` field in the publish payload.
`VERIFIED: unverified`

**[snapshot-parentless]**: An atom commit MUST have zero parents. Atoms
are detached subtrees — they carry no history. Provenance is recorded
in the `src` extra header and the publish payload, not in git's parent
chain. (Satisfies atom-transactions.md `[atom-detached]`.)
`VERIFIED: unverified`

**[snapshot-src-header]**: An atom commit MUST contain exactly one extra
header: `src`, whose value is the hex-encoded ObjectId of the source
revision commit from which the atom's content was extracted. This
header cryptographically binds the atom snapshot to its source revision.

The `src` value in the atom commit MUST match the `src` field in the
corresponding publish payload. A consumer MAY verify this as a quick
integrity check before performing full Coz signature verification.
`VERIFIED: unverified`

**[temporal-vector]**: The atom protocol's git backend enforces a
three-point temporal ordering — the **authenticity vector**:

```
genesis commit (anchor) → claim src → publish src
```

Specifically: (1) the claim's payload `src` MUST point to a commit that
is a descendant of the genesis commit (verifiable by walking the DAG
from the claim's `src` to genesis), AND (2) the source revision
referenced by `src` in the publish payload MUST be at or after the
claim's `src` in the repository's history (i.e., publish `src` is a
descendant of, or equal to, the claim's `src`).

An atom MAY be published from the claim's `src` commit itself (when no
code changes are needed), but MUST NOT be published from a commit that
precedes the claim's `src`. This ensures that a claim establishes a
temporal floor — only content at or after the claim is publishable.

**Per-publish verification:** To verify provenance of a published atom,
the verifier MUST check that the publish's `src` is genuinely in the
repository's history and is at or after the claim's `src`. This uses
treeless commit-only fetching (`tree:0`) for efficiency.
`VERIFIED: unverified`

**[claim-detached]**: A claim commit MUST be parentless (no `parent`
header) with the well-known empty tree as its `tree` and no extra
headers. The claim's `src` is carried in the signed `ClaimPayload`
(the commit message), which contributes to the commit hash. This
design ensures claims are maximally lightweight (no tree/blob download,
no extra headers), immune to rebase hazards, and consistent with
atom commit architecture.
`VERIFIED: unverified`

**[claim-message-is-coz]**: The commit message of a claim commit MUST
be a valid, complete `CozMessage` JSON object whose payload has
`typ: "atom/claim"`. The message MUST be parseable as JSON and MUST
pass Coz verification. No additional text, headers, or formatting
SHOULD be present outside the CozMessage JSON.
`VERIFIED: unverified`

**[publish-tag-targets-correct]**: An initial publish tag MUST target
an atom commit (a deterministic, parentless commit with a `src` extra
header). An update publish tag MUST target the previous publish tag for
the same `(label, version)`. A publish tag MUST NOT target a regular
commit, tree, or blob.
`VERIFIED: unverified`

**[publish-tag-claim-binding]**: A publish tag's CozMessage payload
MUST contain a `claim` field whose value is the czd of the authorizing
claim. The claim commit is looked up via `refs/atom/claims/d/{czd}`
(or `refs/atom/claims/pub/{label}` in the originating registry). No
extra header is needed — the signed payload is the sole binding.
`VERIFIED: unverified`

**[publish-tag-message-is-coz]**: The message body of a publish tag
MUST be a valid, complete `CozMessage` JSON object whose payload has
`typ: "atom/publish"`. The message MUST be parseable as JSON and MUST
pass Coz verification. No additional text or formatting SHOULD be
present outside the CozMessage JSON.
`VERIFIED: unverified`

**[tag-chain-immutable]**: Publish tag updates MUST be implemented by
creating a new tag object that targets the _previous_ tag object (not
the atom commit directly). Git's tag-peeling mechanism resolves the
chain to the underlying atom commit. The ref is updated to point to
the new tip of the chain. Old tag objects MUST NOT be deleted — they
persist in the git object database as an immutable audit trail.

This ensures: (a) lock files referencing old tag ObjectIds remain
resolvable, (b) the full update history is structurally represented,
(c) consumers can reconstruct the complete chain of ownership/signing
by walking the tag chain.
`VERIFIED: unverified`

**[coz-bit-perfect]**: All `CozMessage` JSON stored in git objects
(commit messages, tag messages) MUST be preserved bit-for-bit. The
backend MUST NOT reformat, re-serialize, or alter the JSON in any
way. (Satisfies atom-transactions.md `[backend-bit-perfect]`.)
`VERIFIED: unverified`

**[single-active-claim-registry]**: In a registry (source repository),
at most one active claim MUST exist per `AtomId`. If a claim is
replaced (e.g., key compromise), the old claim commit remains in
history for audit. The registry's claim ref
(`refs/atom/claims/pub/{label}`) MUST point to the currently active claim
commit—the ref is the authoritative indicator of which claim is active.
`VERIFIED: unverified`

**[store-claim-disambiguation]**: In a store, multiple claims for the
same `AtomId` MAY coexist (from different sources/forks). The store's
ref layout MUST disambiguate them by claim `czd`. Two atoms with the
same `AtomId` but different claim czds MUST occupy different ref paths.
Claim czd is cryptographically unique, preventing collisions by
construction — this is the fundamental advantage of claim-czd-addressed
storage.
`VERIFIED: unverified`

### Ref Layout

#### Registry Refs (source repository)

```
refs/atom/claims/pub/{label}                       → claim commit (active, mutable pointer)
refs/atom/claims/d/{claim_czd}                     → claim commit (permanent, fetchable by czd)
refs/atom/pub/{label}/{version}                  → publish tag [→ chain] → atom commit
refs/atom/src/{oid}                              → src commit (provenance-protected)
```

The `pub/` sub-prefix under `claims/` is the mutable "which claim is
active?" pointer. The `d/` sub-prefix is the permanent "fetch this
specific claim" ref — it ensures all claims (including historical ones
after key rotation) remain fetchable as ref tips, regardless of server
`uploadpack.allowReachableSHA1InWant` policy. Both are written at claim
time. `refs/atom/src/` protects source revision commits from GC if the
originating branch is deleted. This mirrors the store's `claims/d/`
layout for consistency.

**[registry-ref-label-unique]**: Within a single registry, the `{label}`
segment MUST be unique. No two atoms in the same registry MAY share a
label. (Satisfies atom-transactions.md `[atomid-per-source-unique]`.)
`VERIFIED: unverified`

**[registry-ref-claim]**: The ref `refs/atom/claims/pub/{label}` MUST
point to the currently active claim commit for that atom. When a claim
is replaced, this ref MUST be updated to point to the new claim commit.
The old claim commit remains in the repository's history. Claim refs
are separated from atom version refs to avoid polluting the version
subtree.
`VERIFIED: unverified`

**[registry-ref-version]**: The ref `refs/atom/pub/{label}/{version}`
MUST point to an annotated tag object (the publish tag tip), which
either directly targets the deterministic atom commit (initial publish)
or targets a previous tag in the update chain (updates). The
`{version}` segment is the `RawVersion` string from the publish payload.
`VERIFIED: unverified`

#### Store Refs (consumer repository)

```
# Published atoms (d = digest-addressed)
refs/atom/d/{claim_czd}/{version}                → publish tag [→ chain] → atom commit
refs/atom/claims/d/{claim_czd}                   → claim commit (shallow-fetched)

# Development atoms (unsigned, digest-addressed by AtomId, versioned)
refs/atom/dev/{atom_digest}/{dev_version}        → atom commit (no tags, no claims)
```

Dev versions SHOULD include the tree object hash to avoid clobbering
and ensure new dev versions are only created when content genuinely
changes. For example: `{manifest_version}.dev-{tree_hash_prefix}`.
The version string is opaque to the protocol — tooling MAY adopt any
scheme that guarantees uniqueness per content snapshot.

The `d/` sub-prefix under `claims/` denotes digest-addressed claims
(store-side). This consolidates all claim refs under a single
`refs/atom/claims/` namespace while preventing collision with
registry label-addressed claims.

**[store-ref-by-claim-czd]**: Store version refs MUST be keyed by the
claim `czd` (the canonical digest of the claim). This ensures global
uniqueness — two forks of the same source with different owners
produce different claim czds and therefore different ref paths.
`VERIFIED: unverified`

**[store-claim-ref]**: Each ingested claim MUST have a corresponding
ref at `refs/atom/claims/d/{claim_czd}` pointing to the claim commit.
The claim commit SHOULD be shallow-fetched from the source (no need
to pull its ancestors). This ref serves two purposes: (1) protecting
the claim commit from garbage collection, and (2) enabling efficient
claim retrieval by czd for local verification. If a publish tag's
payload references a claim czd but no corresponding claim commit
exists in the object store, this MUST be treated as an error.
`VERIFIED: unverified`

**[store-ownership-migration]**: If the ownership of an atom changes
(new claim, new czd), the atom's versions in the store naturally
migrate to a new ref path (under the new `{claim_czd}`). Versions
published under the old claim remain under the old czd — they are
still valid artifacts signed by the old claim.
`VERIFIED: unverified`

**[store-claim-cleanup]**: When the last version ref under a
`refs/atom/d/{claim_czd}/` prefix is deleted (e.g., cache eviction),
the backend SHOULD also delete the corresponding
`refs/atom/claims/d/{claim_czd}` ref to prevent orphaned claim
accumulation. Git has no cross-namespace reference counting, so
this cleanup is the backend's responsibility.
`VERIFIED: unverified`

### Transitions

**[claim-transition-git]**: An atom MAY be claimed by creating a
detached claim commit.

- **PRE**: The repository MUST have at least one commit. The genesis
  commit (parentless commit) MUST be derivable. No active claim for
  this label MUST exist (or the existing claim is being explicitly
  replaced). The claim `CozMessage` MUST be valid and include a
  `key` field. A source revision (`src`) for the claim point MUST
  be provided.
- **POST**: A claim commit exists as a detached, parentless commit
  with the well-known empty tree and the CozMessage (containing `src`)
  as the commit message. The ref `refs/atom/claims/pub/{label}`
  points to it (mutable active pointer). A permanent ref
  `refs/atom/claims/d/{claim_czd}` is written to ensure the claim
  remains fetchable by czd. A protective ref `refs/atom/src/{src_oid}`
  is written to prevent GC of the claim's source revision.
  `VERIFIED: unverified`

**[publish-transition-git]**: A version MAY be published for a
claimed atom by creating an atom commit and annotating it with a
publish tag.

- **PRE**: An active claim for this label MUST exist in the registry
  (`refs/atom/claims/pub/{label}` is set). The source revision (`src`)
  from which the atom's content is extracted MUST be at or after the
  claim's `src` in the repository's history (`[temporal-vector]`).
  The atom commit MUST be deterministic per `[snapshot-deterministic]`.
  The publish `CozMessage` MUST reference the active claim's `czd`.
- **POST**: An atom commit exists (parentless, deterministic, with
  `src` extra header). A publish tag points at it with the CozMessage
  as the message (containing the `claim` czd binding). The ref
  `refs/atom/pub/{label}/{version}` points to the publish tag.
  A protective ref `refs/atom/src/{src_oid}` is written to prevent
  GC of the source revision. The ref write SHOULD use a
  compare-and-swap (CAS) on `refs/atom/claims/pub/{label}` to ensure
  the claim has not been replaced between payload construction and
  tag creation.
  `VERIFIED: unverified`

**[ingest-transition]**: Atoms MAY be ingested from a registry (or
another store) into a store.

- **PRE**: The source has discoverable atoms (resolvable refs). The
  store is a valid git repository.
- **POST**: For each ingested atom: the atom commit exists in the
  store, the publish tag (and its chain) exists in the store, the
  claim commit is fetched and referenced by
  `refs/atom/claims/d/{claim_czd}`. Claim commits are lightweight
  by design (detached, empty tree) and require no special filtering.
  Version refs follow the store layout:
  `refs/atom/d/{claim_czd}/{version}`. AtomId is preserved through
  ingestion. Refs MUST NOT be committed until all cryptographic
  verification (`[verification-local]`) passes — objects may exist
  in the ODB during verification, but are invisible to consumers
  until the refs transaction is committed. (Satisfies
  atom-transactions.md `[ingest-preserves-identity]`.)
  `VERIFIED: unverified`

**[claim-replacement-transition]**: An atom's active claim MAY be
replaced (e.g., after key compromise).

- **PRE**: An active claim for this label MUST exist. The new claim
  `CozMessage` MUST be valid. The new claim MUST reference the same
  `(anchor, label)`.
- **POST**: A new claim commit exists in history. The ref
  `refs/atom/claims/pub/{label}` is updated to point to the new claim
  commit. Future publishes MUST reference the new claim's `czd` and
  satisfy `[temporal-vector]` with respect to the new claim commit.
  Existing publish tags remain valid under the old claim.
  `VERIFIED: unverified`

**[publish-update-transition]**: A publish tag MAY be updated (e.g.,
after key revocation or resigning) by appending a new tag to the
chain.

- **PRE**: An existing publish tag chain for this `(label, version)`
  exists. The new tag's `CozMessage` MUST be valid.
- **POST**: A new tag object is created targeting the _previous_ tag
  object (not the atom commit). The ref
  `refs/atom/pub/{label}/{version}` (registry) or
  `refs/atom/d/{claim_czd}/{version}` (store) is updated to point to
  the new tip. The old tag object persists in the object database.
  Git's tag-peeling resolves the chain to the underlying atom commit.
  `VERIFIED: unverified`

**[tag-chain-semantic-immutable]**: All tags within a single publish
update chain MUST contain identical values for the immutable payload
fields: `(label, version, dig, src, path)`. Only signing metadata
(`tmb`, `now`, `claim`), the `key` field, and extension fields
(`meta`) MAY differ between tags in the same chain. Altering the
artifact identity (`dig`), version string, or source revision
requires a new atom commit and a new publish ref — not an update
to an existing tag chain.
`VERIFIED: unverified`

**[fs-ingest-transition]**: Atoms from a filesystem `AtomSource` MAY
be ingested into a git `AtomStore` without claims or publishes.

- **PRE**: The filesystem source satisfies the `AtomSource` contract
  (as defined in atom-transactions.md). The store is a valid git
  AtomStore.
- **POST**: Atom commits exist in the store for each discovered atom,
  referenced by `refs/atom/dev/{atom_digest}/{dev_version}`. No publish
  tags or claim refs exist for dev atoms. The store MUST treat dev atoms
  as unsigned/unclaimed. The dev version string SHOULD incorporate the
  tree object hash to prevent clobbering across concurrent evaluations.
  Every atom has an AtomId — git-sourced dev atoms use the source
  repository's genesis commit as anchor; filesystem-sourced atoms use
  a well-known constant sentinel anchor (see atom-transactions.md
  `[fs-source-contract]`). Note: the dev atom's `dig` will inherently
  differ from the published `dig` because published atoms include a
  real `src` extra header — this is by design.
  `VERIFIED: unverified`

**[dev-atom-resolution]**: Tooling consuming atoms from a store
resolves through two namespaces in order of precedence:

1. `refs/atom/dev/{atom_digest}/` — local development atoms (unsigned,
   in-progress evaluations from filesystem or local git sources)
2. `refs/atom/d/{claim_czd}/` — all ingested published atoms, regardless
   of origin (local registry, remote registry, or mirror)

Published atoms from the local registry are ingested into `d/` via
the same `[ingest-transition]` as remote atoms — there is no special
treatment. The `pub/` namespace is registry-write-time only; the
resolver never queries it.

Clients MAY provide a **release mode** which skips step 1, resolving
only from `d/`. This ensures builds use exactly the ingested versions,
matching what downstream consumers would see.

### Forbidden States

**[no-non-empty-claim]**: A claim commit MUST have the well-known
empty tree as its `tree` and MUST be parentless. If a claim commit
has a non-empty tree or any parent, the backend MUST reject it as
malformed.
`VERIFIED: unverified`

**[no-orphan-publish]**: A publish tag MUST NOT exist in a registry
without a corresponding claim commit reachable via
`refs/atom/claims/pub/{label}` or `refs/atom/claims/d/{claim_czd}`.
(Satisfies atom-transactions.md `[no-unclaimed-publish]`.)
`VERIFIED: unverified`

**[no-backdated-src]**: A publish MUST NOT reference a source revision
(`src`) that precedes the claim's `src` in the repository's history.
The publish's `src` MUST be at or after the claim's `src`. This is the
enforcement mechanism for `[temporal-vector]`.
`VERIFIED: unverified`

**[no-label-collision-registry]**: Two atoms in the same registry
MUST NOT share a label. This is enforced by the ref layout — two
claim refs for the same label would conflict.
`VERIFIED: unverified`

**[anchor-oldest-root]**: If multiple parentless commits exist in the
repository's history (e.g., merged independent histories, orphan
branches like `gh-pages`), the **oldest** parentless commit by
committer timestamp MUST be selected as the anchor. This matches the
POC implementation and ensures deterministic anchor discovery without
rejecting repositories with legitimate orphan branches.
`VERIFIED: unverified`

**[no-missing-store-claim]**: In a store, if a publish tag's payload
references a claim czd, the corresponding claim commit MUST exist in
the store's object database and MUST be reachable via
`refs/atom/claims/d/{claim_czd}`. A missing claim MUST be treated as
ingestion corruption.
`VERIFIED: unverified`

### Behavioral Properties

**[anchor-vector-authenticity]**: Given a publish tag in a registry,
the atom MUST be verifiable as authentic by checking the three-point
vector: (1) the genesis commit (anchor) is derivable from the claim's
payload `src`, (2) the publish payload's `claim` czd resolves to a
claim commit whose payload `src` is a descendant of the genesis commit,
(3) the claim `CozMessage` in that commit's message is valid, AND
(4) the publish's `src` is at or after the claim's `src`. This creates
a "vector of authenticity": anchor → claim src → publish src.

- **Type**: Safety
  `VERIFIED: unverified`

**[ingestion-portable]**: Atom commits, publish tag objects (and their
chains), and claim commits MUST be transferable between git
repositories via standard git pack negotiation (`git fetch`). No
custom transport extensions are REQUIRED. Claim commits SHOULD be
shallow-fetched (without their ancestry) into stores to minimize
transfer cost.

- **Type**: Safety
  `VERIFIED: unverified`

**[update-chain-auditable]**: The tag chain structure (new → old → ...
→ atom commit) MUST be traversable. A consumer MUST be able to
reconstruct the full update history by walking the tag chain from the
ref tip. Each tag object in the chain carries its own `CozMessage`,
enabling verification of the complete signing history.

- **Type**: Safety
  `VERIFIED: unverified`

**[store-accumulates]**: After ingestion, a store's `resolve` MUST
return at least what the source's `resolve` returns for every
ingested atom. (Model §2.3, ⊇ condition.)

- **Type**: Safety
  `VERIFIED: unverified`

<!-- [publish-extensible] moved to atom-transactions.md — this is a
     protocol-level property of the Coz message format, not a git
     backend concern. -->

## Verification

| Constraint                   | Method           | Result  | Detail                                            |
| :--------------------------- | :--------------- | :------ | :------------------------------------------------ |
| anchor-is-genesis            | integration-test | pending | Root from genesis ObjectId bytes                  |
| anchor-hash-agile            | agent-check      | pending | gix ObjectId handles both SHA-1/SHA-256           |
| snapshot-deterministic       | unit-test        | pending | Same (tree, src) → same commit hash               |
| snapshot-parentless          | unit-test        | pending | Atom commit has zero parents                      |
| snapshot-src-header          | unit-test        | pending | Atom commit has exactly one extra header `src`    |
| temporal-vector              | integration-test | pending | anchor → claim src → publish src enforced         |
| claim-detached               | unit-test        | pending | Claim commit parentless with empty tree           |
| claim-message-is-coz         | integration-test | pending | Parse claim from commit message, verify           |
| publish-tag-targets-correct  | integration-test | pending | Tag target is atom commit or previous tag         |
| publish-tag-claim-binding    | integration-test | pending | Payload `claim` czd resolves to valid claim       |
| publish-tag-message-is-coz   | integration-test | pending | Tag message is valid CozMessage JSON              |
| tag-chain-immutable          | integration-test | pending | Update creates chain, old tags persist            |
| coz-bit-perfect              | integration-test | pending | Store → retrieve → byte-compare                   |
| single-active-claim-registry | integration-test | pending | Second claim for same label replaces ref          |
| store-claim-disambiguation   | integration-test | pending | Two claims, same AtomId, different ref paths      |
| registry-ref-label-unique    | integration-test | pending | Conflicting labels rejected                       |
| registry-ref-claim           | integration-test | pending | Ref points to active claim commit                 |
| registry-ref-version         | integration-test | pending | Ref points to publish tag tip                     |
| store-ref-by-claim-czd       | integration-test | pending | Store refs use claim czd as key                   |
| store-claim-ref              | integration-test | pending | Claim commit ref exists, GC-protected             |
| store-ownership-migration    | integration-test | pending | New claim → new ref path                          |
| claim-transition-git         | integration-test | pending | Claim creates detached commit + 3 refs            |
| publish-transition-git       | integration-test | pending | Publish creates atom commit + tag + src ref       |
| ingest-transition            | integration-test | pending | Full ingest cycle preserves identity              |
| claim-replacement-transition | integration-test | pending | New claim replaces ref, old commit stays          |
| publish-update-transition    | integration-test | pending | New tag chains to old tag, ref updated            |
| fs-ingest-transition         | integration-test | pending | FS atoms ingested unsigned into store             |
| no-non-empty-claim           | unit-test        | pending | Validation rejects non-empty-tree claim           |
| no-orphan-publish            | integration-test | pending | Publish without claim rejected                    |
| no-backdated-src             | integration-test | pending | publish src before claim src rejected             |
| no-label-collision-registry  | integration-test | pending | Duplicate label rejected                          |
| anchor-oldest-root           | integration-test | pending | Oldest parentless commit selected as anchor       |
| no-missing-store-claim       | integration-test | pending | Missing claim for payload czd detected            |
| anchor-vector-authenticity   | integration-test | pending | Full 3-point vector: genesis → claim src → src    |
| ingestion-portable           | integration-test | pending | git fetch transfers all objects correctly         |
| update-chain-auditable       | integration-test | pending | Tag chain walkable, all CozMessages retrievable   |
| store-accumulates            | integration-test | pending | Post-ingest resolve ⊇ source resolve              |
| dev-atom-resolution          | integration-test | pending | Local → dev/{digest}/{ver}, remote → d/           |
| store-claim-cleanup          | integration-test | pending | Orphaned claim ref cleaned on version eviction    |
| tag-chain-semantic-immutable | unit-test        | pending | Update tags preserve (label,version,dig,src,path) |

## Implications

### Implementation Guidance

- **atom-git**: The reference implementation crate. MUST implement
  `AtomSource`, `AtomRegistry`, and `AtomStore` from atom-core per
  this specification.

- **atom-core FsSource**: See atom-transactions.md for the abstract
  `AtomSource` contract. The git backend's responsibility is to
  ingest atoms from `FsSource` per `[fs-ingest-transition]`.

- **Deterministic commit construction**: Use gix's commit creation API
  with blank author/committer (`<> 0 +0000`), epoch timestamp, empty
  message, and a single `src` extra header. The `dig` field in
  PublishPayload is this commit's ObjectId.

- **Claim commits**: Use gix to create parentless commits with the
  well-known empty tree, no extra headers, ATOM_AUTHOR/ATOM_TIMESTAMP,
  and the CozMessage as commit message. The `src` in the payload ties
  the commit hash to the source revision. Write three refs:
  `claims/pub/{label}`, `claims/d/{czd}`, and `src/{src_oid}`.

- **Publish tags**: Use gix's tag object creation API with the
  CozMessage as tag message. The `claim` czd in the payload identifies
  the authorizing claim. For updates, the tag targets the previous tag
  object (not the atom commit).

- **Publish tag metadata**: Clients SHOULD leverage
  `[publish-payload-extensible]` to provide programmatic lifecycle
  metadata in the publish `CozMessage` payload's `meta` object.
  Recommended fields for client implementors (all nested under `meta`):
  - `meta.broken: true` — marks a version as yanked/broken; clients
    SHOULD warn or refuse to resolve
  - `meta.security: "CVE-2026-XXXX"` — security advisory identifier
  - `meta.superseded-by: "1.2.3"` — recommended replacement version
  - `meta.deprecated: true` — marks version as deprecated
  - `meta.build-hash: "sha256:..."` — reproducible build artifact hash,
    cryptographically tying the final artifact to the source
  - `meta.min-compatible: "1.0.0"` — minimum compatible version for
    semver-unaware ecosystems
    All extension fields are signed as part of the `CozMessage` and
    carry cryptographic assurance. Publish tag updates
    (`[publish-update-transition]`) enable retroactive advisory
    annotation without altering the original publish.

- **Tag peeling**: When resolving a version ref to its content, peel
  the tag chain to the final atom commit. The atom commit is always
  the terminal object (parentless commit with `src` header). gix
  provides `peel_to_commit()` for this.

- **Store ingestion**: Fetch atom commits, tag chains, and claim
  commits via gix pack negotiation. Create published atom refs under
  `refs/atom/d/`, claim refs under `refs/atom/claims/d/`. Claim
  commits can be fetched shallowly (no need for their ancestry).

- **Dev atom ingestion**: Create atom commits from filesystem content.
  Reference under `refs/atom/dev/{atom_digest}/{dev_version}`. No
  tags, no claims, no verification ceremony. Dev version SHOULD include
  tree hash (e.g., `1.0.0.dev-{tree_hash_prefix}`) to avoid clobbering
  across concurrent evaluations. AtomId is derivable for all atoms
  (git atoms use genesis anchor, FS atoms use sentinel anchor).

- **Anchor discovery**: Walk the commit graph from the claim's payload
  `src` to find all parentless commits reachable from that lineage.
  Select the oldest by committer timestamp as the anchor. Multiple
  roots are permitted — the oldest is authoritative per
  `[anchor-oldest-root]`. Always walk from the claim's `src`, not
  HEAD, to verify the anchor is the root of the claim's specific
  lineage.

- **Atomicity**: Multi-ref operations (claim + publish, ingestion of
  many versions) MUST use `gix::refs::Transaction` to batch all
  reference updates atomically. Remote pushes involving multiple refs
  MUST use atomic push semantics (equivalent of `git push --atomic`)
  to prevent torn states.

- **Tree construction**: When building git tree objects from filesystem
  content (`FsSource`), entries MUST follow Git's canonical byte-order
  sorting (directories sort as if their names end with `/`). gix's
  tree construction API handles this — implementations MUST NOT
  manually sort entries using OS-level alphabetical ordering.

### Testing Strategy

- **Unit tests**: snapshot determinism (same tree+src → same hash),
  forbidden state validation, extra header round-trip
- **Integration tests**: full lifecycle (claim → publish → ingest →
  verify), using `tempfile` for ephemeral git repos
- **Cross-reference**: Phase 4 items in atom-transactions.md verification
  table (10 pending integration tests) are satisfied by this spec's
  verification plan

### Model Gaps

- The formal model (publishing-stack-layers.md) explicitly marks "Git
  object internals" as out of scope. This spec fills that gap.
- The model's `PopulateSession` (§3.3) maps to the `[ingest-transition]`
  defined here.
- The model's coalgebraic `AtomSource` observer maps to the git backend's
  ref-based discovery and object-based resolution.
- The filesystem source (`FsSource`) extends the model's `AtomSource`
  coalgebra to non-git backends. The abstract contract is defined in
  atom-transactions.md; this spec covers only the git ingestion path.

### Open Questions

1. **Version string normalization**: `RawVersion` is opaque by protocol.
   Some values may contain characters invalid in git refnames (e.g., `~`,
   `^`, `:`, `..`). A normalization scheme for version-to-refname mapping
   MAY be needed. gix provides ref normalization facilities that SHOULD
   be leveraged. Consider percent-encoding or a restricted character set
   for the ref segment. Slash (`/`) in version strings would cause
   directory/file conflicts in the refname filesystem (e.g., publishing
   `1.0` then `1.0/beta`).

2. **Tag extra header support in gix**: The spec assumes gix supports
   extra headers on tag objects (analogous to commit extra headers). This
3. **Tag extra headers in gix**: Publish tags no longer require extra
   headers (the `claim-commit` header has been removed in favor of
   the signed payload's `claim` czd). This resolves the open question
   about whether gix supports tag extra headers.

4. **FsSource contract**: The abstract `AtomSource` contract for
   filesystem directories (manifest discovery strategy, path resolution,
   ingestion interface) needs formal specification in atom-transactions.md.
   The POC implementation provides a reference. This spec assumes that
   contract exists and specifies only the git ingestion side.

5. **ATOM_AUTHOR identity**: The current blank identity (`"" <> 0 +0000`)
   works with gix but may trigger `git fsck` warnings. For good git
   citizenship, consider alternatives that preserve determinism:
   (a) a static sentinel like `"atom" <atom-protocol> 0 +0000`,
   (b) the `src` commit's author (deterministic since `src` hash is an
   input, but adds a read dependency), or (c) a protocol-derived
   constant like the CozMessage `typ` value (e.g., `"atom/publish"`).
   The choice does not affect protocol correctness — only git tooling
   ergonomics.
