# ADR-0008: Surety of Source — Vouched Source Classes and the Trust Surface

- **Status**: ACCEPTED
- **Date**: 2026-07-22
- **Deciders**: project lead
- **Extends**: [ADR-0007](0007-atom-version-integrity-system.md) — the
  record envelope, the fact mechanism, and the genesis-once discipline
  are consumed unchanged; this ADR adds one `fact_type`, one anchoring
  requirement, and the trust-accounting model that consumes them
- **Normative elaboration**:
  [Surety-of-Source Model](../models/surety-of-source.md) — the formal
  definitions, the ceiling theorems, and the machine-checked safety
  result behind every decision below
- **Related**: [Atom Model](../models/atom-model.md),
  [Trust Model](../specs/trust-model.md),
  [ADR-0006](0006-execution-as-the-primitive.md)

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

ADR-0007 makes every publication a signed, chain-anchored event, and
the trust model makes acceptance of build evidence a consumer policy
decision over anchored signers. One question remained open across that
machinery: **how does a consumer know that what an atom's build
consumed was genuine, inspectable source — all the way down its
dependency closure — rather than a binary laundered through a
source-shaped container?**

The landed corpus had two inputs bearing on that question, and both
were testimony by the party being checked:

- **The source-class declaration** (`ClaimPayload.pkg`,
  [atom-transactions](../specs/atom-transactions.md)) is a free-text
  string signed by the claim owner, whose documented purpose is
  version-dialect and fetch-adapter selection — not source
  attestation. Read as an attestation it is self-signed, repurposed,
  and open-world: an unregistered class string for which no verifier
  adapter exists could silently cause no gate to run at all.
- **The reproducibility mode**
  ([atom-transactions](../specs/atom-transactions.md)
  `[publish-mode]`) is a signed self-declaration by the publisher.
  Taken alone as a verification input, it would let a publisher reach
  the strongest trust tier by declaration.

The formal analysis
([Surety-of-Source Model](../models/surety-of-source.md)) established
what any verifier can and cannot do here. Two hard gates over a
declared class's input tree (container-format detection; parsing)
decide exactly the syntactically detectable launderings, and no sound
decidable test can reject more without rejecting genuine source
(Theorem 2). Past that frontier the question "is this committed
generator a genuine transformation or a disguised emitter of one
binary" is undecidable — even knowing the one input/output pair the
build exhibited (Theorem 1). The consequence is architectural, not
defeatist: what cannot be decided must be **accounted** — classified
fail-closed, booked into an enumerated trust surface, and closed only
over signed, attributable, policy-admitted evidence. That accounting
is machine-checked in Alloy: no laundered-shaped closure member ever
classifies as source-verified, and no atom resting on one ever
presents as `Total`.

This ADR records the protocol-surface decisions that make the
accounting real. The model document is the normative elaboration; the
decisions below are the parts a protocol implementer consumes.

## Decision

### 1. The source-class-vouch `fact_type` [sos-source-class-vouch]

A new `fact_type` on ADR-0007's existing fact mechanism — no new
record kind, no new scope, no new signing machinery:

```
pay.typ       = "fact"
pay.fact_type = "source-class-vouch"      -- version-scope fact
payload:
  target      -- the vouched version identity: the publish czd, which
                 fixes the vouched input tree through the publish's
                 content commitment
  class       -- the vouched source class, the same string vocabulary
                 as ClaimPayload.pkg
```

Its meaning: _the signer asserts that the target version's input tree
is genuine source in the named class — authored in that class, not a
laundering container._

- **Species and acceptance.** It is an **asserted** fact in the
  derived/asserted partition ([Atom Model](../models/atom-model.md)
  §4): acceptance follows the asserted-fact rule — it counts for a
  consumer iff its signer is admitted by that consumer's policy with
  the `assertor` role
  ([trust model](../specs/trust-model.md)
  `[trust-role-authorization]`). No `record_core` quorum applies:
  thresholds gate derived
  records only (`[trust-threshold-rule]`), so one admitted,
  unretracted, anchored vouch establishes the class for that
  consumer.
- **Issuance is unrestricted at the protocol level.** Any keyed party
  may sign a source-class-vouch: the publisher itself (the degenerate
  self-vouch), a distribution's bootstrap set, any third party. There
  is **no protocol-level voucher registry**; which vouchers count is
  downstream admission-control policy, exactly as for builders and
  anchor trust. A consumer whose policy admits publisher self-vouches
  thereby chooses self-attestation — visibly, in its own policy, never
  silently in the protocol.
- **Retraction.** Like every fact, a vouch is retractable by a later
  `retracts` fact from its signer (ADR-0007 §3); establishment is
  evaluated net of retractions at the consumer's evidence snapshot.

### 2. The anchoring requirement [sos-vouch-anchoring]

**A source-class-vouch MUST be anchored; an unanchored signed vouch
carries no verdict.** This instantiates the signature-anchoring law
([Atom Model](../models/atom-model.md) §5) rather than adding to it:
an unanchored signed value is `local` and carries no protocol trust,
whatever its cryptographic validity.

Two anchoring placements satisfy the requirement:

- **In the target's own record log**, under ADR-0007 §3's existing
  authorization unchanged: an owner self-vouch through the label's
  fold; a third party through a charter-declared role (`attesters`).
- **On the voucher's own metadata chain**, as a fact anchored there,
  when the target's charter has not declared the voucher — the
  anchoring law requires _some_ atom's chain, not the target's.

These two roles are distinct and must not be conflated: the `attesters`
role (ADR-0007 §3, charter-declared) authorizes a third party's vouch to
*anchor* in the target's own log, while the `assertor` role (§1, [trust
model](../specs/trust-model.md) `[trust-role-authorization]`) is the
consumer-side policy admission that decides whether an anchored vouch
*counts* for that consumer. Anchoring is valid placement; assertor
admission is whether the verdict credits it — a vouch can be validly
anchored yet admitted by no consumer, and neither substitutes for the
other.

Verifiers evaluating establishment MUST consume only anchored,
unretracted vouches, however transported. The transport surface for
out-of-log vouches — how a consumer discovers vouches anchored on
chains other than the target's — is deliberately deferred design work
(see Open item below); the anchoring requirement itself is not open.

### 3. The totality and trust-surface model [sos-trust-surface]

The classification model of
[Surety-of-Source Model](../models/surety-of-source.md) §2–§7 is
adopted as the normative source-verification semantics of the atom
plane:

- **Total, fail-closed classification.** Every member of an atom's
  dependency closure is classified into exactly one of four buckets
  (`ReproducibleCASource`, `AttestationResidue`, `TrustImport`,
  `SourceClassResidue`); anything not affirmatively established falls
  to a residue bucket. There is no unclassified state. In particular:
  a member with no build record — promoted fetch pins included — is a
  counted `TrustImport`, and a member whose sourcehood rests on
  self-declaration alone is counted `SourceClassResidue`.
- **The two-component trust surface.** The verdict a consumer reads
  has two enumerated components: the **residue** `T(a)` (the closure
  members whose source-level verification did not close) and the
  **assumption basis** `B(a)` (the policy-admitted signatures —
  corroborations and vouches — plus the genesis-seed identities the
  non-residue classifications rest on). Both are derived by the
  consumer's own closure walk over signed records, never asserted by
  the checked party.
- **Total.** `Total(a) ≜ T(a) ⊆ GenesisSeeds`: the residue is reduced
  to the permanent genesis seed(s), and every other grain of trust the
  verdict admits is enumerated in `B(a)`, above the protocol's
  verification floor (the hash and signature schemes and the verifier
  itself, which every verdict presupposes).
- **Reproducibility is empirically grounded.** A `mode = reproducible`
  declaration alone never admits a member to `ReproducibleCASource`;
  admission additionally requires at least one independent
  `record_core`-equal rebuild from a distinct policy-admitted builder
  (`[trust-threshold-rule]`). Declared-but-uncorroborated members are
  `AttestationResidue`. The declaration keeps its existing policy role
  (`[trust-mode-rule]`) — it gates refusal, not bucket membership.
- **Conforming implementations expose the accounting.** A verifier
  implementing this model MUST make `T(a)` and `B(a)` available to
  policy, so that admission decisions and revocation impact are
  computable from the enumeration rather than re-derived ad hoc.

### 4. The source-class gate [sos-source-class-gate]

The gate runs an atom's declared source class against its **input
tree** (never its output), in two decision tiers:

- **Hard gates (decide):** the **format gate** — executable/object
  container formats (magic-byte detectable, whole-file scan) in a
  claimed-source tree fail; and the **parse gate** — tree contents
  must lex/parse in the declared class.
- **Soft tier (evidence only):** opacity signals (large high-entropy
  literals, high blob ratio) and plan-emission correlation (output
  bytes recoverable from tree literals) **flag**, never reject. This
  tier is structurally barred from promotion to a hard gate: deciding
  it in general is deciding the undecidable generator-degeneracy
  property
  ([Surety-of-Source Model](../models/surety-of-source.md) §9.1).

A class is **gate-executable** for a verifier when that verifier
carries a format profile and parser adapter for it. A declared class
that is not gate-executable cannot pass the parse gate, so its atom's
sourcehood cannot be established and it classifies
`SourceClassResidue` — the open-world defect closes by fail-closure,
with no enumerated class registry. `ClaimPayload.pkg` keeps its
documented dialect/fetch-selection purpose; source attestation lives
exclusively in the vouch.

## Rationale

**Unsigned publication is structurally impossible in this system — and
with this ADR, source-class trust is captured as a signed,
attributable vouch — so a trust decision is always available and
always attributable, never an anonymous void.** That is the
engineering kernel of the design. The claim and publish transactions
are signed records and the verification pipeline rejects everything
else; this ADR extends the same discipline to the last unsigned input
of the source-verification story. Before it, an atom's sourcehood
rested on a self-chosen string invisible to every accounting; after
it, sourcehood either closes over a named signature a consumer's
policy admitted — enumerated in `B(a)`, permanently attributable,
retractable but never erasable — or it does not close, and the member
is counted residue that any policy can see and price.

The same one move — replace the checked party's testimony with signed
records from other parties, admitted by the consumer's policy —
repairs both self-signed verification inputs: corroborating rebuild
records for the reproducibility mode, source-class-vouches for
sourcehood. What cannot be decided (Theorem 1) is thereby converted
into what can always be audited: a permanent, czd-addressed trail of
who vouched, who corroborated, and what remains residue.

The minimal form is deliberate. One `fact_type` on the existing
mechanism, no registry, no quorum, no new authority: the protocol's
job is to make the evidence exist, be anchored, and be enumerable;
deciding whose judgment counts is admission-control policy, where
every other trust decision in the system already lives.

## Consequences

### Positive

- The two known laundering escapes are closed structurally: a
  build-record-less payload (a promoted fetch pin) is a counted
  trust-import, and a self-declared source class — recognized or not —
  never establishes sourcehood by itself. Both closures are
  machine-checked
  ([Surety-of-Source Model](../models/surety-of-source.md) §10).
- `Total` becomes satisfiable for real software: with at least one
  policy-admitted voucher, a real atom can verifiably reduce its
  residue to the genesis seeds. Without the vouch mechanism this is
  impossible, not merely hard (machine-confirmed unsatisfiable).
- Trust becomes enumerable: `T(a)` and `B(a)` give a consumer the
  exact set of members still taken on faith and the exact signatures
  the rest depends on — so revoking a signer has a computable blast
  radius.
- No new protocol machinery: one `fact_type`, the existing anchoring
  law, the existing `assertor` role and policy surface.

### Negative / burden

- Consumers acquire a second admission-control dimension: an
  admitted-voucher set alongside admitted builders. Curation judgment
  (keys vs genuinely independent parties) is the consumer's, exactly
  as for builders — the machine-checkable floor is thumbprint
  distinctness only.
- Verdicts are policy- and snapshot-relative: `Total` is evaluated
  against a consumer's policy and a moment's evidence, and can change
  as vouches, corroborations, and retractions accrue. This is the
  honest shape of the problem, not an implementation artifact.
- Ecosystem bootstrap is real work: real atoms reach `Total` only
  after vouchers exist that consumers choose to admit, and the gate's
  reach grows only with per-class format/parser adapters.
- The out-of-log vouch transport surface (discovery of vouches
  anchored on the voucher's own chain) is deferred design work,
  tracked as an open item of this ADR. The anchoring requirement
  bounds it: whatever the transport, unanchored vouches carry no
  verdict.

### Neutral

- The safety half of the model never needed the vouch: with zero
  admitted vouchers every real member is residue and nothing false is
  ever asserted. The vouch buys reachability of `Total`, not
  soundness.
- The gate reads input trees only; build outputs are governed by the
  existing execution-record machinery (ADR-0006) unchanged.
- A consumer that admits publisher self-vouches reproduces the old
  self-attestation posture — for itself alone, as a visible policy
  choice.
