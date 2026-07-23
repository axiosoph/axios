# ADR-0007: Atom as a Version Integrity System — The Compositional Unit

- **Status**: ACCEPTED (contingent on nrd's ratification of the
  2026-07-22 freshen pass — the §7 verification fixes and the model/spec
  reconciliation folded in below; reviewed against the landed atom model
  in `docs/models/atom-model.md` and the glossary, objects mapping
  cleanly). The §7 git-representation layer's residual open items remain
  visible in Open Items.
- **Date**: 2026-07-15, revised 2026-07-16, freshened 2026-07-22
- **Deciders**: nrd
- **Extended by**: [ADR-0008](0008-surety-of-source.md) — surety of
  source: the source-class-vouch fact type, vouch anchoring, and the
  totality / trust-surface accounting
- **Source**: a first-principles re-derivation of atom's model, conducted
  incrementally over an extended session (2026-07-14 through 2026-07-16),
  including two rounds of decorrelated adversarial review (refuter,
  prior-art, security personas) against the 2026-07-15 draft, followed by
  a second, deliberately unprimed session (2026-07-16) that revisited the
  git-representation layer —
  how records, closures, and facts are actually laid out as git
  objects — and found it awkward and redundant. That second pass is
  what this revision folds in: one unified append-only record log per
  anchor (an `eml`-backed, MMR-derived k-ary Merkle tree, single-root
  canonicalized, over git tree objects, wrapped in checkpoint commits —
  RFC 6962 was this construction's starting point, since significantly
  diverged, §7.2), a clean separation between the immutable
  publish closure and the ongoing fact stream, and the discovery that
  most of §15 and all of §21's bespoke liveness/GC machinery become
  unnecessary once content hangs directly off structurally reachable
  git objects instead of being referenced obliquely through signed
  fields.
- **Supersedes**: none. ADR-0007 opens a decision area (atom as a
  version-integrity system, at the git-object layer) that ADRs 0001–0006
  do not cover; ADRs 0005/0006 remain Related, not superseded. "Supersedes"
  in this repo's convention names a prior *ADR decision* replaced — there
  is none.
- **Obligates amendment of**: the atom specs and formal artifacts this
  ADR was deliberately drafted without re-reading, which its `eml`-backed
  single-log representation (§7), unified fact mechanism (§3), and
  genesis-once discipline (§10) change — `git-storage-format.md`,
  `atom-transactions.md`, `atom-backend-contract.md`,
  `lock-file-schema.md`, `docs/models/atom-model.md` §2 (still teaching
  the repudiated `AtomId = (anchor, label)`), and the stale formal models
  that machine-check the superseded design (`AtomCharter_Succession.cfg`,
  `atom_structure.als`). This spec-amendment obligation is distinct from
  ADR supersession — the same relation `docs/models/atom-model.md` §8
  already models as a first-class "doc amendments this model obligates"
  manifest — and is the spec-drafting pass tracked in Open Items.
- **Related**: [Composition Model](../models/composition-model.md),
  [Execution Model](../models/execution-model.md),
  [Storage Model](../models/storage-model.md),
  [ADR-0005](0005-hermetic-transactional-composition.md),
  [ADR-0006](0006-execution-as-the-primitive.md)

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

### The problem that started this

Publishing an existing, pre-atom upstream project (a legacy release of
zlib, say) as an atom without forking its history, and without degrading
the version-integrity guarantee the atom model exists to provide, has no
answer under a model where an atom's identity is fused to a single
git-tree-shaped object embedded in a source commit. The founding capture
of this exploration (2026-07-14) proposed pointing at the pre-existing
commit directly and layering build/dependency declarations on top via
the execution layer, rather than mucking with the original source tree
to attach metadata that could not have existed at that point in history.
That single idea — decouple the *provenance claim* (which commit) from
the *build declaration* (what to do with it) — is the seed the rest of
this document grows from.

### The reframing that made the rest tractable

Pulling on that thread surfaces a larger question the landed model had
never been asked to answer explicitly: **what is atom actually for?** The
answer that emerged, and the single most important finding of this whole
exploration, is that atom is not a packaging convenience bolted onto git
— it is a **second index over git's own content-addressed object store**,
parallel to and independent of git's native version-control index (refs
+ ancestry). Where version control answers "how did this evolve," atom's
index answers "what is this, who says so, in what succession, how
recently." Every git repository's own commit history was always a
partial, accidental instance of this — every commit is, in principle, a
claim about content — but nobody had made the claim *itself* first-class,
signed, and independently verifiable at the granularity of "this specific
act of publication," separate from git's own coarse-grained,
all-or-nothing notion of history. Once atom is understood as a version-
integrity system in its own right, with git as one (excellent,
deliberately chosen, but not structurally required) implementation of
its storage substrate, the bootstrap problem, the equivocation problem,
and the freshness problem reduce to instances of the same small set of
laws rather than needing separate, ad hoc mechanisms each — with one
honesty the rest of this document holds to: equivocation and freshness
are *located and bounded* by those laws, not *statically resolved* by
them. Their non-monotone residue (which authentic head is current, has
anyone forked) is settled only by trusting the charter's signed
canonical-source declaration — the coordination point the charter names,
optionally decentralized to a threshold quorum of its declared trusted
mirrors/witnesses — never by structure a lone consumer can check offline
(§10, §16, §17-L4).

### The second reframing: git structure is a backend, not a trust boundary

A second, later pass (2026-07-16) sharpened this further: everywhere this
document uses a git object — blob, tree, commit, or tag — the object's
*structural* shape (parent pointers, tree entries, tag targets) carries
**zero security weight**. Trust rests entirely, always, on the signed
Coz payload: signature validity and the payload's own `prior`/`under`
chain fields. Git structure is used purely because it is a well-tested,
already-implemented mechanism for cheap, efficient, content-addressed
storage and discovery — reusing git's own object model to avoid
reimplementing chain-walking, garbage collection, and content dedup from
scratch. This principle is what justifies using git commits and tags as
*backend scaffolding* in §7 below, in a way the original derivation's
rejection of "commit as carrier" (Alt 1) did not yet distinguish from
"commit as untrusted DAG index" — the two are different proposals, and
only the first was ever actually rejected.

### Terminology

- **HTC** — Hermetic Transactional Composition, the L2 substrate this
  atom redesign sits beneath and completes the trichotomy of (storage /
  composition / execution), per
  [ADR-0005](0005-hermetic-transactional-composition.md).
- **Anchor** — a project's standing identity: the *scope of ongoing
  authority* its one and only `charter` genesis record establishes (the
  anchor's record log, owner-fold, and anchor-scope facts). The charter's
  czd (a record's own content-address, incorporating its signature — §2)
  **names** the anchor and is the handle every consumer verifies against,
  but is not the anchor itself: the anchor is the linking role the charter
  fills, recomputed from the signed chain, never read off a trusted field
  (see `docs/glossary.md`; the older `anchor = czd(charter)` equation is
  glossary-repudiated). Permanent. The only way to get a new one is a
  genuine fork — a deliberate, out-of-band new identity, never an
  in-protocol succession.
- **Label** — the human-chosen name a project publishes atoms under
  (e.g. `"quill"`). A `claim` record binds a label exclusively to one
  anchor — exclusively *within that anchor's own claim*, not globally:
  labels are deliberately not a global namespace, and two unrelated
  anchors MAY both use the label `"quill"` — a feature, not a
  collision. The label's real identity, once claimed, is the czd of
  that specific claim record — the same discipline as anchor, below.
  Within a verified anchor's own record log, a claim is found by its
  label alone (genesis-once, §10, makes that lookup unambiguous) — the
  label names the claim; the czd is what it is. A deliberate rejection
  of a crates.io/npm-style central namespace authority, not an
  oversight (clarified 2026-07-16).
- **Charter** — the anchor's single genesis record: owner-key set,
  signing threshold (enforced via co-sign facts, §3, not a native
  multisig primitive), declared roles (`attesters`, `witnesses`,
  `freshness-delegate`, `publish-delegate` — each optional, §3, §11),
  and governance parameters (canonical source, trusted mirrors,
  guarantees). Written once, at anchor genesis, never reissued.
  Everything that used to require a new charter record —
  owner-set rotation, threshold changes, governance amendments — is now
  a `fact` on the anchor's own record-log entries (see §3).
- **Claim** — a label's single genesis record: "this anchor publishes
  under this name." Written once, at first claim, never reissued.
  Label transfer and governance amendments are `fact`s on the label's
  record-log entries, the same discipline as charter.
- **Publish** — a version's single genesis record: "this content is
  version V of label L." Written once, per (label, version),
  compare-and-swap (CAS) admitted (§19), never reissued. The version's
  real identity is this record's own czd — the same discipline as
  anchor and label above. Yank, deprecation, advisories, and
  attestations are `fact`s targeting it.
- **Fact** — a signed record layering new information onto an already-
  genesis'd scope (anchor, label, or version) without altering the
  genesis record itself. Nothing is ever deleted or mutated; "removing"
  something means signing a new fact that supersedes or retracts an
  earlier one (§3).
- **The record log** — one append-only, `eml`-backed k-ary Merkle tree
  per anchor (MMR-derived, single-root canonicalized — RFC 6962 was the
  starting point, since significantly diverged, §7.2), containing every
  charter, claim, publish, and fact this anchor has ever signed, in
  true chronological order. The sole source of truth for order and
  validity (§7, §9).
- **Leaf** — one signed record's position in the record log.
- **Checkpoint** — the git commit wrapping the record log's Merkle root
  after a given append; one checkpoint per leaf (§7).
- **Closure** — the fixed, immutable bundle needed to build a specific
  published version: its charter, claim, and publish authorization plus
  its content. Computed once, at publish time, never revised (§7, §9).
  Distinct from, and does not include, the ongoing fact stream about
  that version (§3).

### Forces

- **Detachment.** Atom records are detached from the repository history
  they describe. They are attached by signed metadata (a `prior`
  hash-chain inside the signed payload), never by git ancestry — an
  atom carries none of the transfer/verification burden of the
  repository it points into. This is the load-bearing distinction from
  a naive "make everything a git commit" design, and the reason a
  record can point at an arbitrary, pre-atom historical commit without
  dragging that commit's ancestry into every fetch.
- **The burden law.** The cost of transferring or verifying any protocol
  unit (a record, a chain, a family) must be O(that unit), never O(any
  repository it came from or points into). Every ref-format and
  retrieval-mechanics decision in this document is downstream of this
  one constraint.
- **Self-containment.** All verification-relevant metadata lives inside
  the signed payload; every git carrier — blob, tree, commit, or tag —
  contributes zero semantic fields. Any carrier-level field is
  untrusted-but-visible metadata an operator might mistakenly trust —
  the self-containment law exists specifically to remove that
  temptation structurally, and now extends explicitly to commit and tag
  objects (§7), not only blobs.
- **Forge tolerance.** A registry's ref economy must never grow
  unboundedly with activity it cannot control. This now holds even more
  strongly than the original draft assumed: the whole record log for an
  anchor lives behind exactly *one* moving ref, and each published
  version behind exactly *one* permanent ref — ref count no longer
  scales with fact count at all (§9).
- **Decentralization without ambiguity.** The system must degrade
  gracefully when any single host — including the project's own
  canonical source — is unreachable, without ever becoming ambiguous
  about which source is authoritative when it *is* reachable.
- **Existing formal ground truth.** `docs/models/storage-model.md`,
  `docs/models/execution-model.md`, and `docs/models/composition-model.md`
  already define a substrate trichotomy (storage / composition /
  execution) this atom redesign must compose with, not duplicate or
  contradict.

## Decision

### 1. Atom is the compositional unit of a version integrity system [atom-is-compositional-unit]

An atom composes at three layers, and this is the definition the rest of
this document elaborates, not a metaphor:

1. **The record closure** — an anchor's charter, a label's claim, and a
   specific version's publish record, verified together as the fixed,
   immutable authorization bundle for that version, plus its content.
   The closest equivalent to a Nix derivation: a pure build description,
   but fused with identity and ownership rather than trusting them
   separately. Maps to the build closure. **The closure does not
   include facts** — facts are ongoing, layered status information
   (yank, deprecation, advisories) that can accrue indefinitely after
   publish; baking them into a fixed bundle would make that bundle go
   stale the instant a new fact lands. This corrects an imprecision in
   the original derivation, which described the closure as including
   "bearing facts" — the two are genuinely different things and are
   verified by different mechanisms (§7, §9).
2. **The verified content tree** — once the record closure verifies,
   subjects materialize (composed via the composition model's `⊕`
   disjointness law where more than one subject is declared), each
   identified unambiguously by its own `content_hash` (that subject's
   content-tree digest, §5) regardless of which physical object store
   holds the bytes.
3. **The built artifact / runtime closure** — what HTC's execution layer
   composes from the verified tree, per the record closure's
   description. Layer 3 itself has (at least) three representations,
   already formally defined by
   [Composition Model §4](../models/composition-model.md#4-interfaces-the-typing-of-compositions):
   **package** (an atom's output tree + interface manifest, typed but
   not linked), **environment** (a composition + coherence certificate,
   a linked module), **system** (a declared family of disjoint scopes
   with boundary coherence — not a bigger environment, a different
   artifact).
   [Execution Model §9.10](../models/execution-model.md) already
   ratified (2026-07-12, independently of this exploration) that every
   stratum MAY publish uniformly as a signed atom — this document's
   derivation reached the identical conclusion by a wholly separate
   route (a strong convergence signal, not a coincidence to paper over).

A record is the quantum — an individual signed statement, complete and
meaningful on its own; an atom is the composed, stable unit those quanta
constitute across the three layers. The name predates this understanding
and fits it exactly in retrospect.

### 2. The generic statement entity [atom-record-envelope]

Every atom transaction is a [Coz](https://github.com/Cyphrme/Coz) message
— the record envelope is Coz's own, not something this document invents,
and this clause states precisely how atom's fields map onto it (grounded
against the reference implementation's README and Go structs, not
assumed from memory). Getting this placement right matters: Coz signs
`pay` and only `pay` — every field this model needs to be trustworthy
must live *inside* the payload, and conflating that with the envelope's
outer, unsigned bookkeeping fields would silently unsign exactly the
data this whole model depends on being signed.

Coz's outer envelope (mostly recalculatable transport bookkeeping, not
itself signed):

```
pay   -- the signed payload (everything below lives here)
sig   -- signature over cad; OUTSIDE pay, not itself an input to the
         signature
can   -- (recalculatable) ordered field list used to canonicalize pay
cad   -- (recalculatable) canon digest of pay
czd   -- (recalculatable) = digest(cad, sig) — the record's own identity/
         content-address; incorporates the signature, not only the
         payload bytes, so a different signature over identical payload
         content yields a different czd
key   -- (optional) embedded signer public key; normally looked up
         out-of-band via pay.tmb instead
```

Coz's `pay` already provides the fields this model needs — no separate
top-level `kind`/`signer` fields exist or are needed, because Coz's own
`typ` and `tmb` are exactly those, already inside the signed payload:

```
pay.typ    -- "charter" | "claim" | "publish" | "fact" | "head" — Coz's
              own free-form type-discriminator field, used here as kind
              ("head" is a pay.typ value but structurally distinct from
              the other four: not chained, not a log leaf — §3, §11)
pay.tmb    -- thumbprint of the signer's key — Coz's own field, resolved
              against the charter's active owner set
pay.alg, pay.now, pay.rvk -- Coz core fields (algorithm, signing
              timestamp, key-revocation), unchanged, not elaborated here
```

Coz core has **no native chaining or prior-reference concept** —
`prior` and the governance-pointer field below are axios-specific
extension fields inside `pay`, not part of Coz itself:

```
pay.prior          -- signed czd of the predecessor leaf in this
                       anchor's record log (absent only at the log's
                       very first leaf — the anchor's charter genesis)
pay.under           -- signed czd of the enclosing scope's most recent
                       leaf at signing time (§4). Absent on charter-
                       scoped leaves (nothing encloses the anchor).
                       Generalizes what the original draft called
                       under_charter/under_claim into one field name —
                       the same relation, at whichever scope boundary
                       applies, rather than a different field per kind.
pay.retracts        -- signed czd, optional. Names an earlier fact this
                       one cancels or supersedes. Never used on charter/
                       claim/publish leaves (those are never retracted,
                       only superseded in effect by later facts).
pay.fact_type       -- present only when pay.typ = "fact". Drives per-
                       type authorization dispatch (§3) and what the
                       fact actually asserts — everything from
                       owner-set rotation and label transfer through
                       yank, deprecation, advisories, and third-party
                       attestations. One unified fact mechanism serves
                       every scope; fact_type is what varies, not the
                       record shape.
pay.attests         -- signed czd, present only when pay.fact_type =
                       "co-sign". Names the specific earlier record this
                       leaf co-signs — never a new record kind, never a
                       new aggregation object, just one more fact_type on
                       the existing mechanism (§3's threshold model).
```

**Detachment means no ancestry, not no links.** Every leaf carries
`pay.prior`, chaining it to the immediately preceding leaf in the *same*
anchor's record log — the whole log is one linear signed chain,
regardless of which scope (anchor, label, or version) any given leaf
happens to be about. `pay.under` is the separate, scope-boundary-
crossing pointer: which specific enclosing-scope leaf was authoritative
at signing time. Every leaf therefore sits in a Merkle structure twice
over — once via `prior` (its position in the anchor's total order) and
once via `under` (its authorization context) — both signed, both
independently walkable, neither expressed through git ancestry (§7).

Extension metadata: any record MAY carry an `ext` map of namespaced
key/value entries, each tagged `critical` or `informational`. A
verifier that does not recognize a `critical` entry renders UNKNOWN on
whatever it could affect; an unrecognized `informational` entry is
inert. Bounded by whatever admission-time record size cap a conforming
implementation enforces — an implementation limit, not elaborated
further in this ADR.

### 3. One record shape: genesis-once, facts-forever [atom-genesis-and-facts]

Five `pay.typ` values, unchanged in name from the original derivation,
substantially restructured in meaning: `charter`, `claim`, `publish`,
`fact`, `head`.

**Charter, claim, and publish are each a permanent, single genesis
record for their scope — never reissued, never superseded by a record
of the same kind.** This corrects the original derivation's charter-
succession model (a chain of charter records, each replacing the last),
which turned out to violate a standing invariant: the anchor *is* the
first and only charter, full stop; it is the repository's standing
identity, and the only way to get a new one is a genuine fork. The same
discipline now applies uniformly to claim (a label's identity) and
publish (a version's identity) — each is written exactly once, at the
moment its scope first comes into existence, and never again.

**Everything that happens to a scope after its genesis is a `fact`.**
Owner-set rotation, threshold changes, and governance-parameter
amendments are facts on the *anchor's* leaves (`under` absent, since
nothing encloses the anchor; authorization is fold-based against the
anchor's own leaf history — see below). Label transfer and per-label
governance amendments are facts on the *label's* leaves (`under` points
at the specific charter-scope leaf active at signing time). Yank,
deprecation, advisories, and attestations are facts on the *version's*
leaves (`under` points at the specific claim-scope leaf active at
signing time). One mechanism, one record shape, at every scope — this is
the actual "same fact scheme everywhere" the original exploration was
reaching for and did not yet land on.

Nothing is ever deleted. "Removing" a fact means signing a new one that
`retracts` it — the same succession-by-addition discipline this document
uses everywhere.

**Authorization is fold-based, and it is deliberately not uniform across
scopes — this is a real distinction to preserve, not something to
paper over.** A fact extending the anchor's own leaf history is
authorized by the *current fold* of that same history (a self-
referential governance-token pattern: whoever the anchor's own leaf
history currently says is an owner may sign the next anchor-scope fact).
A fact extending a label's leaf history is authorized by the current
fold of the *label's own* history, cross-checked that the signer is
currently in the *claiming anchor's* owner-fold. A fact targeting a
version is authorized by the current fold of the *label's* history —
"not necessarily the original publisher," the same rule the original
derivation already stated for facts, now understood as the general
rule rather than a version-specific special case. Owner-authoritative
`fact_type`s require the signer to be in the relevant current fold;
third-party `fact_type`s (advisories, attestations) require the signer
to hold a role the charter has explicitly declared (an `attesters` or
`witnesses` set, amended the same way mirrors are, §18) — never an
unbounded acceptance of any signature.

The identical rule governs each scope's own genesis act, not only the
facts layered afterward: a new claim is authorized by the *claiming
anchor's* current owner-fold — the same cross-check label-scope facts
already use — and a new publish is authorized by the *label's* current
fold — the same fold version-scope facts already use. Genesis and fact
are the same authorization question at the same scope; only whether the
record starts or extends that scope's history differs, and only that
difference is what genesis-once (§10) exists to resolve.

**Multi-signature governance thresholds are a `fact_type`, not a new
mechanism.** The charter MAY declare a signing threshold for
owner-authoritative actions — say, requiring 2 of 3 current owners to
agree before an owner-rotation fact takes effect. Coz has no native
multisig primitive, and none is needed: every additional required
signature is just another record. A threshold-gated fact is signed once
by its primary signer, exactly like any other fact; each additional
required signer produces their own, independently signed `co-sign` fact
(`pay.fact_type = "co-sign"`, `pay.attests: <czd>` naming the primary
record's own czd, §2). A co-sign fact is not a scope of its own — it
carries the identical `pay.under` its target would (absent for an
anchor-scope target, pinned to the enclosing scope's active leaf for a
label- or version-scope target), and is authorized by the same
fold-check the target itself uses. Verification counts distinct
current-owner-fold members among the primary signer plus everyone who
has co-signed that czd; the action takes effect only once that count
meets the charter's declared threshold. A record short of threshold is
not invalid or malformed — it simply is not yet effective, the same
distinction the closure/fact split (§1) already draws between a
record's existence and its current standing.

**Crossing threshold is a one-way ratchet, not a live count.** Once a
primary record has, at any point while walking the log forward,
accumulated threshold-many distinct valid co-signers, it is permanently
effective — a later retraction of one of those co-sign facts does not
undo an action that already took effect. Retraction only has power
before threshold is crossed: it removes a co-signer's contribution from
the running count, which may delay or prevent a not-yet-effective
action from ever reaching threshold. Nothing here is rolled back by a
later signature, only ratcheted forward — the same one-way, append-only
discipline this document uses everywhere else (caught by adversarial
review, 2026-07-16).

No new record kind, no aggregation object: threshold enforcement is one
more `fact_type` on the mechanism everything else here already uses.

**An owner-fold MAY also declare a lower-privilege `publish-delegate`
role**, the same declared-role discipline `attesters`/`witnesses`/
`freshness-delegate` already use, authorizing a distinct key to sign
routine `publish` records without holding full owner-governance
authority. Optional, not required: a project with no declared
publish-delegate simply authorizes publish through the ordinary
owner-fold, exactly as before. Declaring one is the same trade the
freshness-delegate role already makes for head rotation (§11) — routine,
frequent signing shouldn't need to expose the same key that governs
owner rotation, label transfer, and threshold membership.

**`head` is not part of this genesis/fact taxonomy — the fifth `pay.typ`
value, but structurally distinct from the other four, and neither a
genesis scope nor a `fact_type`.** Charter, claim, and publish are genesis
records; fact is the one amendment mechanism layered on top of them; head
is neither. It carries no `pay.prior`, is
never chained, and is not a leaf in the anchor's record log at all —
see §11-§12 for what it actually is (a replace-semantics, TTL-bounded
freshness heartbeat) and who may sign it.

### 4. Identity fields disentangled: anchor vs. position [atom-anchor-position]

Two previously-conflated concepts, now separated — and one of them is
deliberately *not* a field:

- **Anchor** — which project this leaf belongs to: the scope the
  anchor's one charter genesis leaf establishes, *referenced* by that
  leaf's czd. The czd names the anchor — it is not the anchor itself,
  which is the linking role the charter fills (see `docs/glossary.md`).
  Not a declared field anywhere — not even on the charter genesis leaf
  itself, which cannot self-reference the value in question. Discovered,
  never carried: walk `pay.prior` back to the log's first leaf, which MUST
  be a `charter` genesis — a log whose first leaf is anything else is
  invalid and names no anchor. This costs nothing extra in practice, because
  the whole record log is fetched as one unit (§9) and the genesis leaf
  is always present in it. A root commit is often near-empty and carries
  no ownership claim; the charter's own hash is high-entropy by
  construction and the ownership claim *is* the identity. **The same
  discipline generalizes to every scope, not just anchor:** a label's
  real identity, once claimed, is the czd of its own claim record; a
  version's real identity is the czd of its own publish record. A label
  string is a lookup convenience a consumer resolves *through* — within
  an anchor's record log, the claim is found by label, made unambiguous
  by genesis-once (§10) — never the identity being resolved *to*. **Not
  redundant terminology, even though the charter's czd is the value that
  numerically names the anchor (the repudiated `anchor = czd(charter)`
  equation reified exactly this coincidence; see `docs/glossary.md`):**
  `charter`,
  `claim`, and `publish` each
  name a specific signed *record*; `anchor`, and a label's or version's
  real identity, name the *scope of ongoing authority* that record
  establishes — an anchor's record log, an anchor-scope fact, an
  anchor's owner-fold are properties of that evolving scope, never of
  the immutable founding document itself. Same relationship, same
  reason, at every level (clarified 2026-07-16).
- **`position`** — where in the project's own git history a record
  (charter, claim, or publish) was issued. A commit OID. Governs the
  named invariant `[temporal-vector]` (an invariant name, not a
  section-anchor tag — like `[verify-before-fetch]` and
  `[k-derivation-signed-only]`, and unlike the section-heading brackets):
  `charter.position ≤ claim.position ≤ publish.position`, checked by
  ordinary git ancestry (a verifier obligation, §10; §8's carve-out).
  Facts carry no
  `position` of their own — they are about an already-published subject,
  not a new one.

### 5. Subject shape [atom-subject-shape]

A subject is `{path, content_hash, tree}` — **not** its own `position`.
Every subject in a publish is verified against that publish's own
top-level `position` field (§4) — there is exactly one position per
publish, shared by every subject it declares (§6), never a second,
independently-declared position per subject.

- **`path`, `content_hash`** — verification and placement.
  `position.tree/path` is a deterministic function of two immutable
  signed fields (the publish's own `position`, and this subject's
  `path`), checkable in O(path-depth) by anyone holding `position`
  (exactly `git rev-parse {position}:{path}`); `content_hash` serves
  verifiers without upstream access. `path` is authoritative for both
  verification and placement — declared, never searched. **`content_hash`
  is a Merkle root computed over the subject's *loaded content* — its file
  bytes and their in-tree structure (paths, modes) — by a supported,
  pluggable hash algorithm (`eml`'s k-ary construction, §7.2, as the
  default), and is *explicitly independent of git's own object/tree
  hashing.*** The subject's git tree is loaded and its contents are
  re-Merkleized under an algorithm *we* choose to support; git is pure
  storage here, never the hasher, and git's native tree OID is never reused
  as this value — that OID is a separately-kept, checked cache (the `tree`
  field below), never the integrity root. It is not a flat hash of the
  bytes either; a single-file subject degenerates to the k-ary root at
  `n = 1`, literally the leaf's own content digest, no internal node
  computed. **Rationale:** re-Merkleizing loaded content under our own
  algorithm decouples content integrity from any particular storage
  backend and its hash choice, and is what lets `content_hash` map cleanly
  onto the composition model's content-addressed values — a
  git-tree-OID-derived hash would instead bind the model to one backend's
  hash function and its versioning.

  **The closure-level content commitment is a distinct object one level up,
  named separately so the two are never conflated: `content_commitment`
  (leaf0 of the §7.4 closure tree) is an `eml` root over the *ordered list
  of declared subjects*, leaf `i` = subject `i`'s own `content_hash`.** It
  is never any single subject's `content_hash` — the two coincide only in
  the common single-subject case, where this outer `eml` root at `n = 1` is
  again literally that one subject's `content_hash`, so the closure costs
  exactly what a flat hash would have. No new signed field:
  `content_commitment` is *derived* from the existing subject list, the same
  "derive, don't declare, when derivable" discipline the anchor-identity
  rule (§4) already uses. The multi-subject case (§6) is where the extra
  structure earns its keep:
  per-subject inclusion proofs become available for free, letting a
  consumer verify one declared subject's inclusion without touching any
  other subject's content — a real capability, not merely a tidier data
  model, and one a flat hash could never offer regardless of how it was
  computed.
- **`tree`** — the git tree OID that `position.tree/path` resolves to,
  stored as a checked cache, not independent information: it MUST equal
  the result of that lookup, and a mismatch is a verification failure
  exactly like any other false signed claim. This is the same object
  that becomes the closure's own content tree (§7) — the same OID,
  referenced a second time, at zero additional storage cost.

**Hash agility and the SHA-1 residual (scoping note, not a normative
MUST).** Because `content_hash` is our own Merkle root over loaded content
(above), **content integrity is fully decoupled from git's object hash**:
a SHA-1 collision in the underlying repository cannot forge a subject's
content, since the served bytes are checked against a signed hash computed
by an algorithm independent of git's. The catastrophic case — serving
different bytes under the same identity — is off the table by
construction, and this is a primary motivation for the Merkle-root
`content_hash`.

The residual SHA-1 exposure is confined to the `position` OIDs (§4) — the
pins into the *upstream* project's git history, on repositories that are
still SHA-1. SHA-1 chosen-prefix collisions are practical ("SHA-1 is a
Shambles," 2019), so the realistic attack is a prepared collision: sign a
benign commit `B`, later substitute a malicious `M` sharing its OID. But
since `content_hash` is independently signed over the actual content, `M`
can differ from `B` only in commit *metadata* — parent links/ancestry,
timestamp, message. The blast radius is therefore temporal / ancestry /
provenance claims (the `[temporal-vector]` and legacy/native ordering,
§8), never content.

Mitigants, in order of strength: git's hardened SHA-1 (`sha1dc`,
collision-detecting) raises the bar in practice; the ancestry check's
failure mode is already bounded and fail-closed (§8's carve-out); and a
SHA-256 repository (git ≥ 2.29) has *zero* residual here. Security-critical
deployments should prefer SHA-256 upstreams where available. Fully
decoupling the *history pin* as well (not just content) would mean
re-Merkleizing the entire upstream history, which is impractical — so for
SHA-1 repositories the provenance residual is inherent, scoped rather than
eliminated.

*Possible future hardening, deliberately not adopted:* additionally
pinning each `position` by a strong hash of its commit object
(`H_strong(commit)`) would close endpoint substitution — an attacker with
a SHA-1 OID collision could no longer swap the pinned commit for one with
forged metadata, shrinking the residual to the already-bounded, soft
between-pins ancestry path. It is not adopted because the git OID must stay
in `position` for native lookup regardless, so the strong hash is purely
additive (sign and store both) — extra complexity for a residual that is
already bounded and non-load-bearing, since the upstream ancestry is soft
corroboration and the anchor's own record log gives the authoritative
ordering. Recorded as an option the design leaves open, not a requirement;
the normative `position` definition (§4) is unchanged.

### 6. Multi-tree subjects, scoped and bounded to a sibling neighborhood [atom-multi-tree]

A publish MAY declare more than one subject tree — every subject verified
against the same single `position` field the publish record itself
carries (§5; same-repository only). Justified by two independent, real
cases: a native atom whose logical unit spans multiple directories in
one repo, and load-bearing symlink resolution — a cross-tree symlink is
resolved not by dynamically following it but by declaring its target as
its own independently-verified subject.

**Sibling constraint (2026-07-16 addition):** let `P =
dirname(main_subject.path)`, where the main subject is whichever is
declared first. Every additional declared subject's `path` MUST lie
under `P` — a direct sibling of the main tree, or nested arbitrarily
deep beneath one — and MUST NOT reach above `P`. This is a
**path-segment containment** check, not a raw string-prefix one: normalize
both `P` and each subject `path` first (resolve `.`/`..`, reject any that
escapes the repo root), then require each subject path to equal `P` or lie
under it *at a component boundary* — `P = foo` contains `foo/bar` but not
`foobar`, the string-prefix footgun a naive check leaves open. No new
field is needed, and no separate "primary subject" flag is needed either,
since declaration order settles it.

This closes a real gap the unconstrained version left open: composing
subjects from arbitrary, unrelated repository locations either drags in
enough of the source tree's real skeleton to keep relative references
(imports, symlinks, includes) meaningful, or silently breaks them. The
sibling constraint matches the symlink-resolution motivating case almost
exactly as-is — a symlink using one `../` naturally lands on a sibling of
its own containing directory — while forbidding reach into unrelated,
distant parts of a monorepo. That line is deliberate, not an accidental
limitation: it keeps the protocol's own reasoning bounded to one
relationship (sibling-or-descendant) rather than an open-ended "how far
can a subject reach," and it pushes the cost of needing to reach further
onto a one-time, deliberate maintainer choice — position the atom's
manifest such that what it needs is a sibling — rather than onto every
downstream verifier and composition tool indefinitely. It also makes
materialization a guarantee rather than best-effort: reproduce `P`'s own
layout, pruned to the declared subset, and relative references keep
working because relative positions are preserved exactly.

An undeclared symlink target is inert (nothing outside the union of
declared, verified subjects is ever materialized), not merely blocked by
a check; a materialization-time containment check remains as defense in
depth for the intra-tree escape case only. Composition across multiple
subjects at one position uses the composition model's own `⊕` law
(disjoint union, conflict surfaces as an error, never silently resolved).

### 7. The git object model: an append-only record log, checkpoints, closures, and fact-tags [atom-git-object-model]

This section replaces the original derivation's blob-carrier clause with
the full mechanism, following directly from the second reframing in
Context: every git object below is backend scaffolding, never a trust
boundary. It exists to make retrieval, ordering, and garbage collection
cheap and largely free by reusing git's own machinery, not to add or
substitute for any part of the signed-payload verification story.

**7.1. One append-only Merkle log per anchor.** Every leaf a project
ever signs — its charter genesis, its labels' claim geneses, every
publish, every fact at any scope — is a leaf in one continuous,
append-only Merkle tree, in true chronological signing order. `head`
records are not leaves in this log and are not part of this
enumeration — they are never chained and live at their own separate ref
(§3, §11-§12). Splitting
this into siloed per-label or per-version logs (an earlier position in
this same exploration) turned out to solve a fetch-cost problem the
Merkle structure itself already solves better: inclusion proofs give
O(log n) verification of any single leaf without downloading the whole
log, making physical siloing redundant. The remaining concern — write
contention between independent concurrent fact-writers (an owner
publishing while a third party files an advisory) — is real in
principle but negligible at this domain's actual traffic (single digits
to dozens of records over a project's whole life); a rare CAS retry on
the log's one moving tip is cheap, invisible plumbing, not worth
preserving multi-log complexity to avoid.

**7.2. Shape: append-only, single-root, k-ary — MMR-derived, not a
generic/updatable tree; RFC 6962 was the starting point, not the
mechanism.** Two structural choices, evaluated and not merely assumed:

- **Append-only, never a generic/updatable structure** (sparse tree,
  Patricia trie). The property this system needs is not "tamper-evident
  single snapshot" — any Merkle variant gives that — it is "root-then to
  root-now proves nothing was removed or overwritten, only added," and
  that is only a *structural* guarantee, checkable without trusting a
  policy convention, when the shape makes an in-place edit
  unconstructible. A generic updatable tree can produce a valid
  transition proof for an edit — proving the edit happened, not
  preventing it from mattering. This also already matches this
  document's own `retracts`-not-delete discipline (§3) — the tree shape
  makes that protocol rule structurally inevitable rather than a
  convention layered on top.
- **A single, directly fetchable root — not a raw Merkle Mountain
  Range's bag of peaks.** This design's own discipline is that identity
  always equals a content-address you can fetch; a raw MMR's root is a
  *derived* value (bag the peaks), corresponding to no single fetchable
  object — a real seam against that discipline. RFC 6962 was this
  requirement's original reference point (one canonical tree hash, the
  top tree's own fetchable identity) but is not the mechanism actually
  used here — see the next bullet for what `eml` actually does, which is
  internally MMR-shaped and gets to a single root by a different route.
  Raw MMR's actual advantage (cheap amortized appends at large scale)
  does not engage at this domain's real cardinality regardless — a
  worst-case-every-append cost and an amortized cost are the same
  handful of objects here. At n=1 — the common case, since most
  publishes never accrue a fact — the tree hash degenerates to
  literally the leaf hash, no internal node needed at all: cheapest
  possible case for the common case, by construction.
- **The root is computed by our own algorithm, never git's.** An earlier
  position in this exploration treated git's native tree-hashing as
  providing the Merkle structure directly — a git tree's own OID
  standing in for the root. That is superseded (2026-07-16): the actual
  root computation is [Cyphrme's `eml`](https://github.com/Cyphrme/eml)
  — a formally verified (Lean 4, sorry-free, ≤4 structural axioms),
  canonical, `k`-ary Merkle log, already the production dependency for
  Cyphrme's own work — computed independently of whatever git object
  shape happens to store the underlying bytes. Git is a pure storage
  backend here: whatever layout `eml`'s own storage interface
  (`polydigest::Storage`) asks for, a git-backed implementation of that
  trait persists; no git object's own hash is required to carry any
  meaning (the exact git-side layout is an implementation detail of that
  driver, not specified at the protocol level — Open Items).
  What actually resolves the original MMR-vs-single-root tension (Alt
  8) is `eml`'s own canonicalization — collapse and promotion fold the
  frontier down to one root. This is a materially different mechanism
  from RFC 6962's own recursive definition, not a reformulation of it:
  RFC 6962 was this construction's starting point — specifically for
  wanting one fetchable root rather than a derived bagged value — and
  the actual construction has significantly diverged from it since,
  independent of whether the folded root ever happens to equal any
  external specification's exact recursive definition. An earlier
  version of this clause additionally claimed
  `eml`'s k=2 root was machine-checked equal to RFC 6962 §2.1's MTH
  recursion, citing a theorem (`rfc9162_mth_bridge`) — **that citation
  was wrong and has been removed** (2026-07-16, caught by adversarial
  review): no such theorem exists in `eml`'s current, mainline corpus.
  It lived only on an unmerged branch, and the corpus's own
  documentation explicitly marks that entire proof chain "CT-lineage
  (relegated — reference build, NOT authoritative)." The claim this
  document actually needs never depended on RFC 6962 equivalence in
  the first place — see below.

  `eml` deliberately omits RFC 6962's `0x00`/`0x01` leaf/internal
  domain-separation prefix — a considered position, not an oversight:
  prefixing obscures that a leaf hash represents its underlying data
  directly, and their actual defense against the type-confusion attack
  prefixing exists to block is **positional**, not value-level —
  `eml`'s inclusion verifier reconstructs the exact expected proof shape
  from *trusted* `(index, tree_size, arity)` before inspecting the proof
  at all, and rejects a zero-sibling step (what a promoted internal node
  presented as a leaf would produce). This is backed by machine-checked
  Lean theorems (`kary_inclusion_soundness`, `inclusion_proof_unique`,
  `proofs/lean/EMLProof/Kary.lean`) whose soundness is conditioned on
  two explicit, non-axiomatized hypotheses — `¬NodeHashCollision` and
  `¬CollapseAmbiguity` — the same ordinary collision-resistance
  assumption everything else in this document already rests on, not a
  weaker one — and this is the theorem this document's soundness
  actually rests on, independent of anything RFC-6962-shaped. The
  construction also has no odd-leaf-count padding or duplication step
  of any kind (the classical CVE-2012-2459 attack class), since the
  base-`k` frontier decomposition handles any exact leaf count without
  rounding.

  **Named honestly, not glossed over:** this positional defense is a
  genuinely different evidentiary posture than RFC 6962's byte-prefix.
  A prefix is a value-level guarantee, enforced by construction,
  independent of any verifier's logic being correct. Positional
  reconstruction is a protocol-level guarantee whose soundness depends
  on every verifier correctly threading `(index, tree_size)` through
  the check, forever — backed here by one team's Lean proofs, not by
  RFC 6962's roughly thirteen years of adversarial production exposure
  across Certificate Transparency and its descendants. Not a reason to
  reject the design — the underlying theorem is real and independently
  checked — but a real asymmetry worth stating plainly rather than
  treating the two mechanisms as equivalent for the same reason.

**7.3. Checkpoints: one commit per append, entirely backend, never
trusted.** Each leaf's record content is a blob (`record.json`, the
Coz envelope verbatim), persisted through `eml`'s own `Storage`
interface (`polydigest::Storage` — `store_leaf`/`store_node`/
`write_batch`, atomic per append) via a git-backed implementation of
that trait — never embedded in a commit message, which would risk
porcelain cleanup silently mangling signed bytes on any non-plumbing
write path. The commit wrapping each append has: fixed, inert
message/author/committer fields (any git-native signing slot such as
`gpgsig` is explicitly irrelevant and never checked — only the blob's
Coz signature is, and separately, `eml`'s own computed root is never
taken from a git object's hash, §7.2), and `parent` set to the previous
checkpoint commit — a real git
ancestry edge, but purely for cheap local walkability (`git log`-native
history reconstruction instead of hand-rolled chain-walking code), never
trusted as proof of anything; every hop is cross-checked against the
leaf's own signed `pay.prior` regardless. `refs/atom/log` (or an
equivalent single per-anchor ref) is this chain's one moving tip,
CAS-updated on every append — git's own ref-update compare-and-swap is
the entire write-gate mechanism for the log (§19 has no separate
mechanism to add).

**7.4. The closure artifact: an `eml` tree embedding the content
commitment and the anchor's authorization state, wrapped in a bare
commit.** `refs/atom/pub/{label}/{version}` is a **separate**, permanent
ref, pinned at creation and never rewritten to a *different* history: its
target only ever advances forward along the §7.5 CAS-gated fact-tag chain
(bare closure-commit → `tag_1` → … as facts land), so a consumer MUST
re-check the ref for tip movement on each resolution rather than caching
the first object it resolves. ("Written once, never touched again"
described only the steady state where no fact ever lands, and misread
against §7.5 as a cache license — corrected here.)

An earlier position in this exploration made the closure a bare commit
— `tree` = content only, plus a nonstandard header line recording a
pointer into the record log, hashed but explicitly not load-bearing, a
hint nobody was required to check. **Corrected (2026-07-16):** the
closure is a **2-leaf `eml` tree**:

```
closure-tree (eml, k=2)
  leaf0 = content_commitment    -- §5: an eml root over the declared
                                    subjects (leaf i = subject i's own
                                    content_hash), never a flat hash
  leaf1 = R_anchor@K             -- embedded, opaque: the anchor record
                                    log's own root at leaf-count K, the
                                    exact moment this closure was created
```

using `eml`'s own opaque-leaf-embedding property directly: any tree's
root can be placed as raw leaf data in another tree, with no
special-casing and no composite proof type — composition is two
independent inclusion/root checks. Both leaves are digest-sized values,
never the actual content; the real subject files remain a wholly
separate git tree — for a native atom, the *same* tree OID already
resolved from `position:path` (§5), referenced a second time at zero
additional storage cost; for a legacy atom, identical, since legacy
only changes whether the manifest/lock *declaration* is embedded in the
payload or read in-tree (§14), and has nothing to do with content.
Dependencies are never embedded either way; they are resolved and
fetched separately at build time. Fetched only in Phase B (§20),
completely unaffected by this change — embedding lives entirely inside
Phase A's already-small, always-fetched territory.

Verification is genuinely two independent checks — and, correcting the
2026-07-16 draft, **neither compares the closure tree's root to any
signed field, because there is none: the closure artifact is deliberately
unsigned (below).** The 2-leaf tree carries no signature to match against,
so a verifier instead *reconstructs each leaf from independently trusted,
signed data* and confirms the closure's presented leaf equals that
reconstruction:

- **leaf0** — recompute `content_commitment` per §5 by folding the
  publish record's own *signed* declared `content_hash`es (so leaf0 is
  checkable from signed data in Phase A, before any content is fetched;
  the bytes are separately re-derived against those same signed
  `content_hash`es at Phase B, §20) and confirm the fold equals the
  closure tree's leaf0. Content trust flows from that signed-field
  recomputation, never from the leaf the untrusted closure presents — the earlier "confirm the root matches what was signed"
  phrasing let a literal implementation trust `leaf0`/content straight off
  the closure, so a mirror could swap content plus a matching `leaf0`,
  keep the honest `leaf1`, and pass (B1, adversarial review 2026-07-22).
- **leaf1** — recompute `R_anchor@K`, the anchor log's real historical
  root at leaf-count K, from the verifier's *own* `pay.prior` walk (§7.1;
  §10 inclusion) and confirm it equals the closure tree's leaf1.

**K is a signed historical index, never a git-structural quantity
[k-derivation-signed-only].** K is *the count of signed `pay.prior` hops*
from this publish's own leaf back to the anchor's charter genesis, the
same signed chain §4 walks to discover an anchor: genesis is leaf 1, the
publish leaf is the K-th, and `R_anchor@K` is the log root as of that
publish leaf (the off-by-one pinned here rather than left to a reader).
K is **never** the count of git checkpoint-commit ancestors (§7.3) and
**never** the log's write-time tip — both are attacker-influenceable
backend structure a malicious host could fabricate to pair a real,
honestly-computed `R_anchor` at the wrong K with a genuine content
commitment and otherwise pass (B2). Binding K to the verifier's own
`pay.prior` count, and reconstructing both leaves from signed data, is
what closes that pairing attack. A forged or stale reference is a hard
verification failure, not a cosmetic mismatch nobody was required to
check.

This recomputation stays cheap and stable as the log grows past K —
`eml`'s durable witnesses mean a leaf's inclusion path to its mountain
peak never changes once written — **provided** the git-backed storage
driver keeps every historical node structurally reachable (§21's GC
discipline must never prune a node this recomputation still needs; a
real, stated obligation on that not-yet-built driver, not an automatic
consequence of depending on `eml`).

The wrapping commit's `parent` is still deliberately **none** — no true
git-structural edge to the record log, for the same reason as always
(git ancestry carries no security weight anywhere in this design, §7.2)
— but the closure no longer *needs* a git-level pointer at all,
structural or otherwise, to establish real provenance: the embedded
leaf does that job cryptographically.

Keeping the closure separate from the log this way is what makes §1's
closure/facts distinction real rather than aspirational: the closure
never needs to duplicate any record content, since the whole authorizing
chain (charter, claim, publish) is independently resolvable from the
already-fetched record log by tuple lookup; the closure's own tree only
ever holds the two things genuinely specific to this publish — content,
and a provable snapshot of the authorization state that produced it.

**7.5. Fact discovery: a chain of tag objects anchored at the same
ref.** `refs/atom/pub/{label}/{version}` starts out pointing directly at
the bare closure-commit — the steady state, since most publishes never
accrue a fact. The moment the first fact lands, that same ref is
CAS-updated to point at a new **tag** object whose `object` field is the
closure-commit; the second fact creates another tag whose `object` field
is the *first* fact-tag; and so on — `ref → tag_N → ... → tag_1 →
closure-commit`. Each tag's message body carries a nonstandard, non-
`parent` pointer to the actual signed leaf in the record log this fact
corresponds to — the same git-header technique the closure itself used
before §7.4's embedding upgrade, deliberately retained here rather than
also upgraded: embedding solves a different problem (provable cross-
reference) than fact discovery needs (cheap existence signaling), and
making an *existing* tag more rigorously self-verifying does nothing to
catch a tag that was never created — the actual threat here. The
completeness delta-scan below is what closes that gap, not embedding.

The object *type* at the ref's tip does the discovery work for free: a
bare commit means nothing has ever happened, zero further lookups
needed; a tag means something happened, and a consumer is structurally
forced to see that — there is no way to peel past a tag layer without
acknowledging it exists. Git's native peeling (`ref^{commit}`)
recursively dereferences every tag layer straight to the closure-commit
regardless of chain depth, for the common "I just want the content" case
— but it deliberately bypasses the fact chain, so a consumer that needs
current trust status MUST walk the tags (and run the §7.5 completeness
scan), never substitute the peel for that. This yields exactly one
mutable ref per version, ever — not a ref per fact plus a separate ref
per version, which an earlier position in this exploration proposed and
was wrong to.

**Not load-bearing for security — a collection accelerator only.** The
tag chain's own object-graph order is never trusted as the real ordering
or as proof of validity; both come from independently verifying each
pointed-to record's own signed `prior` chain in the log. If a malicious
or buggy mirror serves an incomplete, reordered, or forged tag chain, the
worst outcome is a consumer misses a lookup hint or wastes one on
something that doesn't verify — never that a false fact gets accepted,
since every record still independently clears signature and chain
verification regardless of what a tag claimed.

**Closing the completeness gap, unconditionally.** The tag chain alone
cannot catch a lie by omission — a mirror simply not appending a new
fact-tag even though the record legitimately exists in the log. The
fix: after walking the tag chain to its last known record-log pointer
(leaf K), a consumer performs a bounded scan of the record log from leaf
K+1 to the log's current tip, checking whether anything in that delta
targets this (label, version). This is cheap because the delta is
typically zero (nothing happened since last check) or small (bounded by
the whole log's own small size at this domain's cardinality) — never a
scan from genesis. It composes for free with the two-phase store-
ingestion protocol (§20): a store that has already fetched the whole
anchor's record history before ingesting has this delta sitting in hand
already, as a local computation; a lightweight single-atom consumer pays
one small additional delta-fetch. See §16 for why this makes
completeness — as distinct from recency — unconditional across every
degradation tier.

### 8. The ref-leaf discriminator principle [atom-ref-leaf-principle]

**Governing law, applying to every ref-format decision in this
document:** a ref's leaf discriminator is chosen purely for
query/UX usefulness, per family, and never bears on security — security
rests entirely and always on the signed payload (signature + `prior`/
`under` chains), checked regardless of what a ref happens to be named or
what git object it happens to resolve to. The self-certifying property
of a czd-named ref (recompute the hash from fetched bytes, compare to
the name) is a cheap bonus sanity check, never the load-bearing
mechanism. This principle now also governs the choice of git object
*type* at a ref's tip (§7.5's bare-commit-vs-tag discriminator) — the
same rule, one level more general than the original draft stated it.

**The one carve-out, stated rather than glossed:** this document's broader
"git structure carries zero security weight" (Context, §7.2) has exactly
one exception, and naming it is more honest than an absolute that is false
as written. The `[temporal-vector]` invariant (`charter.position ≤
claim.position ≤ publish.position`, §4) and the legacy/native gate
(`is-ancestor(charter.position, publish.position)`, §14) both *do* consult
git ancestry to reach a verification decision — the one git-structural
relation anywhere in the design that does. It is safe, by an argument the
rest of the document's blanket phrasing skips: the OIDs being compared are
themselves *signed* `position` fields, so an attacker cannot substitute a
different commit without breaking the signature; forging a false ancestry
edge between two fixed content-addressed OIDs is a preimage/collision break
of the same class every other guarantee here already assumes away
(collision-bound in the forgeable direction — strongly so under SHA-256 or
`sha1dc`-hardened SHA-1, and reduced but not eliminated under raw SHA-1,
whose chosen-prefix collisions are practical; even there the exposure is
confined to this ordering/provenance claim and never to content, per §5's
hash-agility note); and the sole remaining residue — a host withholding the
intervening commit objects so the ancestry query cannot complete — **fails
closed** (the record is treated as unverifiable and rejected), a
denial-of-service, never the acceptance of a false ordering. So the principle is precise: git *object identity and type*
never bear on security; git *ancestry between signed `position` OIDs* bears
on exactly one ordering check, fail-closed, and nowhere else.

### 9. Ref format: one log, one ref per publish [atom-ref-format]

Applying §8's principle now that the record shape and git object model
have both changed substantially from the original draft:

```
refs/atom/log                          -> the anchor's record-log tip
                                           (§7.3, one moving ref, CAS-updated
                                           on every append, walked backward
                                           via checkpoint-commit parents)

refs/atom/pub/{label}/{version}        -> the closure commit, or — once
                                           any fact exists for this
                                           version — the tip of its
                                           fact-tag chain (§7.4, §7.5).
                                           One permanent ref per ever-
                                           published version.

refs/atom/head                         -> the freshness heartbeat
                                           (§3, §11-§12), replace-
                                           semantics, one per registry.
                                           Tier-2-only: exists
                                           operationally only for
                                           projects that opt into
                                           recency guarantees; a Tier
                                           0/1 registry has no ref
                                           here at all.
```

That is the entire registry-side ref surface — two ref families that
always exist (the log, and one per published version), plus one
optional ref (head) that exists only for projects opted into Tier 2.
Cardinality no longer scales with fact count at all — every fact, at
every scope, lives inside the one record log, discovered via inclusion
proofs and (for published versions specifically) the fact-tag-chain
fast path, never via a dedicated ref of its own. This is a strict
reduction from the original draft's charter/claims/facts ref families,
each with its own dedicated "tip" ref for write serialization (§19) —
that whole write-gate apparatus collapses into "one branch, one CAS,"
since a git ref update already *is* a compare-and-swap.

### 10. Verification: inclusion and consistency proofs, plus the completeness scan [atom-verification]

Verifying any leaf's authorization no longer means enumerating a
ref-prefixed family and reassembling a chain in application code (the
original draft's two-request scheme). Instead:

- **Inclusion**: given a leaf's claimed position in the record log, an
  O(log n) inclusion proof (sibling hashes along `eml`'s tree path, §7.2)
  confirms the leaf is genuinely part of the log at that position,
  without fetching the whole log.
- **Consistency**: given two states of the log a consumer has seen at
  different times, an O(log n) consistency proof confirms the later
  state is a genuine append-only extension of the earlier one — nothing
  was removed or rewritten. This is the direct mechanism behind TOFU
  monotonicity (§16) and needs no separate apparatus to provide it.
- **Authorization**: resolving `pay.under` and folding the relevant
  scope's history (§3) — unchanged in substance from the original draft,
  now walked against one unified log instead of several ref-enumerated
  families.
- **Position ordering** (`[temporal-vector]`, §4): confirm
  `charter.position ≤ claim.position ≤ publish.position` by git ancestry
  over the signed `position` OIDs, and, for a legacy publish, the §14
  legacy/native gate. Named here as an actual verifier obligation rather
  than left implicit in §4: it is the one git-structural relation
  verification consults, safe and fail-closed for the reasons §8's
  carve-out states — a withheld intervening object makes the ordering
  unverifiable and the record is rejected, never accepted on an
  unprovable order.
- **Genesis-once**: for any scope whose identity is a permanent genesis
  (charter, claim, publish, §3), the *canonical* genesis is the first
  such leaf encountered in the anchor's `prior`-chain order from anchor
  genesis forward. Any later leaf declaring the same genesis scope — a
  second `claim` for an already-claimed label, a second `publish` for an
  already-published (label, version) — is void and MUST be rejected by
  every conforming verifier, independent of any registry's own ref
  state. This verification rule is a fail-safe, not the primary
  discipline: an honest client is expected to check its own
  already-fetched log and simply never construct a second genesis for
  a scope in the first place, the same way it never constructs an
  invalid signature. The fail-safe exists for when that discipline
  doesn't hold — a buggy client, a dishonest one, or any writer whose
  key has been compromised — and the write gate (§19) additionally
  helps prevent it cheaply at one registry in the honest case; but
  this verification rule is what makes genesis-once hold *within any
  single, shared view of the log* — a second genesis is void wherever the
  first is also visible, and ref-existence CAS at one host is correctly
  not part of the trust boundary. What it does **not** do, and must not
  claim to, is statically resolve a *fork*: an equivocating source can
  show two consumers two different "first" genesis leaves for the same
  scope, and no offline check against one view detects the other. That
  residue is the same equivocation residue §16 and §17 disclose —
  settled not by this rule but by trusting the charter's signed
  canonical-source declaration (optionally a threshold quorum of its
  declared mirrors/witnesses), and bounded, never fully closed, against a
  fully-targeted victim partition (§17-L4). (Re-scoped 2026-07-22; the
  earlier "holds under adversarial conditions regardless" overclaimed
  static fork-resolution.)
- **Completeness** (§7.5): the bounded delta-scan from a fact-tag
  chain's last known point to the log's current tip.

In practice a consumer or store typically fetches the whole record log
for an anchor at once — it is small at this domain's cardinality — making
individual inclusion proofs an optimization for lightweight, single-leaf
lookups rather than the only path; both are valid and give the same
answer. That lightweight path specifically — naming only the sibling
objects one proof needs, without fetching the whole log — depends on
protocol-v2's want-list-based object negotiation; a legacy dumb-HTTP
remote cannot do this and falls back to fetching more than strictly
necessary (Open Items).

### 11. Head detects omission of freshness, not completeness [atom-head-purpose]

Head's function is exposing an incomplete or stale *view of recency* —
fetch-and-fold of a stale mirror's complete, internally-consistent chain
can never prove the *current* state is what it claims to be as of right
now; head is the only mechanism that can, because it is a signed,
TTL-bounded, periodically-rotated heartbeat whose sole content is "as of
time T, here's current." Only meaningful at Tier 2 of the degradation
ladder (§16) — Tiers 0-1 carry no freshness claim in force, so head does
not exist operationally for those projects. **This is now a narrower
claim than the original draft made** — see §16 for why completeness
(did I miss anything my already-fetched view claims to cover) is a
separate property from recency (how current is my view), and the former
no longer depends on head at all.

**Head is not a leaf in the anchor's record log, and it is not chained
(§3, §7.1).** Where every charter/claim/publish/fact leaf carries
`pay.prior` and accumulates permanently, a head record simply *replaces*
the previous one at its own separate ref (§9) — there is nothing to
preserve about a superseded head once a fresher one exists, since only
the current heartbeat is ever meaningful. This is a structural
consequence of what head is for: a point-in-time freshness claim, not a
historical fact about anything.

**Head is signed by a declared freshness-delegate role, distinct from
full owner-set custody.** Rotation happens on a routine, frequent
operational cadence (whatever TTL a project chooses) — requiring the
same high-security owner keys used for governance-critical acts (owner
rotation, label claims, mirror changes) on every rotation would either
force those keys into an automatable, less-protected operational
surface, or make Tier 2 impractical to actually run. Instead, the
charter declares a lower-privilege `freshness-delegate` role — the same
declared-role discipline `fact_type` authorization already uses for
`attesters`/`witnesses` (§3), applied here to a signing role rather than
a fact type — and only a key holding that role may sign a head record.
This is precisely why §18 requires mirror-set amendments to go through
full owner-set authority rather than the freshness-delegate key: mirror
steering is trust-adjacent in a way routine freshness rotation is not,
and the two must not share a key.

**Head rejection is a stated invariant, not an aspiration.** A
freshness-delegate key is, by its own design rationale above, a
lower-privilege, more-exposed key than full owner custody — meaning a
head-specific compromise is the more likely failure mode, not an
exotic one. Without an explicit freshness ordering, a compromised
delegate key or a coerced mirror could replay an older, validly-signed,
still-technically-within-its-original-TTL head record to hide a
recently-landed yank, deprecation, or owner-rotation fact from a Tier-2
consumer — defeating the one property Tier 2 exists to provide. This
is promoted here from a background formal-verification item (Open
Items previously listed "head monotonicity" as future Lean work) to a
protocol rule every conforming verifier MUST already enforce: **reject
any head record whose freshness ordinal (timestamp or explicit counter)
is not strictly greater than the last head this consumer has itself
observed for this registry.** This does not require a proof to hold
today — it requires every Tier-2 implementation to check it before Tier
2 is meaningfully deployed at all (caught by adversarial review,
2026-07-16).

**The global ordinal is necessary but not sufficient — a second,
per-entry MUST closes a per-label rollback (added 2026-07-22).** Because a
head bundles many per-label entries (§12) under one ordinal, the
strictly-greater check above blocks only whole-head replay; it does not
stop a compromised freshness-delegate from signing a *genuinely fresh*
head (ordinal T+1, not the blocked replay case) whose entry for a victim
label names an *older* `latest_publish` or `fact_commitment` than the
consumer last saw — silently hiding a recent publish, yank, deprecation,
or owner-rotation, the exact threat this section exists to defeat. The
completeness scan (§7.5) does not save it: a hostile mirror can serve a
truncated-but-internally-consistent prefix. Therefore, on every head a
consumer accepts, **for each label entry, alongside the global-ordinal
check: the `latest_publish` it names MUST be at or after (in the anchor
log's own `prior` order) the last `latest_publish` this consumer observed
for that label, and `fact_commitment` MUST NOT regress below the last
fact-tip observed for that label's `latest_publish`.** A head that
regresses any per-label entry is rejected exactly as a stale-ordinal head
is; the monotonicity is enforced per label, not merely on the bundle's one
freshness ordinal (soundness/security convergence, adversarial review
2026-07-22).

### 12. Head set-scoped, one ref per registry [atom-head-scope]

Exactly one head ref per registry (`refs/atom/head`), bundling every
label's current state under one signature and one rotation obligation —
no per-label variant, no opt-in choice between shapes. The fold rule
evaluates each label's bundled entry independently: a stale or absent
entry for one label never affects the freshness verdict for another.
This — not ref separation — is what prevents "one slow label poisons a
fast one's freshness signal," and it buys the operational property that
matters: one rotation job, one key touch, one TTL clock, regardless of
how many labels an anchor publishes.

**Scoped to the label's latest version, not every version's fact
history.** A label's per-entry shape is `{latest_publish: czd,
fact_commitment: czd}`, where `fact_commitment` is the czd of the latest
signed fact leaf on *`latest_publish`'s own* fact stream — not the git
tag OID at the tip of its fact-tag chain (the tag chain, §7.5, is an
untrusted discovery accelerator; the czd names the signed record-log leaf,
keeping this field czd-typed and backend-agnostic like every other
identity here), and not an aggregate over every version's fact history. Both fields are governed by §11's per-entry
monotonicity MUST: neither may regress for a label across the heads a
consumer accepts, independent of the bundle's global freshness ordinal.
Tier 2 freshness therefore covers the
actively published version specifically; a consumer resolving an older,
non-latest version gets Tier 0/1 guarantees for recency about that
specific version (though, per §16, unconditional completeness
regardless), the same degradation-ladder honesty already applied to
whether a project opts into Tier 2 at all, applied one level finer, to
which of a label's versions the freshness apparatus actually covers for
recency purposes.

### 13. Interface mandatory in publish, stratum-aware [atom-interface-mandatory]

Every publish record MUST carry the completeness artifact appropriate to
its stratum
([Execution Model §1.5](../models/execution-model.md#15-the-two-strata-of-intent),
[Composition Model §4-5](../models/composition-model.md#4-interfaces-the-typing-of-compositions)),
checked at admission — never modeled as an optional fact, because a fact
may legitimately be omitted and this data structurally may not be.

- **Package atoms (executable intent):** the mandatory artifact is the
  built interface/output descriptor, producible only by actually
  executing a build ("verifiable only by building" — no registry can
  refuse a package that won't build without building it).
- **Environment/system atoms (algebraic intent):** the mandatory
  artifact is the coherence certificate, producible by pure
  recomputation of the formation fixpoint — requiring no execution at
  all.

These are the same law — nothing enters the registry without its
stratum-appropriate completeness proof — applied to two already-
formally-distinguished verification modes. The "executable intent
required somewhere" property is not lost for composite atoms, only
discharged transitively: environments and systems are algebraic *over*
packages, so a composite atom cannot be composed at all unless every
package inside it has already independently satisfied its own
executable-intent gate at its own publish time.

Independent third-party reproducibility corroboration (for package atoms
specifically) remains a legitimate `fact_type` layered on top of the
publisher's own mandatory assertion — baseline claim vs. independent
verification of that claim, not competing mechanisms. Precise field
shape per stratum is left to the spec-drafting pass, not settled here.

**Positioned honestly against the supply-chain-attestation maturity
ladder:** this `attesters`/`witnesses` model — a flat, self-asserted
claim plus optional independent corroboration — sits closer to SLSA
L1-L2 than to in-toto's richer multi-party layout and step-verification
model (SLSA L3-L4). That is not a gap this document needs to close —
nothing here claims that stronger guarantee level — but it should be
stated rather than left for a reader to assume parity with a maturity
tier this design doesn't target (prior-art review, 2026-07-16).

### 14. Bootstrap and the legacy/native boundary [atom-legacy-boundary]

**Registries never ingest.** A registry deals exclusively with its own
git history — there is no cross-repository ingestion mechanism at the
registry level. Becoming a registry for an existing upstream project
means pushing atom refs directly into that project's already-existing
repository — no copying is required at all when the party doing this
already has write access (the ordinary case: the maintainers themselves
are publishing their own project). A repository copy is needed only as a
fallback, when a third party wants to bootstrap atoms for a project whose
own maintainers have not and who lacks push access to the original.
Foreign-`src` multi-subject composition (§6) is rejected for the same
reason: it would reintroduce a live, uncontrolled cross-project URI
dependency the trusted-mirror system (§18) exists specifically to avoid.

**Legacy vs. native, checkable, and content-identical either way.** A
publish MAY use the legacy (payload-embedded manifest/lock, rather than
in-tree) mechanism **if and only if the charter's own `position` is not
an ancestor of the publish's `position`** — checkable via ordinary git
ancestry (`is-ancestor(charter.position, publish.position)`). This
governs the manifest/lock **declaration** only — whether dependency
metadata is embedded in the signed payload or read from an in-tree file.
It has no bearing on subject content: native and legacy publishes
resolve their content tree identically (§5, §7.4), and dependencies are
never embedded in either case — they are resolved and fetched separately
at build time, always. "Once native, stays native" needs no separate
rule: it falls out of the ancestry check automatically, since `position`
determines the actual content verified and real project history only
advances forward from the charter once it exists.

### 15. Liveness of published content: structural, not a bespoke obligation [atom-content-liveness]

The original draft's liveness-pin mechanism (`refs/atom/src/{position}`,
unconditionally pinning every position value a signed record carries)
existed to protect a *referenced* commit — one the registry does not
control the branch hygiene of — from being lost to ordinary project
housekeeping, since a signed field alone gives git's own garbage
collector no reason to consider that commit reachable.

**This is no longer needed for content.** Once a version's actual
content tree hangs directly off the closure commit (§7.4) — the same
tree OID `position:path` resolved to, referenced a second time — that
tree is kept alive by ordinary git reachability from
`refs/atom/pub/{label}/{version}` for as long as that ref exists,
regardless of what happens to the original `position` commit or the
branch it lived on. The thing that needed protecting (the tree) is
protected automatically, as a structural consequence of §7.4's design,
not as a separately-engineered obligation.

**`position` itself is now a provenance claim, not a liveness
guarantee.** The signed payload still records which commit and path a
subject's content came from — a meaningful, checkable fact for anyone
who wants to independently confirm it against the original project
history, when that history happens to still be reachable — but no part
of ongoing verification requires the original `position` commit object
to remain fetchable anywhere. Content verification (recomputing each
subject's own `content_hash` and folding them into the closure's
`content_commitment`, §5, against the closure's own tree) never
re-derives from `position` after publish time. This is a real narrowing of what the registry is obligated
to uphold, made explicitly here rather than left implicit, since it is a
new call this revision is making, not a restatement of the original
draft's own liveness-pins section.

### 16. The degradation ladder: completeness and recency are separable [atom-degradation-ladder]

Three tiers, each a read-time fact policies act on, never a silent
default:

- **Tier 0 — bare set.** No canonical source, no freshness claim. Full
  validity intact (signatures, signed chains, content binding), fork
  evidence wherever both halves co-locate, per-consumer trust-on-first-
  use (TOFU) monotonicity, and the write gate at the publisher's own
  home. Policies see `freshness: unclaimed`. This is exactly the landed
  status quo — this model never degrades below what exists today.
- **Tier 1 — canon declared.** Adds where-to-look-first, mirror-
  divergence-as-tamper-evidence, signed auditable migration. Still no
  TTL floor.
- **Tier 2 — canon + freshness-in-force + chained head.** The full
  hardened recency column (§11-§12); a declared freshness claim strips
  fail-closed.

**Revision (2026-07-16): completeness is unconditional, available at
every tier, and this is a genuine strengthening over what the original
draft claimed.** The original framing treated "did I see everything" as
bundled into the same recency guarantee head/TTL provides, available
only at Tier 2. §7.5's mandatory delta-scan separates these: *recency*
("how current is my view, right now") genuinely does require Tier 2's
signed, TTL-bounded heartbeat — nothing else can prove freshness as of a
specific moment. But *completeness* ("given whatever view I already
have, did I miss anything it claims to cover") is a property of the
record log being append-only and checkpoint-chained plus one cheap local
scan against a log a consumer has already fetched — it needs no
freshness apparatus at all, and holds at Tier 0 — **relative to the
source a consumer queried.** A Tier 0 consumer who fetches an anchor's
record log gets a view that is *complete with respect to the branch that
source served*: provably nothing was omitted from, removed from, or
reordered within it, as of the moment of that fetch (the append-only /
consistency-proof guarantee, §10). What the scan does **not** prove — and
must not be read as proving — is that the source is not *equivocating*,
maintaining a second, forked branch this consumer never sees; completeness
proves append-only extension of the source queried, never immunity to an
undetected single-branch fork. That residue is the same fork residue
§17-L4 discloses, located at the charter's canonical-source declaration
(optionally a threshold quorum of its declared mirrors/witnesses), and not
closed by the scan. What Tier 0 *additionally* lacks is any guarantee
about how stale that (complete-relative-to-its-source) fetch already was
the moment it completed. Chosen tradeoff, restated precisely: sovereignty
and legible absence of *recency* guarantees over Go-sumdb-style mandatory
central infrastructure — never absence of the *completeness* guarantee
this model provides unconditionally, understood as completeness relative
to the queried source, not detection of a hidden fork.

**Named explicitly, not left implicit:** this is a real divergence from
TUF's own baseline, where mandatory metadata expiration applies to
every role, not as an additive hardening tier. That is a legitimate,
different tradeoff given this document's sovereignty goals — a project
choosing Tier 0 or 1 is choosing to accept freeze-attack exposure it
could close by opting into Tier 2, whereas TUF simply does not offer
that choice. Stated here so it reads as a deliberate divergence from an
established reference design, not an oversight (prior-art review,
2026-07-16).

### 17. Fork proofs prove divergence, never intent [atom-fork-proofs]

Four layers, precisely scoped:

- **L1 validity** — signatures + signed `prior`/`under` chains.
  Absolute.
- **L2 fork evidence** — two leaves sharing a `prior`, both validly
  signed, is a portable, self-incriminating, offline-verifiable proof of
  *divergence only* — never of intent or custody (legitimate owner-set
  races can share a `prior` too). Now additionally visible, for free, as
  an actual divergent branch in the record log's own checkpoint-commit
  ancestry if both forks ever land in the same store — a git-native
  echo of the same signed evidence, not a separate mechanism. Fail-closed
  is scoped to downstream of the divergence and above the consumer's own
  recorded head, superseded by succession, never an unconditional
  freeze.
- **L3 freshness** — canonical source + TTL head records (§11, §16).
  Bounded, not absolute.
- **L4 split-view detection** — aggregating stores passively detect
  cross-published forks, but only catch attackers who let both forks
  reach some aggregator; a targeted, victim-only split view requires
  victim-side gossip or witness-cosigning, which this design does not
  provide and does not claim to.

### 18. Signed trusted mirrors [atom-trusted-mirrors]

The charter's canonical-source declaration extends to a signed mirror set
(`canon`, `mirrors`), owner-signed, amended by an ordinary anchor-scope
fact (§3) — never by the lower-privilege freshness-delegate key, since
mirror steering is trust-adjacent. Lock writers populate a consumer's
lock mirror list from this signed set, with charter-czd provenance,
closing an unsigned-mirror-steering hole an unsigned lock-side mirror
list would otherwise leave open. Canon and trusted mirrors are supposed
to serve identical families, so divergence among them is itself
immediate tamper evidence; repeated TTL violations from a listed mirror
are an evidence-based signal for owner-signed removal from the set, not
a vibes call.

### 19. The write gate is git's own compare-and-swap [atom-write-gate]

A registry is a store, a label index, and a write gate. This is now a
single mechanism, not two:

- **The record log** (`refs/atom/log`, §7.3, §9) — every append, at
  every scope, CAS-updates this one ref: a writer reads its current
  target, chains the new leaf's `pay.prior` to it, constructs the new
  checkpoint commit, and attempts git's own atomic ref-update
  compare-and-swap from the old target to the new. A losing writer's CAS
  fails cleanly and retries against the new tip. Nothing further is
  needed — the original draft's separate, dedicated "tip" ref for write
  serialization (distinct from the permanent per-record refs it
  serialized writes onto) no longer exists, because there are no longer
  separate per-record refs to serialize writes onto in the first place.
- **Publish** additionally needs the closure ref itself
  (`refs/atom/pub/{label}/{version}`) to transition from non-existent to
  existing as its own CAS point — unchanged in substance from the
  original draft's publish-uniqueness rule, just now describing one ref
  family instead of one among several.

The tip (`refs/atom/log`) is never trusted for verification, same
discipline as always — a verifier walks the real chain via `pay.prior`/
`pay.under` and inclusion proofs (§10), never taking the tip's word for
what is current.

### 20. Store: two-phase fetch, verify-before-fetch [atom-store-two-phase]

Store ingestion is still two distinct fetch operations against different
parts of the object graph, restated against the new mechanism:

- **Phase A (records)** — fetch `refs/atom/log` and walk its checkpoint-
  commit chain (or request specific inclusion proofs for targeted
  lookups). Small, always fetched, pure verification; needs no content
  at all — L1 validity is fully checkable from the record log alone.
- **Phase B (content)** — for the specific closures actually being
  materialized into a build, fetch the corresponding
  `refs/atom/pub/{label}/{version}` commit's tree content. This is the
  expensive step; it is what makes `content_hash` verification possible
  and what hands real files to the build environment.

**A new operational dependency, stated plainly rather than assumed
free:** because content now hangs directly off the same object graph as
the closure ref (§7.4), a naive unfiltered `git fetch` of a closure ref
would eagerly pull its content along with everything else. Conforming
clients MUST use protocol-v2 partial/filtered fetch (`--filter=blob:none`
or equivalent) to defer content blobs until a specific closure's content
is actually needed, preserving the Phase A/Phase B split as an
operational discipline rather than a structural one. This is the same
class of residual dependency §10's protocol-v2 reliance already accepts
(Open Items), not a new category of risk — but it is now load-bearing
for the fetch-cost story in a way it wasn't when content lived behind a
dedicated, always-deferred ref family.

**`[verify-before-fetch]`, a hard admission invariant, unchanged:** the
store MUST NOT attempt Phase B for a subject until that subject's
governing closure — and the chain of authority behind it (charter/claim
validity, resolved from Phase A) — has passed full signature and chain
verification. Fetch success alone is not sufficient for admission
either: once content is fetched, each declared subject's own
`content_hash` must be recomputed and folded into the closure's
`content_commitment` (§5), which MUST equal leaf0 of the closure tree;
only a match admits the content into the store's served space.

### 21. Store ref format and GC: ordinary git reachability, nothing bespoke [atom-store-refs-and-gc]

**The store mirrors the registry's own ref shapes directly — no separate
czd-only addressing scheme is needed anymore.** The original draft's
store-side design (`refs/atom/rec/d/{czd}`, a flat czd-keyed namespace
distinct from the registry's human-addressed refs) existed to solve
collision-safety across many mutually-untrusted registries aggregating
into one store. That concern is unchanged in principle, but the object
graph it needs to address has changed enough that this deserves a fresh
pass rather than a direct restatement — left to the spec-drafting phase
(Open Items) rather than asserted here without having actually worked
through the multi-registry collision case against the new closure/
fact-tag shapes.

**Garbage collection, however, is now settled, and it is a deletion, not
a simplification.** The original draft's own store-GC section (direct
refs under a shared scope prefix, explicit closure-pin membership
tracking) existed
specifically because content used to be reachable only obliquely —
through a signed field git's own reachability analysis cannot see —
requiring a bespoke, hand-maintained liveness mechanism to compensate.
Once content hangs directly off a structural ref (§7.4), that compensating
mechanism is unnecessary in its entirety:

Cleaning up every atom under a set is: walk `refs/atom/log`, filter for
publish leaves, collect their `(label, version)` tuples, delete the
corresponding `refs/atom/pub/{label}/{version}` refs, run ordinary
`git gc`. Nothing else is needed. This is correct, not merely plausible,
on one point worth stating precisely: the retained record log still
*textually* mentions old content tree OIDs (as declared subject fields
inside kept record blobs), but git's reachability walk only ever follows
*structural* edges — a tree's entries, a commit's `tree`/`parent`, a
tag's `object` — never blob content. So deleting a pub-ref genuinely
severs the only structural path to that closure and its entire fact-tag
chain (each tag only points backward, so the whole chain goes
unreachable in one shot once the ref at its tip is gone), even though the
kept record log still mentions the same hash as inert data. Content
shared between two versions (an unchanged subtree reused across
releases) survives correctly for the identical reason — reachable via
whichever pub-ref still exists, reclaimed once none do. Manual
operator GC-by-anchor needs no dedicated mechanism, exactly as the
original draft already argued for a different reason (Alt 6) — the
record log is already the source of truth and is already fetched to
learn what exists.

## Simplicity and Volatility Boundaries (Hickey/Lowy Audits)

Following the same audit convention as prior ADRs in this repo (e.g.
[ADR-0005](0005-hermetic-transactional-composition.md)'s own Hickey/Lowy
section): a Hickey audit checks for spatial simplicity — has the design
decomplected genuinely independent concerns, rather than merely moved
complexity around; a Lowy audit checks temporal volatility — do things
that change for different reasons live in different places, so a change
to one doesn't force a change to another.

1. **Spatial Simplicity (Hickey Audit):**
   - **One record shape, one log, one closure shape.** Charter, claim,
     and publish are the same kind of thing (a permanent scope genesis)
     at three different scopes; fact is the same kind of thing
     (a layered amendment) at any of the three. Five `pay.typ` values
     remain, but they now express two structural roles, not five —
     genesis and amendment — rather than five independently-justified
     record kinds each with their own succession or chaining story.
   - **Git objects carry exactly the roles they are structurally suited
     for and no others.** Blobs hold signed content, unconditionally
     (§7.3, §7.4). Commits provide cheap, walkable, git-native ancestry
     for backend bookkeeping only, never trust (§7.3, §7.4). Tags
     provide cheap, discoverable, git-native "something is attached
     here" signaling, also never trust (§7.5). Trees are used only where
     they represent genuine filesystem-shaped content — published
     subject trees (§5, §7.4) — nowhere else. The record log's own Merkle
     structure is **`eml`'s, computed by `eml` over data persisted through
     its `Storage` trait (§7.2), never git's own tree hashing**; whatever
     git objects the backend driver uses to store those bytes are pure,
     opaque storage, not the Merkle structure itself. (An earlier draft had
     git trees *be* the record-log Merkle structure — superseded by §7.2.)
   - **Decomplecting identity from transport.** czd is the only identity
     a record has; git OIDs, ref names, commit ancestry, and tag chains
     are all pure, interchangeable transport/query conveniences that
     never bear on security (§8's ref-leaf-discriminator principle, now
     explicitly extended to git object type as well).
2. **Temporal Volatility (Lowy Audit):**
   - **The record log changes because new events happen; a closure
     changes never, once written.** These are genuinely different
     volatility profiles and this revision is what makes them live in
     genuinely different places (§7.3 vs. §7.4) — the original draft's
     "record closure includes bearing facts" conflated them.
   - **Recency and completeness vary for different reasons and are now
     kept in different places.** Recency depends on active, ongoing
     publisher cooperation (the head rotation job, §11-§12); completeness
     depends only on the append-only log's own structure plus a cheap
     local scan, and needs no ongoing cooperation from anyone once a
     consumer has a copy of the log. Bundling them, as the original
     draft did, would have forced every future change to one to be
     re-examined against the other.
   - **The degradation ladder isolates the volatility axis that
     actually changes independently: recency infrastructure specifically,
     not verification infrastructure generally.** Tier 0 is exactly
     today's guarantees, now understood to include unconditional
     completeness; Tiers 1-2 are additive, opt-in hardening a project
     adopts on its own schedule for recency specifically, never a
     retroactive requirement on projects that never opt in, and never
     required for the completeness guarantee this revision adds.

## Consequences

### Positive

- Bootstrapping any pre-existing upstream project has a real, checkable
  mechanism (§14) rather than an unresolved special case, unchanged from
  the original draft.
- Chain verification for any leaf is O(log n) via inclusion proofs,
  regardless of log length, with no bespoke infrastructure required of
  any forge beyond protocol-v2 (§10) — an improvement over the original
  draft's O(1)-round-trips-but-O(n)-application-side-reassembly scheme.
- The store admits nothing unverified, ever, and pays zero content-fetch
  bandwidth for anything that fails signature/chain verification (§20).
- Canonical source is a genuine opt-in enhancement for recency
  specifically, never a hard dependency for completeness or validity
  (§16) — a stronger claim than the original draft could make, since
  completeness (relative to the queried source, not fork-detection —
  §16) is now unconditional.
- Ref count no longer scales with fact count at all (§9) — a strict
  reduction from the original draft's per-record ref families.
- Garbage collection on the store side requires no bespoke mechanism
  whatsoever (§21) — a full deletion of what the original draft's own
  store-GC section had to hand-build, not a simplification of it.
- The original draft's separate write-gate mechanism for chain families
  (its own, differently-numbered write-gate section) is subsumed
  entirely by git's own ref-update compare-and-swap (§19 here).
- The atom model remains independently corroborated by formal models
  ratified before this exploration began (`composition-model.md` §4,
  `execution-model.md` §9.10).
- Multi-signature governance thresholds and delegated publish-signing
  (§3) both fall out of the existing `fact_type`/declared-role
  mechanisms with no new record kind, no aggregation object, and no
  multisig primitive Coz would otherwise need to provide — closing a
  real gap the original threshold field left unenforceable, without
  adding a second authorization mechanism alongside the first
  (security review, 2026-07-16).

### Negative

- This design now depends on an external formally-verified library
  (`eml`, §7.2) for the Merkle log's canonicality and inclusion-soundness
  guarantees, rather than atom needing to build and independently verify
  that machinery itself — a dependency risk (atom's correctness is now
  partly downstream of a project still under active development
  elsewhere), traded for a substantially stronger foundation than a
  hand-rolled canonicality verifier would have started with (Lean 4,
  sorry-free, ≤4 structural axioms, collision-resistance discharged as
  an explicit hypothesis rather than assumed). Atom's own new
  implementation surface narrows accordingly to: a correct git-backed
  implementation of `eml`'s `Storage` trait, and a correct mapping
  between atom's own protocol fields and `eml`'s leaves — real,
  load-bearing work, but a materially smaller and better-bounded surface
  than reimplementing canonical Merkle-tree verification from scratch.
- Partial/filtered git fetch (protocol-v2) is now load-bearing for the
  Phase A/Phase B fetch-cost split (§20), not merely a nice-to-have —
  conforming clients that fetch closures without filtering will eagerly
  pull content they didn't need.
- `[verify-before-fetch]` and stratum-aware `[atom-interface-mandatory]`
  remain real, load-bearing implementation obligations on the store and
  publish tooling respectively.
- The Tier 2 freshness apparatus (head, rotation, TTL) remains a
  genuine, ongoing operational cost for any project that opts into
  recency guarantees specifically — narrower in scope than the original
  draft implied, since completeness no longer depends on it, but the
  recency cost itself is unchanged.
- Several residual risks remain named rather than eliminated (Open
  Items): the tier-0 long tail (now specifically a recency, not
  completeness, long tail), the multi-home concurrent writer, the k=1
  attestation tail, and the first-contact-under-a-single-hostile-host CAP
  corner.
- Threshold-counting (§3) is new, real verifier logic — walking a
  scope's co-sign facts and counting distinct current-owner-fold
  signers against a declared threshold — that every conforming verifier
  now needs, on top of the single-signer fold-check every other
  owner-authoritative fact already required.

### Risks Accepted

- **The tier-0 long tail — now scoped to recency only.** Small or
  dormant projects will sit at Tier 0 indefinitely; their consumers get
  validity and completeness without recency. Chosen tradeoff: sovereignty
  and legible absence over a Go-sumdb-style mandatory central log,
  narrowed by this revision to apply to recency specifically.
- **The multi-home concurrent writer.** No protocol can serialize a
  publisher who concurrently writes to independent, unsynced ref stores;
  protection is publisher-side discipline (one write home + the write
  gate, §19), recovery is fork adjudication via succession (§17).
- **The dead anchor.** If every key satisfying an anchor's owner-fold is
  genuinely, permanently lost, that anchor can never be extended again —
  no further claim, publish, or owner-rotation fact can ever be validly
  signed for it. This is not a protocol defect to patch; it is the
  necessary, correct consequence of what losing all your keys means in
  any sound cryptographic system — nothing here should or could override
  it without a centralized backdoor that would undermine the whole point
  of owner authority being self-sovereign. Every record the anchor ever
  signed remains permanently hostable and verifiable; only the ability
  to extend that specific identity further is lost, and re-anchoring (a
  genuine fork, a new charter) is the correct, expected response, not a
  workaround.

  How often this actually triggers is substantially reduced by keeping
  "owner" deliberately abstract rather than binding it to one specific
  key-management scheme (§3): an owner MAY be backed by a richer
  identity system — [Cyphr](https://github.com/Cyphrme/Cyphr), notably,
  since it already composes with this design trivially via the same
  embedding mechanism central to §7.4 (a Cyphr principal's own root can
  sit as a leaf inside an anchor's data, or the reverse, with no new
  machinery) — in which case ordinary key rotation and device changes
  are absorbed entirely inside that system and never touch the anchor's
  own owner-fold at all. GPG's web-of-trust offers a looser version of
  the same idea, without Cyphr's rigor. This mitigates the common case;
  it does not change the case actually being accepted here — total,
  permanent loss of whatever backs the owner-fold, however that owner is
  itself managed, remains unrecoverable by design, and correctly so
  (clarified 2026-07-16).
- **The single-attester tail (a corroboration quorum of one — an
  unrelated use of the letter "k" from `eml`'s tree arity, §7.2).**
  Long-tail atoms attested by exactly one builder have no counter-signal;
  a quorum-of-one default accepts it.
- **First contact under a single hostile host.** A consumer whose only
  view is one hostile host, with no TTL policy in force, can be shown a
  coherent stale-or-forked world for recency purposes specifically —
  completeness of that (stale) view is still guaranteed by this
  revision, which narrows but does not close this corner.

## Open Items

- **Reconciliation against the current landed model.** Still the
  explicit next step after this draft is reviewed: read
  `docs/models/atom-model.md`, `docs/architecture/atom-sad.md`, and the
  atom specs, and enumerate exactly what changes, clause by clause.
- **Nested subject-path disjointness.** Whether nested declared paths
  under the sibling constraint (§6) are permitted or forbidden under the
  existing `⊕` disjointness law needs a direct restatement in the spec
  pass; unresolved from the original draft, unaffected by this revision.
- **Precise interface-descriptor field shape, per stratum.** §13
  commits to the field being mandatory and stratum-aware; the concrete
  schema is spec-drafting work, not decided here.
- **Store ref-keying, revisited.** The original draft's czd-direct
  store-side addressing needs a fresh pass against the new closure/
  fact-tag object shapes (§21) — not simply restated, since the object
  graph it needs to address collision-safely across mutually-untrusted
  registries has changed.
- **The adversarial review pass over the full 2026-07-16 revision** (the
  git-representation layer rework and the `eml` integration) **ran
  2026-07-22** — decorrelated rounds over §7 and over the whole document,
  outside §7 — and its fixes are folded into this freshen revision: the
  §7.4 verification rewrite (B1/B2/B3) and §8 carve-out, the per-entry
  head-monotonicity MUST (§11), the EON re-scoping of static
  fork/freshness claims (§10, §16, Context), and the stale-residue purge.
  The two surety-layer findings it surfaced — the `B(a)` anchor-admission
  enumeration overclaim and the Alloy `srcEstablished` anchoring gap —
  are deferred to the surety mechanization, not this ADR.
- **The formal Lean spine, revised scope (2026-07-16).** The structural
  canonicality and inclusion-soundness proofs this item originally
  called for already exist, sorry-free, in `eml`'s own corpus
  (`proofs/lean/EMLProof/Kary.lean` et al. — §7.2) — atom does not need
  to build or independently re-derive that machinery. What remains
  genuinely open, narrower than the original item: atom-*specific*
  protocol claims layered on top of `eml`'s structural guarantees — fold
  well-definedness for atom's own owner/claim/label authorization folds
  (§3), genesis-once uniqueness under the log's own total order (§10),
  chain-splice impossibility for atom's own `prior`/`under` fields, and
  head-rejection ordering (§11-§12, now a stated protocol rule as of
  this revision, not yet a Lean-proven one) — none of which `eml` proves
  or is responsible for, since they're atom's own protocol semantics, not
  Merkle-structure properties.
- **Independent confirmation that `eml`'s no-prefix design closes the
  classical attack classes — partially done, this session.** Read (not
  merely summarized from docs): `kary_inclusion_soundness` and
  `inclusion_proof_unique` in `proofs/lean/EMLProof/Kary.lean`. Their
  soundness is conditioned on two explicit, non-axiomatized hypotheses
  (`¬NodeHashCollision`, `¬CollapseAmbiguity`) — the ordinary collision-
  resistance assumption, not a domain-separation byte, closing the
  leaf/internal type-confusion class RFC 6962's prefix targets. Also
  confirmed: the construction has no odd-leaf-count padding/duplication
  step at all (base-`k` frontier decomposition handles any exact count),
  so the classical CVE-2012-2459 duplicate-leaf attack does not apply
  structurally, independent of prefixing. This closes a concern raised
  earlier in this same revision's own reasoning — no longer open.
- **Content_hash tier (OPTIONAL vs. SHOULD/MUST).** Unresolved from the
  original draft, unaffected by this revision.
- **Protocol-v2 transport dependence.** Now load-bearing in two places
  instead of one (§10's inclusion proofs and §20's Phase A/B fetch
  split) — legacy dumb-HTTP remotes degrade further than the original
  draft anticipated. Accepted as a residual, not solved.
- **`eml`'s `Storage` trait, assessed (2026-07-16).** Read
  `polydigest/src/storage.rs`: the trait is genuinely defined at the
  `polydigest` (multi-algorithm combinator) layer, already parameterized
  by `alg_id` throughout — not hardcoded to EML's `k=2`/arbitrary-subtree
  choices, a real point in its favor. One structural finding worth
  keeping on record regardless of whether atom ever needs it: `cml`
  ("Canonical Merkle Log," the single-algorithm engine `eml` is built
  from by adding multi-algorithm support) has its own `NodeReader`
  interface that is explicitly read-only and documented as never owning
  a store ("the engine never owns the store, so the `polydigest`
  combinator can drive N views over one shared substrate") — there is
  currently no independent *write*-storage abstraction at the raw-`cml`
  layer, only at `polydigest`. Not a blocker for atom (the decision is
  to go through `polydigest`/`eml` — decided this session, not yet
  elaborated in this ADR's own text), but it means "use raw `cml`,
  bypass `polydigest`" does not currently have a persistence
  story to plug into — worth keeping on record for Cyphrme's own
  potential future use of raw `cml`, not something this revision needs
  to fix.
- **The git-backed `Storage` trait implementation itself.** Not started.
  Feasibility reasoning is sound (`write_batch`'s all-or-nothing
  semantics map naturally onto "write immutable git objects, then one
  atomic ref-update"; `store_node`/`get_node`'s `(alg_id, left, height)`
  keying needs some deterministic git-side addressing scheme, not yet
  designed) but the actual implementation, and its exact object layout,
  remains real, unstarted work — not a design question this ADR
  resolves.
- **Client-side genesis-once discipline, not yet specified.** §10 states
  genesis-once as a verifier-side fail-safe: a second genesis for an
  already-founded scope is void and rejected on read. That's the
  backstop, not the primary discipline — a conforming client is
  expected to check its own already-fetched log and refuse to construct
  a duplicate charter/claim/publish before ever signing one, so an
  honest writer never relies on the fail-safe catching its own mistake.
  This is a behavioral requirement on client implementations, not an
  architectural decision this ADR needs to settle; belongs in the
  future spec-drafting/doc-consolidation pass as an actual normative
  constraint on conforming clients (clarified 2026-07-16).

## Alternatives Considered

### Alt 1: Commit or tag as the record carrier

Rejected for the record's own content (§7.3, §7.4) — records are
still, unconditionally, blobs, never embedded in commit messages or tag
messages. **This needs a sharper restatement than the original draft
gave it, because this revision does use commits and tags extensively —
just never as carriers.** The original rejection reasoning (ancestry-
shaped baggage, unconstrained bytes, fetch/advertisement nonstandardness)
was scored against commit/tag-*as-trust-boundary* — a proposal where the
carrier's own structure would need to be part of what a verifier
reasons about. That rejection stands, unmodified. What this revision
adds is a proposal the original elimination pass never separately
scored: commit/tag-*as-untrusted-DAG-scaffolding*, where the carrier's
structure does real backend work (cheap ancestry walks, cheap discovery
signaling) but is never part of the trust boundary, cross-checked
against the signed payload at every hop. The elegance metric the
original pass used (mirror invariants, normalization rules, unconstrained
bytes, fetch/advertisement nonstandardness) was implicitly scored
*assuming* the carrier had to be trusted; once that assumption is
dropped, "unconstrained bytes" stops being a security concern and
becomes purely a backend-hygiene one (documented explicitly, §7.3: fixed
inert fields, git-native signing slots never checked).

### Alt 2: Tree-container refs for chain liveness, registry side and store side

Rejected registry-side (§9) and store-side (§21) in the original draft,
for cardinality-related and semantic reasons respectively. **This
revision's append-only Merkle tree (§7.2) is a real, different
instance of "taking control of the tree object," worth distinguishing
explicitly
from what Alt 2 rejected.** Alt 2 proposed a tree as a *container* —
using tree entries to index a set of otherwise-unrelated objects, purely
for ref-count economy, a purpose git trees are not semantically suited
for (a tree is filesystem-shaped; a flat index of unrelated records
isn't). §7.2's Merkle log is not a container in that sense either — but
the sharper correction is that it is **not a git tree used as a Merkle
tree at all**: the Merkle structure is `eml`'s, computed by `eml` (§7.2,
deliberately *without* git's own tree hashing, and without any
leaf/internal domain-separation prefix), and whatever git objects a
backend driver persists it through are opaque storage, not the structure.
What distinguishes §7.2 from Alt 2 is therefore not "correct vs. incorrect
reuse of the git tree primitive" but that §7.2 buys a genuine capability
(append-only history with inclusion/consistency proofs) no git-native
primitive provides — at the cost of an external library — while Alt 2
bought cardinality economy that was never actually needed.

### Alt 3: Foreign-`src` multi-subject composition (cross-repository atoms)

Rejected (§6, §14), unaffected by this revision. Kept live, it would
reintroduce a hardcoded, uncontrolled cross-project URI dependency
directly undermining the mirror/canon decentralization system (§18).

### Alt 4: Per-label head, rather than set-scoped

Considered, adopted, then reversed within the original exploration.
Rejected in favor of set-scoped, per-entry-independent fold (§12),
unaffected by this revision.

### Alt 5: Trunk-only publishing, as the liveness mechanism

Rejected in the original draft in favor of `position` as a checkable
provenance claim. This revision changes *what* the registry is
obligated to do about `position` (§15 — no longer a liveness pin) but
does not reopen this alternative: trunk-restriction would still make an
ordinary project workflow (branching off an old commit for a point
release) structurally impossible, which remains too high a cost.

### Alt 6: An external database for store-side GC-by-anchor bookkeeping

Rejected in the original draft in favor of the registry's own ref
listing. This revision strengthens the rejection rather than reopening
it: §21's GC procedure now needs no bookkeeping structure at all, on
either side, external or internal — ordinary git reachability from a
small, fixed set of `refs/atom/pub/*` refs is sufficient by itself.

### Alt 7: Concatenated JSONL record bundles instead of per-record refs

Considered at length in the original draft (2026-07-15) and shelved, not
permanently closed. **This revision's unified single record log (§7.1)
is spiritually the same move — one continuous append-only structure per
anchor, rather than many small per-record refs — reached independently,
by a different route, and worth reconciling explicitly rather than
leaving two contradictory verdicts in this document.**

Alt 7's three original rejection reasons, revisited:

1. **Redundant semantics** — still applies to Alt 7's own proposal
   (bundling signed governance-pointer fields as a fetched artifact adds
   no information the fields don't already carry) but does not apply to
   §7's design, which adds genuine new capability (inclusion/consistency
   proofs, O(log n) verification) rather than merely re-materializing
   existing signed relationships in bulk.
2. **Contention is undesirable even at low probability** — this was the
   sharpest of Alt 7's three reasons, and this revision's own analysis
   (§7.1) concluded the concern, while real in principle, was
   over-weighted relative to this domain's actual write frequency:
   dozens of records over a project's lifetime makes a rare CAS retry
   negligible, not an architectural cost worth designing multiple logs
   around. This is a genuine, explicit revision of the original draft's
   own reasoning, not a new argument invented to justify a foregone
   conclusion — the original Alt 7 rejection was evaluating a strictly
   worse mechanism (a shared blob requiring full-content reconstruction
   on every append) against an already-decided direct-refs baseline
   where zero contention was a free byproduct of a design chosen for
   other reasons; this revision is choosing unification deliberately,
   for reasons unrelated to contention, and re-litigating whether the
   resulting contention is actually costly at this domain's real
   traffic — concluding it is not.
3. **The problem bundling solves may not exist** — remains true of
   Alt 7's specific proposal (optimizing round-trip count against a
   primitive that was never the bottleneck) but does not apply to §7's
   design, whose actual justification (the log's own inclusion/
   consistency proofs, structural GC via ordinary reachability, §21) is
   a real capability gain, not a round-trip optimization.

Net: Alt 7's proposed *mechanism* (a flat JSONL blob) remains rejected,
for reason 1 and 3 above. Its *contention* objection (reason 2), which
was the load-bearing reason at the time, is revised — not because the
concern was wrong in principle, but because it was scored against a
write-frequency assumption this domain's own established cardinality
does not support.

### Alt 8: Merkle Mountain Range instead of RFC 6962 — superseded, not merely rejected (revised 2026-07-16)

Originally framed and rejected as a live tradeoff: MMR's root as a
derived value with no single fetchable object identity, breaking this
design's uniform "identity equals a content-address you can fetch"
discipline; its amortized-append advantage not engaging at this
domain's cardinality; its peak-bagging conventions having fragmented
across real implementations where RFC 6962 has one published
definition. **That framing is now stale, not merely a settled
rejection — though RFC 6962 equivalence turns out not to be the reason
why.** Adopting `eml` (§7.2) resolves the question rather than picking a
side of it: `eml`'s construction is internally MMR-shaped (a frontier of
peaks, cheap incremental appends, durable witnesses) *and* its
canonicalization folds that frontier down to a single root rather than
leaving it as separate, unbagged peaks. That single property — not any
claim of exact equivalence to an external specification's recursion —
is what dissolves the original tradeoff: MMR's real advantage (cheap
incremental appends) and RFC 6962's real advantage (one fetchable root
identity, no derived/bagged value) turn out not to be in tension at all
once the frontier is canonically folded, independent of whether that
folded root also happens to match RFC 6962's specific MTH definition. An
earlier version of this entry additionally claimed a machine-checked
equivalence to RFC 6962's MTH (`rfc9162_mth_bridge`) — that citation was
wrong (§7.2; caught by adversarial review, 2026-07-16) and has been
removed, since the argument above never actually needed it. Kept as a
record of reasoning that was sound given what was known when it was
written, not as a live alternative.

### Alt 9: A generic/updatable Merkle tree instead of append-only (new, 2026-07-16)

Considered directly and rejected — see §7.2. The property this system
needs (root-then to root-now proves nothing was removed, only added) is
only a structural guarantee, checkable without trusting a policy
convention, when the shape makes in-place edits unconstructible. A
generic updatable tree can prove an edit happened; it cannot prevent one
from mattering, which is the opposite of what this system needs.

## Related Documents

- [Coz](https://github.com/Cyphrme/Coz) — the signed-message envelope
  format every atom record is (§2); not vendored in this repo as of this
  draft, and worth vendoring or otherwise pinning before the spec-
  drafting pass.
- [`eml`](https://github.com/Cyphrme/eml) — the formally verified,
  canonical, multi-hash-algorithm Merkle library (`polydigest(cml)@k=2`)
  this ADR's record-log, closure, and content-commitment structures
  (§5, §7) are built on, not a hand-rolled construction; see §7.2 for
  the canonicalization argument and the no-prefix design rationale. Not
  yet vendored/pinned as a dependency — implementation work, not
  decided here.
- [Composition Model](../models/composition-model.md) — the `⊕`
  disjointness law this ADR reuses at the subject-composition scale
  (§6), and the package/environment/system trichotomy Layer 3 (§1)
  instantiates.
- [Execution Model](../models/execution-model.md) — the executable/
  algebraic intent stratification `[atom-interface-mandatory]` (§13) is
  stated against, and §9.10's independent ratification of uniform
  cross-stratum atom publication.
- [Storage Model](../models/storage-model.md) — §5's "share the content-
  addressing idea, not a namespace" framing this ADR's storage/atom
  coupling is a concrete instance of.
- [ADR-0005: Hermetic Transactional Composition](0005-hermetic-transactional-composition.md)
  — the L1/L2 layer designation this ADR sits beneath, and the substrate
  trichotomy this atom redesign completes the third leg of.
- [ADR-0006: Execution as the Primitive](0006-execution-as-the-primitive.md)
