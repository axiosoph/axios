# ADR-0009: The Atom Composition Plane — Spine and Cloud

- **Status**: ACCEPTED
- **Date**: 2026-07-30
- **Deciders**: nrd
- **Supersedes**: [ADR-0007](0007-atom-version-integrity-system.md) §3
  (`[atom-genesis-and-facts]`) — its genesis/facts-forever taxonomy is
  replaced by the three-class, two-algebra split below — and §19
  (`[atom-write-gate]`) — its single-write-gate-for-every-append claim
  is refuted and replaced. See "What changes, and what does not," below,
  for the full section-by-section map; no ADR-0007 subsection is deleted.
- **Related**: [ADR-0008](0008-surety-of-source.md) — the source-class-
  vouch fact type is unaffected in mechanism, its anchoring language
  wants a later reading against the cloud (Open Items); [ADR-0005](0005-hermetic-transactional-composition.md) §2 — a named,
  unresolved tension, not settled here (Decision 9); [Composition
  Model](../models/composition-model.md), [Atom Model](../models/atom-model.md), [Storage Model](../models/storage-model.md)

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

[ADR-0007](0007-atom-version-integrity-system.md)'s Context — the
problem that started it (publishing a pre-existing upstream project
without forking its history), and its two reframings (atom as a second,
independent index over git's content-addressed store; git object
structure as backend scaffolding, never a trust boundary) — is inherited
by citation and not restated here. Both reframings hold unchanged; what
changes is what that second index is shaped like once a genesis record
exists.

### The contradiction this decision resolves

[Composition Model §6](../models/composition-model.md#6-the-fact-set-the-substrates-only-state)
states this corpus's own general law for anything shaped like a fact:
"insertion is commutative and idempotent (set-union shape)"; replicas
"converge by union," and "nothing anyone writes can invalidate what
another party already consumed." [Atom Model
§4](../models/atom-model.md#4-the-metadata-partition-law) instantiates
that law for atom's own metadata chain: fact is "appended, monotone, and
signer-attributed," riding the same commutative, idempotent state as
every other fact in the substrate. `atom-transactions.md`'s
`[fact-kind-table]` makes the `advisory` fact kind **open**-tier
specifically "because independent security-research value depends on
non-project-controlled signers being admissible; gating it to the claim
owner would let a compromised or uncooperative owner suppress advisories
about their own package."

[ADR-0007 §7.1](0007-atom-version-integrity-system.md) and [§19](0007-atom-version-integrity-system.md)
built something stronger than that law permits: "every leaf a project
ever signs — its charter genesis, its labels' claim geneses, every
publish, every fact at any scope — is a leaf in one continuous,
append-only Merkle tree" (§7.1), and "every append, at every scope,"
CAS-updates the anchor's one moving ref (§19) — a single, project-owned,
push-gated write path. A non-project-controlled security researcher has
no push access to the project's own repository. The design that
`[fact-kind-table]` requires and the storage mechanism that §7.1/§19
built cannot both be true: the fact-kind table needs advisories writable
by anyone with the right role; the write gate admits only whoever can
push to the anchor's ref. This is not merely a stronger-than-needed
guarantee — it is unimplementable against the corpus's own stated
requirement, and it is the specific contradiction this decision
resolves.

### Forces

Inherited by citation from [ADR-0007](0007-atom-version-integrity-system.md)'s own Forces, unchanged: **detachment**
(records are attached by signed metadata, never by git ancestry);
**the burden law** (transfer/verification cost is O(the unit), never
O(any repository it points into)); **self-containment** (all
verification-relevant metadata lives inside the signed payload); **forge
tolerance** (a registry's ref economy must never grow unboundedly with
activity it cannot control).

**The write-gate refutation — the force this decision exists to
resolve, and the one absent from a bare statement of the split.**
[ADR-0007 §19](0007-atom-version-integrity-system.md) states, as a
named decision, that a registry's write gate is "a single mechanism, not
two": "every append, at every scope,"
CAS-updates `refs/atom/log`. That claim is refuted by the contradiction
above — not weakened, not narrowed, refuted: no single, project-owned
CAS gate can admit an append from a signer the project does not control
push access for, and the corpus already requires exactly such an append
to be admissible (`[fact-kind-table]`'s open-tier `advisory`). A
decision that states a spine and a cloud without naming this is a
conclusion without the force that produced it. The split below is not a
preference for a different shape; it is what a write-gated,
project-owned mechanism cannot be, once a fact class the corpus already
requires is writable by parties the project does not control.

## Decision

### 1. The plane splits into two structures, by algebra rather than by scope [acp-spine-cloud-split]

|                | spine                                    | cloud                                |
| :------------- | :---------------------------------------- | :------------------------------------ |
| contents       | charter, claims, publishes, amendments    | facts                                 |
| shape          | append-only ordered log                   | history-independent authenticated set |
| scope          | one per anchor                            | one per subject                       |
| writers        | the owner-set alone, one write gate       | any anchored signer holding the role  |
| order          | meaning — the fold, genesis-once          | noise                                 |
| bounded by     | release + governance cadence              | nothing                               |

The criterion is [Atom Model §4](../models/atom-model.md#4-the-metadata-partition-law)'s own metadata partition law, read as a
placement rule rather than only a field-classification one: spine holds
what the set's own authority asserts (intent); cloud holds what the
world observes about it (fact). A criterion tried and rejected during
this exploration — "spine iff it changes who is authorized" — left
genesis records with no home, since a charter or publish record asserts
identity without itself changing any authorization; the intent/fact
line does not have that gap.

The spine is chain-complete: [ADR-0007 §2](0007-atom-version-integrity-system.md)'s existing `pay.prior` field
chains every spine leaf to its predecessor, so omitting one breaks the
walk — omission is unconstructible, not merely detectable.
[ADR-0007's Alt 9](0007-atom-version-integrity-system.md) rejected a
generic, updatable tree for exactly this reason: such a tree can prove
an edit happened, never that omitting one didn't matter. That rejection
holds for the spine unmodified. It does not transfer to the cloud:
there, omission by a dishonest or incomplete source is already the
baseline threat no structure removes, so an append-only shape there
would pay for a guarantee it cannot supply. Mutability follows the
algebra the content actually has, not a uniform policy across both.
(§7's own append-only/checkpoint mechanism moves to specification —
see "What changes, and what does not," below — so this document cites
the shape's rationale without resting on §7 as still-operative ADR
content.)

### 2. Three record classes replace genesis-once/facts-forever [acp-record-classes]

| class         | shape                              | home  | answers                |
| :------------ | :---------------------------------- | :---- | :---------------------- |
| **genesis**   | write-once, per scope               | spine | what is this            |
| **amendment** | ordered chain                       | spine | who is authorized       |
| **fact**      | monotone assertion, withdrawable    | cloud | what is known about it  |

[ADR-0007 §3](0007-atom-version-integrity-system.md)'s taxonomy — charter/claim/publish as permanent genesis,
everything after as one undifferentiated "fact" — no longer suffices,
because "fact" was covering two things with different algebras: owner-
set rotation, threshold changes, label transfer, and governance
amendment (ordered, spine-resident, authorized by the current fold —
[ADR-0007 §3](0007-atom-version-integrity-system.md)'s fold-based authorization rule, inherited by citation
unchanged) are now **amendment**; yank, deprecation, advisories, and
attestations (any anchored role-holder, cloud-resident, union-merged)
remain **fact**. [ADR-0007 §3](0007-atom-version-integrity-system.md)'s genesis-once reasoning — a charter,
claim, or publish is written once, at the moment its scope first comes
into existence, and never again; the only way to get a new one is a
genuine fork — is inherited by citation unchanged; only which acts
belong to "genesis" is untouched, since amendment and fact are both
disjoint from it already.

### 3. Overwrite-class fields dissolve into assertion and withdrawal [acp-assertion-withdrawal]

A field specified as latest-wins (a version's `mode`, for instance)
needs a total order the cloud's set-union algebra cannot supply, and a
signing timestamp is not trustworthy as that order: an adversary can
replay a still-valid, older, favorable assertion while withholding a
later, unfavorable one, and a consumer requiring that assertion passes
a gate that should have refused. A signed, permanent, replayable
assertion with no ordering is a rollback primitive by construction, not
by mistake.

The repair sits upstream of the ordering problem rather than inside it:
re-express the field as an assertion plus a withdrawal, using the
`pay.retracts` field [ADR-0007 §2](0007-atom-version-integrity-system.md) already defines — no new mechanism.
A withdrawal names its target by czd, so no order is ever consulted;
the only question a verifier asks is whether a given assertion has been
withdrawn.

### 4. Terminality is forced by content immutability, not imposed by design [acp-terminality]

A published version's content is immutable, so any property computed
from that content — reproducibility, among others — is a fixed fact
about it, never a state it transitions through. A promotion asserts a
fixed fact; a withdrawal corrects a false assertion of one; there is no
third transition, because a fix is new content and therefore a new
version, outside the field's own state space entirely. The reachable
states form a three-element progression with no cycles, so the
order-freedom Decision 3 relies on is a structural consequence of what
the field is about, not a policy choice layered on top of it.

### 5. The published object is reached only through the record; the closure has no artifact of its own [acp-published-object]

A publish's ref targets the tip of the publish's own materialized view
chain — an ordinary commit chain over the append-only record, starting
at the publish and extended by one commit per later fact, never a tag
or a tag chain. A consumer reads and verifies the record that chain
carries before content is reachable at all: the ref names the record,
never a bare content pointer, so there is no path that bypasses
reading and verifying the record first (Decision 2 of the
record-anchored publishing amendment this section now states). The
atom's content is a second, parentless commit — tree equal to the
source tree, the same OID `position:path` already resolves to, fixed
inert fields, empty message — hung off the chain's FIRST commit as a
second parent, contributing exactly itself, its tree, and its blobs;
nothing else is reachable from it. Both objects are transport, never
authority. What this document depends on explicitly, rather than by
inheriting it silently, is [ADR-0007's Alt
1](0007-atom-version-integrity-system.md): the carrier's structural
shape — parent pointers, tree entries, commit chains — carries zero
security weight, distinguished sharply from a proposal (rejected,
unmodified) where the carrier's own structure would be part of what a
verifier reasons about. This is the property the design strengthens,
not the one it revises.

What changes is the mechanism by which a consumer trusts the object.
[ADR-0007 §7.4](0007-atom-version-integrity-system.md)'s closure — a
two-leaf `eml` tree materialized at publish time, embedding a content
commitment alongside a snapshot of the anchor log's own historical
root — has no place in this design and is not carried forward by
citation: this document's publish verification instead proves
inclusion of the publish record directly against the spine's own,
current root, following its `claim`/`anchor`/charter chain to fold
governance. There is no closure object distinct from that proof; a
publish's authorization state is a property of (record, spine),
computed on demand, never a fixed artifact frozen at the moment of
publication. `[k-derivation-signed-only]` — the rule pinning §7.4's
embedded snapshot index — has no referent to carry forward for the
same reason: nothing embeds a snapshot to index. A record known only
from its own carrier is self-declaration regardless of which of these
two verification shapes is used; the inclusion proof against the
spine is what keeps a publish from reducing to exactly that.

A depth-1 fetch is correct for the main log ref alone; a publish view
ref must never be fetched at depth 1 — its chain is bounded by the
records relevant to that publish, not by history depth, and the
correct fetch is the whole chain, unconditionally. Applying the main
log's depth-1 discipline to a publish ref would silently omit the
content commit and hand a client a verified record with none of the
bytes it just verified the hash of.

### 6. The store is a flat, content-addressed keyspace, with every index derived [acp-store]

`store : czd → object`. Every object — charter, claim, publish, fact,
amendment — is a signed envelope whose identity is its own czd; one
keyspace, one lookup, no reduction to a uniform hash space and no
double-keying by subject, since a fact's own czd already covers a
payload naming its subject. Every index over the store — a ref, a
name-keyed lookup, a subject index — is a pure, recomputable,
discardable function of the store's own contents, never a value fetched
and trusted on its own, the same discipline [Storage Model
§5](../models/storage-model.md) already states for sharing "the
content-addressing idea, not a namespace."

The cloud is realized as an authenticated set per subject, embedded into
a store-level tree: a history-independent structure (its root a
function of the set alone, never of arrival order), sorted rather than
insertion-ordered, so it can be shipped as a summary without being
trusted — supplying, among other things, the fact-set snapshot name
[Composition Model §6](../models/composition-model.md#6-the-fact-set-the-substrates-only-state) already requires and nothing in the prior
corpus provided.

### 7. The fact channel: subject-scoped pull, union merge [acp-fact-channel]

```
facts.roots(subjects: [czd]) → [(subject, root)]   -- over the ADVERTISED set
facts.diff(subject, my_root) → [fact_czd]
facts.get(czds)              → [bytes]
```

Bounded by a set the consumer already holds — its own closure — never by
a peer's total fact count; merge is union, so no coordination, no
ordering, and no conflict resolution is needed. This is the channel
that dissolves the Context contradiction directly: a fact lives in its
own author's store and propagates peer to peer; nobody writes into
anyone else's registry, and no push access to the subject's own project
is required to hold or serve an opinion about it. Divergent roots for
one subject from two independently-queried sources is, for free, an
equivocation signal — the store is the only place records from multiple
sources meet, and a split view surfaces as exactly that divergence.

### 8. The write gate governs the spine alone [acp-write-gate-scope]

[ADR-0007 §19](0007-atom-version-integrity-system.md)'s mechanism — a writer reads the current ref target,
chains the new leaf's `pay.prior` to it, and attempts git's own atomic
compare-and-swap — is inherited by citation, unchanged, for spine
appends: genesis and amendment. It no longer governs facts, which
Decision 7's channel admits without any CAS point at all. This is the
narrowing the Forces section's refutation requires: not a weaker write
gate, a write gate scoped to exactly the content whose algebra needs
one.

### 9. A named, unresolved tension with ADR-0005's action identity [acp-action-identity-tension]

[ADR-0005 §2](0005-hermetic-transactional-composition.md) frames
`action_id` as "the cache-key primitive," "replacing the drv hash" —
language that imports the drv hash's bidirectional property (same key
implies same inputs) onto an identifier that, once transitive
configuration can vary independently of the intent-level closure root it
is computed from, does not have it. This plane's own published-object
and fact-channel design does not depend on that property anywhere above,
and this document does not amend, supersede, or resolve [ADR-0005
§2](0005-hermetic-transactional-composition.md) — the disagreement is
tracked as [issue #101](https://github.com/axiosoph/axios/issues/101),
filed against the build engine's use of `action_id` as a cache and job
key, out of this decision's own scope.

### 10. Interface manifests are facts, not mandatory publish content [acp-interface-manifests-are-facts]

[ADR-0007 §13](0007-atom-version-integrity-system.md)'s package-atom
clause — the built interface/output descriptor is publish's mandatory
artifact, "verifiable only by building" — is superseded: an interface
manifest is a fact (derived, signed by its deriving analyzer or
executor), the same classification [Atom Model
§4](../models/atom-model.md#4-the-metadata-partition-law) and its
[§5](../models/atom-model.md#5-the-signature-anchoring-law)
signature-anchoring table already give it. A store populated with
content but empty of facts fails to compose while appearing complete,
since interface resolution cannot run without the manifests it
needs — the same closure-completeness obligation Decision 7's channel
exists to make an explicit property of ingestion rather than an
assumed one. §13's environment/system half — whether a coherence
certificate remains publish-mandatory for algebraic intent — is not
addressed by anything in the design this document draws on, and is not
resolved here (Open Items).

## What changes, and what does not

Every subsection of [ADR-0007](0007-atom-version-integrity-system.md)
§§1–21 retained content as of this document: none was wholly deleted
(§6's later exception is noted below the table). Four dispositions
apply: **survives** (inherited by citation into this document),
**moves to spec** (normative mechanism content, out of an ADR's genre,
relocated by a separate pass), **split** (part survives or moves, part
is superseded), and **superseded and deleted** (refuted outright, no
destination). This is the full map.

| ADR-0007 §                                            | Disposition            | Ground                                                                                                        |
| :------------------------------------------------------ | :----------------------- | :---------------------------------------------------------------------------------------------------------------- |
| §1 (compositional unit)                                  | **survives**             | inherited by citation, the founding framing                                                                    |
| §2 (record envelope)                                     | **survives**             | `pay.prior`/`pay.retracts` reused directly (Decisions 1, 3)                                                     |
| §3 (genesis-once, facts-forever)                          | **split**                | taxonomy superseded (Decision 2); genesis-once reasoning survives, inherited by citation                        |
| §4 (anchor vs. position)                                  | **survives**             | inherited by citation; its `[temporal-vector]` ordering rationale is flagged for a later restatement not performed here (Open Items) |
| §5 (subject shape)                                        | **moves to spec**        | normative field shape, outside this ADR's genre                                                                 |
| §6 (multi-tree subjects)                                  | **moves to spec** as of this document; **superseded and deleted, 2026-08-23** | same as §5, as this document originally had it; the relocation never executed, and is now moot — [ADR-0007 §6](0007-atom-version-integrity-system.md) itself retires multi-tree subjects on the head's ruling (single content tree), so there is no longer any content to move to spec |
| §7 (record log, checkpoints, closure, fact-tags)          | **split, three ways**    | the record-log/checkpoint structure moves to spec as the spine's own mechanism; fact-tags are superseded (facts move to the cloud, Decision 7); the closure artifact (§7.4) is deleted with no destination (Decision 5) |
| §8 (ref-leaf discriminator)                               | **moves to spec**        | a governing rule, not a rationale                                                                               |
| §9 (ref format)                                           | **moves to spec**        | `refs/atom/pub/{label}/{version}` retained; one log survives as the spine (Decision 8)                          |
| §10 (inclusion/consistency/completeness)                  | **split**                | inclusion and consistency proofs move to spec; the completeness scan is superseded and deleted outright — it proved nothing was missed from an ordered fact stream that no longer exists |
| §11–§12 (head, per-label freshness)                       | **split**                | Tier-2 gating superseded together with §16; the completeness role was already gone by §11's own text; recency attestation is the one genuinely open residue (Open Items) |
| §13 (interface mandatory)                                 | **split**                | interface manifests are facts (Decision 10), superseding the package-atom mandatory-artifact clause; the stratum-aware half is unaddressed (Open Items) |
| §14 (legacy/native boundary)                              | **split**                | "registries never ingest" survives; the ancestry predicate is superseded by a declared-plus-spine-monotone check |
| §15 (content liveness)                                    | **survives**             | zero normative keywords; its point — liveness is structural, not bespoke — is preserved by the second-parent content commit (Decision 5) |
| §16 (degradation ladder)                                  | **split**                | completeness/recency separability survives; Tier 2's fail-closed default is superseded by a scoped freshness-refusal policy |
| §17 (fork proofs)                                         | **survives**             | zero normative keywords; divergence-is-not-intent survives, given a second expression by the fact channel's equivocation detection (Decision 7) |
| §18 (signed trusted mirrors)                              | **survives**             | inherited by citation, unaffected                                                                               |
| §19 (write gate)                                          | **split**                | the every-append claim is refuted and deleted (Forces); the publish-ref existence CAS point moves to spec       |
| §20 (store two-phase fetch)                               | **moves to spec**        | verify-before-fetch survives as spec content, extended to ingest-closure-completeness over records, facts, and content |
| §21 (store ref format & GC)                               | **split**                | the ref-format half is superseded by the store's flat `czd → object` keying (Decision 6); GC by ordinary reachability survives and moves to spec |

Ten of twenty-one split rather than surviving or moving wholesale — a
binary retire-or-keep would have mis-typed roughly half this table. As
of this document, zero were wholly deleted: every one retained something
that survived, moved, or was inherited. **That changed 2026-08-23:**
§6's "moves to spec" disposition never executed, and [ADR-0007
§6](0007-atom-version-integrity-system.md)
itself retires multi-tree subjects outright, on the head's ruling. §6
is now the one row of twenty-one that is wholly deleted rather than
split, moved, or surviving.

This document inherits by citation only §§1, 2, 4, 18, and §3's
reasoning (Decisions 1–4 above), and carries §19's refutation in
Forces — the narrowest set that actually supports Decisions 1–10.
Every other row above is disposition information for the reader, not
content this document relies on as still-operative ADR-0007 material;
the sections marked "moves to spec" are, by this document's own genre
rule, exactly the kind of content an ADR must not carry even by
citation-with-endorsement. Their relocation is a separate, obligated
pass — extending, not replacing, the spec-amendment obligation
[ADR-0007](0007-atom-version-integrity-system.md)'s own "Obligates
amendment of" header field already names (`git-storage-format.md`,
`atom-transactions.md`, `atom-backend-contract.md`, `trust-model.md`,
`lock-file-schema.md`, `docs/models/atom-model.md` §2).

## Consequences

### Positive

- The Context contradiction is dissolved by construction: an advisory,
  or any other open-tier fact, needs no push access to the subject's own
  project, so `[fact-kind-table]`'s existing requirement and the storage
  mechanism agree for the first time.
- Equivocation detection over facts is free: a divergent cloud root for
  one subject, observed from two independently-queried sources, is the
  signal itself, with no additional protocol.
- Genesis-once — omission as unconstructible, not merely detectable — now
  applies to exactly the content whose algebra can honor it, rather than
  being asked to also cover a state shape (union-merged facts) that
  structurally cannot.
- [Composition Model §6](../models/composition-model.md#6-the-fact-set-the-substrates-only-state)'s own requirement for a fact-set snapshot
  name is supplied by the store-level cloud tree (Decision 6), closing a
  gap the prior corpus stated but never filled.
- [ADR-0007 §21](0007-atom-version-integrity-system.md)'s own
  already-open question — collision-safety when a store aggregates
  mutually untrusted registries — is resolved by construction, not
  merely deferred further: a flat, content-addressed keyspace
  (Decision 6) has no human-readable namespace for two registries to
  collide over in the first place.

### Negative / burden

- The cloud's store-level realization (a sorted, history-independent,
  sparse authenticated tree, Decision 6) is new implementation surface
  with no prior art in this corpus's landed mechanism; its structure
  choice is not settled here (Open Items).
- [ADR-0007 §§11–12](0007-atom-version-integrity-system.md)'s head
  mechanism loses two of its three roles outright (Tier-2 gating,
  completeness) and the third — recency attestation — has no successor
  named by the design this document draws on (Open Items). A consumer
  loses head's specific guarantee (a signed, TTL-bounded "as of right
  now") until that residue is resolved.
- Decision 9's tension with [ADR-0005 §2](0005-hermetic-transactional-composition.md) stands named and unresolved,
  tracked externally.

### Neutral

- [ADR-0007](0007-atom-version-integrity-system.md) §§15 and §17
  (content liveness; divergence proves nothing about intent) are
  unaffected in substance and carry zero normative keywords of their
  own — this decision neither redesigns nor relocates them. Every
  other subsection either moves to specification or splits; see "What
  changes, and what does not" for the full, disposition-by-disposition
  map — it is not restated here.
- The `[fact-kind-table]`'s derived-fact kinds (`build-record`,
  `interface-manifest`, `observation-record`, `trial-attestation`) and
  its asserted-fact kinds (`advisory`, `deprecated`, `yanked`,
  `superseded-by`, `runtime-requires`) are unaffected in kind or tier by
  this decision; only their storage home moves, from a scope inside the
  single log to the cloud.

## Open Items

- **What attests recency is not settled.** [ADR-0007
  §11](0007-atom-version-integrity-system.md) names three roles for
  its head mechanism: Tier-2 gating (superseded together with §16's
  fail-closed default), completeness ("no longer depends on head at
  all," by §11's own text — already gone before this decision), and
  recency — head is "the only mechanism" that can prove current state
  "as of right now." The fact channel's `facts.roots` comparison
  (Decision 7) detects *divergence* between two sources; it does not
  *attest* recency, a different property, and §11's own account of a
  head record — an object that "simply replaces the previous one at
  its own separate ref" — is an overwrite-at-a-mutable-ref shape this
  document's move to assertion-plus-withdrawal (Decision 3) and
  immutable, czd-keyed envelopes (Decision 6) does not have room for.
  Under the fact channel and a T3-shaped freshness problem, what
  attests recency is genuinely new design, not a restatement of §11,
  and it is not settled by anything this document draws on. Not
  guessed here.
- **The cloud's store-level tree shape.** Sparse, sorted, and
  history-independent are named constraints (Decision 6); the concrete
  structure (a Merkle search tree, specifically) is not decided at the
  protocol level in this document.
- **§13's stratum-aware half.** Whether an environment or system
  atom's coherence certificate remains publish-mandatory, now that
  manifests generally are reclassified as facts (Decision 10), is not
  addressed by anything in the design this document draws on.
- **§4's `[temporal-vector]` rationale.** [ADR-0007
  §4](0007-atom-version-integrity-system.md)'s position-ordering
  invariant survives by citation (Decision 1 does not touch it), but
  its stated rationale is flagged, not restated, as needing a fresh
  pass once recency attestation (above) is resolved — this document
  does not have enough settled about that residue to state what the
  restatement should say, and does not guess.
- **ADR-0008's anchoring language.** [ADR-0008 §2](0008-surety-of-source.md) permits anchoring
  "in the target's own record log"; under Decision 2, a source-class-
  vouch is a fact and therefore cloud-resident, not spine-resident.
  Whether "record log" there should now read "cloud" is a reading this
  document flags but does not make — [ADR-0008](0008-surety-of-source.md) is Related, not
  superseded, and amending its language is outside this decision's
  surface.
## Alternatives Considered

### Alt: retain one write-gated log for both spine and cloud content

This is [ADR-0007 §7.1](0007-atom-version-integrity-system.md) and
[§19](0007-atom-version-integrity-system.md) as written. Rejected for
the reason stated in Forces: it is unimplementable against
`[fact-kind-table]`'s own open-tier requirement, not merely stronger
than needed.

### Alt: carrier as trust boundary (cited, not restated)

[ADR-0007's Alt 1](0007-atom-version-integrity-system.md) rejected
using git commit or tag structure as part of what a verifier reasons
about, distinct from using that structure as untrusted scaffolding
cross-checked against the signed payload. That rejection stands
unmodified and is load-bearing for Decision 5's published object.

### Alt: retain §11–§12's head record, unchanged, as the recency mechanism

Considered and rejected rather than merely deferred: [ADR-0007
§11](0007-atom-version-integrity-system.md)'s head is, by its own
account, an object that "replaces the previous one at its own separate
ref" — an overwrite-at-a-mutable-ref shape this document's move to
assertion-plus-withdrawal (Decision 3) and immutable, czd-keyed
envelopes (Decision 6) has no room for, independent of whether head is
scoped to the spine or extended to the cloud. What replaces it for
recency specifically is not decided here (Open Items), but keeping
§11–§12 as written is not a live option under Decisions 3 and 6.

## Related Documents

- [ADR-0007: Atom as a Version Integrity System](0007-atom-version-integrity-system.md) — superseded in part; see "What changes, and what does not."
- [ADR-0008: Surety of Source](0008-surety-of-source.md) — related, its
  fact mechanism unaffected in kind; its anchoring language flagged in
  Open Items.
- [ADR-0005: Hermetic Transactional Composition](0005-hermetic-transactional-composition.md) §2 — a named, unresolved
  tension (Decision 9), tracked as
  [issue #101](https://github.com/axiosoph/axios/issues/101).
- [Composition Model §6](../models/composition-model.md#6-the-fact-set-the-substrates-only-state) — the commutative, idempotent,
  set-union law this decision applies to atom's own facts.
- [Atom Model §4](../models/atom-model.md#4-the-metadata-partition-law) — the intent/fact partition this decision uses as its
  placement criterion; §5's signature-anchoring law, unaffected.
- [Storage Model §5](../models/storage-model.md) — "share the
  content-addressing idea, not a namespace," the discipline Decision 6
  applies to the store's derived indexes.
