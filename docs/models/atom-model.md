# MODEL: Atom — the Protocol Plane

_2026-07-12. Status: v0.1 — the naming-and-trust plane given its own
formal model. The [Composition Model](composition-model.md) §0 names two
deliberate non-primitives, and the first is this document's subject:
"naming and trust (atom identity, `publish_czd`, signatures, trust
anchors) are the protocol plane _above_ the substrate… The omission is a
decision, not an oversight." This model fills the reserved seat: it does
not add a fourth primitive, it formalizes the plane the trichotomy
deliberately excluded. Substance is drawn from the landed atom corpus
(atom-sad, atom-transactions, git-storage-format, atom-sourcing — cited,
never restated) and from design decisions ratified in working session
with nrd, 2026-07-12: the composite duality (§2), the metadata partition
law (§4), the signature-anchoring law (§5), the reproducibility contract
(§6), environments-as-atoms and the system kind (§7). Doc amendments
this model obligates are listed in §8; none are performed here._

---

## 0. The plane above the trichotomy

The substrate trichotomy (storage, composition, execution — Composition
Model §0) answers "what is computed." This plane answers the questions
the substrate deliberately cannot: **what is this thing, who says so,
and who believes what** — naming, authorship, and trust. Its primitives
are the atom protocol's transactions (charter, claim, publish —
[atom-transactions](../specs/atom-transactions.md)) and the trust
semantics the execution model delegates upward (fact-hood is
signer-relative, Execution Model §3.4; the anchor format and acceptance
policy "ride on atom's existing signing machinery," §9.7).

The plane's own founding question is older than the substrate. The
derivation — Nix's unit — is simultaneously too granular and too
abstract: humans care about a package, a development environment, an
operating system, and the derivation representation distinguishes none
of them. This model's central object, the composite, is the answer to
that question, and the atom is its publishable reification.

## 1. The one-sentence model

> **The atom plane assigns every human-meaningful unit of software — the
> composite — a permanent name, a signed intent reification, and a
> single anchor point where all trust about it accumulates; everything
> the substrate computes is bound to the world of humans and keys
> through exactly this plane.**

## 2. The composite duality

A **composite** is the genus: a human-meaningful unit of software at one
of the three granularity strata the composition model defines —
**package**, **environment**, **system** (Composition Model §4). The
composite is never itself a concrete object; it exists in exactly two
reifications, one on each side of every execution edge:

```
atom         the INTENT reification: signed, published, versioned
             declaration of what the composite is and how it comes to be
             (sources + manifest + lock under one snapshot identity)

composition  the ARTIFACT reification: the content-addressed value the
             substrate produces and arranges (Composition Model §2)
```

The formal core of this duality is already proven: endpoint completeness
(Composition Model §1.1) states that every execution edge connects two
fully-determined content-addressed points — the atom snapshot on the
intent side, the composition root on the artifact side — in two
deliberately distinct content-addressed spaces (Storage Model §5), while
the edge itself stays a witnessed relation. This model adds only the
vocabulary: both endpoints are _the same composite_, reified twice. The
lock↔composition isomorphism (htc-sad §6.9 — `name → signed content
pointer`, one layer apart) is the same duality observed at the binding
level: the two reifications carry the one algebra.

**Identity names the composite, not a reification.** `AtomId = (anchor,
label)` — the abstract pair, permanent across versions, owners, and
keys (atom-sad §6.1, `[identity-content-addressed]`,
`[identity-stability]`) — names the composite through time. One
publish (one `publish_czd`) names one intent instance; one composition
root names one artifact instance. The olog's identity-stability diagram
([publishing-stack-layers](publishing-stack-layers.md) §1) commutes
unchanged under this reading.

**Closures are projections, included by determination.** The composite
"includes the formal closure of all dependencies" in a precise and
deliberately indirect sense: the atom's signed content _determines_ its
closures without containing them by value. `[lock-action-totality]`
([lock spec](../specs/lock-file-schema.md)) makes the passage total —
lock + manifest determine, by pure elaboration, every input of
`action_id`, including `atom_czd_closure_root` from the dep pins; the
build closure and the runtime closure (justified closure `J`, Execution
Model §4) are then _projections_ computed from what the composite
specifies, never free-standing objects with independent authority.
Atom identity is untouched by any of this: closure roots feed
`action_id` (Execution Model §2.4), never `AtomId`. A closure is
included the way a theorem is included in axioms.

This is what dissolves the founding question. Nix has only the artifact
reification (store paths and their unexplainable closures) and no
first-class intent reification — the derivation is a build instruction,
not a signed, human-meaningful declaration. Here the human-meaningful
unit is the identity-bearing object, at every granularity stratum.

## 3. What the plane owns per stratum

The three strata are law-bearing at the composition layer (linked
module, boundary declaration — Composition Model §4) and
schema-open at this plane (kinds — Composition Model §5; Execution
Model §9.13). The plane's own contribution is uniform across strata:
every composite, at any stratum, MAY be published as an atom, and
publication is the _only_ door from private value to public claim
(§5). Ratifications recorded in §7 make this uniformity real for
environments and systems.

## 4. The metadata partition law

htc-sad §4.2 states, for fetch pins: _"lock = intent (before the
build); metadata = fact (after the build)."_ This model generalizes
that rule to the whole plane:

> **Placement of every datum about a composite is decided by epistemic
> time.** What is knowable _before_ the build and could affect its
> result is **intent**: it lives in the atom — in the sources where it
> naturally lives (a license file, an adopted ecosystem lockfile), else
> as a signed publish-time field. What is derivable _only by building_
> — output digests, interface manifests, observation records, build
> records — is **fact**: it is appended to the atom's metadata chain,
> signed by whoever derived it.

The two sides have different laws, and the difference is load-bearing:

- **Intent is czd-covered and chain-invariant.** It participates in the
  snapshot identity (`dig`) or the signed publish payload, hence in
  `action_id` via `[lock-action-totality]`; the chain-invariant core
  `(label, version, dig, src, path)` cannot change behind a version
  (`[tag-chain-semantic-immutable]`,
  [git-storage-format](../specs/git-storage-format.md)).
- **Fact is appended, monotone, and signer-attributed.** The metadata
  chain is the concrete realization of the substrate's only state — the
  fact-set (Composition Model §6): insertion commutative and
  idempotent, several witnesses for one action a legitimate state,
  fact-hood judged signer-relative at read time (Execution Model §3.4).
  Facts never occupy identity-bearing positions (obligation P12).

The partition decides field placement mechanically. A description or
license: knowable before, intent. An expected output digest: derivable
only by building — fact, _unless_ asserted as a contract (§6, where the
assertion is intent and the evidence stays fact). An interface
manifest: fact, memoized per `(analyzer, blob)` (htc-sad §2.2). There
are no taste calls left, only classifications.

**Two hardening gaps become normative obligations of this plane.** Both
are already registered (atom-sad §9 gap 5; htc-sad §6.10) and are
design work this model now governs rather than defers:

1. **Builder ≠ owner authorization.** Facts are signed by their
   deriving executor, which is generally not the claim owner. The
   metadata chain MUST admit fact appends from signers outside the
   ownership chain while keeping them distinguishable from
   owner-signed appends — acceptance is the consumer's trust judgment
   (Execution Model §3.4), never the chain's.
2. **The fact-append carve-out.** A routine fact append MUST NOT be
   presented to consumers as an ownership-relevant event: atom-sad
   §8.6's moved-tip warning exists to surface signing/ownership
   changes, and fact traffic through the same mechanism
   (`[publish-update-transition]`) currently trips it. The carve-out
   is a fact-kind convention distinguishing appends that touch the
   trust chain from appends that only accumulate facts.

## 5. The signature-anchoring law

Signers differ across the stack — publishers sign intent, executors
sign records, analyzers' outputs are keyed by the analyzer's own czd,
composers sign compositions — but anchoring does not:

> **Every signature offered across a trust boundary is anchored to an
> atom.** It either IS atom-plane intent (a charter, claim, or publish
> transaction), or it is a fact anchored to some atom's metadata chain.
> A signed value that is neither is **local**: private state whose
> trust never leaves its holder.

The atom is thereby the single human-identifiable, machine-enumerable
anchor point of trust: to audit what is believed about a composite, one
walks one chain, whoever the signers were. The classification is total
(obligation P13):

| Signed object                                | Class                                            |
| :------------------------------------------- | :----------------------------------------------- |
| Charter, claim, publish                      | intent (the plane's own transactions)             |
| Build record, interface manifest, observation record | fact — appended atom metadata (htc-sad §6.10) |
| Lock, adopted ecosystem lockfile             | intent — inside the snapshot, czd-covered         |
| Environment certificate, published           | intent — the environment atom's pinned elaboration (Composition Model §5) |
| Composition, published as/within an atom     | intent — content of an environment/system atom    |
| Composition or certificate, unpublished      | **local** — a dev shell, a machine's own root     |
| Promotion (attestation → lock entry)         | intent — lands czd-covered on the next publish    |

The local class is not an exception to the law but its boundary
condition: an unpublished composition is its holder trusting themself,
and the moment its trust claim must travel — a binary offered for
substitution, an environment offered for reuse — the value must cross
the publication door and acquire an anchor. There is deliberately no
third path; "trust me, it's signed" with no anchor is structurally
unexpressible.

## 6. The reproducibility contract

The execution model settles what reproducibility _is_ — an accumulating
attestation on an action, empirical, never proven (§2.3), with
multiplicity of witnesses a legitimate state (§2.2) — and delegates to
this plane what it _means to consumers_ (§3.4, §9.7). This model
supplies the atom-side half of that commissioned semantics; the
policy-language spec inherits it.

**The mode is declared, signed intent.** A publish carries a
reproducibility mode:

```
mode ∈ { reproducible, witnessed }        (default: witnessed)
```

- Under **witnessed**, nothing changes: records accumulate
  multi-valued, cache hits serve any trust-acceptable witness
  (Execution Model §2.2).
- Under **reproducible**, the publisher asserts: for every action this
  atom denotes, fixed `action_id` ⇒ `record_core`-equal records
  (Execution Model §3.1 — equality over `(req_digest, exit_code,
  outputs)`, never full records). The claim quantifies over _actions_,
  not the atom in the abstract: outputs legitimately vary across
  toolchain closures, so the contract binds per `action_id`, which is
  exactly what accumulated build records already carry (htc-sad §2.3).

**Violation is a defect finding, not a datum.** Under the declared
mode, a `record_core`-divergent record from a _trusted_ signer at a
fixed `action_id` is never absorbed into the witness set as one more
observation. It is a contradiction between signed intent and signed
fact, and it demands adjudication: either the claim is wrong (the
publisher amends the mode, signed) or something is compromised — a
builder, a toolchain, the atom's own determinism assumption. Trust
policies MUST be able to refuse cache service on the contradicted
action while it stands. This is the entire point of declaring: without
the declaration, a trojaned artifact and benign nondeterminism are
indistinguishable — the alarm semantics is what the signature buys.

**Emergence feeds declaration through the promotion door.** Witness
convergence (k `record_core`-equal records from independent signers —
the hardening mechanism of Execution Model §3.4) is _evidence_, and
evidence crosses into intent only over a signature (Execution Model
§3.3, the plane-wide promotion law). A publisher holding convergence
evidence MAY promote `witnessed → reproducible`; adjudication of a
violation MAY demote `reproducible → witnessed`. Both transitions are
signed appends on the publish chain — auditable, never silent
(obligation P14).

**Consumers compose policy on declarations, not statistics.** A policy
like "toolchain roles require `reproducible` atoms" is sound because
the mode is signed intent with defined violation semantics; the same
policy over emergent witness counts would be a moving target with no
accountable party.

## 7. Kinds, strata, and the recorded ratifications

Strata are closed and law-bearing; kinds are open and schema-level
(Composition Model §5). This model records two decisions, ratified
2026-07-12, as normative content of the plane:

1. **Environments-as-atoms: yes** — resolving Execution Model §9.10.
   An environment MAY publish as a signed atom whose content is the
   composition and whose pinned elaboration is the certificate; ion's
   existing version machinery covers it with nothing new invented
   (Composition Model §4, "Versioning"). This is what makes the
   composite duality uniform at the environment stratum, and it is
   load-bearing for §5: publication is how an environment's trust claim
   travels.
2. **The system kind stands** — the boundary-declaration artifact of
   Composition Model §4, publishable as an atom like any composite
   (the system atom is already the canonical mixed-pipeline case,
   Composition Model §5).

The kind discriminator's _schema shape_ (tables vs field, mixed-atom
hygiene) remains open (Execution Model §9.13) — a manifest/lock
redesign concern, not a law of this plane.

## 8. Doc amendments this model obligates

Recorded here so the reconciliation sweep has a manifest; none are
performed by this document:

- **execution-model.md §9**: item 10 (environments-as-atoms) →
  RATIFIED yes, per §7; item 7 → note that the atom-side acceptance
  semantics (mode, violation, promotion) is specified by this model.
- **atom-transactions.md**: the reproducibility mode as a
  protocol-defined publish field (§6) — root-level protocol namespace,
  not `meta.*` (root keys are reserved for protocol fields; the mode
  is one); the fact-append signer-authorization and carve-out
  obligations (§4).
- **git-storage-format.md `[tag-chain-semantic-immutable]`**: the
  mode joins the chain-_variable_ class (it is promotable/demotable
  per §6), which the constraint's variance list must name explicitly.
- **atom-sad.md**: §6 gains the plane framing (composite duality,
  anchoring law); §9 gap 5 narrows from open design to the P12/P13
  obligations and carve-out convention this model states.
- **htc-sad.md §6.10**: cite this model as the governing law of the
  fact-publication channel.

## 9. What this model deliberately does not own

- **The substrate.** Storage identity, composition algebra, execution
  semantics — the three sibling models. This plane consumes their
  values and laws; it defines who names and who believes.
- **Resolution and the lock schema.** Ion's algebra and the lock spec
  instantiate the plane's laws (`[lock-action-totality]`,
  `[lock-set-charter-head]`); this model cites them as discharged
  obligations, never re-derives them.
- **Backend mechanics.** How a backend stores transactions, walks
  chains, and enforces ancestry is the backend contract's subject — a
  companion specification (planned as `docs/specs/atom-backend-contract.md`)
  axiomatizing what ANY content-addressed VCS must provide to host
  this plane, with the git backend
  ([git-storage-format](../specs/git-storage-format.md)) as its
  reference instantiation.
- **Key management and identity frameworks.** Below the plane
  (Coz/Cyphr); `owner` stays opaque (`[owner-abstract]`).
- **The acceptance-policy language.** Execution Model §9.7's spec
  remains future work; this model fixes only its atom-side semantics
  (§6).

## 10. Proof obligations

Continuing the substrate-wide P-numbering (P1–P11 are homed in the
three substrate models):

- **P12 — metadata-partition well-formedness.** Every field of the
  publish payload, the chain-variable set, and every fact kind on the
  metadata chain is classifiable under §4 (intent xor fact), and no
  post-build fact occupies an identity-bearing position: nothing in
  the chain-invariant core `(label, version, dig, src, path)` and no
  input of `action_id` derives from a fact. An audit obligation with a
  checkable inventory, P5/P10-style — the schema is finite; enumerate
  and classify it.
- **P13 — anchoring totality.** The signed-object classification of §5
  is total and stays total: every signed object class in the corpus is
  intent, anchored fact, or local, and any new signed object class
  names its row before landing. Audit obligation over a checkable
  inventory; the table in §5 is its initial state.
- **P14 — reproducibility-contract coherence.** (a) Mode transitions
  occur only as signed publish-chain appends (promotion/demotion per
  §6) — no unsigned or out-of-chain path can alter the mode a consumer
  sees; (b) under `reproducible`, a `record_core`-divergent trusted
  record at fixed `action_id` is classified a defect finding and is
  not servable as an ordinary cache hit while unadjudicated; (c) under
  `witnessed`, semantics are exactly Execution Model §2.2 (this
  contract adds nothing). Small state-machine model, TLC-able in the
  AtomCharter style, owed when the acceptance-policy spec lands; until
  then the classification stands as specification.

Closure determination deliberately contributes **no new obligation**:
`[lock-action-totality]` and the lock spec's formal requirements
(`[lock-sufficiency]`, `[lock-groundness]`,
`[lock-closure-completeness]`) already carry it, and P7 (Execution
Model §8) already gates the arrow. This model only names what they
jointly mean: the closure is a projection of the composite.
