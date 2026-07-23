# Axios Glossary

Canonical terminology for the Axios project. This document is the
source of truth for every term it defines: specs, ADRs, the FAQ, and
the agent-facing AGENTS.md tables use these terms in these senses, and
on any conflict this document wins — a divergence discovered elsewhere
is a defect in the diverging document or a pending amendment here,
never a silent third meaning.

The glossary grows by section. This first section covers the Atom
plane; composition/HTC, Eos, and Ion sections will follow as their
vocabulary is ratified. It is deliberately not exhaustive: a term's
absence here means only that it has not yet had its ratification
round — not that it is free for reinterpretation. When a needed term
is missing, the governing document for its plane holds until an entry
lands here.

Entries are in dependency order: each term is defined before it is
used.

---

## The Atom plane

**Atom** — capital-A — is the version-integrity system: the records,
the append-only log, and the verification discipline this section
defines. Lowercase _atom_ is the unit it publishes. "Version-integrity
system" (VIS) is the category Atom belongs to, per ADR-0007's title
("Atom as a Version Integrity System"). The system is never called
"Axios" — that is the project umbrella (see _Axios_).

### czd (Coz digest)

The content-address of a signed record: a digest computed over the
complete signed envelope, signature included — so a record's czd
commits to the _authorization event_, not merely to payload bytes. A
property of the signed record rather than of any git object, hence
backend-agnostic. Every scope's identity is some record's czd. No
separate term exists for particular czd values: prose says _the
charter's czd_, _the claim's czd_, _the publish's czd_. (FAQ entry 3;
ADR-0007 §2.)

### Record

One signed statement: a Coz envelope whose signed payload carries the
statement's type (`pay.typ`), its subject, and its chain links (see
_Prior / Under_). A record is complete and verifiable entirely on its
own — signature first, then chain position. The record is the quantum
of the whole system; everything larger (atom, closure, log) is
composed of records. Five types exist: `charter`, `claim`, `publish`,
`fact`, and the structurally distinct `head`. (ADR-0007 §1, §2, §3.)

### Genesis record / genesis-once

Charter, claim, and publish are each the **single, permanent genesis
record of their scope** — written exactly once, at the moment the
scope first comes into existence, never reissued, never superseded by
a record of the same kind. There is no charter succession and no claim
replacement; the only way to a "new charter" is a genuine fork, i.e. a
deliberately new identity. The rationale is load-bearing, not
stylistic: publishes must stay immutable so that each version's
permanent record closure stays immutable, independent of whatever
facts accrue later. (ADR-0007 §3; FAQ entry 5.)

### Fact

Everything that happens to a scope **after** its genesis: owner
rotation, threshold changes, label transfer, yank, deprecation,
advisory, attestation, co-sign. One record shape and one mechanism at
every scope; only `fact_type` varies. Facts are appended, never
mutated, never deleted — "removing" a fact means signing a new one
that `retracts` it. Authorization is fold-based: whoever the relevant
scope's own leaf history currently says holds authority may sign the
next fact, with third-party fact types bounded to charter-declared
roles (`attesters`, `witnesses`). (ADR-0007 §3.)

**NOT:** facts are not part of the record closure — they layer trust
status on top of it. There is no other mutation mechanism anywhere in
the system.

### Charter

**The point at which signed records begin for a project — and itself a
claim on the project repository as a whole.** The set scope's one
genesis record, carrying the owner-key set, signing threshold,
declared roles, and governance parameters, issued at a specific point
in the repository's history (see _Position_). The charter is the
anchor for any published atom (see _Anchor_): the point in time and
the associated signed metadata linking the project to its repository —
not the repository's genesis, but a chosen moment, linked through
ancestry back to that genesis. "Charter" always names the _record_ —
the immutable founding document — never the evolving scope it
establishes. (ADR-0007 §3, §4; FAQ entry 5.)

### Anchor

**The general term for the mechanism of linking to structure — a role,
not a value.** Two structures run through the system — the source
history and the record hierarchy — so anchoring comes in **two
families**:

- **Source anchoring**, the structural and temporal family: a link
  into the repository's own history. Pre-signing it is a development
  atom's _only_ anchor — the **genesis commit**, pure structure with
  no authority (no later commit can replace a genesis commit without
  invalidating the identity of everything built on it). Once signing
  exists, **every genesis record carries its own source anchor: its
  _position_**, the commit current at issuance — charter, claim, and
  publish are each source-anchored directly, which is what makes the
  temporal-vector invariant checkable. (FAQ entries 2–3; ADR-0007 §4.)
- **Authority anchoring**, the scope-nesting family: a link into the
  record hierarchy, expressed by `under`. A publish is anchored to its
  claim — its **label anchor**; a claim is anchored to its charter —
  its **set anchor**. (ADR-0007 §2, §3.)

**The charter is where the two families meet**: itself a claim on the
repository as a whole — source-anchored at its position, with more
entropy, temporal meaning, and signed metadata than the bare genesis
commit — and the root of authority anchoring for everything beneath
it. Outside any repository there is no history to anchor into, so the
source anchor degrades to a **well-known constant** and atoms still
work locally. (FAQ entries 3, 5.)

The charter's czd **names** the chartered anchor — it is the reference
and verification handle every consumer uses (log keying, verification
pins, ref layout) — but the czd is not the anchor itself: the anchor
is the linkage the charter effects; its czd is how you refer to it and
check it. In records, the anchor is committed to through the signed
chain links (see _Prior / Under_), which walk back to the charter
genesis leaf; there is no separate trusted `anchor` payload field, so
a verifier recomputes the anchor's czd from the chain rather than
reading it off any record. The word "anchor" also names the **scope of
ongoing authority** the charter establishes — the anchor's record log,
anchor-scope facts, the anchor's owner-fold — as distinct from the
charter document itself. (ADR-0007 §2, §4.)

**NOT:** the anchor is not a digest — "anchor = czd(charter₀)" in
older documents reifies the role into its name. The genesis commit is
not the anchor of any published atom. The anchor is never a declared,
trusted field of a record.

### Label

The human-chosen name an atom is published under. A label is always
tied to the **set anchor** — the charter: claiming a label means
binding it against one specific charter, exactly once, and the label
has no meaning apart from that repository-level anchor. A declared
field of the claim record. Labels are deliberately per-set, never a
global namespace: two unrelated charters may both claim `"quill"`, and
that is a feature, not a collision. Within a verified set's record
log, a claim is found by its label alone — the label names the claim;
the claim's czd is what it _is_. (ADR-0007 Terminology, §4.)

### Claim

The label-scope genesis record: one signed statement binding a label
to an anchor, exactly once, authorized by the claiming set's current
owner-fold. **The signed claim is the center of the system**: its czd
is the claimed atom's identity — a digest that incorporates the
signature itself, so identity commits to an _authorization event_, not
merely to publicly computable content. (FAQ entries 3–4; ADR-0007 §3.)

### Publish

The version-scope genesis record: the single, immutable statement that
a specific version of a claimed atom exists, with its content
commitment. Authorized by the label's current fold (or a declared
`publish-delegate`). Its czd is the version's identity. Everything
that happens to the version afterward — yank, deprecation, advisory,
attestation — is a fact, never an edit. (ADR-0007 §3; FAQ entry 3.)

### Version

The scope a publish record brings into existence. A version's identity
is its publish record's czd; the human-readable version string plays
the same naming role for publishes that the label plays for claims.
(ADR-0007 §3, §4.)

### Position

A record's **source anchor**: where in the project's own git history
the record was issued, as a commit OID, carried by charter, claim, and
publish (facts carry none — they are about an already-published
subject, not a new one). Position is the same anchoring role `under`
plays toward the record hierarchy, aimed at the other structure — and
the two families deliberately keep separate wire representations:
`under` is a signed record czd (backend-agnostic), position a commit
OID (the backend-specific seam). Position governs the temporal-vector
invariant `charter.position ≤ claim.position ≤ publish.position`,
checked by ordinary git ancestry, and is what gives the chartered
anchor its "specific point in time" character. (ADR-0007 §4.)

### Prior / Under

Atom's two chain links inside every record's signed payload (extension
fields of the generic Coz envelope — Coz itself has no chaining
concept), running in two different directions. **`prior` runs in log
order** — the czd of the previous leaf in the set's append-only record
log, absent only on the charter genesis leaf. The whole log is one
linear signed chain, so every record commits, transitively through
`prior`, back to the charter genesis. **`under` runs in scope order —
the authority-anchoring direction**: it links a record to its
enclosing scope (a claim's `under` points into the set scope; a
version-scope fact's `under` into the claim scope; absent on
charter-scope leaves, since nothing encloses the set). One precision
matters: `under` is pinned to the enclosing scope's most recent
_authoritative_ leaf at signing time — an authorization snapshot, "who
was in charge over me when I signed" — not necessarily the enclosing
scope's genesis record; the static closure lineage is recovered by
walking from that leaf back to its scope's genesis. Every leaf
therefore sits in the Merkle structure twice — once by time, once by
nesting — both signed, both independently walkable, neither expressed
through git ancestry. This is how the charter's czd is included to
complete the chain: committed through the links, verified by walking
them. (ADR-0007 §2, §3.)

### Atom (the unit)

The composed, stable unit the records constitute — build intent with
identity and ownership fused in rather than trusted separately. An
atom composes at three layers: the **record closure** (charter + claim

- publish, verified together, plus the content they authorize), the
  **verified content tree** (the exact bytes, identified by a
  content-**tree** digest — a Merkle root over the subject's loaded
  content, its bytes and in-tree structure, computed by a supported,
  pluggable algorithm (eml's k-ary construction by default) that is
  explicitly independent of git's own object/tree hashing: never a flat
  hash, and never git's tree OID reused as the digest), and the **built
  artifact** (what
  the build substrate produces from the verified tree). The record is
  the quantum; the atom is the composition. (ADR-0007 §1; FAQ entry 1.)

### Atom identity (and the unclaimed development atom)

Identity follows the anchor's ladder and _changes at each boundary, by
design_: outside any repository, the source anchor degrades to a
well-known constant and identity is a digest of that constant and the
atom's label; for an **unclaimed development atom** inside a
repository, identity is a digest computed from the genesis commit and
the label — the only stable content that exists before a signature;
for a claimed atom it is the claim record's czd. The pre-signature
digest preimage is **the only mechanism anywhere in the system where
anchor and label combine as a unit** — at exactly those two pre-claim
levels, and nowhere else. Code type names for identity values are
convention, not doctrine — this glossary deliberately defines no
identifier-level terms. (FAQ entry 3.)

**NOT:** the fused `(anchor, label)` pair has no role outside the
pre-signature preimage — not identity, not lookup path, not
coordinates. Each component has real, _separate_ significance. There
is no "hashed atom id."

### Atom-set

The collection of atoms sharing a common set anchor — in practice, a
single repository under one charter. (ADR-0007 Terminology.)

### Record closure — permanent and temporal

The fixed authorization bundle for one version: charter, claim, and
publish verified together, plus the content they authorize — and
deliberately **excluding facts**. Every publish carries two verifiable
record closures, which are **two views over the same log, not two
stored artifacts**: the **permanent closure** is what the log held at
publish — the initial, immutable, clean mapping from input to final
output; the **temporal closure** is the same closure with every fact
appended since. Keeping them distinct is what lets the permanent
record stay meaningful forever, no matter what facts later accrue:
"what is it" and "should I currently trust it" never collapse into
each other. In the git object model the permanent closure is realized
as the 2-leaf `eml` closure artifact — leaf 0 the content commitment,
itself an eml root over the declared subjects (leaf _i_ = subject
_i_'s own content-tree digest: Merkle, never a flat hash), leaf 1 the
set's authorization snapshot at publish; the temporal closure is
computed by layering the fact chain on top. (FAQ entries 1, 8;
ADR-0007 §1, §5, §7.4.)

### Composite

The covering term for the layered shape that recurs on both sides of
the system: the atom's three layers, and the output side's package /
environment / system. One concise word for a single piece or the whole
thing — an approachable analog of "closure," and like closures,
composites are made of smaller composites. (FAQ entry 1.)

### Head

A TTL-bounded freshness heartbeat with replace semantics: it asserts
"as of time T, here's current," so that a stale _view of recency_
becomes detectable. Structurally distinct from everything above — not
a genesis record, not a fact, carries no `prior`, is never a leaf in
the record log. **Deliberately outside the log**: the log is the
permanent history and stays total over everything that _is_ history —
charter, claim, publish, fact; a head is a point-in-time claim _about_
the log's currency, not a historical fact about anything, and there is
nothing to preserve about a superseded head once a fresher one exists.
It lives at its own ref, each head replacing the last, guarded by a
strict freshness-ordinal rejection rule. Detects staleness, not
incompleteness. (ADR-0007 §3, §11–§12.)

### Axios

The name of the whole endeavor and nothing inside it: the umbrella
over Atom, the composition substrate, Eos, and Ion. Greek for
"worthy," via _axiosophy_ ("worthy wisdom" — wisdom that proves out,
not merely loved); a repository label that hardened into a proper name
and turned out apt for a project premised on demonstrated worth. Using
"Axios" for any single system — Atom especially — is a category error.
(FAQ entry 34.)

---

## Deprecated and repudiated formulations

Explicit negative examples, so the next reader — human or agent —
cannot reconstruct them innocently:

- **"The `(anchor, label)` pair is the atom's identity."** Repudiated
  (2026-07-18). Identity is the claim record's czd — the same rule at
  every scope.
- **"The pair is the lookup path / query coordinates / handle."** The
  same error after a predicate swap; equally repudiated. Discovery
  needs no fused pair: within a set's log, a claim is found by label.
- **"Anchor = czd(charter₀)" as a definition.** The czd _names_ the
  chartered anchor; the anchor is the linking mechanism, not a digest.
  This entry supersedes that equation where older text states it.
  (Provenance, honestly: unlike the pair errors above, the equation
  was ratified into the FAQ on 2026-07-18 and refined away on
  2026-07-21 — a deliberate self-correction, listed here only so the
  reification cannot re-propagate.)
- **"The genesis commit is the anchor" — of a published atom.** True
  only at the pre-signing level; for published atoms the anchor is the
  charter.
- **Charter succession / claim replacement.** No scope's genesis
  record is ever reissued. Change is facts, forever.
- **"Hashed atom id" / "atom_id-as-digest."** Banned phrases from the
  pair era; correct prose names the actual digest — the claim's czd
  (or the charter's/publish's at those scopes).
