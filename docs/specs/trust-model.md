# SPEC: Trust Model — Acceptance Policy

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

_Status: v0.1 (2026-07-13). This is the acceptance-policy language
commissioned by [Execution Model §9.7](../models/execution-model.md):
the trust-anchor set format, the acceptance thresholds for
reproducibility corroboration, the observability of standing
divergence findings, the cache-refusal hook, and the signer-role
semantics for fact authorization. It is the POLICY layer only: the
atom-side semantics it builds on — the declared reproducibility mode,
the classification of divergence under that mode, and the
promotion/demotion law — are fixed in
[atom-model.md §6](../models/atom-model.md) and are cited here, never
restated._

## Domain

**Problem Domain:** Fact-hood in this stack is signer-relative
([Execution Model §3.4](../models/execution-model.md)): a record
arriving signed by an executor the consumer does not control is
accepted _as a fact_ — cache-usable, substitution-grade — only when
its signer lies within the consumer's configured trust anchors;
outside them the same record is evidence, subject to acceptance
policy. The execution model deliberately left the anchor format and
the policy language to this spec (§3.4: "deliberately left to the
trust-model spec, not fixed here"), and
[atom-model.md §9](../models/atom-model.md) registers the same
deferral from the atom side. This specification closes that
commission: it defines the concrete anchor-set format, the verdict a
policy assigns to a signed record, the k-of-n corroboration
thresholds, the contested-action refusal surface, and the role
semantics that authorize fact classes per signer.

**Model Reference:**
[atom-model.md](../models/atom-model.md) — §5 (the anchoring law: what
may enter acceptance at all), §6 (the reproducibility contract this
policy language inherits), §4 (the metadata partition: the two fact
species and the builder≠owner obligations);
[execution-model.md](../models/execution-model.md) — §3.4 (signer
trust), §2.2 (the multi-valued record store; witness selection as a
recorded choice), §3.1 (`record_core`, the equality every counting
clause below quantifies over).

**Parent Specification:**
[atom-transactions.md](atom-transactions.md) — the signing machinery
this language rides on (Coz `tmb` thumbprints, ownership
authorization), per Execution Model §9.7 ("riding on atom's Coz
identities");
[atom-backend-contract.md](atom-backend-contract.md) — the carriage
laws (`[backend-chain-append]`, `[backend-enumeration]`) that make
every input to a verdict chain-observable.

**Criticality Tier:** High — this spec decides when a foreign binary
is accepted in place of a local build. Its failure mode is the
supply-chain attack.

## Overview

A consumer holds one **acceptance policy**: a set of **trust anchors**
(signing keys, each scoped by role and quorum) plus an ordered list of
**rules** (scoped requirements on mode, corroboration, and contested
behavior). Evaluating the policy against a signed, chain-anchored
record yields exactly one **verdict**:

```
fact      — cache-usable, substitution-grade (Execution Model §3.4)
evidence  — visible, inert: never served, never counted, MAY inform
            a human or a future policy edit
refused   — policy forbids serving this record even though its signer
            is anchored (mode gate or contested action)
```

The policy is **consumer-local configuration, never chain state**.
Fact-hood is signer-relative, so there is no global verdict for the
chain to carry; two consumers holding different policies lawfully
disagree about the same record. The chain carries what verdicts are
computed _from_ — signed records and the publish chain — and the
backend contract guarantees those inputs are enumerable and
append-only.

Design decisions this spec fixes (delegated by its commission), with
rationale inline where each lands:

| Decision | Choice |
| :--- | :--- |
| Anchor-set shape | structured entry list (signer + roles + quorum), not a bare key list and not a general policy language (§Type Declarations) |
| Threshold syntax | per-anchor quorum AND per-rule minimum, over a policy default — no single global k (`[trust-threshold-rule]`) |
| Rule resolution | ordered first-match, document order (`[trust-rule-determinism]`) |
| Owner selector | `owner` as a pseudo-signer resolved through the protocol's own ownership machinery (`[trust-owner-selector]`) |
| Refusal surface | `on_contested` rule field, default `refuse` (`[trust-contested-refusal]`) |

## Constraints

### Sorts and Type Declarations

```
SORT  Tmb      -- Coz signing-key thumbprint
                  (atom-transactions.md, payload `tmb` fields)
SORT  Anchor   -- atom-set anchor = czd(charter₀) (atom-transactions.md
                  [charter-anchor]) — a Czd value, NOT a trust anchor
SORT  AtomId   -- (anchor, label), the composite's identity

TYPE  SignerRole = "builder" | "assertor"
      -- builder:  authorizes DERIVED facts — execution records and
      --           attestations (build records, interface manifests,
      --           observation records, trial attestations)
      -- assertor: authorizes ASSERTED facts — keyed post-hoc
      --           assertions (advisories, lifecycle markers, declared
      --           runtime requires)
      -- The derived/asserted species are the metadata partition's
      -- (atom-model.md §4); this spec binds to that classification.

TYPE  SignerRef = tmb(Tmb) | owner
      -- `owner` matches the atom's effective CLAIM owner (single
      -- value, never the charter's owner set) ([trust-owner-selector])

TYPE  TrustAnchor = {
        signer:  SignerRef,
        roles?:  Set<SignerRole>,   -- absent ⇒ both roles
        quorum?: u32,               -- absent ⇒ 1; see
                                    -- [trust-threshold-rule]
      }

TYPE  TrustAnchorSet = Set<TrustAnchor>
      -- at most one entry per SignerRef ([trust-anchor-set-format])

TYPE  Mode = "reproducible" | "witnessed"
      -- the declared publish mode; defined by atom-transactions.md
      -- [publish-mode] and atom-model.md §6, consumed here unchanged

TYPE  RuleScope = all | set(Anchor) | atom(AtomId) | role(String)
      -- role(_) selects atoms by declared dependency role (the
      -- `cc`-style toolchain roles, atom-model.md §7); its manifest
      -- binding awaits the kind-schema design (Execution Model §9.13)
      -- and it is specified here so the language can express the
      -- inherited contract's own motivating policy ("toolchain roles
      -- require reproducible atoms", atom-model.md §6)

TYPE  PolicyRule = {
        scope:          RuleScope,
        require_mode?:  "reproducible",  -- "witnessed" is the floor
                                         -- every publish satisfies;
                                         -- requiring it is vacuous,
                                         -- so it is not expressible
        min_witnesses?: u32,
        on_contested?:  "refuse" | "serve",
      }

TYPE  AcceptancePolicy = {
        anchors:  TrustAnchorSet,
        rules:    [PolicyRule],          -- ordered; first match wins
        defaults: { min_witnesses: u32 = 1,
                    on_contested:  "refuse" },
      }

TYPE  Verdict = fact | evidence | refused
```

The **rendering** of an `AcceptancePolicy` (TOML in a consumer config
file, a CLI flag surface, a daemon's deployment config) is an
implementation concern; the data model above is the normative content.
A non-normative TOML rendering appears in Appendix A.

### Invariants

**[trust-anchor-set-format]**: A conforming implementation MUST
represent the consumer's trust configuration as an `AcceptancePolicy`
per the type declarations above: a `TrustAnchorSet` holding at most
one entry per `SignerRef`, an ordered rule list, and explicit
defaults. An anchor entry MUST carry a resolvable `SignerRef`; `roles`
and `quorum` are OPTIONAL with the stated defaults. _Rationale for
the shape (delegated decision): a bare thumbprint list cannot express
the corroboration tier Execution Model §3.4 requires ("k record-equal
executions from independent signers upgrade confidence") nor the
builder/assertor split atom-model.md §4 obligates, while a general
policy language would exceed this spec's commission; per-entry role
and quorum is the minimal shape that covers both._
`VERIFIED: unverified`

**[trust-anchor-sort]**: A trust anchor's `tmb(_)` reference is
`Tmb`-sorted. `Tmb`, `Anchor`, `Czd`, and `OID` are pairwise disjoint
sorts: a trust anchor is never the atom-set anchor
(`czd(charter₀)`), never a protocol content-address, and never a
backend object id. Any comparison, assignment, or fabrication across
these sorts is ill-typed and MUST be rejected at the type level where
the implementation language permits. (This extends
[atom-backend-contract.md](atom-backend-contract.md)
`[backend-seam-typed]`'s discipline to the trust surface; the shared
word "anchor" is an unfortunate collision this constraint exists to
defuse.)
`VERIFIED: unverified`

**[trust-anchored-input]**: Only protocol-anchored signed objects
enter acceptance evaluation: the record MUST verify under the
protocol's verification pipeline
([atom-transactions.md](atom-transactions.md) §Verification Pipeline)
and MUST be anchored per the anchoring law
([atom-model.md §5](../models/atom-model.md)) — intent on the
transaction chains or a fact on some atom's metadata chain. A signed
value that is local in §5's sense (unanchored) MUST NOT be assigned
any verdict: it is out of protocol, and no anchor entry, quorum, or
rule can admit it. Consuming one is out-of-protocol trust by
definition, outside this policy language entirely.
`VERIFIED: unverified`

**[trust-signer-relative]**: Verdicts are per-consumer. An
implementation MUST compute verdicts against the local
`AcceptancePolicy` and MUST NOT treat another party's verdict as its
own input: an imported verdict is at most evidence. (Inherited frame:
Execution Model §3.4 — the stratification governs what a record _can_
be; signer trust governs what it _is, to you_.)
`VERIFIED: unverified`

**[trust-role-authorization]**: An anchored signer's records count
only within its roles: a `builder`-role entry authorizes derived
facts, an `assertor`-role entry authorizes asserted facts (species
per [atom-model.md §4](../models/atom-model.md)), and an entry with
no `roles` field authorizes both. A record whose signer is anchored
but whose fact class lies outside the entry's roles MUST receive
verdict `evidence`. Membership in the atom's ownership chain is
NEITHER necessary NOR sufficient for fact acceptance: signers outside
the ownership chain are admissible exactly like any other anchored
signer (the builder≠owner obligation, atom-model.md §4), and the
owner's own key earns no verdict the policy does not grant it. The
concrete fact-kind wire encoding is registered design work
([htc-sad §6.10](../architecture/htc-sad.md)); until it lands, an
implementation MUST derive the fact class from the record's object
kind per the classification table of
[atom-model.md §5](../models/atom-model.md).
`VERIFIED: unverified`

**[trust-owner-selector]** _(disambiguated 2026-07-14 — charter vs.
claim ownership no longer share one shape)_: The `owner` signer
reference matches a record's signer iff that key equals (or is
authorized under) the atom's EFFECTIVE CLAIM's `owner` —
`ClaimPayload.owner`, a single `OwnerRef`
([atom-transactions.md](atom-transactions.md) `[claim-owner-single]`)
— at the record's chain position, under
`[owner-authorization-delegated]`'s per-value semantics. This is
deliberately the CLAIM layer, never the charter layer: charter
ownership is now a non-empty SET of principals
(`[charter-owner-set]`, atom-transactions.md) governing who MAY hold
a claim under this anchor (`[claim-charter-authorization]`) — a
decision already made and recorded at claim-authorization time, not
a pool of keys this selector re-checks per record. "Effective charter
ownership" and "claim ownership" are therefore two distinct concepts
with two distinct shapes (set vs. single value); `owner` here always
means the latter. The selector introduces no new authorization
machinery — it reuses the protocol's own claim-level judgment,
evaluated per chain position so that a later claim replacement does
not retroactively re-classify old appends. When a record's signer
matches BOTH an explicit `tmb(_)` entry and the `owner` selector, the
explicit entry MUST govern alone — roles and quorum both: per-key
curation is more specific than the blanket selector, and the
precedence must be stated because `TrustAnchorSet` is unordered —
without it, two conforming implementations could apply different
quorums to the same record, violating `[trust-policy-pure]`.
_Rationale (delegated decision): "trust the publisher's own builds"
is the single most common real policy; without this selector it
would require per-atom anchor maintenance that tracks every key
rotation by hand._
`VERIFIED: unverified`

**[trust-threshold-rule]**: Acceptance of a derived execution record
as `fact` for cache service is quorum-gated. For a candidate record
`r` at action identity `a`, signed by anchored signer `s` (entry
quorum `q_s`, matched rule minimum `k_r`, both defaulting per the
policy), the effective threshold is `K = max(q_s, k_r)`, and `r` MUST
be servable only when at least `K` records at `a` — `r` included —
are `record_core`-equal to `r` ([Execution Model
§3.1](../models/execution-model.md)) and carry signatures from `K`
DISTINCT anchored signers whose entries cover the `builder` role.
This is the policy-language half of the corroboration semantics whose
atom-side is fixed by [atom-model.md §6](../models/atom-model.md)
("k `record_core`-equal executions from independent signers" — cited
as the inherited contract): §6 supplies what corroboration _means_;
this constraint supplies the POLICY — what k is and who sets it (the
consumer, per anchor entry and per rule, defaulting to 1).
Corroborating records count regardless of their own signers' quorum
values: quorum gates the record a consumer would _serve_, not the
evidence that corroborates it. Thresholds do not apply to asserted
facts: an asserted fact is accepted iff its signer is anchored with
the `assertor` role — there is no `record_core` for independent
parties to agree on.
`VERIFIED: unverified`

**[trust-threshold-independence]**: Signer distinctness is the
enforced independence floor: two records signed by the same `Tmb`
MUST count as one toward any quorum, and one record MUST NOT be
counted twice. Distinct thumbprints are machine-checkable; genuinely
independent operation (distinct operators, distinct infrastructure)
is not — it is the consumer's judgment, exercised in anchor
curation, and this spec states that honestly rather than pretending
the floor is the ceiling.
`VERIFIED: unverified`

**[trust-rule-determinism]**: Rule resolution MUST be deterministic:
the applicable rule for an atom is the FIRST rule in document order
whose `scope` matches (an `atom(_)` scope matches that composite; a
`set(_)` scope matches every atom anchored to that set; `role(_)`
matches per its binding once specified; `all` matches everything),
and unset fields of the matched rule take the policy defaults. If no
rule matches, the defaults apply alone. _Rationale (delegated
decision): first-match over an ordered list is the simplest total,
deterministic resolution; most-specific-wins was rejected because
specificity between `set(_)` and `role(_)` scopes has no natural
order and would demand a tie-break rule of its own._
`VERIFIED: unverified`

**[trust-mode-rule]**: A rule carrying `require_mode: "reproducible"`
MUST cause verdict `refused` for any derived execution record of an
in-scope atom whose effective declared mode is not `reproducible`
(mode per [atom-transactions.md](atom-transactions.md)
`[publish-mode]`; declaration semantics per
[atom-model.md §6](../models/atom-model.md)). The gate reads the
SIGNED DECLARATION, never witness statistics — a policy composed on
declarations has an accountable party behind every tier
(atom-model.md §6, "Consumers compose policy on declarations");
witness counts enter only through `[trust-threshold-rule]`'s
corroboration of individual records.
`VERIFIED: unverified`

**[trust-contested-refusal]**: The contested-action hook. A
**standing divergence finding** exists at action identity `a`, for
this consumer, iff the in-scope atom's effective declared mode is
`reproducible` AND there exist two records at `a` from distinct
anchored `builder`-role signers whose `record_core` values differ,
AND neither of the two lawful exits fixed by
[atom-model.md §6](../models/atom-model.md) (a signed mode amendment
on the chain, or this consumer's own anchor revision) has resolved
it. While a standing divergence finding exists at `a`:

- an implementation MUST evaluate the matched rule's `on_contested`
  field;
- under `refuse` (the default), every derived execution record at `a`
  MUST receive verdict `refused` — the contradicted action gets no
  cache service from this consumer, which discharges, as a concrete
  policy surface, the capability atom-model.md §6 requires trust
  policies to have;
- under `serve`, records remain eligible via the ordinary gates —
  a consumer opting out of refusal does so explicitly, in reviewable
  configuration, never by default;
- the finding itself MUST be surfaced to the operator (log, alert —
  the channel is an implementation concern; silence is not).

Consumer-side resolution is a policy edit: removing or re-scoping an
anchored signer dissolves the finding _for that consumer_ — findings
are as signer-relative as fact-hood itself, which is why an alarm is
a per-consumer investigation trigger rather than a global verdict
(atom-model.md §6).
`VERIFIED: unverified`

**[trust-finding-derivable]**: Standing divergence findings MUST be
derivable from chain state plus the local policy alone: a consumer
holding the atom's publish chain and metadata (enumerable per
[atom-backend-contract.md](atom-backend-contract.md)
`[backend-enumeration]`, carried append-only per
`[backend-chain-append]`) and its own `AcceptancePolicy` can compute
every finding this spec defines with no out-of-band data. Both exit
events are observable the same way: a mode amendment is a signed
chain append; an anchor revision is a local policy edit. An
implementation MUST NOT require a coordination service, registry, or
third-party feed to detect or resolve findings.
`VERIFIED: unverified`

**[trust-policy-pure]**: Verdict computation MUST be a pure function
of `(record, fact snapshot, policy)`: the same record evaluated
against the same chain snapshot under the same policy yields the same
verdict, on every conforming implementation. This is what lets the
witness pick at request formation be a _recorded choice over the fact
snapshot_ ([Execution Model §2.2](../models/execution-model.md)) —
an unrepeatable acceptance decision would poison P7's determinism
one layer up. When a witness pick is recorded, the implementation
SHOULD record a digest of the policy that made it, so the choice is
auditable against the configuration that produced it.
`VERIFIED: unverified`

### The acceptance procedure

**[trust-acceptance-procedure]**: A conforming implementation MUST
evaluate every signed record it would consume as a fact — at cache
service, at substitution, at resolution over asserted facts — by the
following ordered procedure, and MUST NOT consume a record as a fact
through any other path:

1. **Anchoring.** Verify the record per
   `[trust-anchored-input]`. Unanchored ⇒ no verdict; stop.
2. **Signer.** Resolve the record's signing `tmb` against
   `policy.anchors` (including `owner` expansion per
   `[trust-owner-selector]`; on a double match, the explicit
   `tmb(_)` entry governs, per that constraint). No matching
   entry ⇒ `evidence`.
3. **Role.** Check the entry's roles against the record's fact class
   per `[trust-role-authorization]`. Not covered ⇒ `evidence`.
4. **Rule.** Resolve the applicable rule per
   `[trust-rule-determinism]`.
5. **Mode.** Apply `[trust-mode-rule]`. Gate fails ⇒ `refused`.
6. **Contested.** Apply `[trust-contested-refusal]`. Standing finding
   with `refuse` ⇒ `refused`.
7. **Quorum.** For derived execution records, apply
   `[trust-threshold-rule]`. Threshold unmet ⇒ `evidence` (the record
   stands, more corroboration may yet arrive); met ⇒ `fact`.
   For asserted facts, the verdict is `fact` at this step.

The procedure is total over the anchored signed-object classes of
[atom-model.md §5](../models/atom-model.md)'s classification table
(intent-class objects are governed by the protocol's own transaction
verification, not by this policy — step 1 admits them as chain
context, not as verdict subjects).
`VERIFIED: unverified`

### Behavioral Properties

**[trust-verdict-total]**: For every anchored fact-class signed
object and every well-formed `AcceptancePolicy`, the acceptance
procedure MUST terminate with exactly one verdict. No record is ever
part-fact; no policy state makes the procedure abstain.

- **Type**: Safety
  `VERIFIED: unverified`

**[trust-no-silent-serve]**: A record MUST NOT be served as a cache
hit or substitution unless the procedure returned `fact` for it under
the serving consumer's policy at a snapshot at or after the record's
append. Serving evidence is a conformance violation, whatever the
record's cryptographic validity.

- **Type**: Safety
  `VERIFIED: unverified`

## Proof Obligations

Continuing the substrate-wide P-numbering (P1–P11 in the substrate
models; P12–P14 in [atom-model.md §10](../models/atom-model.md);
P15–P16 in [atom-backend-contract.md](atom-backend-contract.md)):

- **P17 — acceptance coherence.** (a) Totality and determinism: the
  acceptance procedure is a total function of
  `(record, snapshot, policy)` over the fact classes of P13's
  classification table (`[trust-verdict-total]`,
  `[trust-policy-pure]`); (b) quorum soundness: no sequence of
  appends by FEWER than `K` distinct anchored signers can drive a
  record to `fact` under threshold `K`
  (`[trust-threshold-independence]`); (c) refusal soundness: while a
  standing divergence finding is unresolved, no `refuse`-configured
  consumer serves the contested action
  (`[trust-contested-refusal]`). Small state machine, TLC-able in
  the AtomCharter style, and the natural companion of P14: atom-model
  §10 defers P14's model "when the acceptance-policy spec lands" —
  this spec is that landing, so P14 and P17 SHOULD be discharged as
  one model (the chain-side classification and the policy-side
  verdict over the same state space).

## Verification

Every constraint above carries `VERIFIED: unverified` — this spec
lands as specification, with its evaluators named here and owed. The
owed machine check, named precisely: **the P14+P17 TLC model** (see
Proof Obligations) covering mode declarations, record appends,
finding formation, the two exits, and the verdict function. Until it
lands, discharge is by the test batteries below plus review.

| Constraint | Method | Result | Detail |
| :--- | :--- | :--- | :--- |
| trust-anchor-set-format | review + rustc at impl | pending | schema literal; one-entry-per-signer enforced by type/dedup |
| trust-anchor-sort | rustc (disjoint newtypes) | pending | no cross-sort construction; candidate future row in the seam Alloy model (owed by atom-backend-contract §Verification) |
| trust-anchored-input | policy test | pending | unanchored signed value → no verdict, refused as input |
| trust-signer-relative | review | pending | design-level: no verdict-import path exists in the API surface |
| trust-role-authorization | policy test | pending | builder-only entry offered an asserted fact → evidence; non-owner builder accepted |
| trust-owner-selector | integration test | pending | needs charter/claim succession fixtures; ownership judged per chain position |
| trust-threshold-rule | policy test + TLC (P17b) | pending | K = max(entry quorum, rule minimum); insufficient corroboration → evidence |
| trust-threshold-independence | policy test + TLC (P17b) | pending | same-tmb double-count and same-record double-count rejected |
| trust-rule-determinism | property test | pending | first-match total order; verdict invariant under re-evaluation |
| trust-mode-rule | policy test | pending | require_mode against witnessed-mode atom → refused |
| trust-contested-refusal | policy test + TLC (P17c) | pending | finding stands → refuse default; serve requires explicit opt-out; operator surfaced |
| trust-finding-derivable | integration test | pending | finding computed from chain fixtures + policy alone, offline |
| trust-policy-pure | property test | pending | verdict(record, snapshot, policy) stable across runs/implementations |
| trust-acceptance-procedure | policy test battery | pending | golden verdict vectors over the worked example of Appendix B |
| trust-verdict-total | property test + TLC (P17a) | pending | fuzz records × policies; exactly one verdict, always |
| trust-no-silent-serve | integration test | pending | serve path rejects evidence-verdict records |

## Annex: Split-view (equivocation) detection — OPEN

This annex registers, as a named open problem of THIS work package,
what this spec deliberately does not solve. It closes the loop opened
by [atom-backend-contract.md](atom-backend-contract.md) §Open
Questions item 1, **"Split-view (equivocation) detection"**, which
assigned the problem to "the acceptance-policy spec commissioned by
Execution Model §9.7 (the same spec that owns anchor sets and
attestation thresholds)" — this document.

**The boundary, restated from where it is proven.** Per-consumer
linearizability plus chain monotonicity
(`[backend-refs-linearizable]`, `[chain-monotonicity]`) detect
regression within one consumer's own observed history and nothing
across consumers: a hostile backend can maintain N individually
consistent timelines and serve each consumer exclusively from one,
and every constraint in this spec evaluates honestly inside a single
served timeline. Everything above — verdicts, quorums, findings — is
therefore sound _relative to the state a consumer was served_.
Equivocation attacks that boundary, not any constraint above.

**The direction, named but not specified.** Detection requires
cross-consumer machinery, in one (or both) of the shapes the corpus
already points at ([Execution Model §9.7](../models/execution-model.md);
atom-backend-contract, Open Questions):

- **gossip** — consumers exchange observed chain heads out of band,
  so divergent served timelines eventually collide in somebody's
  view;
- **witness cosigning** — independent witnesses countersign served
  chain state, so a backend must compromise the witness set, not just
  its own serving path, to equivocate.

**What this spec reserves for it.** Both shapes reduce, at the policy
layer, to new evidence kinds evaluated by the same machinery this
spec defines: a gossiped head or a cosigned state observation is a
signed object whose signer a policy anchors (a natural third
`SignerRole`), and a detected split is finding-shaped state with
refusal semantics adjacent to `[trust-contested-refusal]`'s. The
`AcceptancePolicy` type is deliberately extensible at exactly those
two points (roles; rule fields). No constraint is stated now: the
detection machinery's own soundness conditions (witness independence,
gossip coverage) are unsolved here, and a normative surface over an
unsolved mechanism would be aspiration wearing constraint tags.

## Appendix A: Non-normative TOML rendering

One possible concrete rendering of an `AcceptancePolicy`; the data
model in §Type Declarations is the normative content, this rendering
is illustration.

```toml
[trust]
min_witnesses = 1          # default quorum floor
on_contested  = "refuse"   # default contested behavior

[[trust.anchors]]
signer = "owner"           # the publisher's own effective key(s)

[[trust.anchors]]
signer = "b3f1…9a"         # an independent CI farm this org operates
roles  = ["builder"]

[[trust.anchors]]
signer = "77aa…04"         # a community rebuilder: corroboration only
roles  = ["builder"]
quorum = 2                 # never sole-witness; must be corroborated

[[trust.anchors]]
signer = "c9d2…5e"         # an advisory feed's signing key
roles  = ["assertor"]

[[trust.rules]]
scope        = { role = "cc" }   # toolchain roles…
require_mode = "reproducible"    # …require reproducible atoms
min_witnesses = 2                # …and two agreeing builders

[[trust.rules]]
scope = "all"                    # everything else: defaults apply
```

## Appendix B: Worked example (non-normative)

Under the Appendix A policy, a build record `r` arrives for an atom
declaring role `cc`, signed by the community rebuilder (`77aa…04`),
at action identity `a`.

1. `r` is chain-anchored and verifies — proceed.
2. Signer `77aa…04` is anchored — proceed.
3. Fact class: derived; entry roles: `builder` — covered.
4. Rule: first match is the `role = "cc"` rule.
5. Mode: the atom's effective declared mode is `reproducible` — gate
   passes (had it been `witnessed`, verdict `refused`).
6. Contested: suppose the CI farm's record at `a` is
   `record_core`-equal to `r` — no standing divergence finding.
7. Quorum: `K = max(quorum 2, min_witnesses 2) = 2`; `r` plus the
   CI farm's equal record from a distinct anchored builder ⇒ met.

Verdict: `fact` — `r` is servable. Two contrasting turns of the same
crank: if the CI farm's record had a differing `record_core`, step 6
finds a standing divergence finding (two distinct anchored builders,
declared mode `reproducible`) and the default refuses cache service
at `a` for every record until an exit resolves it. If instead the
only corroboration were a second record signed by `77aa…04` itself,
step 7 counts one distinct signer (`[trust-threshold-independence]`)
and `r` stays `evidence`.

## Implications

- **Implementation guidance.** The policy evaluates at every
  fact-consumption point: eos's substitution path, the cache-service
  path, and resolution over asserted facts. The existing
  scheduler-layer threshold
  ([eos-network-protocol.md](eos-network-protocol.md)
  `[eos-wot-substitution-threshold]`, M-of-N over configured trusted
  builders) is a deployment-level instance of
  `[trust-threshold-rule]`; when eos consumes this spec, that
  constraint SHOULD be re-stated as an instantiation of this policy
  language rather than a parallel mechanism (doc amendment candidate,
  not performed here).
- **Testing strategy.** The Appendix B vectors seed the policy test
  battery; the P14+P17 TLC model (§Proof Obligations) is the owed
  machine check and SHOULD land as one model covering chain-side
  classification and policy-side verdicts together.
- **Registered dependencies.** `role(_)` scope binding awaits the
  kind-schema design (Execution Model §9.13); fact-class derivation
  awaits the fact-kind encoding
  ([htc-sad §6.10](../architecture/htc-sad.md)); both are named at
  their constraints.

### Open Questions

1. **Trial-attestation freshness.** Acceptance of trial attestations
   has a second dimension this spec does not fix: staleness (how old,
   how many, which executors — [Execution Model
   §9.3](../models/execution-model.md)). The signer dimension is
   covered here (trials are `builder`-role derived facts); the
   freshness parameters are a natural future `PolicyRule` extension
   once §9.3 is decided.
2. **Split-view detection** — the Annex: the machinery is open; the
   policy language's extension points for it are reserved.
3. **Anchor-set distribution.** An organization will want to ship one
   vetted `AcceptancePolicy` to many consumers. A policy is plain
   data and MAY be published as an atom — at which point the
   anchoring law applies to it like any published intent — but a
   convention for policy atoms (kind, review posture) is future work,
   deliberately outside this spec (no registry, no governance
   process).

### Scope Boundaries

This specification explicitly does NOT define:

- **The atom-side semantics** — the declared mode, what divergence
  under it is classified as, and the promotion/demotion law:
  [atom-model.md §6](../models/atom-model.md), inherited whole.
- **Intent validity** — which charters, claims, and publishes are
  valid, and who owns a label: the protocol's transaction
  verification ([atom-transactions.md](atom-transactions.md)). This
  policy judges FACTS; it grants and revokes no ownership.
- **Key management** — key rotation, revocation, thumbprint
  computation: Coz/Cyphr, below the plane (`[owner-abstract]`).
- **Chain carriage** — how findings' inputs are stored, enumerated,
  and protected:
  [atom-backend-contract.md](atom-backend-contract.md).
- **Split-view detection machinery** — the Annex.
