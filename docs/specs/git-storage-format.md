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

**Backend Contract:**
[atom-backend-contract.md](atom-backend-contract.md) — this
specification is the **git reference instantiation** of the abstract
backend contract. Appendix A there maps every `[backend-*]` obligation
to its discharge site in this document (or honestly names the gap);
the `[backend-*]` discharge notes on constraints below are the
receiving ends of that mapping.

**Criticality Tier:** Medium — the git backend is the reference storage
implementation. Correctness failures affect supply chain integrity, but
the protocol-level cryptographic guarantees (in atom-transactions.md) are
the primary defense layer. This spec constrains the storage layer beneath
those guarantees.

## Overview

### Registry vs Store

The git backend serves two distinct roles with different addressing
schemes, ref layouts, and invariants:

| Property       | Registry                        | Store                                         |
| :------------- | :------------------------------ | :-------------------------------------------- |
| Purpose        | Source-side: claims + publishes | Consumer-side: aggregation + consumption      |
| Addressing     | Human-readable by label         | Machine-addressed by `blake3(publish_czd)`    |
| Scope          | Single source repository        | Many sources, many registries                 |
| Collision-free | Labels unique per registry      | publish_czd cryptographically unique globally |
| Claim per atom | Exactly one, ever (genesis-once)| Multiple (from different sources/forks)       |
| Anchor         | One anchor (`czd(charter₀)`)    | Many anchors (one per ingested source)        |
| Ref prefix     | `refs/atom/{pub,claims/pub}/..` | `refs/atom/{d,dev,claims/d}/...`              |

A repository MAY serve as both a registry and a store simultaneously
(e.g., a project that publishes its own atoms and ingests dependencies).

### Git Object Architecture

Every signed record this backend ever stores — the charter genesis,
each label's claim genesis, every publish, and every fact at any scope
— is first and foremost a **leaf in the anchor's record log**: one
continuous, append-only Merkle structure per anchor, in true
chronological signing order (ADR-0007 §7.1). This document does not
define the log's own internal shape, its leaf-ordering algorithm, or
`refs/atom/log`'s own checkpoint-commit chaining — ADR-0007 §7.1–§7.3
governs those, and the log's internal git-tree layout is an explicit
open item there, not invented here (`[log-tree-opaque]`, below). What
this document DOES specify is how each of the four git object
categories below relates to that log: each carries, as its own `tree`,
the SAME git tree object the log's own checkpoint carries at the point
its leaf was appended — never a synthesized subset, and never the
well-known empty tree (ADR-0007 §7.5) — so a client already holding
the object can always recompute the log's own root and detect a fact
the object's own chain omitted. Every one of these objects' own
`message` field stays fixed and inert (`author`/`committer` are also
fixed for charter, claim, and fact commits; a publish chain commit
keeps the publisher's or fact signer's real author/committer instead,
unchanged from before this amendment); the
signed record itself lives as a leaf inside the tree the commit
carries, never as a second copy in the commit's own `message`.

The git backend uses four categories of git objects:

1. **Charter transactions** — the anchor is `czd(charter₀)`, the coz
   digest of the atom-set's one and only charter transaction
   (atom-transactions.md `[charter-anchor]`), not a property of any git
   object. The genesis commit is no longer the anchor; it remains
   transitively pinned by the charter's `src` (a revision hash commits
   to its entire ancestry), but nothing selects or derives it. The
   charter is genesis-once (atom-transactions.md
   `[atom-genesis-and-facts]`): written exactly once, never reissued,
   never superseded by a second charter commit — owner-set membership
   change and key rotation are layered on afterward as facts, not a new
   charter. The git object encoding and ref layout for storing and
   retrieving the charter transaction, and the fact history layered on
   it, is the **charter commit** and the `refs/atom/charter/d/{charter_czd}`
   ref family, specified under Constraints below
   (`[charter-commit-format]`, `[charter-fact-chain]`,
   `[charter-ref-by-czd]`).

2. **Claim commits** — a commit whose `tree` is the record log's own
   tree at the point the claim genesis leaf was appended, and whose
   `message` is fixed and inert. A claim is genesis-once
   (atom-transactions.md `[atom-genesis-and-facts]`): written exactly
   once, at first claim, and parentless forever after — there is no
   "subsequent claim" and no key-rotation-by-replacement. Ownership
   change (key rotation, transfer, seizure) is layered on afterward as
   a claim-owner **fact** commit chained onto the claim
   (`[claim-fact-chain]`), never a second claim commit. Their identity
   is the claim `czd` — computed over the signed claim payload's own
   bytes, never over this git object — fixed forever at first claim.

3. **Atom commits** — parentless commits whose tree contains the atom's
   content subtree and nothing else. These are the atom snapshots
   referenced by `dig` in publish transactions. Their hash is
   reproducible given the same tree alone: identity carries no source
   binding, because that binding now lives one layer up, on the
   publish commit that reaches this object (below) and in the signed
   record itself — never inside the content object. Unlike the other
   three categories, an atom commit carries no log tie: content is not
   itself a record and is never a log leaf.

4. **Publish commits** — the git encoding of a publish's materialized
   view chain (§Ref Layout): an ordinary commit, not a tag. The
   chain's first commit (the publish itself) carries the atom commit
   as its git parent — the only path by which a consumer reaches a
   package's content — and its `tree` is the record log's own tree at
   the point the publish leaf was appended; its `message` is fixed and
   inert, like every other log-anchored commit this backend writes. A
   later commit in the same chain (a fact about this publish — yank,
   advisory, deprecation, mode promotion, meta revision) parents onto
   the commit before it instead, and likewise carries the log's own
   tree at ITS append point rather than the atom commit. The ref
   `refs/atom/pub/{label}/{version}` points to the chain's tip; there
   is no publish tag and no tag chain, and a publish chain has no path
   to a source revision commit at all.

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
No publish commits or claim refs are created for unsigned atoms.
Unsigned dev atoms are stored under `refs/atom/dev/{anchor}/{label}`
(see Ref Layout).

## Constraints

### Type Declarations

```
TYPE  Anchor         = Vec<u8>                    -- opaque bytes: czd(charter₀), backend-agnostic
TYPE  LogTree        = TreeId                     -- the anchor's record-log tree (ADR-0007
                                                    -- §7.1-§7.3) at some point in its append
                                                    -- history -- the same tree object a
                                                    -- checkpoint commit on refs/atom/log
                                                    -- carries at that point. Its internal
                                                    -- layout is not specified here
                                                    -- ([log-tree-opaque]).
TYPE  ClaimCommit    = ObjectId                   -- commit whose tree is a LogTree
TYPE  AtomCommit     = ObjectId                   -- deterministic, parentless commit
TYPE  PublishChainCommit = ObjectId               -- a commit in a publish's own materialized view chain
TYPE  RefPath        = String                     -- e.g. "refs/atom/pub/mylib/0.1.0"

TYPE  AtomSnapshot = {
        tree:          TreeId,                    -- content subtree (the atom's files)
        author:        ATOM_AUTHOR,               -- constant: blank identity
        committer:     ATOM_AUTHOR,               -- constant: same as author
        timestamp:     0,                         -- Unix epoch zero, UTC
        message:       "",                        -- empty
      }
      -- The hash of this commit object is the `dig` in PublishPayload,
      -- and is a pure function of `tree` alone: identical content
      -- published from two different source revisions is ONE object,
      -- not two. The source binding lives one layer up -- on the
      -- publish commit that carries this object as its git parent
      -- (`[publish-commit-format]`) -- and in the signed record itself,
      -- never inside the content object.

TYPE  CharterCommitFormat = {
        tree:          LogTree,                     -- the record log's own tree, at the
                                                       -- point the charter genesis leaf was
                                                       -- appended (ADR-0007 §7.1, §7.5) --
                                                       -- never the well-known empty tree
        parent:        ∅,                           -- ALWAYS parentless: genesis-once, one ever
        author:        ATOM_AUTHOR,                 -- constant: blank identity
        committer:     ATOM_AUTHOR,                 -- constant: same as author
        timestamp:     ATOM_TIMESTAMP,              -- epoch zero
        message:       ATOM_MESSAGE,                -- fixed, inert -- the charter payload is
                                                       -- read as the log leaf this commit's
                                                       -- tree carries, never a second copy here
      }
      -- Identity is the charter czd (czd(charter)) -- computed over the signed charter
      -- payload's own bytes, the log leaf this commit's tree carries, never over any
      -- property of this git object itself. This single, permanent charter commit's czd
      -- IS the atom-set Anchor. Genesis-once
      -- (atom-transactions.md [atom-genesis-and-facts]): exactly one
      -- charter commit per anchor, ever — no successor, no `prior` field
      -- on CharterPayload (atom-transactions.md's Type Declarations
      -- dropped it). Owner-set membership change and key rotation happen
      -- afterward as a charter-owner fact commit chained onto this one
      -- ([charter-fact-chain], FactCommitFormat below), never as a
      -- second charter commit.

TYPE  ClaimCommitFormat = {
        tree:          LogTree,                     -- the record log's own tree, at the
                                                       -- point the claim genesis leaf was
                                                       -- appended (ADR-0007 §7.1, §7.5) --
                                                       -- never the well-known empty tree
        parent:        ∅,                           -- ALWAYS parentless: genesis-once, one ever
        author:        ATOM_AUTHOR,                 -- constant: blank identity
        committer:     ATOM_AUTHOR,                 -- constant: same as author
        timestamp:     ATOM_TIMESTAMP,              -- epoch zero
        message:       ATOM_MESSAGE,                -- fixed, inert -- the claim payload is
                                                       -- read as the log leaf this commit's
                                                       -- tree carries, never a second copy here
      }
      -- Genesis-once (atom-transactions.md [atom-genesis-and-facts]):
      -- exactly one claim commit per label, ever, parentless forever —
      -- no "subsequent claim", no ClaimPayload.prior or .governance
      -- (atom-transactions.md's Type Declarations dropped both). Ownership
      -- change (key rotation, transfer, seizure) is layered on afterward
      -- as a claim-owner fact commit chained onto THIS commit
      -- ([claim-fact-chain], FactCommitFormat below), never a second
      -- claim commit. The `src` field in the signed ClaimPayload ties
      -- the claim to its source revision cryptographically (the czd
      -- covers it); the git object's OWN hash carries no such binding
      -- (`[log-tree-opaque]`) -- git is storage, never a verification
      -- surface, in this design.

TYPE  FactCommitFormat = {
        tree:          LogTree,                      -- the record log's own tree, at the
                                                        -- point this fact leaf was appended
                                                        -- (ADR-0007 §7.1, §7.5) -- never the
                                                        -- well-known empty tree
        parent:        CharterCommit | ClaimCommit
                       | FactCommit,                  -- the scope's genesis commit (first
                                                        -- fact in this scope) or the previous
                                                        -- fact commit in this SAME scope's
                                                        -- own chain — never a different scope's
        author:        ATOM_AUTHOR,
        committer:     ATOM_AUTHOR,
        timestamp:     ATOM_TIMESTAMP,
        message:       ATOM_MESSAGE,                   -- fixed, inert -- the fact payload is
                                                         -- read as the log leaf this commit's
                                                         -- tree carries, never a second copy
      }
      -- The git-level container for a fact extending a charter's or a
      -- claim's own scope (atom-transactions.md [charter-owner-fact],
      -- [claim-owner-fact]) — never a new genesis, always chained onto
      -- the permanent genesis commit or an earlier fact in the SAME
      -- scope's own chain, never across scopes. Reuses the shape the
      -- (now-retired) claim-replacement mechanism used — log-tree
      -- carrying, git-parent chaining for a cheap, walkable
      -- audit trail — for a different purpose: the chain no longer
      -- replaces an identity, it layers status on a fixed one, exactly
      -- as [publish-fact-chain](#publish-fact-chain) already layers
      -- facts onto a fixed publish identity at the publish scope.
      -- `FactPayload`'s own field shape
      -- (its `typ` value; the owner-vs-governance discriminator
      -- atom-transactions.md's [claim-owner-fact] requires; a `retracts`
      -- analogue) is NOT settled here: atom-transactions.md explicitly
      -- defers "the concrete fact-record wire shape" to this document
      -- and to atom-backend-contract.md as an Open Item, and it remains
      -- open pending a follow-up amendment — this type declares the git
      -- container, not the payload's protocol-level content (Open
      -- Questions #6).

TYPE  PublishChainCommitFormat = {
        tree:          LogTree,                   -- the record log's own tree, at the
                                                     -- point this publish's or fact's leaf
                                                     -- was appended (ADR-0007 §7.1, §7.5) --
                                                     -- never the well-known empty tree
        parent:        AtomCommit                 -- the content commit (chain's
                       | PublishChainCommit,        -- first commit only) or the
                                                      -- previous commit in THIS
                                                      -- publish's own chain (every
                                                      -- later, fact-extending commit)
        author:        <real author>,             -- the publisher, or fact signer
        committer:     <real committer>,
        timestamp:     <real timestamp>,          -- time of publish or fact
        message:       ATOM_MESSAGE,              -- fixed, inert -- the signed record is
                                                     -- read as the log leaf this commit's
                                                     -- tree carries, never a second copy here
      }
      -- The chain's first commit (the publish itself, before any facts
      -- extend it) carries the content commit as its ONLY git parent —
      -- the auxiliary reachability edge that keeps the content object
      -- alive from this chain, the same mechanism the anchor's own
      -- record log uses to keep a source commit alive from its
      -- checkpoints (Decision 5 of the record-anchored publishing
      -- amendment). This is the ONLY path by which a consumer obtains
      -- a package's content; the chain has no other edge to it, and no
      -- edge at all to a source revision commit. Every later commit in
      -- the chain — a fact about this publish (yank, security
      -- advisory, deprecation, mode promotion, meta revision) — parents
      -- onto the previous commit in the SAME chain instead, exactly as
      -- FactCommitFormat already does for the charter and claim scopes.
      -- The `claim` czd in the root's PublishPayload provides the
      -- cryptographic binding to the authorizing claim; no extra git
      -- header is needed, the signed payload — read from the leaf this
      -- commit's tree carries — is the sole binding.
      -- Publish and fact payloads alike MAY include additional
      -- user-defined fields beyond the required set — a property of
      -- the Coz message format and the protocol's payload structure,
      -- not of this git encoding.
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

#### [anchor-is-genesis]

_(retired 2026-07-08 — superseded by the charter
amendment)_: The former rule (anchor = raw bytes of the repository's
genesis commit ObjectId, discovered by walking the claim's lineage to
its oldest parentless root) is retired. The anchor is backend-agnostic:
`Anchor := czd(charter₀)` per atom-transactions.md `[charter-anchor]`.
No graph-walking anchor discovery exists; the backend resolves and
verifies the charter instead (storage encoding: Open
Questions #5). The genesis commit survives only transitively — the
charter's `src` commits to its entire ancestry.

```claim
kind: constraint
evaluator: none
```

#### [anchor-hash-agile]

The anchor is a coz digest (`czd(charter₀)`),
not a git ObjectId, so the anchor does not inherit the repository's
object hash algorithm at all. Git hash agility (SHA-1 → SHA-256) is
handled by the charter, not by the backend — and genesis-once
([atom-genesis-and-facts](atom-transactions.md#atom-genesis-and-facts))
forecloses the mechanism this constraint used to name for it: there is no successor
charter to chain continuity across a re-hash to. A re-hash rewrites
history; the re-hashed repository is, unconditionally, a distinct
atom-set (a genuine fork, its own new charter, its own new anchor) —
never a continuation of the pre-rehash anchor by any in-protocol
mechanism. The backend MUST treat the anchor as opaque bytes and MUST
NOT rehash, truncate, or transform them.

```claim
kind: constraint
evaluator: none
depends: [atom-genesis-and-facts]
```

#### [log-tree-opaque]

The anchor's record log's own git-tree
internal layout — leaf storage keys, internal node encoding, anything
about HOW a specific leaf is located inside a `LogTree` — is NOT
specified by this document. ADR-0007 §7.1–§7.3 governs the log's
abstract shape (one continuous, append-only, `eml`-computed Merkle
structure per anchor) and explicitly leaves its git-side storage
layout an open item there; a conforming implementation MAY choose any
internal layout the log's own storage interface accepts. Every
constraint in this document that names `LogTree` depends only on the
external property that the resulting git tree OBJECT is shared,
byte-identically, between the log's own checkpoint commit and every
materialized-view commit recording the same point in the log's
history — never on any detail of what is inside it. This is why a
CharterCommitFormat, ClaimCommitFormat, FactCommitFormat, or
PublishChainCommitFormat commit's `tree` field can be stated
normatively ([charter-commit-format], [claim-detached],
[charter-fact-chain], [claim-fact-chain], [publish-commit-format],
[publish-fact-chain]) without this document inventing the log's own
internal encoding.

```claim
kind: constraint
evaluator: none
```

#### [snapshot-deterministic]

An atom commit MUST be deterministic given
the same inputs. Given the same content tree alone, any party MUST
produce an identical commit object (and therefore identical
ObjectId) — identity carries no source binding. This is achieved by
fixing: (1) author and committer to `ATOM_AUTHOR`, (2) timestamps to
`ATOM_TIMESTAMP`, (3) commit message to `ATOM_MESSAGE`, (4) no parent
commits, (5) no GPG signatures. The commit hash is the `dig` field in
the publish payload. A direct, real-world consequence: identical
content published from two different source revisions produces ONE
atom commit, not two — the atom commit alone cannot distinguish them,
by design (`[snapshot-src-header]`, retired, carried this
distinction; the source binding now lives on the publish commit that
reaches this object — `[publish-commit-format]` — and in the signed
record, never inside the content object itself).

```claim
kind: constraint
evaluator: test
```

#### [snapshot-parentless]

An atom commit MUST have zero parents. Atoms
are detached subtrees — they carry no history. Provenance is recorded
in the publish record and, at the git level, in the publish commit
that carries this object as its own git parent
(`[publish-commit-format]`) — never in the atom
commit's own parent chain, which stays empty. (Satisfies
[atom-detached](atom-transactions.md#atom-detached).)

```claim
kind: constraint
evaluator: none
because: [atom-detached]
```

#### [snapshot-src-header] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required an atom commit to
carry exactly one extra header, `src`, binding the snapshot to its
source revision at the git-object level, and required that header to
match the publish payload's own `src` field as a quick pre-signature
integrity check. The corpus's own record-anchored publishing
amendment rules this out directly: identity MUST become a pure
function of the content tree alone
([snapshot-deterministic](#snapshot-deterministic)), because the
publish ref no longer points at a detached content object a client
could use without first reading the signed record — it points at the
record itself, which already declares `src`, and the record
checkpoint that anchors the record in the anchor's own history
intrinsically binds the source revision one layer up. Asserting the
same fact a second time, in a git header on the content object, would
make that header load-bearing for a check nothing signed actually
requires — git is storage, never a verification surface, in this
design. No extra header survives on an atom commit; there is nothing
left for this constraint to bind.

```claim
kind: constraint
evaluator: none
```

#### [temporal-vector]

The atom protocol's git backend enforces a
three-point temporal ordering — the **authenticity vector**
([charter-ancestry](atom-transactions.md#charter-ancestry)):

```
charter src → claim src → publish src
```

Specifically: (1) the claim's payload `src` MUST point to a commit that
is a descendant of (or equal to) the charter's `src`
(verifiable by walking the DAG from the claim's `src`), AND (2) the
source revision referenced by `src` in the publish payload MUST be at
or after the claim's `src` in the repository's history (i.e., publish
`src` is a descendant of, or equal to, the claim's `src`). History
prior to `charter.src` is visible but unowned by the set — orphaned
unless re-claimed after the chartering point.

An atom MAY be published from the claim's `src` commit itself (when no
code changes are needed), but MUST NOT be published from a commit that
precedes the claim's `src`. This ensures that a claim establishes a
temporal floor — only content at or after the claim is publishable.

**Per-publish verification:** To verify provenance of a published atom,
the verifier MUST check that the publish's `src` is genuinely in the
repository's history and is at or after the claim's `src`. This uses
treeless commit-only fetching (`tree:0`) for efficiency. The check
relies on the backend actually instantiating
[backend-ancestry-sound](atom-backend-contract.md#backend-ancestry-sound)
and [backend-ancestry-queryable](atom-backend-contract.md#backend-ancestry-queryable),
over the parent relation [ancestry-hash-committed](#ancestry-hash-committed)
commits into the object hash — the vector quantifies over that relation
and no other.

```claim
kind: constraint
evaluator: test
depends: [charter-ancestry, backend-ancestry-sound, backend-ancestry-queryable, ancestry-hash-committed]
```

#### [charter-commit-format]

A charter transaction MUST be stored as a
**charter commit**: a commit whose `tree` is the anchor's record-log
tree at the point the charter genesis leaf was appended
(`LogTree`, Type Declarations, above — `[log-tree-opaque]`), whose
author and committer are `ATOM_AUTHOR` at `ATOM_TIMESTAMP`, and whose
commit `message` is fixed and inert (`ATOM_MESSAGE`) —
[log-anchored-message-fixed](#log-anchored-message-fixed). The charter
`CozMessage` (JSON, `typ: "atom/charter"`) is read as the log leaf
this commit's `tree` carries, never from the commit's own message
([record-is-log-leaf](#record-is-log-leaf)). A charter commit MUST
have **zero git parents** — genesis-once
([atom-genesis-and-facts](atom-transactions.md#atom-genesis-and-facts))
means there is exactly one charter commit
per anchor, ever, and it is always parentless; "founding" and
"successor" are not two cases, because there is no successor case. The
charter's identity is its czd (`czd(charter)`), computed over the
signed charter payload's own bytes — never over this git object — and
this single commit's czd is the atom-set `Anchor` (`[charter-anchor]`),
permanently.

This adopts the claim commit's *object shape* (log-tree-carrying,
lightweight object) for the same reason: cheap to fetch,
isolated from the main branch DAG, protected from GC by its own ref
([charter-ref-by-czd](#charter-ref-by-czd)). Unlike the (retired)
successor-chaining design this constraint used to specify, there is no
signed `prior` field to walk — `CharterPayload` carries none
(Type Declarations, above) — because there is nothing for it to chain
to. Owner-set membership change and key rotation happen afterward as a
charter-owner fact commit chained onto this one via a git parent link
([charter-fact-chain](#charter-fact-chain)) — the parent chain now encodes fact
history layered on a fixed identity, never a competing claim to be that
identity.

```claim
kind: constraint
evaluator: test
depends: [charter-ref-by-czd, atom-genesis-and-facts]
```

#### [charter-message-is-coz] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required a charter commit's
own `message` field to carry a valid, complete charter `CozMessage`
directly, and to pass Coz verification as read from that field. The
record-anchored publishing design supersedes this for every
log-anchored commit this backend writes: the signed record is a leaf
inside the record log, carried in the commit's `tree`
([charter-commit-format](#charter-commit-format)), never a second copy
in the commit's own `message`. `[log-anchored-message-fixed]` and
`[record-is-log-leaf]`, below, restate the identical Coz-verification
obligation against the log leaf instead of the message field — the
requirement that a charter's signed payload MUST parse and MUST pass
Coz verification is unchanged; only where a verifier reads the bytes
from has changed.

```claim
kind: constraint
evaluator: none
```

#### [log-anchored-message-fixed]

Every log-anchored commit this backend
writes — a charter commit, a claim commit, a charter or claim fact
commit, or any commit in a publish's materialized view chain
(`CharterCommitFormat`, `ClaimCommitFormat`, `FactCommitFormat`,
`PublishChainCommitFormat`, Type Declarations, above) — MUST have a
`message` field equal to `ATOM_MESSAGE` (fixed, empty) and MUST NOT
carry a `CozMessage`, or any other signed or unsigned content, in its
own `message`. This is the git-encoding half of
[log-tree-opaque](#log-tree-opaque)'s doctrine: the record lives once,
as a log leaf inside the commit's `tree`; a second copy in `message`
would be an unchecked duplicate with nothing to keep it in agreement
with the leaf ([record-is-log-leaf](#record-is-log-leaf) states the
positive half — where the record IS read from).

```claim
kind: constraint
evaluator: test
depends: [log-tree-opaque]
```

#### [record-is-log-leaf]

For any log-anchored commit this backend
writes, the signed record it represents (a `CharterPayload`,
`ClaimPayload`, `FactPayload`, `PublishPayload`, or amendment payload)
MUST be read as the corresponding leaf inside the record log — the
`LogTree` the commit's own `tree` field carries
([log-tree-opaque](#log-tree-opaque)) — and MUST NOT be read from, or
expected to be present in, the commit's own `message`
([log-anchored-message-fixed](#log-anchored-message-fixed)). A record
so read MUST be parseable as a valid, complete `CozMessage` JSON
object whose payload's `typ` matches the scope this commit records
(`atom/charter`, `atom/claim`, a fact `typ`, or `atom/publish`), and
MUST pass Coz verification. This is the log-anchored replacement for
every one of this document's retired per-scope message-validation
constraints (`[charter-message-is-coz]`,
`[claim-message-is-coz]`,
`[publish-message-is-coz]`): the verification
obligation these named is unchanged, only where the bytes are read
from.

```claim
kind: constraint
evaluator: test
depends: [log-tree-opaque, log-anchored-message-fixed]
```

#### [charter-succession-via-prior] — RETIRED 2026-08-05

**Retired, not amended.** This constraint specified walking a
**successor-charter chain** by the signed `prior` czd of each
`CharterPayload`, resolving "the effective charter" as the chain's head,
and required consumers to fail closed on two charters sharing one
`prior` (a divergent-successor fork). ADR-0007 §3 forecloses the shape
this constraint walked: charter is one of the three genesis record
kinds "never reissued, never superseded by a record of the same kind"
(`[atom-genesis-and-facts]`) — the same rule
atom-transactions.md already retired `[charter-succession]` and
`[charter-succession-linear]` under. `CharterPayload` no longer carries
`prior` (Type Declarations, above); there is no chain to walk and no
"effective charter" to select, because there is exactly one charter,
always.

What this constraint protected — a bounded, chain-length-only cost to
reach the charter governing a given anchor, and a structural,
enumerable git encoding that lets a resolver observe two claimants to
the same authority directly rather than trusting a mutable pointer — is
real and still required. `[charter-fact-chain]`, below, provides the
bounded-cost property for the charter's fact history (which now grows
where the old succession chain used to); the second property —
divergent-claimant detection — no longer has a succession chain to
apply to, because a second parentless charter commit for the same
anchor is not a divergent successor, it is a malformed or attacking
`refs/atom/charter/d/*` entry: `[no-second-charter-genesis]`, Forbidden
States, below, covers it directly.

```claim
kind: constraint
evaluator: none
```

#### [charter-fact-chain]

Owner-set membership change and key rotation
(atom-transactions.md `[charter-owner-fact]`) are stored as a **charter
fact commit** (`FactCommitFormat`, Type Declarations, above): a
log-tree-carrying commit whose `parent` is the charter commit itself (first
fact) or the previous charter fact commit (subsequent facts), forming a
chain rooted at, but never replacing, the permanent charter commit. The
ref `refs/atom/charter/d/{charter_czd}` (unchanged — the segment is
still the charter's own, permanent czd) MUST point to the **tip** of
this fact chain rather than the bare charter commit once any fact
exists; a resolver reaches the charter's own `src` and identity by
walking the chain's `parent` links to its root, which MUST be the
parentless charter commit (`[charter-commit-format]`) — never a second
genesis (`[no-second-charter-genesis]`). This is the direct structural
analogue of `[publish-fact-chain]`'s own chain: a fixed identity
at the root, status layered on top by appending, never replacing.

```claim
kind: constraint
evaluator: test
depends: [charter-commit-format, no-second-charter-genesis]
```

#### [charter-src-reachable]

A resolver MUST be able to obtain the
charter's `src` for the `[temporal-vector]` ancestry check by resolving
the charter commit (walking `[charter-fact-chain]`'s tip back to its
root, if any facts have landed) and reading the `src` field of its
signed `CharterPayload` — the log leaf the root commit's own `tree`
carries ([record-is-log-leaf](#record-is-log-leaf)) — one further read
once the root is reached.
Locating the charter for a known anchor costs exactly one ref lookup
(`refs/atom/charter/d/{charter_czd}`, keyed by the anchor's own czd —
`[charter-ref-by-czd]`) plus the fact chain's own walk to its root,
whose length is bounded by the fact count for that ONE anchor
(`[no-second-charter-genesis]`'s exactly-one-parentless-root guarantee)
— O(1) ref resolution, O(fact count) walk, never O(succession depth)
or O(family size). This is the bounded replacement for the retired
genesis-graph walk (`[anchor-oldest-root]`), which walked unbounded
commit ancestry.

```claim
kind: constraint
evaluator: test
depends: [no-second-charter-genesis]
```

#### [dev-atom-unchartered-local]

A development atom built from a git
source that carries **no charter yet** MUST be treated as **local class**
(atom-model.md §5): it acquires no set anchor, because the anchor is
`czd(charter₀)` and no charter exists yet to supply one. For the
`refs/atom/dev/{anchor}/{label}/{dev_version}` ref segment, such an atom
MUST use the same well-known constant sentinel anchor as a filesystem
source ([fs-source-contract](atom-transactions.md#fs-source-contract)) — an unchartered git
source is, for identity purposes, exactly as anchorless as a filesystem
source. A local-class dev atom is available for local build consumption
only; it carries no protocol trust and cannot be claimed or published
until the set is chartered, after which atoms built from
charter-descended revisions anchor to `czd(charter₀)` normally
(`[fs-ingest-transition]`).

```claim
kind: constraint
evaluator: none
depends: [fs-source-contract]
```

#### [claim-detached]

A claim commit MUST have the anchor's
record-log tree at the point the claim genesis leaf was appended as
its `tree` (`LogTree`, `[log-tree-opaque]`) and no extra headers, and
MUST be parentless — genesis-once (`[atom-genesis-and-facts]`): there
is exactly one claim commit per label, ever; a claim commit with a
parent is malformed (there is no "subsequent claim"). Ownership change
(key rotation, transfer, seizure) is a claim-owner **fact** commit
chained onto the claim afterward (`[claim-fact-chain]`), not a second
claim commit — see that constraint for the chain's own shape and the
resolver's walk. The claim's `src` is carried in the signed
`ClaimPayload`, read as the log leaf this commit's `tree` carries
([record-is-log-leaf](#record-is-log-leaf)) — the czd computed over
those payload bytes is what ties the claim to its source revision
cryptographically; the git object's own hash carries no such binding
(`[log-anchored-message-fixed]`). This design ensures claims are
lightweight, the full ownership history is structurally auditable by
walking the fact chain, and claim objects are isolated from the main
branch DAG.

```claim
kind: constraint
evaluator: test
```

#### [claim-message-is-coz] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required a claim commit's
own `message` field to carry a valid, complete claim `CozMessage`
directly. Superseded for the same reason as
`[charter-message-is-coz]`:
[record-is-log-leaf](#record-is-log-leaf) restates the identical
Coz-verification obligation against the log leaf the commit's `tree`
carries instead of the message field.

```claim
kind: constraint
evaluator: none
```

#### [claim-fact-chain]

Ownership change on a label (key rotation,
transfer, or a charter-fold seizure — atom-transactions.md
`[claim-owner-fact]`'s owner-fact/governance-fact split) is stored as a
**claim fact commit** (`FactCommitFormat`, Type Declarations, above):
a log-tree-carrying commit whose `parent` is the claim commit itself (first
fact) or the previous claim fact commit (subsequent facts), forming a
chain rooted at, but never replacing, the permanent claim commit. The
ref `refs/atom/claims/pub/{label}` (registry) MUST point to the
**tip** of this fact chain — unchanged in name and role from the
retired design, but the tip is now a fact, never a competing claim
commit. A resolver reaches the claim's own `src` and owner-at-genesis
by walking the chain's `parent` links to its root, which MUST be the
parentless claim commit (`[claim-detached]`) — never a second genesis
(`[no-second-claim-genesis]`). This is the direct structural analogue
of `[charter-fact-chain]` at the claim scope, and of
`[publish-fact-chain]` at the publish scope: one
unified shape (a fixed genesis object, a chain of appended facts) at
every scope this document instantiates, matching ADR-0007 §3's "one
unified fact mechanism serves every scope."

```claim
kind: constraint
evaluator: test
depends: [claim-detached, no-second-claim-genesis]
```

#### [publish-tag-targets-correct] — RETIRED 2026-08-23

**Retired, not amended.** This constraint governed what a publish
*tag* MAY target: an atom commit for the initial publish, the
previous tag for an update, nothing else. There is no publish tag
under the record-anchored publishing amendment — a publish is a
materialized view chain of ordinary commits, not a chain of tag
objects — so there is nothing left with a tag's targeting rule to
police. `[publish-commit-format]`, below, states the analogous
targeting rule for the chain's own commits.

```claim
kind: constraint
evaluator: none
```

#### [publish-commit-format]

The chain's first commit — the
publish itself — MUST have the anchor's record-log tree at the point
the publish leaf was appended as its `tree` (`LogTree`,
`[log-tree-opaque]`) — never the well-known empty tree — and MUST
target the atom (content) commit
([snapshot-parentless](#snapshot-parentless)) as its sole git parent; it MUST NOT target
any other object, and MUST NOT be itself parentless. Every later
commit in the SAME publish's chain (a fact about this publish) MUST
likewise carry the log's own tree at ITS append point, MUST target the
immediately preceding commit in that chain instead, and
MUST NOT target the content commit directly — only the chain's first
commit reaches it. A commit in a publish chain MUST NOT target a
regular commit, tree, or blob outside these two cases
(`PublishChainCommitFormat`, Type Declarations, above). This is the
ONLY path by which a consumer obtains the package's content, and this
chain has no path at all to a source revision commit.

```claim
kind: constraint
evaluator: test
depends: [snapshot-parentless]
```

#### [publish-tag-claim-binding] — RETIRED 2026-08-23

**Retired, not amended.** This constraint bound a publish *tag's*
CozMessage payload to its authorizing claim by the payload's `claim`
field. The binding mechanism is unchanged in substance — it was
always the signed payload, never a git header — but there is no
publish tag left to carry it. `[publish-claim-binding]`, below,
restates the identical requirement for the publish chain's first
commit.

```claim
kind: constraint
evaluator: none
```

#### [publish-claim-binding]

The publish chain's first commit's
CozMessage payload — read as the log leaf the commit's `tree` carries
([record-is-log-leaf](#record-is-log-leaf)) — MUST contain a `claim`
field whose value is the czd of the authorizing claim. The claim
commit is looked up via `refs/atom/claims/d/{czd}` (or
`refs/atom/claims/pub/{label}` in the originating registry). No extra
git header is needed — the signed payload is the sole binding.

```claim
kind: constraint
evaluator: test
```

#### [publish-tag-message-is-coz] — RETIRED 2026-08-23

**Retired, not amended.** This constraint validated a publish tag's
message body as a CozMessage. There is no publish tag;
`[publish-message-is-coz]`, below, restates the identical requirement
for the publish chain's first commit.

```claim
kind: constraint
evaluator: none
```

#### [publish-message-is-coz] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required a publish chain's
first commit to carry a valid, complete publish `CozMessage` directly
in its own `message` field. Superseded for the same reason as
`[charter-message-is-coz]` and
`[claim-message-is-coz]`:
[record-is-log-leaf](#record-is-log-leaf) restates the identical
Coz-verification obligation against the log leaf the commit's `tree`
carries instead of the message field — the requirement that a
publish's signed payload MUST parse and MUST pass Coz verification is
unchanged; only where a verifier reads the bytes from has changed.

```claim
kind: constraint
evaluator: none
```

#### [tag-chain-immutable] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required a publish update
to append a new tag targeting the previous tag, never deleting old
tag objects, so lock pins stayed resolvable and the full update
history stayed walkable. There is no tag chain to keep immutable.
What it protected — every prior state of a publish staying reachable,
never rewritten or dropped — is now the general append-only property
every protocol object already has (`[odb-immutable]`),
applied to the publish chain's own commits the same way it already
applies to the charter's and claim's fact chains
(`[charter-fact-chain]`, `[claim-fact-chain]`). `[publish-fact-chain]`,
below, states the publish scope's own chain-extension rule this
constraint's audit-trail argument now falls out of directly.

```claim
kind: constraint
evaluator: none
```

#### [publish-fact-chain]

A fact about an existing publish
(yank, security advisory, deprecation, supersession, a
reproducibility-mode promotion or demotion per atom-model.md §6, or a
`meta` revision — previously carried by a publish tag update,
[publish-update-transition](#publish-update-transition)) is stored as
a commit in `PublishChainCommitFormat`'s fact-extending shape (Type
Declarations, above): its `parent` is the chain's first commit (the
publish itself) for the first fact, or the previous fact commit in
the SAME chain for a later one — never the content commit, which only
the chain's first commit ever targets
([publish-commit-format](#publish-commit-format)). The ref
`refs/atom/pub/{label}/{version}` MUST point to the chain's **tip**
once any fact exists, exactly as `[charter-fact-chain]` and
`[claim-fact-chain]` already require at their own scopes — the same
unified fact mechanism, extended here to the publish scope. A
resolver reaches the publish's own identity and content by walking
the chain's `parent` links to its root, which MUST be the chain's
first commit; every earlier fact and the first commit itself remain
in the repository's history, walkable from the tip, since nothing in
this design deletes a chain commit
([odb-immutable](#odb-immutable)). Every commit in this chain — the
first as much as any fact — carries the anchor's own record-log tree
at the point ITS leaf was appended, never the well-known empty tree
([log-tree-opaque](#log-tree-opaque)). This is not a performance nicety: a client
already holding a chain commit can recompute the log's own root from
the tree it carries and detect a relevant fact the chain's own edges
omitted, exactly the "traversal accelerator, never a fetch reducer"
property ADR-0007 §7.5 states for the record portion of every
materialized view — publish included. An earlier revision of this
constraint claimed the opposite (that this chain's commits carry the
well-known empty tree because the signed payload lives in the message
instead) — that was wrong, not merely imprecise: the signed payload
was never load-bearing in the commit's own `message`
([log-anchored-message-fixed](#log-anchored-message-fixed),
[record-is-log-leaf](#record-is-log-leaf)), and a synthesized empty
tree would have left a client holding a publish chain with no way to
check it against the log it is drawn from.

```claim
kind: constraint
evaluator: test
depends: [publish-commit-format, odb-immutable, log-tree-opaque, log-anchored-message-fixed, record-is-log-leaf]
```

#### [coz-bit-perfect]

All `CozMessage` JSON stored as a record-log
leaf ([record-is-log-leaf](#record-is-log-leaf)) MUST be preserved
bit-for-bit. The backend MUST NOT reformat, re-serialize, or alter the
JSON in any way — this bound applied to a git commit's own `message`
field before the record-anchored publishing design and now applies to
the log leaf a commit's `tree` carries instead, since that leaf is
where a `CozMessage`'s bytes actually live for every log-anchored
commit this backend writes. (Satisfies
[backend-bit-perfect](atom-transactions.md#backend-bit-perfect);
discharges [backend-carriage-bit-perfect](atom-backend-contract.md#backend-carriage-bit-perfect).)

```claim
kind: constraint
evaluator: test
depends: [record-is-log-leaf]
because: [backend-bit-perfect, backend-carriage-bit-perfect]
```

Verification will store a `CozMessage`, retrieve it, and byte-compare
against the original.

#### [odb-immutable]

The backend MUST treat the object database as
append-only for protocol objects (charter/claim/atom commits, publish
commits): a protocol object, once written, MUST NOT be mutated in place,
and MUST NOT be deleted while protocol-reachable (the protocol refs
define reachability). Deletion is permitted only through
garbage collection of protocol-unreachable objects. Git's
content-addressed object store provides this by construction; the
constraint makes the reliance explicit rather than implicit.
(Discharges [backend-store-immutable](atom-backend-contract.md#backend-store-immutable).)

```claim
kind: constraint
evaluator: none
because: [backend-store-immutable]
```

#### [refs-sole-mutable]

Refs are the backend's ONLY mutable protocol
state. Objects are immutable ([odb-immutable](#odb-immutable)) and ancestry is
derived from hash-committed object content
([ancestry-hash-committed](#ancestry-hash-committed)), so every protocol state transition is
exactly a refs transaction. Implementations MUST NOT keep protocol
state in any side store (index files, database tables, packed
metadata) that can diverge from the refs — such state MAY exist only
as a cache reconstructible from refs + objects. This is what makes
liveness-by-reachability, rollback-by-old-ref, and GC arguments
transfer to the intent store. (Discharges
[backend-refs-sole-mutability](atom-backend-contract.md#backend-refs-sole-mutability).)

```claim
kind: constraint
evaluator: none
depends: [odb-immutable, ancestry-hash-committed]
because: [backend-refs-sole-mutability]
```

#### [ancestry-hash-committed]

A git ObjectId commits to its entire
ancestry: a commit's parent OIDs are part of the hashed commit
preimage, so asserting false ancestry for an existing OID requires a
hash collision. Every ancestry-dependent check in this spec
([temporal-vector](#temporal-vector), [no-backdated-src](#no-backdated-src),
[anchor-vector-authenticity](#anchor-vector-authenticity)) quantifies over THIS parent relation —
the hash-committed one — never over whatever a query layer happens to
answer. (Discharges
[backend-ancestry-sound](atom-backend-contract.md#backend-ancestry-sound).)

```claim
kind: constraint
evaluator: test
because: [backend-ancestry-sound]
```

#### [ancestry-query-path]

Protocol ancestry queries MUST resolve
parents from hash-committed object content with object replacement
disabled (`--no-replace-objects` semantics): `refs/replace/*` and
grafts MUST NOT influence [temporal-vector](#temporal-vector), [no-backdated-src](#no-backdated-src),
or [anchor-vector-authenticity](#anchor-vector-authenticity) checks — replacement rewrites the
operational parent function without changing the OID, which is
exactly the divergence [ancestry-hash-committed](#ancestry-hash-committed) exists to exclude.
Ancestry verification MUST NOT be attempted against a depth-limited
(shallow) clone: a shallow boundary is indistinguishable from a true
root to a naive walk, so the check MUST fail closed rather than pass;
treeless fetching (`tree:0`, `[temporal-vector]`) is the sanctioned
minimal mode — it limits blobs, never ancestry.

```claim
kind: constraint
evaluator: none
depends: [ancestry-hash-committed]
```

#### [czd-oid-disjoint]

The seam law's git instance
([backend-seam-typed](atom-backend-contract.md#backend-seam-typed)): `Czd` values and
git `ObjectId`s are disjoint types. The only OID-typed protocol
identities are the payload fields `dig` and `src`; every other
protocol identity (anchor, claim czd, publish czd) is a coz digest —
the atom snapshot carries no OID-typed field of its own at all
(`[snapshot-src-header]`, retired: the atom commit's identity is a
pure function of its tree). Implementations MUST represent the two as
distinct types such that any czd↔OID comparison is rejected at the
type level. Ref paths are typed encodings, not free strings: the
identity-bearing segments come in two families — czd-hex
(`refs/atom/claims/d/{claim_czd}`, the `{anchor}` segment of dev
refs) and blake3-hex (`refs/atom/d/{blake3(publish_czd)}`) — each
family renders exactly one sort, parsing MUST rehydrate the segment to
that sort, and comparison of raw segments across families is ill-typed.

```claim
kind: constraint
evaluator: test
depends: [backend-seam-typed]
```

#### [oid-hash-inheritance]

`dig` and `src` inherit the repository's
object hash algorithm; under git's SHA-1 default they are NOT
collision-resistant, unlike every czd-sorted identity
([anchor-hash-agile](#anchor-hash-agile)). This inheritance MUST be documented wherever
`dig` is presented as an artifact identity. The closure for `dig` is
the protocol-level, publisher-signed `content_hash` field
([content-hash-is-tree-digest](atom-transactions.md#content-hash-is-tree-digest)), not any
git-specific mechanism — the git instantiation carries it bit-perfectly
like any other payload field and does nothing further. `src`'s
inheritance is NOT closed by `content_hash` or by any mechanism this
specification defines: `src`-anchored ancestry verification
([temporal-vector](#temporal-vector)) depends on this repository's own commit-parent
graph, whose links are this repository's own object hashes, and is
irreducible without migrating this repository's history to a stronger
hash (atom-transactions.md `[content-hash-obligation]`'s "Known
limit"). (Registers [backend-hash-strength](atom-backend-contract.md#backend-hash-strength)
for the git instantiation.)

```claim
kind: constraint
evaluator: none
depends: [anchor-hash-agile, content-hash-is-tree-digest]
because: [backend-hash-strength]
```

#### [src-hash-kind-disambiguated] — RETIRED 2026-08-23

**Retired, not amended.** This constraint disambiguated the hash
algorithm of the atom commit's `src` extra header by its hex length
(40 characters SHA-1, 64 SHA-256), since that header carried no
algorithm marker of its own. The header itself is gone
(`[snapshot-src-header]`, retired): the atom
commit carries no OID-typed field to disambiguate. The underlying
caution this constraint carried — a 64-hex-character SHA-256 OID and
a 64-hex-character BLAKE3 `content_hash` digest are the same length
and MUST NOT be inferred from one another — still applies wherever a
git OID and a `content_hash` might otherwise be confused
([oid-hash-inheritance](#oid-hash-inheritance)), but has no `src`
header left to be a caution ABOUT.

```claim
kind: constraint
evaluator: none
```

#### [single-active-claim-registry]

In a registry (source repository), at most
one claim commit MUST ever exist per label — genesis-once
(`[atom-genesis-and-facts]`), not "at most one *active*" (that phrasing
presumed the retired replace-a-claim design). Ownership change (e.g.,
key compromise) is a claim fact commit chained onto the one claim,
never a new claim commit (`[claim-fact-chain]`). The ref
(`refs/atom/claims/pub/{label}`) points to the tip of the claim's own
fact chain, updated as facts land. All historical facts are reachable
by walking the chain from the tip to its root, the permanent claim
commit.

```claim
kind: constraint
evaluator: test
```

#### [store-claim-disambiguation]

In a store, multiple publishes for the
same `AtomId` MAY coexist (from different sources/forks). The store's
ref layout MUST disambiguate them by `blake3(publish_czd)`. Distinct
publishes produce distinct publish czds and therefore distinct flat ref
keys — disambiguation is guaranteed by the cryptographic uniqueness of
the publish czd, without coordination.

```claim
kind: constraint
evaluator: test
```

### Ref Layout

#### Registry Refs (source repository)

```
refs/atom/claims/pub/{label}                       → claim commit [→ fact chain] → (tip)
refs/atom/pub/{label}/{version}                  → publish commit [→ fact chain] → (tip)
```

The claim ref is the tip of the claim's own fact chain (`[claim-fact-
chain]`): claim-owner facts (key rotation, transfer, seizure) parent to
the previous fact commit, or to the claim commit itself for the first
fact — never a second claim commit. All historical facts are reachable
by walking the chain to its root, the one permanent claim commit — no
separate `claims/d/` refs are needed in the registry. There is no
dedicated `refs/atom/src/` protective-ref family: GC protection for a
source revision commit is no longer a ref an implementer maintains by
hand — the anchor's own record log carries it intrinsically for every
publish ever made, transitively, from a single ref, and a registry
implementing this specification does not additionally write or read
`refs/atom/src/*`.

##### [registry-ref-label-unique]

Within a single registry, the `{label}`
segment MUST be unique. No two atoms in the same registry MAY share a
label. (Satisfies [atomid-per-source-unique](atom-transactions.md#atomid-per-source-unique).)

```claim
kind: constraint
evaluator: none
because: [atomid-per-source-unique]
```

##### [registry-ref-claim]

The ref `refs/atom/claims/pub/{label}` MUST
point to the claim commit for that label, or — once any claim-owner
fact exists — the tip of its fact chain (`[claim-fact-chain]`). When a
fact lands, this ref MUST be updated to point to the new fact commit.
Every earlier fact and the claim commit itself remain in the
repository's history, walkable from the tip. Claim refs are separated
from atom version refs to avoid polluting the version subtree. This is
one of three per-scope materialized view chains this backend
maintains — charter, claim, publish
([registry-ref-version](#registry-ref-version) is the publish-scope
analogue) — and, like the other two, its target is derivable directly
from the scope's own key (`label`, here) with no discovery round trip.

```claim
kind: constraint
evaluator: test
```

##### [registry-ref-version]

The ref `refs/atom/pub/{label}/{version}`
MUST point to the tip of the publish's own materialized view chain:
the chain's first commit
([publish-commit-format](#publish-commit-format)) for a publish with
no facts yet, or the most recent fact commit
([publish-fact-chain](#publish-fact-chain)) once any fact has landed.
This chain shares no prefix with the charter or claim chains — it
starts at the publish itself, never at either of them, even though a
publish is logically under both a charter and a claim — and its name
is fully derivable from `(label, version)` alone, with no discovery
round trip: a resolver holding a lock entry can request this ref
alongside the claim and charter refs in one connection. The
`{version}` segment is the `RawVersion` string from the publish
payload.

```claim
kind: constraint
evaluator: test
depends: [publish-commit-format, publish-fact-chain]
```

##### [view-ref-liveness]

A registry MUST advance each of its
three materialized view refs — charter
([charter-ref-by-czd](#charter-ref-by-czd)), claim
([registry-ref-claim](#registry-ref-claim)), and publish
([registry-ref-version](#registry-ref-version)) — to the tip of the
chain reachable from the anchor's own record log for that scope, as
each new relevant fact lands. A view ref, at any point in time, MUST
point at or beyond the tip reachable by walking the record log's own
prior-record chain for that scope; it MUST NOT lag behind a fact the
log already carries. The worst a malfunctioning or malicious registry
can do by violating this is fail to advance a ref — never fabricate a
fact the log does not carry, since a materialized chain is only ever
a derived convenience index over the signed log, never itself
authoritative. Failing to advance a ref is therefore never
deceptive, only more expensive to detect than trusting the view: any
client willing to pay the cost of a full main-log fetch can check a
view ref's claimed tip against what the log itself reaches for that
scope. A client that fetches only view refs is trusting the
registry's liveness; a client that walks the main log directly trusts
nothing.

```claim
kind: constraint
evaluator: none
depends: [charter-ref-by-czd, registry-ref-claim, registry-ref-version]
```

#### Store Refs (consumer repository)

```
# Published atoms (d = digest-addressed)
refs/atom/d/{blake3(publish_czd)}                → publish commit [→ fact chain] → atom commit
refs/atom/claims/d/{claim_czd}                   → claim commit (shallow-fetched)

# Development atoms (unsigned, identified by the (anchor, label) pair, versioned)
refs/atom/dev/{anchor}/{label}/{dev_version}        → atom commit (no publish, no claims)
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

##### [store-ref-by-publish-czd]

Store version refs MUST be keyed by
`blake3(publish_czd)`, where `publish_czd` is the `czd` of the
**specific commit's `CozMessage` the ref addresses** — the chain's
first commit or any fact commit in the same materialized view chain
(atom-transactions.md `[amendment-field-classification]`) — never a
single identifier fixed for the whole chain. Each commit in a chain
has its own distinct `czd` and therefore its own store ref: landing a
new fact commit creates a NEW `refs/atom/d/{blake3(new_publish_czd)}`
ref without invalidating or rewriting any earlier commit's ref
([publish-update-transition](#publish-update-transition)'s POST states this for the amendment
case directly). This ensures global uniqueness — distinct commits,
base or amendment, produce distinct publish czds and therefore
distinct flat ref keys — and ensures a lock pinned to any commit in
the chain, not only the first, remains resolvable for as long as that
commit's ref persists.

```claim
kind: constraint
evaluator: test
depends: [publish-update-transition]
```

##### [store-claim-ref]

Each ingested claim MUST have a corresponding
ref at `refs/atom/claims/d/{claim_czd}` pointing to the claim commit or
— once any claim-owner fact exists — the tip of its fact chain
(`[claim-fact-chain]`). The full fact chain (including every historical
fact) SHOULD be fetched from the source — claim and fact commits alike
carry the record log's own tree, which stays small at this domain's
real cardinality (ADR-0007 §7.2), so the chain remains lightweight
even though its trees are no longer the well-known empty tree.
This ref serves two purposes: (1) protecting the claim commit and its
fact chain from garbage collection, and (2) enabling efficient claim
retrieval by czd for local verification. The chain also makes the full
ownership history locally auditable. If a publish's payload
references a claim czd but no corresponding claim commit exists in the
object store, this MUST be treated as an error.

```claim
kind: constraint
evaluator: test
```

##### [store-ownership-migration] — RETIRED 2026-08-05

**Retired, not amended.** This constraint assumed that an ownership
change produces a **new claim** with its own new czd, so that publishes
made before vs. after the change could be distinguished by which
claim's czd their `refs/atom/d/{blake3(publish_czd)}` key traced back
to. Genesis-once (`[atom-genesis-and-facts]`) forecloses the premise: a
claim's czd is fixed forever at first claim, and ownership change is a
fact chained onto that SAME, unchanging claim (`[claim-fact-chain]`) —
there is no "new claim" for a later publish to reference instead, so
this constraint's distinguishing mechanism no longer exists to state.

What it protected — that a consumer can tell whether a given publish
was authorized under the current owner or an earlier one — is real and
is now answered a different way: every publish's payload still
references the one, permanent claim czd (`[publish-claim-binding]`
is unaffected), and "which owner was in force" is resolved by walking
the claim's fact chain to the state that was current at the publish's
signing time (`[publish-transition-git]`'s claim-authorization PRE),
not by ref-key structure. No git-side distinguishing mechanism is
needed beyond what `[claim-fact-chain]` already provides.

```claim
kind: constraint
evaluator: none
```

##### [store-claim-cleanup]

When a store ref
`refs/atom/d/{blake3(publish_czd)}` is deleted (e.g., cache eviction),
the backend SHOULD check whether any remaining store refs reference the
same claim czd (by inspecting the publish payloads reachable from
surviving `refs/atom/d/` refs). If no remaining store ref references
that claim czd, the backend SHOULD also delete the corresponding
`refs/atom/claims/d/{claim_czd}` ref to prevent orphaned claim
accumulation. Git has no cross-namespace reference counting, so this
cleanup is the backend's responsibility.

```claim
kind: constraint
evaluator: test
```

#### Charter Refs (source and store)

```
refs/atom/charter/d/{charter_czd}                → charter commit [→ fact chain] → (tip)
```

A charter has no label — its identity is its czd — so a single
czd-addressed family serves both a source (registry) and a store, with
no label-addressed sibling. The charter is genesis-once
(`[atom-genesis-and-facts]`): its czd IS the anchor `czd(charter₀)`,
permanently — there is no successor to chain to, and no `prior` field
to chain it with (`CharterPayload`, Type Declarations, above).
Owner-set and key-rotation facts chain onto it via git parents instead
(`[charter-fact-chain]`). Charter and charter-fact commits both carry
the record log's own tree — small at this domain's real cardinality
(ADR-0007 §7.2) — so the whole fact chain remains lightweight to
enumerate and fetch, exactly as a claim's fact chain is.

##### [charter-ref-by-czd]

Each charter commit MUST be referenced by
`refs/atom/charter/d/{charter_czd}`, a czd-hex ref family (conforming to
the typed ref-path discipline of [czd-oid-disjoint](#czd-oid-disjoint)) that both protects
the charter commit from garbage collection and addresses it by czd for
`CharterStore::get_charter`. Keying by the charter's own czd makes
one-entry-per-anchor structural, not merely conventional: a distinct
`charter_czd` names a distinct anchor by construction
(atom-transactions.md `[charter-fork-distinction]`), so a store
ingesting multiple, independent anchors accumulates one entry per
anchor with no possibility of collision between them
(`[no-second-charter-genesis]` names the malformation this family CAN
still exhibit — a corrupted fact chain — never "too many charters").
Enumerating `refs/atom/charter/d/*` yields the charter set an
[anchor-resolvable](atom-transactions.md#anchor-resolvable) consumer verifies
against — the git instance of [charter-transition](atom-transactions.md#charter-transition)
POST ("stored in the source's atom refs, enumerable by consumers and
retrievable by its czd"). The family is identical in a source and a
store; ingesting a set fetches its charter and fact chain into the
same family.

```claim
kind: constraint
evaluator: test
depends: [czd-oid-disjoint, anchor-resolvable, charter-transition]
```

### Transitions

#### [charter-transition-git]

An atom-set MAY be chartered on the git
backend by creating a parentless charter commit.

- **PRE**: The repository MUST have at least one commit, and the
  charter's `src` MUST reference a revision that exists in it. This
  registry MUST NOT already carry a charter commit — genesis-once
  ([atom-genesis-and-facts](atom-transactions.md#atom-genesis-and-facts))
  means a repository is chartered exactly once, enforced on read by
  [no-second-charter-genesis](#no-second-charter-genesis); `CharterPayload` carries no
  `prior` field to resolve against a prior charter, because there is no
  prior charter to chain to (Type Declarations, above). The charter
  `CozMessage` MUST be valid
  ([record-is-log-leaf](#record-is-log-leaf)). Authorization
  of a charter over a source that already carries pre-charter claims is
  governed protocol-side by atom-transactions.md
  `[charter-transition]` (the bootstrap gate); the backend enforces
  storage, not that authorization decision.
- **POST**: A parentless charter commit exists with the anchor's
  record-log tree and a fixed, inert message
  ([charter-commit-format](#charter-commit-format)), referenced by
  `refs/atom/charter/d/{charter_czd}` ([charter-ref-by-czd](#charter-ref-by-czd)). The
  charter's czd is thereby the atom-set `Anchor`, permanently. The charter is
  enumerable and retrievable by czd, discharging
  [charter-transition](atom-transactions.md#charter-transition)
  POST for the git backend. Multi-ref writes
  follow [refs-atomic-multi](#refs-atomic-multi).

```claim
kind: constraint
evaluator: test
depends: [atom-genesis-and-facts, no-second-charter-genesis, record-is-log-leaf, charter-transition, charter-commit-format, charter-ref-by-czd, refs-atomic-multi]
```

#### [claim-transition-git]

An atom MAY be claimed by creating a
detached claim commit.

- **PRE**: The repository MUST have at least one commit. The atom-set's
  charter MUST be resolvable and verified — the claim's `anchor` field
  is its czd ([claim-chains-charter](atom-transactions.md#claim-chains-charter)).
  Genesis-once (`[no-second-claim-genesis]`): no claim commit MUST
  already exist for this label — a label already claimed is amended by
  a claim-owner fact (`[claim-fact-chain]`, `[claim-owner-fact-storage]`),
  never by a second claim commit. The claim `CozMessage` MUST be valid
  and include a `key` field. A source revision (`src`) for the claim
  point MUST be provided.
- **POST**: A claim commit exists, parentless, with the anchor's
  record-log tree (carrying the CozMessage, containing `src`, as a log
  leaf — never in the commit's own message) and a fixed, inert message
  ([claim-detached](#claim-detached)). The ref
  `refs/atom/claims/pub/{label}` points to it. No
  protective ref is written for the claim's source revision — GC
  protection for it is the anchor's record log's job, not this
  transition's (see the Registry Refs prose above).

```claim
kind: constraint
evaluator: test
depends: [claim-chains-charter, no-second-claim-genesis]
```

#### [publish-transition-git]

A version MAY be published for a
claimed atom by creating a content commit and a publish commit that
targets it as its sole git parent.

- **PRE**: A claim for this label MUST exist in the registry
  (`refs/atom/claims/pub/{label}` is set — the permanent claim commit,
  or the tip of its fact chain if any facts have landed,
  [claim-fact-chain](#claim-fact-chain)). The source revision (`src`)
  from which the atom's content is extracted MUST be at or after the
  claim's `src` in the repository's history ([temporal-vector](#temporal-vector)).
  The content commit MUST be deterministic per [snapshot-deterministic](#snapshot-deterministic).
  The publish `CozMessage` MUST reference the claim's `czd` — always the
  permanent claim commit's own czd, never a fact commit's, per
  genesis-once ([atom-genesis-and-facts]).
- **POST**: A content commit exists (parentless, deterministic, a pure
  function of its tree). A publish commit targets it as its sole git
  parent, carrying the anchor's record-log tree (containing the
  `claim` czd binding as a log leaf — never in the commit's own
  message) and a fixed, inert message —
  [publish-commit-format](#publish-commit-format),
  [publish-claim-binding](#publish-claim-binding). The ref
  `refs/atom/pub/{label}/{version}` points to the publish commit. No
  protective ref is written for the source revision — GC protection
  for it is the anchor's record log's job (see the Registry Refs prose,
  above), not this transition's. The ref write MUST use a
  compare-and-swap (CAS) on `refs/atom/claims/pub/{label}` to ensure
  the claim's fact chain has not advanced (an ownership-affecting fact
  landed) between payload construction and publish commit creation.
  _(Raised from SHOULD 2026-07-12: the non-CAS write is a TOCTOU
  window — a publish can land against a claim state that no longer
  matches what the signer verified against. Discharges the per-name
  register law,
  [backend-refs-linearizable](atom-backend-contract.md#backend-refs-linearizable).)_

```claim
kind: constraint
evaluator: test
depends: [temporal-vector, snapshot-deterministic, publish-commit-format, publish-claim-binding]
because: [backend-refs-linearizable]
```

#### [ingest-transition]

Atoms MAY be ingested from a registry (or
another store) into a store.

- **PRE**: The source has discoverable atoms (resolvable refs). The
  store is a valid git repository.
- **POST**: For each ingested atom: the atom (content) commit exists
  in the store, and the publish's materialized view chain — fetched
  WHOLE, from its tip to its first commit, never depth-limited
  ([publish-chain-fetched-whole](#publish-chain-fetched-whole); a
  depth-limited fetch would omit the content commit hanging off the
  chain's first commit) — exists in the store, the claim commit (and
  its chain of previous claims) is fetched and referenced by
  `refs/atom/claims/d/{claim_czd}`. Claim chains are lightweight by
  design (the record log's own tree stays small at this domain's real
  cardinality, ADR-0007 §7.2) and require no special filtering. Version refs
  follow the store layout: `refs/atom/d/{blake3(publish_czd)}`. AtomId
  is preserved through ingestion. Refs MUST NOT be committed until all
  cryptographic verification ([verification-local](atom-transactions.md#verification-local)) passes — objects may exist
  in the ODB during verification, but are invisible to consumers
  until the refs transaction is committed. (Satisfies
  [ingest-preserves-identity](atom-transactions.md#ingest-preserves-identity).)

```claim
kind: constraint
evaluator: test
depends: [verification-local, publish-chain-fetched-whole]
because: [ingest-preserves-identity]
```

#### [claim-replacement-storage] — RETIRED 2026-08-05

**Retired, not amended.** This transition constructed a **replacement
claim** — a new claim commit parented onto the old one, becoming the
new tip an ownership-change fact would instead now be. Genesis-once
(`[atom-genesis-and-facts]`) forecloses this shape: claim is a
permanent, single genesis record, "never reissued, never superseded by
a record of the same kind." `[claim-owner-fact-storage]`, below,
restates what this transition protected — a label's controlling
identity can change, and a change forced by the wider charter-owner
fold against the current holder's wishes is a first-class, marked,
detectable event — over the fact mechanism instead.

```claim
kind: constraint
evaluator: none
```

#### [claim-owner-fact-storage]

A label's currently-effective owner MAY
change by chaining a **claim fact commit** onto the claim
(`[claim-fact-chain]`), per atom-transactions.md `[claim-owner-fact]`'s
two-tier authorization (owner fact: signed by the label's own current
owner; governance fact: signed by a charter-fold member who is not the
label's current owner — a marked seizure event).

- **PRE**: The claim for this label MUST exist (genesis-once,
  `[claim-detached]`). The fact `CozMessage` MUST be valid and
  authorized under exactly one of the two tiers
  ([claim-owner-fact](atom-transactions.md#claim-owner-fact)).
- **POST**: A new fact commit exists (`FactCommitFormat`, carrying the
  anchor's record-log tree at this fact's append point)
  whose parent is the current tip of the claim's fact chain (the claim
  commit itself, if this is the first fact). The ref
  `refs/atom/claims/pub/{label}` is updated to point to the new tip.
  The claim commit itself, and its czd, are unchanged — future
  publishes continue to reference the SAME permanent claim czd
  (`[publish-update-transition]`'s identity-class discipline, the
  same rule at the publish scope) and satisfy [temporal-vector](#temporal-vector)
  against the claim's own, unchanging `src`. Existing publishes
  remain valid, their authorization now resolved against whichever
  owner-fold state was current at their own signing time, not against
  the fact chain's current tip. The full fact history is walkable from
  the new tip to the claim's own genesis commit.

```claim
kind: constraint
evaluator: test
depends: [claim-fact-chain, claim-detached, temporal-vector]
```

#### [publish-update-transition]

A publish MAY be updated (e.g., after
key revocation or resigning, to carry a reproducibility-mode
transition — promotion or demotion per atom-model.md §6 — or to append
overwrite- or append-class fields per atom-transactions.md
`[amendment-field-classification]`) by chaining a new fact commit onto
its materialized view chain.

- **PRE**: An existing publish chain for this `(label, version)`
  exists ([publish-fact-chain](#publish-fact-chain)). The new commit's
  `CozMessage` MUST carry an **amendment payload**: the same payload
  shape as the base publish, but with every identity-class field
  (atom-transactions.md `[amendment-field-classification]`)
  structurally absent — not present and checked equal, absent. The
  amendment payload MUST be valid per
  [sig-over-pay](atom-transactions.md#sig-over-pay) (atom-transactions.md) and MUST set at least one overwrite- or
  append-class field. Authorization is per amended-field class:
  - A commit that sets an overwrite-class field (`[amendment-field-
    classification]` class 2 — signing identity, `mode`, `meta`) MUST
    be signed by a key authorized by the base publish's claim's single
    owner ([claim-owner-single](atom-transactions.md#claim-owner-single), under
    [owner-authorization-delegated](atom-transactions.md#owner-authorization-delegated)'s per-value semantics — the same check
    [publish-transition](atom-transactions.md#publish-transition)'s own PRE already applies to the base
    payload). This authorization check is against the SIGNING key, not
    the declared `tmb` field: a conforming verifier MUST first bind
    the declared `tmb` to the actual key that produced the signature —
    `tmb(verifying_key) == pay.tmb` MUST hold, the same binding
    atom-transactions.md's Verification Pipeline step 6 already
    requires for charter/claim — and only THEN evaluate claim-owner
    authorization against that bound key. A commit whose declared
    `tmb` does not equal the thumbprint of its actual signing key MUST
    be rejected before any claim-owner authorization check runs:
    without this binding, a declared `tmb` naming the claim owner
    proves nothing — the envelope's own `key` field can belong to an
    unrelated signer, and the signature still verifies validly against
    IT, letting an attacker declare `tmb = <claim owner's thumbprint>`
    while actually signing with their own key. This is a
    protocol-level validity requirement: a commit that sets an
    overwrite-class field without both the tmb-binding check and
    claim-owner authorization MUST be rejected — it is not a valid
    amendment. This closes the gap an unauthorized `tmb`/`key`/`alg`/
    `mode` change would otherwise leave open ("no undeclared key signs
    anything").
  - A commit that sets an append-class field (class 3, a fact entry)
    carries NO protocol-level signer restriction: any validly-signed
    commit MAY add a fact entry, from any signer, regardless of
    fact-kind tier — this is a structural validity statement, not a
    trust one. Which verdict (`fact`/`evidence`/`refused`) a given
    consumer assigns a fact entry is governed entirely by the entry's
    fact-kind authorization tier ([fact-kind-table](atom-transactions.md#fact-kind-table)) as
    evaluated through trust-model.md's consumer-local acceptance
    policy ([trust-role-authorization](trust-model.md#trust-role-authorization),
    [fact-claim-owner-gated](atom-transactions.md#fact-claim-owner-gated)) — fact acceptance is deliberately
    signer-relative ([trust-signer-relative](trust-model.md#trust-signer-relative)), never
    a chain-level accept/reject gate.
  - A commit MAY set fields of both classes in one amendment; the
    overwrite-class authorization requirement above applies regardless
    of any append-class content also present.
- **POST**: A new fact commit exists (`PublishChainCommitFormat`'s
  fact-extending shape) targeting the chain's previous tip. The ref
  `refs/atom/pub/{label}/{version}` (registry) is updated to point to
  the new tip. In the store, a new flat ref
  `refs/atom/d/{blake3(new_publish_czd)}` is created pointing to the
  new commit; the previous ref `refs/atom/d/{blake3(old_publish_czd)}`
  persists, keeping any lock pin on the old publish_czd resolvable
  (consistent with [publish-fact-chain](#publish-fact-chain)'s
  never-delete guarantee). The identity-class fields of the publish
  payload — `label`, `version`, `dig`, `src`, `path`, `content_hash`,
  `anchor`, and `claim`
  ([amendment-field-classification](atom-transactions.md#amendment-field-classification)) — remain set ONLY by the
  chain's first commit; the amendment payload's shape has no slot for
  them, so a backend or consumer walking the chain MUST read
  identity-class field values from the first commit only, and MUST
  NOT read them from, or expect them present on, any later commit.
  (`typ` is identity-class in value — it never changes across a chain
  — but, uniquely, IS required and present on every commit, base or
  amendment alike: it is the `CozMessage` payload-family discriminator,
  [publish-typ](atom-transactions.md#publish-typ), needed to parse the
  message at all, not per-chain data a resolver accumulates from the
  first commit.) Overwrite-class fields (signing metadata — `tmb`,
  `alg`, the envelope `key`, `now` — `meta`, the reproducibility `mode`
  field — chain-VARIABLE by design, promotable/demotable via signed
  chain appends per atom-model.md §6 and
  [publish-mode](atom-transactions.md#publish-mode)) and append-class
  fields (facts, [fact-kind-table](atom-transactions.md#fact-kind-table))
  MAY be set by the first commit or by any later commit in the chain.
  Altering the artifact identity (`dig`), version string, or source
  revision requires a new content commit and a new publish ref — not
  an update to an existing chain.

```claim
kind: constraint
evaluator: test
depends: [sig-over-pay, claim-owner-single, owner-authorization-delegated, publish-transition, fact-kind-table, trust-role-authorization, fact-claim-owner-gated, trust-signer-relative, publish-fact-chain, amendment-field-classification, publish-typ, publish-mode]
```

#### [tag-chain-semantic-immutable] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required identity-class
publish fields to be set only by the base tag, never by a later tag
in the chain. There is no tag chain; the identical requirement is now
stated directly in `[publish-update-transition]`'s own POST, alongside
the mechanism that carries it (the amendment payload's shape has no
slot for identity-class fields, chain-commit or tag alike), rather
than as a separate claim about tag semantics specifically.

```claim
kind: constraint
evaluator: none
```

#### [refs-atomic-multi]

Multi-ref protocol transitions (ingestion, writing
both a store version ref and a store claim ref for one atom;
multi-atom ingestion) MUST be committed atomically within the
store — all named
refs move together or none do — so no consumer can observe a torn
intermediate state. This binds the authoritative store per-store;
cross-mirror consistency is convergence, not atomicity
(atom-backend-contract.md `[backend-replica-reads]`). _(Promoted
2026-07-12 from the Atomicity implementation guidance below to a
tagged constraint; discharges the per-store half of
[backend-refs-atomic-multi](atom-backend-contract.md#backend-refs-atomic-multi).)_

```claim
kind: constraint
evaluator: test
because: [backend-refs-atomic-multi]
```

#### [fs-ingest-transition]

Atoms from a filesystem `AtomSource` MAY
be ingested into a git `AtomStore` without claims or publishes.

- **PRE**: The filesystem source satisfies the `AtomSource` contract
  (as defined in atom-transactions.md). The store is a valid git
  AtomStore.
- **POST**: Atom commits exist in the store for each discovered atom,
  referenced by `refs/atom/dev/{anchor}/{label}/{dev_version}`. No
  publish or claim refs exist for dev atoms. The store MUST treat dev atoms
  as unsigned/unclaimed. The dev version string SHOULD incorporate the
  tree object hash to prevent clobbering across concurrent evaluations.
  Every atom has an AtomId — git-sourced dev atoms use the set's
  charter anchor (`czd(charter₀)`); filesystem-sourced atoms use
  a well-known constant sentinel anchor (see
  [fs-source-contract](atom-transactions.md#fs-source-contract)). A git source that has no charter yet is
  identity-equivalent to a filesystem source: its dev atoms are
  local-class and carry the same sentinel anchor
  ([dev-atom-unchartered-local](#dev-atom-unchartered-local)). Note:
  the dev atom's `dig` MAY equal a later published atom's `dig` for
  identical content — content identity is a pure function of the tree
  alone ([snapshot-deterministic](#snapshot-deterministic)), so a dev
  atom that is later published for the same content becomes the SAME
  underlying git object, not a distinct one. Provenance is carried
  entirely by which refs point at the object (`refs/atom/dev/...`
  vs. `refs/atom/pub/...` / `refs/atom/d/...`), never by the atom
  commit's own hash — this is by design.

```claim
kind: constraint
evaluator: test
depends: [fs-source-contract, dev-atom-unchartered-local]
```

#### [dev-atom-resolution]

Tooling consuming atoms from a store
resolves through two namespaces in order of precedence:

1. `refs/atom/dev/{anchor}/{label}/` — local development atoms (unsigned,
   in-progress evaluations from filesystem or local git sources)
2. `refs/atom/d/` — all ingested published atoms, regardless of origin
   (local registry, remote registry, or mirror); each keyed flat by
   `blake3(publish_czd)`

Published atoms from the local registry are ingested into `d/` via
the same [ingest-transition](#ingest-transition) as remote atoms — there is no special
treatment. The `pub/` namespace is registry-write-time only; the
resolver never queries it.

Clients MAY provide a **release mode** which skips step 1, resolving
only from `d/`. This ensures builds use exactly the ingested versions,
matching what downstream consumers would see.

```claim
kind: constraint
evaluator: none
depends: [ingest-transition]
```

### Forbidden States

#### [no-non-empty-claim] — RETIRED 2026-08-23

**Retired, not amended.** This constraint required a claim commit's or
claim fact commit's `tree` to be the well-known empty tree, rejecting
any non-empty tree as malformed. The record-anchored publishing design
inverts this: `[claim-detached]` and `[claim-fact-chain]` now REQUIRE
these commits to carry the anchor's record-log tree, which is never
empty once the anchor's charter genesis has landed (the log's first
leaf). `[claim-tree-is-log]`, below, restates the forbidden-state half
this constraint protected — a claim or claim-fact commit whose `tree`
is anything OTHER than the record log's own tree at that leaf's append
point, empty tree included, is malformed. A claim commit's parent (if
present) MUST still be another claim commit's — but this never happens
under genesis-once: see `[no-second-claim-genesis]`. A fact commit's
parent MUST still be the claim commit or another fact commit in the
SAME label's own chain (`[claim-fact-chain]`) — neither of those
parent-shape rules changes here.

```claim
kind: constraint
evaluator: none
```

#### [claim-tree-is-log]

A claim commit's or claim fact commit's
`tree` MUST be the anchor's record-log tree at the point that leaf was
appended ([log-tree-opaque](#log-tree-opaque)) — the backend MUST
reject any other tree, including the well-known empty tree, as
malformed. This is the forbidden-state half of
[claim-detached](#claim-detached) and
[claim-fact-chain](#claim-fact-chain)'s own requirement, named
directly for a resolver auditing storage rather than constructing it.

```claim
kind: constraint
evaluator: none
depends: [log-tree-opaque, claim-detached, claim-fact-chain]
```

#### [no-second-charter-genesis]

**Named carefully: a second charter commit
is not itself a forbidden state — treating it as continuing the FIRST
anchor's identity is.** This constraint is the git-level enforcement of
genesis-once
([atom-genesis-and-facts](atom-transactions.md#atom-genesis-and-facts))
at the charter scope. Because `refs/atom/charter/d/{charter_czd}` is
keyed by the charter's own czd, two different, validly-signed charter
commits are, by construction, two different anchors
(atom-transactions.md `[charter-fork-distinction]`: distinct czds are
distinct anchors, full stop) — there is no ref-path collision for a
second charter TO cause, unlike a second claim
(`[no-second-claim-genesis]`, where the ref key is the human-chosen
label, not a content address). This constraint discharges
`[charter-fork-distinction]` at the git storage layer instead: a
resolver MUST NOT treat a second `refs/atom/charter/d/*` entry as a
successor, an update, or any kind of continuation of a
previously-verified anchor's identity — there is no successor shape
for it to be (`[charter-succession-via-prior]` retired) — and MUST
verify it as what it structurally is, a wholly independent anchor
requiring its own independent trust decision. What IS forbidden at the
git-object level is a malformed **fact chain**: walking a
`refs/atom/charter/d/{charter_czd}` ref's `parent` links backward MUST
terminate in exactly one parentless commit (the charter,
`[charter-commit-format]`); a chain whose walk produces zero, or more
than one, parentless commit is malformed storage and MUST be rejected
— never resolved by picking one.

```claim
kind: constraint
evaluator: none
because: [atom-genesis-and-facts]
```

#### [no-second-claim-genesis]

A second parentless claim commit for a
label that already has one MUST be rejected as invalid — the concrete
git-level form of the same
[atom-genesis-and-facts](atom-transactions.md#atom-genesis-and-facts)
rule, at the claim scope. A
resolver MUST reject a `refs/atom/claims/pub/{label}` (or
`refs/atom/claims/d/{claim_czd}`) chain whose root is not the unique,
already-verified parentless claim commit for that label; an ownership
change is only ever a fact chained onto that root
(`[claim-fact-chain]`), never a second parentless claim. Distinguish
this from a legitimate fact commit: a fact commit MUST always carry a
parent (the claim commit or an earlier fact commit); a claim commit
with a parent, or two DIFFERENT parentless claim commits both claiming
the same label, is the forbidden state this constraint names.

```claim
kind: constraint
evaluator: none
because: [atom-genesis-and-facts]
```

#### [no-orphan-publish]

A publish commit MUST NOT exist in a registry
without a corresponding claim commit reachable via
`refs/atom/claims/pub/{label}` or `refs/atom/claims/d/{claim_czd}`.
(Satisfies [no-unclaimed-publish](atom-transactions.md#no-unclaimed-publish).)

```claim
kind: constraint
evaluator: none
because: [no-unclaimed-publish]
```

#### [no-backdated-src]

A publish MUST NOT reference a source revision
(`src`) that precedes the claim's `src` in the repository's history.
The publish's `src` MUST be at or after the claim's `src`. This is the
enforcement mechanism for [temporal-vector](#temporal-vector).

```claim
kind: constraint
evaluator: test
because: [temporal-vector]
```

#### [no-label-collision-registry]

Two atoms in the same registry
MUST NOT share a label. This is enforced by the ref layout — two
claim refs for the same label would conflict.

```claim
kind: constraint
evaluator: none
```

#### [anchor-oldest-root]

_(retired 2026-07-08 — superseded by the
charter amendment)_: The oldest-parentless-commit selection rule
existed only to make genesis-based anchor discovery deterministic.
With `Anchor := czd(charter₀)` there is no anchor discovery to
disambiguate; the constraint is retired along with
`[anchor-is-genesis]`.

```claim
kind: constraint
evaluator: none
```

#### [no-missing-store-claim]

In a store, if a publish's payload
references a claim czd, the corresponding claim commit MUST exist in
the store's object database and MUST be reachable via
`refs/atom/claims/d/{claim_czd}`. A missing claim MUST be treated as
ingestion corruption.

```claim
kind: constraint
evaluator: none
```

### Behavioral Properties

#### [anchor-vector-authenticity]

Given a publish in a registry,
the atom MUST be verifiable as authentic by checking the vector:
(1) the claim's `anchor` field equals `czd(charter₀)` of the
verified charter ([charter-anchor](atom-transactions.md#charter-anchor),
[claim-charter-authorization](atom-transactions.md#claim-charter-authorization)), (2) the publish payload's `claim` czd
resolves to a claim commit whose payload `src` is a descendant of (or
equal to) the charter's `src` ([charter-ancestry](atom-transactions.md#charter-ancestry)),
(3) the claim `CozMessage` — read as the log leaf that commit's
`tree` carries ([record-is-log-leaf](#record-is-log-leaf)) — is valid, AND
(4) the publish's `src` is at or after the claim's `src`. This creates
a "vector of authenticity": charter src → claim src → publish src.

- **Type**: Safety

```claim
kind: constraint
evaluator: test
depends: [charter-anchor, claim-charter-authorization, charter-ancestry, record-is-log-leaf]
```

#### [ingestion-portable]

Atom commits, publish commits (and their
materialized view chains), and claim commits MUST be transferable
between git repositories via standard git pack negotiation
(`git fetch`). No custom transport extensions are REQUIRED. Claim
commits SHOULD be shallow-fetched (without their ancestry) into
stores to minimize transfer cost; a publish's materialized view chain
MUST NOT be shallow-fetched — see
[publish-chain-fetched-whole](#publish-chain-fetched-whole).

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [publish-chain-fetched-whole]

A publish's materialized view
chain MUST always be fetched in its entirety — from the ref tip to
the chain's first commit — and MUST NEVER be fetched depth-limited
(`--depth=1` or any other shallow bound). This is not a performance
compromise a client is trusted to apply carefully; a shallow fetch of
a publish ref is not merely incomplete, it is silently WRONG: the
content commit hangs off the chain's first commit as that commit's
sole git parent ([publish-commit-format](#publish-commit-format)), so
a depth-limited fetch of any commit before that root would omit the
content commit entirely, handing a client a verified publish record
with none of the bytes it just verified the hash of. This is the
opposite discipline from the anchor's own main record log
(specified in the ADR governing this backend's git object model),
which IS correctly fetched at depth 1 — its entire append-only record
lives in the tip's own tree, so shallow-on-commits is complete on
that ref. The two refs are structurally different objects and MUST
NOT be given the same fetch discipline: depth-1 is the correct,
complete read of the main log ref; depth-1 on a publish view ref is a
silent data-loss bug.

- **Type**: Safety

```claim
kind: constraint
evaluator: test
depends: [publish-commit-format]
```

#### [update-chain-auditable]

The publish's materialized view chain
([publish-fact-chain](#publish-fact-chain)) structure (tip → ... →
first commit) MUST be traversable. A consumer MUST be able to
reconstruct the full update history by walking the chain's ordinary
git parent links from the ref tip to the first commit. Each commit in
the chain carries its own log leaf, readable from the `LogTree` its
`tree` field carries ([record-is-log-leaf](#record-is-log-leaf)),
enabling verification of the complete signing history.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
depends: [publish-fact-chain, record-is-log-leaf]
```

#### [peel-content-integrity]

When acquiring an atom via
`refs/atom/d/{blake3(publish_czd)}`, a consumer MUST walk the publish's
materialized view chain from the tip, following ordinary git parent
links, until reaching the chain's first commit, then take THAT
commit's own parent — the content commit
([publish-commit-format](#publish-commit-format)) — and verify that
its content-addressed sha equals `payload.dig` in the publish
`CozMessage`. (This replaces the retired tag-peeling mechanism with
an ordinary commit-parent walk; there is no tag left to peel.) A
mismatch MUST be treated as tampering — the fetched atom MUST be
rejected (SAD §8.3). Open failure mode: if a serving mirror has
garbage-collected a chain's first commit (e.g., for a superseded
publish_czd no longer advertised), the walk may fail to locate the
content commit; the backend SHOULD surface this as a distinct "chain
root unavailable" error and MUST NOT silently fall back to a
different publish.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
depends: [publish-commit-format]
because: [docs/architecture/atom-sad#8]
```

#### [store-accumulates]

After ingestion, a store's `resolve` MUST
return at least what the source's `resolve` returns for every
ingested atom. (Model §2.3, ⊇ condition.)

- **Type**: Safety

```claim
kind: constraint
evaluator: test
depends: [docs/models/publishing-stack-layers#2.3]
```

#### [client-evaluates-not-replays]

A client MUST NOT merge or
replay the charter, claim, and publish materialized view chains to
reconstruct the anchor's own log order. Because every chain commit
that carries the anchor's own record data does so via the signed
payload in its message (never a synthesized subset of the log), there
are no partial views for a client to reconcile against one another —
the three refs are freshness bookmarks naming where each scope's
latest relevant record sits, not three separate data sources. What a
client MUST do instead is evaluate trust state — which facts bear on
this publish, is the signing key still trusted, has anything been
withdrawn — directly against the records it already holds, never by
replaying chain order to derive it. All three ref names are fully
derivable from data the client already has, with no discovery round
trip: `refs/atom/charter/d/{charter_czd}`
([charter-ref-by-czd](#charter-ref-by-czd)) from the anchor's own czd
(there is exactly one, per anchor), `refs/atom/claims/pub/{label}`
([registry-ref-claim](#registry-ref-claim)) from the label alone
([registry-ref-label-unique](#registry-ref-label-unique)), and
`refs/atom/pub/{label}/{version}`
([registry-ref-version](#registry-ref-version)) from the label and
version together. A resolver holding a lock entry MAY therefore
request all three refs in one connection.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
depends: [registry-ref-claim, registry-ref-version, charter-ref-by-czd]
```

<!-- [publish-extensible] moved to atom-transactions.md — this is a
     protocol-level property of the Coz message format, not a git
     backend concern. -->

## Implications

### Implementation Guidance

- **atom-git**: The reference implementation crate. MUST implement
  `AtomSource`, `AtomRegistry`, and `AtomStore` from atom-core per
  this specification.

- **atom-core FsSource**: See atom-transactions.md for the abstract
  `AtomSource` contract. The git backend's responsibility is to
  ingest atoms from `FsSource` per `[fs-ingest-transition]`.

- **Deterministic commit construction**: Use gix's commit creation API
  with blank author/committer (`<> 0 +0000`), epoch timestamp, and
  empty message — no extra headers. The `dig` field in PublishPayload
  is this commit's ObjectId, a pure function of its tree.

- **Claim commits**: Use gix to create commits carrying the anchor's
  record-log tree at the claim genesis leaf's append point, no extra
  headers, ATOM_AUTHOR/ATOM_TIMESTAMP, and a fixed, empty message — the
  CozMessage is written to the log as the claim genesis leaf, never to
  this commit's own message
  ([log-anchored-message-fixed](#log-anchored-message-fixed)). The
  claim commit is always parentless —
  genesis-once, one ever per label (`[claim-detached]`). A claim-owner
  fact (`[claim-owner-fact-storage]`) reuses the same object shape but
  parents onto the claim commit or the previous fact commit, forming
  the fact chain (`[claim-fact-chain]`). The `src` in the claim payload
  ties the claim to the source revision cryptographically (via the
  claim's czd); the commit's own hash carries no such binding. Write
  one ref at claim time, `claims/pub/{label}`; a fact commit updates
  that same ref to its own new tip. No `src/{src_oid}` protective ref
  is written.

- **Publish commits**: Use gix's commit creation API with a fixed,
  empty message — the CozMessage is written to the log as the
  publish's (or fact's) own leaf, never to this commit's own message.
  The chain's first commit targets the
  content commit as its sole git parent; the `claim` czd in its
  payload identifies the authorizing claim. For fact-extending
  updates, the new commit targets the previous commit in the SAME
  chain instead (never the content commit).

- **Publish fact metadata**: Clients SHOULD leverage
  `[publish-payload-extensible]` to provide programmatic overwrite-
  class metadata in the publish `CozMessage` payload's `meta` object
  (atom-transactions.md `[amendment-field-classification]` — `meta` is
  an overwrite-class field; the latest commit's `meta` value is
  current).
  One recommended field remains here:
  - `meta.min-compatible: "1.0.0"` — minimum compatible version for
    semver-unaware ecosystems. This is a publisher-asserted
    compatibility *declaration*, not a post-hoc observation — the
    publisher states it deliberately, at or after publish time, rather
    than it being something building or independent inspection would
    discover the way a security advisory or a build-artifact hash is.
    It therefore stays an overwrite-class `meta` field rather than
    moving to the fact-kind table below.

  The five fields formerly recommended here — `meta.broken`,
  `meta.security`, `meta.superseded-by`, `meta.deprecated`, and
  `meta.build-hash` — are **retired as `meta` fields**. Each is a
  post-hoc, append-class fact under atom-model.md §4's partition law
  (an advisory or a lifecycle marker is exactly its "asserted fact"
  example; a build hash is exactly its "derived fact" example), and
  each is now a fact kind in atom-transactions.md's fact-kind table
  (`[fact-kind-table]`): `meta.broken` → `yanked`, `meta.security` →
  `advisory`, `meta.superseded-by` → `superseded-by`,
  `meta.deprecated` → `deprecated`, `meta.build-hash` →
  `build-record`. All `meta` and fact-kind fields alike are signed as
  part of the `CozMessage` and carry cryptographic assurance. Publish
  updates (`[publish-update-transition]`) enable both
  overwrite-class `meta` revision and append-class fact accumulation,
  without altering the base publish.

- **Claim-owner fact metadata**: Clients SHOULD leverage
  `[claim-payload-extensible]` (or its `FactPayload` analogue, Open
  Questions #6) to provide programmatic transition metadata on a
  claim-owner fact (`[claim-owner-fact-storage]`). The fact's `meta`
  object communicates the intent to consumers who may hold a stale
  view of the claim's ownership from a mirror lagging behind the fact
  chain's tip — the claim itself never changes, so this is metadata
  about the transition the fact records, not about a new identity.
  Recommended fields for client implementors (all nested under `meta`),
  carried forward unchanged from the retired replacement-claim design
  (`[claim-replacement-storage]`) since the trust semantics they
  describe did not change, only what object carries them:
  - `meta.supersedes: "update" | "revoke"` — why the owner changed.
    Two states with distinct trust implications: `"update"`: the old
    key is no longer active but was valid at the time — covers routine
    key rotation, ownership transfer, or any benign transition;
    versions published under the old owner remain trustworthy.
    `"revoke"`: the old key is considered compromised; clients SHOULD
    warn users before consuming versions signed under the old owner
  - `meta.announcement: "https://..."` — link to an official
    communication about the ownership change (e.g., compromise
    disclosure, transfer announcement, rotation notice). Clients
    SHOULD surface this URL when prompting users about claim changes
  - `meta.effective-after: <timestamp>` — if set, only versions
    published under the old owner AFTER this timestamp are considered
    suspect; versions before it remain trusted. Limits blast radius
    for targeted compromise windows. Only meaningful when
    `supersedes` is `"revoke"`

  All fact meta fields are signed as part of the `CozMessage` and carry
  cryptographic assurance. Because the fact chains onto the claim (or
  the previous fact) via a git parent link, the meta is structurally
  bound to the specific transition it describes — the same binding the
  retired design had, now expressing a fact rather than a new identity.

- **Resolving content**: When resolving a version ref to its content,
  walk the materialized view chain's ordinary git parent links from
  the ref tip to the chain's first commit, then take that commit's own
  parent — the atom (content) commit. There is no tag to peel; this is
  an ordinary commit-parent walk, always terminating in a parentless
  content commit ([snapshot-parentless](#snapshot-parentless)).

- **Store ingestion**: Fetch atom commits and a publish's whole
  materialized view chain (never depth-limited —
  [publish-chain-fetched-whole](#publish-chain-fetched-whole)), claim
  commits and their fact chains, via gix pack negotiation. Create
  published atom refs under `refs/atom/d/`, claim refs under
  `refs/atom/claims/d/`. Claim and claim-fact commits can be fetched
  shallowly (no need for their ancestry) — a publish chain MUST NOT be.

- **Dev atom ingestion**: Create atom commits from filesystem content.
  Reference under `refs/atom/dev/{anchor}/{label}/{dev_version}`. No
  publish, no claims, no verification ceremony. Dev version SHOULD include
  tree hash (e.g., `1.0.0.dev-{tree_hash_prefix}`) to avoid clobbering
  across concurrent evaluations. AtomId is derivable for all atoms
  (git atoms use the set's `czd(charter₀)` anchor, FS atoms use the
  sentinel anchor).

- **Anchor verification**: The anchor is not discovered from the commit
  graph — it is the charter's own czd, genesis-once and permanent. The
  backend resolves the charter from the source's atom refs (storage
  encoding: Open Questions #5), verifies it, and checks
  `Anchor == czd(charter₀)`. Ancestry checks (`[temporal-vector]`) walk
  the DAG from the claim's `src` to the charter's `src`, not to a
  parentless root.

- **Atomicity**: The normative rule is `[refs-atomic-multi]`; the gix
  mechanism is `gix::refs::Transaction` to batch all reference updates
  atomically. Remote pushes involving multiple refs MUST use atomic
  push semantics (equivalent of `git push --atomic`) to a single
  remote; cross-mirror consistency is convergence
  (`[backend-replica-reads]`), never coordinated atomicity.

- **Tree construction**: When building git tree objects from filesystem
  content (`FsSource`), entries MUST follow Git's canonical byte-order
  sorting (directories sort as if their names end with `/`). gix's
  tree construction API handles this — implementations MUST NOT
  manually sort entries using OS-level alphabetical ordering.

### Testing Strategy

- **Unit tests**: snapshot determinism (same tree → same hash, across
  distinct source revisions), forbidden state validation
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

2. **Tag extra headers in gix** — RESOLVED, then MOOT: the question
   was already answered (the `claim-commit` header was removed in
   favor of the signed payload's `claim` czd) before the
   record-anchored publishing amendment retired publish tags
   entirely; a publish is now an ordinary commit
   ([publish-commit-format](#publish-commit-format)), so the question
   of tag extra-header support in gix no longer applies to this
   document at all.

3. **FsSource contract**: The abstract `AtomSource` contract for
   filesystem directories (manifest discovery strategy, path resolution,
   ingestion interface) needs formal specification in atom-transactions.md.
   The POC implementation provides a reference. This spec assumes that
   contract exists and specifies only the git ingestion side.

4. **ATOM_AUTHOR identity**: The current blank identity (`"" <> 0 +0000`)
   works with gix but may trigger `git fsck` warnings. For good git
   citizenship, consider alternatives that preserve determinism:
   (a) a static sentinel like `"atom" <atom-protocol> 0 +0000`,
   (b) the `src` commit's author (deterministic since `src` hash is an
   input, but adds a read dependency), or (c) a protocol-derived
   constant like the CozMessage `typ` value (e.g., `"atom/publish"`).
   The choice does not affect protocol correctness — only git tooling
   ergonomics.

5. **Charter storage representation** — RESOLVED (2026-07-13);
   **re-resolved for genesis-once (2026-08-05)**; **re-resolved again
   for the record-anchored publishing design (2026-08-23)**: The git
   object encoding and ref layout for the charter transaction is
   specified. A charter is a **charter commit** — a parentless commit
   carrying the anchor's record-log tree at the charter genesis leaf's
   append point, with a fixed, inert message, the charter `CozMessage`
   read as that log leaf (`[charter-commit-format]`,
   `[record-is-log-leaf]`) — addressed by
   `refs/atom/charter/d/{charter_czd}` (`[charter-ref-by-czd]`).
   Genesis-once (`[atom-genesis-and-facts]`) retired the original
   resolution's successor-charter chain
   (`[charter-succession-via-prior]`, retired): there is exactly one
   charter commit per anchor, ever, and a resolver reaches its `src` for
   `[temporal-vector]` by walking the charter's own fact chain
   (`[charter-fact-chain]`) to its root, in bounded, chain-length-only
   cost (`[charter-src-reachable]`) — O(1) in the well-formed case,
   since the fact chain is normally empty or short, never O(succession
   depth). A dev atom in a repository with no charter yet is
   local-class and carries the filesystem sentinel anchor
   (`[dev-atom-unchartered-local]`). Charter authorization over a
   pre-claimed source (the bootstrap gate) remains protocol-side in
   atom-transactions.md `[charter-transition]`, not a backend concern.

6. **`FactPayload` wire shape** — OPEN (2026-08-05): This amendment
   introduces the git-level container for a charter- or claim-owner
   fact (`FactCommitFormat`, Type Declarations, above; used by
   `[charter-fact-chain]` and `[claim-fact-chain]`), but the payload it
   carries — `FactPayload`'s field shape, its `typ` value(s), and the
   owner-vs-governance discriminator atom-transactions.md's
   `[claim-owner-fact]` requires every consumer be able to distinguish
   — is NOT settled by this document. atom-transactions.md explicitly
   defers "the concrete fact-record wire shape... scoping it is a
   git-storage-format.md / atom-backend-contract.md concern" as an Open
   Item; this pass supplies the git container per that obligation but
   deliberately does not fabricate the payload's protocol-level content.
   **Updated 2026-08-23**: the earlier form of this note claimed
   ADR-0007 didn't settle this either because its §7.4/§7.5 were
   "never given their own numbered subsection" — that claim is now
   stale and was struck: ADR-0007 §7.1–§7.5 (the record log,
   checkpoints, the content commit, and the three materialized view
   chains) are now fully drafted, numbered sections on
   `campaign/atom-version-integrity`, and this document's own git
   object model has been brought into agreement with them (`LogTree`,
   `[log-tree-opaque]`, `[record-is-log-leaf]`, and the tree/message
   fields of `CharterCommitFormat`, `ClaimCommitFormat`,
   `FactCommitFormat`, and `PublishChainCommitFormat`). What ADR-0007
   §7 STILL does not settle — and what remains genuinely open here —
   is `FactPayload`'s own field shape: its `typ` value(s) and the
   owner-vs-governance discriminator `[claim-owner-fact]` requires. A
   follow-up amendment to
   atom-transactions.md (or a joint one with atom-backend-contract.md)
   MUST settle `FactPayload` before `[charter-fact-chain]` /
   `[claim-fact-chain]` / `[claim-owner-fact-storage]` can move past
   `evaluator: test` in name only.
