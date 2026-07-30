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
walk — omission is unconstructible, not merely detectable, the same
property [ADR-0007 §7.2](0007-atom-version-integrity-system.md) already argued for and Alt 9 already
rejected the alternative (a generic, updatable tree) for. That argument
is inherited by citation for the spine, unchanged. It does not transfer
to the cloud: there, omission by a dishonest or incomplete source is
already the baseline threat no structure removes, so an append-only
shape would pay for a guarantee it cannot supply. Mutability follows the
algebra the content actually has, not a uniform policy across both.

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

### 5. The published object's carrier remains untrusted scaffolding, not a trust boundary [acp-published-object]

[ADR-0007 §7.3](0007-atom-version-integrity-system.md) and [§7.4](0007-atom-version-integrity-system.md)'s published object — a parentless commit
whose tree is the source tree at the same OID `position:path` already
resolves to, and whose message carries a verbatim copy of the publish
record — is inherited by citation, unaffected: publish remains a spine
genesis record under Decision 2, and nothing about the spine/cloud split
touches how a version's content is packaged for transport. What this
decision depends on explicitly, rather than by inheriting it silently,
is [ADR-0007's Alt 1](0007-atom-version-integrity-system.md): the carrier's structural shape —
parent pointers, tree entries, tag targets — carries zero security
weight, distinguished sharply from a proposal (rejected, unmodified)
where the carrier's own structure would be part of what a verifier
reasons about. A record known only from its own carrier is
self-declaration; the inclusion proof against the spine's own root —
[ADR-0007 §7.4](0007-atom-version-integrity-system.md)'s step 3, inherited unchanged — is what keeps a
publish from reducing to exactly that.

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

## What changes, and what does not

Every subsection of [ADR-0007](0007-atom-version-integrity-system.md)
§§1–21 retains content: none is deleted. This is the full map.

| ADR-0007 §                                              | Fate under this decision                                                                                         |
| :-------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------ |
| §1 (compositional unit)                                   | inherited by citation, unaffected                                                                                    |
| §2 (record envelope)                                       | inherited by citation, unaffected — `pay.prior`/`pay.retracts` reused directly (Decisions 1, 3)                     |
| §3 (genesis-once, facts-forever)                            | **superseded** — taxonomy replaced by Decision 2; genesis-once reasoning inherited by citation                     |
| §4 (anchor vs. position)                                    | inherited by citation, unaffected — reasoning only, per this document's own citation discipline                     |
| §5 (subject shape)                                          | unaffected; retained as ADR-0007's own operative content pending its existing spec-amendment obligation             |
| §6 (multi-tree subjects)                                    | unaffected, same as §5                                                                                                |
| §7.1 (one log, every fact at every scope)                    | **superseded in part** — the log now holds spine content (genesis, amendment) only; facts move to Decision 7's channel |
| §7.2–§7.4 (`eml` shape, checkpoints, closure artifact)        | inherited by citation, unaffected — spine-scoped content (Decision 5)                                               |
| §7.5 (fact discovery via tag chain)                          | **superseded for facts** (Decision 7 replaces it); unaffected for amendment discovery on the spine                  |
| §8 (ref-leaf discriminator)                                  | inherited by citation, unaffected                                                                                    |
| §9 (ref format)                                              | **superseded in part** — `refs/atom/log` now names the spine only; fact discovery no longer resolves through it     |
| §10 (inclusion/consistency/completeness)                     | inherited by citation for spine content; the completeness scan is spine-scoped, not cloud-scoped                    |
| §11–§12 (head, per-label freshness)                          | unaffected for spine recency; per-label `fact_commitment` tracking needs reconciliation to the cloud — Open Items    |
| §13 (interface mandatory)                                    | inherited by citation, unaffected                                                                                    |
| §14 (legacy/native boundary)                                 | inherited by citation, unaffected                                                                                    |
| §15 (content liveness)                                       | inherited by citation, unaffected                                                                                    |
| §16 (degradation ladder)                                     | inherited by citation for spine recency/completeness, unaffected                                                     |
| §17 (fork proofs)                                            | inherited by citation, unaffected — spine-scoped                                                                    |
| §18 (signed trusted mirrors)                                 | inherited by citation, unaffected                                                                                    |
| §19 (write gate)                                             | **superseded** — refuted; replaced by Decision 8                                                                    |
| §20 (store two-phase fetch)                                  | inherited by citation for spine/closure fetch; the cloud is a further, additive phase, not yet specified            |
| §21 (store ref format & GC)                                  | inherited by citation, unaffected for the spine/closure; ADR-0007's own already-deferred store-ref-keying pass now additionally covers the cloud |

The parts marked **superseded** are this decision's actual content:
Decisions 1–2 and 7–8 above. Every other row is either untouched or
retained pending the spec-amendment obligation
[ADR-0007](0007-atom-version-integrity-system.md)'s own "Obligates
amendment of" header field already
names (`git-storage-format.md`, `atom-transactions.md`,
`atom-backend-contract.md`, `trust-model.md`, `lock-file-schema.md`,
`docs/models/atom-model.md` §2) — this decision extends that existing
obligation to the cloud's own realization; it does not open a new one.

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

### Negative / burden

- The cloud's store-level realization (a sorted, history-independent,
  sparse authenticated tree, Decision 6) is new implementation surface
  with no prior art in this corpus's landed mechanism; its structure
  choice is not settled here (Open Items).
- [ADR-0007 §11–§12](0007-atom-version-integrity-system.md)'s per-label
  `fact_commitment` freshness check assumes an ordered fact stream that
  no longer exists once facts are union-merged; reconciling head's
  freshness apparatus to the cloud is unresolved (Open Items).
- [ADR-0007 §21](0007-atom-version-integrity-system.md)'s already-open
  store-ref-keying question now additionally needs to address the
  cloud's own collision-safety across mutually untrusted sources — an
  extension of an existing obligation, not a new category of one.
- Decision 9's tension with [ADR-0005 §2](0005-hermetic-transactional-composition.md) stands named and unresolved,
  tracked externally.

### Neutral

- [ADR-0007](0007-atom-version-integrity-system.md) §§5–8, 13–15
  (publish mechanics), §10 (verification procedure), §16 (degradation
  ladder), §17 (fork-proof layering), §18 (mirror set), and §21 (store
  GC) are unaffected in substance — this decision narrows what the
  record log covers; it does not redesign any of them.
- The `[fact-kind-table]`'s derived-fact kinds (`build-record`,
  `interface-manifest`, `observation-record`, `trial-attestation`) and
  its asserted-fact kinds (`advisory`, `deprecated`, `yanked`,
  `superseded-by`, `runtime-requires`) are unaffected in kind or tier by
  this decision; only their storage home moves, from a scope inside the
  single log to the cloud.

## Open Items

- **Head's per-label freshness needs reconciliation to the cloud.**
  [ADR-0007 §12](0007-atom-version-integrity-system.md)'s
  `fact_commitment` field names "the czd of the latest signed fact leaf
  on `latest_publish`'s own fact stream" — a concept that presupposes an
  ordered stream. Under Decision 7, a subject's facts are a set with no
  total order. Whether head's freshness claim narrows to cover the
  spine alone (charter/claim/publish/amendment recency), or the cloud
  gains its own freshness primitive, is not settled by the design this
  decision draws on, and is not guessed here.
- **The cloud's store-level tree shape.** Sparse, sorted, and
  history-independent are named constraints (Decision 6); the concrete
  structure (a Merkle search tree, specifically) is not decided at the
  protocol level in this document.
- **ADR-0008's anchoring language.** [ADR-0008 §2](0008-surety-of-source.md) permits anchoring
  "in the target's own record log"; under Decision 2, a source-class-
  vouch is a fact and therefore cloud-resident, not spine-resident.
  Whether "record log" there should now read "cloud" is a reading this
  document flags but does not make — [ADR-0008](0008-surety-of-source.md) is Related, not
  superseded, and amending its language is outside this decision's
  surface.
- **Store ref-keying across mutually untrusted sources.** Already open
  in [ADR-0007 §21](0007-atom-version-integrity-system.md); this
  decision extends its scope to the cloud (Consequences, Negative)
  without resolving it.

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

### Alt: a single, uniform freshness mechanism across spine and cloud

Considered and set aside rather than decided: extending
[ADR-0007 §11–§12](0007-atom-version-integrity-system.md)'s head
heartbeat to cover cloud facts would require an ordering the cloud's
algebra does not have, the same objection Decision 1 raises against an
append-only cloud generally. Left to the spec-drafting pass (Open
Items) rather than resolved here, since no candidate design for it
exists yet to evaluate.

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
