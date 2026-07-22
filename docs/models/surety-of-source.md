# MODEL: Surety of Source — the Source-Verification Ceiling

_2026-07-22. Status: v0.1 — the source-level verification ceiling of
the atom plane, given its own formal model. This document defines the
objects (dependency closure, classification, trust surface, assumption
basis, `Total`, the source-class gate, the forced generator), states
the ceiling theorems over them, and reports the machine-checked safety
result. Substance is drawn from the landed atom corpus
([Atom Model](atom-model.md), [ADR-0007](../adr/0007-atom-version-integrity-system.md),
[trust model](../specs/trust-model.md),
[lock spec](../specs/lock-file-schema.md) — cited, never restated).
The normative decisions this model grounds — the source-class-vouch,
vouch anchoring, and the trust-surface accounting — are recorded in
[ADR-0008](../adr/0008-surety-of-source.md). Every claim below names
its evaluator: proof, the Alloy model checker, or prose argument —
never an unmarked assertion._

---

## 0. What this models

The question this model answers precisely: **what can a consumer
soundly conclude, from signed records alone, about whether everything
an atom's build consumed was genuine, inspectable source — and what
can no verifier ever conclude?** The answer has two halves, and both
are stated as theorems: a decidable half (hard gates reject exactly
the syntactically detectable launderings, and nothing more is soundly
decidable), and an accounting half (everything past the decidable
frontier is booked, fail-closed, into an enumerated trust surface —
nothing is silently trusted). The model exists so that the system's
central verdict, `Total`, is a machine-derived function of signed
evidence with a precisely bounded meaning, rather than a slogan.

## 1. The dependency closure

The ratified objects this model builds on are used unchanged and in
their glossary senses: **Record**, **czd**, **charter / claim /
publish**, **fact** (with `pay.fact_type`), **Record closure**, and
**build record** — see the [glossary](../glossary.md),
[ADR-0007](../adr/0007-atom-version-integrity-system.md) §2–§3, and
[Atom Model](atom-model.md) §4–§6.

> **Definition (dependency closure).** The **dependency closure** of
> an atom `a`, written `depclosure(a)`, is the least set of artifacts
> (atoms and raw fetched byte-payloads) closed under the "is a build
> input of" relation starting from `a`'s build record: the build
> record names an input-closure; each atom-typed input contributes its
> own build record's inputs, recursively; each non-atom input (a
> promoted fetch pin, an adopted-lockfile pin, any content-addressed
> byte-payload) contributes itself as a leaf. A **closure member** is
> any element of `depclosure(a)`. `depclosure` is reflexive: `a` is a
> member of its own dependency closure.

Concretely, the dependency closure is what walking the lock's
requires-graph plus its promoted fetch pins transitively yields
([lock spec](../specs/lock-file-schema.md)). It is structurally
distinct from the glossary's **Record closure**, and the two are never
conflated — bare "closure" is barred in this model's vocabulary:

|             | Record closure                                        | Dependency closure                                         |
| :---------- | :---------------------------------------------------- | :--------------------------------------------------------- |
| Scope       | one atom                                              | transitive over many atoms and raw payloads                |
| Shape       | fixed 2-leaf `eml` tree                               | open-ended DAG, unbounded depth                            |
| Members     | charter, claim, publish, content of _this_ atom       | other atoms' records _and_ non-atom fetched payloads       |
| Realized by | the closure artifact at the publish ref (ADR-0007 §7) | the lock's requires-graph + fetch pins, walked recursively |

Dependency closures are acyclic by construction: a build input is a
czd/hash reference to an already-fixed artifact, so no artifact can
(transitively) name itself as its own input. The classification below
depends on this acyclicity, and the machine-checked model (§10)
demonstrates that it is load-bearing, not decorative.

## 2. The classification function

> **Definition (classification).** `classify : Artifact → Bucket` is a
> **total** function assigning every closure member exactly one
> bucket:
>
> ```
> Bucket ::= ReproducibleCASource   -- closed: build record, established
>                                      source class, empirically
>                                      corroborated reproducible plan,
>                                      content-addressed output,
>                                      recursively closed inputs
>          | AttestationResidue     -- build record + established source;
>                                      fails corroboration, CA output,
>                                      or recursive closure
>          | TrustImport            -- no build record; at most a signed
>                                      vouching over a digest
>          | SourceClassResidue     -- sourcehood claimed but NOT
>                                      established: self-declared class
>                                      only, no admitted vouch, no
>                                      gate-executable class, or a
>                                      hard-gate failure
> ```
>
> `classify` is evaluated against two explicit parameters (§7): the
> consumer's **admission policy `P`** (which signers count as
> corroborating builders and as source-class vouchers) and the
> **evidence snapshot `σ`** (which signed records — build records,
> vouches, retractions — exist at evaluation time). Given `(P, σ)`,
> `classify` is a deterministic, machine-derived function of signed
> records.

**Fail-closed default (the load-bearing clause).** A member is
`ReproducibleCASource` **only** when every positive condition below is
affirmatively established by the closure walk; a member the walk
cannot so establish falls, by default and with no third option, into a
residue bucket. There is no "unclassified", "pending", or "unknown"
state — unclassifiability is resolved _to a residue bucket_ by the
default.

**Membership.** A member `m` is `ReproducibleCASource` iff all five
positive conditions hold — written `Established_RCAS(m)`:

- **(i)** `m` is an atom carrying a build record;
- **(ii)** the build record's plan is declared `reproducible`
  ([atom-transactions](../specs/atom-transactions.md)
  `[publish-mode]`) **and** the declaration is empirically
  corroborated: at least one independent `record_core`-equal rebuild
  from a distinct policy-admitted builder exists
  ([trust model](../specs/trust-model.md) `[trust-threshold-rule]`;
  [Atom Model](atom-model.md) §6) — see §6;
- **(iii)** the output is content-addressed by the digest of its
  bytes;
- **(iv)** `m`'s declared source class is **established** (§5): its
  input tree passes the hard gates against a gate-executable declared
  class, **and** at least one anchored, unretracted source-class-vouch
  for that (member, class) pair from a policy-admitted voucher exists
  in `σ`. Gate passage against a self-declared class is NOT sufficient
  on its own;
- **(v)** recursively, every member of _`m`'s own_ dependency closure
  is itself `ReproducibleCASource` or a permanent genesis seed (§4).

The bucket has no other entry path:

```
classify(m) = ReproducibleCASource  ⇔  Established_RCAS(m)
```

This biconditional is the single classification function; the
membership conditions here and the vouch mechanics in §5 are two views
of one predicate, never two predicates.

**Residue precedence.** When a member is not `ReproducibleCASource`,
its bucket is the **first** matching clause in this fixed order, so a
member is booked against the deepest reason its verification did not
close:

```
1. no build record at all                  → TrustImport
2. sourcehood not established (§5):
   hard-gate failure, no gate-executable
   class, or no policy-admitted vouch      → SourceClassResidue
3. otherwise (build record + established
   source, but (ii), (iii), or (v) fails)  → AttestationResidue
```

A member with no build record has nothing to attest and no tree the
gate ran over (a raw fetched payload), so clause 1 fires first; an
unestablished sourcehood is a deeper defect than an established-source
build whose only gap is determinism, so clause 2 precedes 3. A
hard-gate failure lands in clause 2 rather than ejecting the member
from the domain: gate failure additionally triggers downstream
_refusal_, but refusal is a policy consequence — classification stays
total. A clause-3 (v)-failure cascades: every ancestor resting on a
dirty member books in residue too, which is harmless for `Total` and
load-bearing for the accounting a consumer reads when deciding what
moves if a signer is revoked.

**Totality as a checkable predicate.**

```
Exhaustive(a)  ≜  ∀ m ∈ depclosure(a) :
                    exactly one bucket holds for m
                  ∧ ( ¬ Established_RCAS(m)
                       ⇒ classify(m) ∈ {AttestationResidue,
                                        TrustImport,
                                        SourceClassResidue} )
```

The second conjunct is the fail-closed law: negation of the positive
conditions _forces_ a residue bucket. `Exhaustive` is the safety
invariant the machine-checked model asserts for every atom in scope
(§10); a counterexample would be a member escaping all four buckets.

## 3. The trust surface and the assumption basis

> **Definition (trust surface — residue set).** The **trust surface**
> of an atom `a` is the union of the three residue buckets over its
> dependency closure:
>
> ```
> T(a)  ≜  { m ∈ depclosure(a) : classify(m) ∈
>            {TrustImport, AttestationResidue, SourceClassResidue} }
> ```
>
> Equivalently, `T(a)` is `depclosure(a)` minus its
> `ReproducibleCASource` members. It is machine-derived by the closure
> walk, never testimony. The three residue buckets are the three ways
> source-level verification fails to close, kept distinct because
> their evidentiary posture differs: a vouched binary, a
> non-deterministic build, a self-classified tree.

> **Definition (assumption basis).** The **assumption basis** of an
> atom `a`, written `B(a)`, is the derived set of policy-admitted
> signature evidence that the walk's _non-residue_ classifications
> rest on:
>
> ```
> B(a)  ≜    { corroborating build-record signatures counted for each
>              ReproducibleCASource member of depclosure(a) }
>          ∪ { admitted source-class-vouch signatures counted for each
>              established-source member of depclosure(a) }
>          ∪ { the genesis-seed identities the closure grounds in }
> ```

`B(a)` is **the enumerated, attributable, policy-admitted trust the
verdict rests on**. It is computed by the same closure walk that
computes `T(a)`: machine-derived, and complete **over
admission-dependent evidence** — every signature whose policy
admission the verdict depends on appears in it. Its two signature
classes are distinct evidence species, and a basis reading keeps them
apart: a counted corroboration is a **derived** fact — a re-runnable
execution record, falsifiable by `record_core` comparison — while a
counted vouch is an **asserted** fact — pure keyed judgment, with no
`record_core` for independent parties to agree on ([Atom
Model](atom-model.md) §4). A basis reads "3 corroborations + 2
vouches", never "5 signatures". `B(a)` is exactly what a consumer who
ceases to admit a signer must recompute.

**The verification floor.** `B(a)` does not (and does not claim to)
enumerate the record-verification substrate the walk itself
presupposes: the hash and signature schemes under which every czd and
every signature verifies at all, the verifier's own implementation,
and the verification of the genesis records themselves. Those are the
protocol's floor — the assumed base every classification sits above —
not admissions of `P`. Every claim in this model is scoped above that
floor.

## 4. Total and the genesis seeds

> **Definition (genesis seed).** A **genesis seed** is a permanent
> bootstrap trust-import at the base of a toolchain lineage (a
> stage0/hex0-style tiny binary). It is an in-protocol, permanent
> member of `TrustImport`; each subsequent toolchain generation is an
> ordinary `ReproducibleCASource` output of a build record referencing
> the previous one. `GenesisSeeds` is the set of all such seeds a
> dependency closure grounds in — one per not-yet-unified toolchain
> lineage (C's hex0 line, Rust's blessed prebuilt compiler, and so
> on), so `|GenesisSeeds| = N`, never assumed to be 1.

> **Definition (Total).** An atom `a` is **Total** iff its trust
> surface contains no members other than permanent genesis seeds:
>
> ```
> Total(a)  ≜  T(a) ⊆ GenesisSeeds
> ```
>
> — equivalently, every non-seed closure member is
> `ReproducibleCASource`.

The subset form is deliberate: every steady-state closure's regress is
walkable to a genesis seed, which correctly _is_ a `TrustImport` (it
fails the source gate, and must), so `T(a)` is never empty for real
software and an empty-surface definition would be unsatisfiable.
`Total` does not mean trust reduced to zero. It means the residue is
reduced to the permanent, named genesis seed(s), and every other grain
of trust the verdict admits is located: named, signed,
policy-admitted, enumerated in the assumption basis `B(a)` — above the
protocol's verification floor (§3), which the verdict presupposes
rather than enumerates.

**The two senses of "total" — never conflate them.** The word does
duty for two different mathematical claims, and every statement in
this model lives in exactly one of them:

1. **`classify` is a total function — the safety sense.** Every
   closure member lands in exactly one bucket, fail-closed, with or
   without any vouch ever having been issued. The safety theorem —
   no laundered member ever classifies as `ReproducibleCASource`, and
   no atom resting on one presents as `Total` (§10) — holds in this
   sense alone. Under a policy admitting no vouchers, the theorem is
   still sound; real software is simply all residue.
2. **`Total(a)` is an atom-level predicate — the satisfiability
   sense.** Whether any real (non-seed) atom can actually satisfy
   `Total` is a different question, and the answer is: only because
   establishment (§5) is reachable — that is, only because the
   source-class-vouch exists. Without it, every real member's
   sourcehood rests on self-declaration, lands in
   `SourceClassResidue`, and no real atom is ever `Total`: the
   predicate would be vacuously safe and empirically empty.

Sense 1 is a theorem about the classifier; sense 2 is a reachability
fact about the predicate. The vouch mechanism is load-bearing for
sense 2 while contributing nothing to (and needing nothing from) sense
1's soundness. Both facts are machine-checked (§10).

## 5. The source-class gate and the source-class-vouch

### 5.1 The gate

The gate runs an atom's **declared source class** against its **input
tree** (never the output). Three checks, two decision tiers:

- **(a) format gate — hard fail.** Executable/object container
  formats (ELF, PE, Mach-O, wasm, JVM class files, pyc — magic-byte
  detectable, whole-file scan regardless of offset) present in a
  claimed-source tree fail the gate.
- **(b) parse gate — hard fail.** Tree contents must lex/parse in the
  declared class, else fail.
- **(c) opacity and plan-emission correlation — soft flag.** Large
  high-entropy literals or a high blob ratio _flag_ the tree; and if
  the build output's bytes are recoverable from literals present in
  the input tree, the "build" was emission (copy), not compilation.
  Gate (c) contributes evidence, never a decision — a consequence of
  Theorem 1 (§9.1): promoting it to a hard gate would require deciding
  an undecidable property or accepting unsoundness.

A class is **gate-executable** for a verifier when that verifier
carries a format profile and parser adapter for it — a property of the
verifier, not a protocol registry. A declared class that is not
gate-executable cannot pass gate (b) (nothing can parse it), so its
atom's sourcehood cannot be established and it falls to
`SourceClassResidue`: the open-world hole closes by fail-closure, with
no enumerated class registry. Evaluator: gates (a)/(b) are decidable
by construction (P1, §9.2); the fail-closure routing is part of the
machine-checked classification (§10).

### 5.2 Why a vouch is required at all

The source-class declaration in the landed corpus is
`ClaimPayload.pkg` ([atom-transactions](../specs/atom-transactions.md)):
a free-text PURL-type string, signed by the claim owner — the same
party who publishes the (possibly laundering) atom — whose documented
purpose is version-dialect and fetch-adapter selection, not source
attestation, and which is open-world. Three defects follow: it is
self-signed (testimony by the checked party); it is repurposed (a
dialect-selection field read as an attestation); and it is open-world
with no default-deny (an unregistered class string for which no
adapter exists must not silently skip the gate). The third defect is
closed by gate-executability fail-closure (§5.1). The first two are
closed by the vouch: source attestation moves into a separate signed
record whose signer is chosen by the consumer's evidence policy, not
by the checked party's own pen. `ClaimPayload.pkg` keeps its
documented dialect/fetch purpose untouched.

The residual frontier — a tree that parses in a permissive-but-wrong
declared class — is not mechanically closable in general (choosing a
parse-compatible class is undetectable syntactically), which is
precisely why closing it takes a _signature_ rather than a gate, and
why the closing signature is itself booked in `B(a)` rather than
vanishing from the accounting.

### 5.3 The source-class-vouch and establishment

The **source-class-vouch** is a signed record on the existing fact
mechanism — `pay.typ = "fact"`,
`pay.fact_type = "source-class-vouch"`, a version-scope fact naming a
`target` (the vouched publish czd) and a `class` (the vouched source
class). Its meaning: _the signer asserts that the target version's
input tree is genuine source in that class — authored in it, not a
laundering container._ It is an **asserted** fact in the
derived/asserted partition ([Atom Model](atom-model.md) §4): accepted
for a consumer iff its signer is admitted by that consumer's policy
with the `assertor` role; no quorum applies
([trust model](../specs/trust-model.md) `[trust-threshold-rule]`), so
one admitted, unretracted, **anchored** vouch establishes. Issuance is
unrestricted at the protocol level — any keyed party may sign one,
including the publisher itself (the degenerate self-vouch) — and which
vouchers count is downstream admission-control policy. The normative
mechanics (anchoring requirement included) are
[ADR-0008](../adr/0008-surety-of-source.md)'s decision; this model
consumes them.

For a member `m` with declared class `c`, under policy `P` and
snapshot `σ`:

```
established(m)  ≜  gateExecutable(c)
                 ∧ passesFormatGate(m) ∧ passesParseGate(m)
                 ∧ ∃ v ∈ σ : v.fact_type = "source-class-vouch"
                       ∧ v.target = publish-czd(m)
                       ∧ v.class  = c
                       ∧ v is anchored and not retracted in σ
                       ∧ signer(v) ∈ P.admittedVouchers
```

A member with `established(m)` and a build record proceeds past
precedence clause 2 to the reproducibility conditions; a member
without it — self-declared only, vouched only by non-admitted signers,
vouched for a different class than declared, gate-failing, or
classless — stays in `SourceClassResidue`. Establishment is
per-(member, class) and per-consumer: the same atom can be established
for one consumer and residue for another (§7).

**Every counted vouch enters the accounting.** A vouch counted by
`established(m)` for any member of `depclosure(a)` enters `B(a)`: the
trust the verdict rests on is enumerated, signed, and attributable —
moved from an invisible self-declaration to a named signature — never
erased from the books. A voucher who vouches a laundering tree has
signed a permanent, czd-addressed record of that judgment.

## 6. The reproducibility mode is empirically grounded

The `ReproducibleCASource` bucket turns on condition (ii), and the
reproducibility mode is a signed self-declaration
([Atom Model](atom-model.md) §6) — the same self-signed shape as the
source class: a totality input asserted by the party being checked. It
is repaired by the same discipline: **a `mode = reproducible`
declaration alone does not admit a member to the closed bucket.**
Admission requires the declaration _plus_ independent corroboration —
at least one `record_core`-equal rebuild from a distinct
policy-admitted builder ([trust model](../specs/trust-model.md)
`[trust-threshold-rule]`). A member declared reproducible but not yet
corroborated falls, fail-closed, into `AttestationResidue`. Membership
is determined by what independently happened, not by what was
declared; otherwise a laundering publisher would present as `Total` on
testimony. The declaration still does real work — it is signed intent
with defined violation semantics, enabling the
`[trust-mode-rule]` alarm — but it gates _policy_, not _bucket
membership_.

**What "empirical" can and cannot mean.** The corroboration is
machine-checkable only down to the independence floor
(`[trust-threshold-independence]`): distinct thumbprints, no
double-counting. Whether two distinct keys are _genuinely_ independent
— distinct operators, distinct infrastructure, non-Sybil — is not
machine-checkable; it is the consumer's curation judgment, for
builders exactly as for vouchers. "Empirically grounded" means the
evidence is signed records of what happened, never that the
independence of their signers is itself a theorem.

## 7. What every verdict is relative to

> **Relativity statement.** `classify`, `T(a)`, `B(a)`, and `Total(a)`
> are functions of `(depclosure(a), P, σ)`: the consumer's admission
> policy `P` (which signer keys count as corroborating builders and as
> source-class vouchers, and any corroboration quorum it demands) and
> the evidence snapshot `σ` (which signed records exist, net of
> retractions, at evaluation time). There is no policy-free, timeless
> `Total` for real software, and this model does not pretend one
> exists.

This does not break "derived, not asserted" — the property the trust
surface exists to guarantee. Given `(P, σ)`, the classification is a
deterministic, machine-derived function of signed records; no unsigned
claim enters anywhere, and the checked party's self-declarations
(class, mode) gate nothing by themselves. `P` decides _which signers
count_, never _what the records say_ — the same sovereignty a consumer
already exercises in anchor curation. The contrast that matters is
**asserted-Total vs derived-Total**, not relative vs absolute: every
alternative system's "trusted" verdict is also relative to somebody's
policy; the difference here is that the policy surface is explicit and
minimal, and the entire remainder is machine-derived with the residue
(`T(a)`) and basis (`B(a)`) enumerated.

## 8. The forced generator and degeneracy

### 8.1 The objects

> **Definition (admissible-tree language).** For a gate-executable
> declared class `c`, `L_c` is the set of trees the hard gates admit:
> those passing the format gate and parsing in `c`.

> **Definition (forced generator).** When a laundering atom's
> committed tree evades the hard gates yet its build plan reconstructs
> a chosen target binary `B`, the tree necessarily contains a program
> `G` — committed, permanently recorded, signed, attributable to a key
> — whose execution under the atom's build plan produces `B`.
> (Forcing lemma: under hermeticity (A1, §9.2), `B`'s bytes are the
> image of committed closure bytes under the plan's composed programs;
> if no committed literal carries them, a committed program computed
> them. Evaluator: prose dataflow argument from A1.)

> **Definition (degeneracy).** A generator is considered with its
> nominal, unbounded input signature; `[G]` is the partial function it
> computes. `G` is **degenerate** iff `range([G])` is finite —
> emission from a finite table, uniformly covering the constant
> emitter and the lookup-table emitter — as opposed to a genuine
> parameterized transformation, whose range over an unbounded domain
> is infinite. The unbounded domain is load-bearing: over any finite
> domain every function is a lookup table and the property
> trivializes.

> **Definition (laundering — operational scope).** **Laundering** is
> presenting an atom whose output bytes were not derived, via the
> committed plan, from inspectable committed source: a smuggled
> container, a non-parsing payload, a literal emission, or a computed
> emission through a degenerate generator. Explicitly outside this
> definition: committing genuinely modifiable, parse-valid source that
> was mechanically derived from someone else's binary (decompilation).
> That tree _is_ inspectable source; what it violates is historical
> authorship, which is not a function of the bytes and not adjudicable
> by any gate. The construction's guarantee is therefore
> **inspectability and attributability of everything the build
> consumed — never historical authorship**; the decompiled case is
> exactly what the human-adjudicated, attributably-booked vouch
> channel (§5.3) exists to judge.

**Which object is adjudicated.** Degeneracy is a property of a
_chosen (generator, input) factoring_ of the build, and the factoring
is not canonical: a launderer can commit a nullary program that makes
the whole build one closed constant term, or scatter the
reconstruction across stages. The impossibility (§9.1) is
factoring-independent — every factoring of a laundering build contains
the undecidable question somewhere, and an obfuscated factoring only
hardens it — so no theorem below depends on which factoring is chosen,
and no claim below is about the historical origin of any factoring.

### 8.2 Why degeneracy is a valid undecidability target

Rice's theorem requires a **semantic** property: a property of the
function computed, invariant under every I/O-preserving rewrite, and
non-trivial. Degeneracy qualifies on all three counts: finiteness of
`range([G])` depends only on `[G]`; degenerate emitters and genuine
transformations are both non-empty classes; neither is all programs.
The contrast object fails the precondition, and the distinction
carries the whole model: **"is this tree genuine source" is not a
valid undecidability target** — it is not a function of the bytes at
all (two byte-identical trees can differ in genuineness by unrecorded
history), and even as a bytes-predicate it is representational, not
extensional (minified and pretty-printed JavaScript compute the same
function with opposite source-form status). No claim in this model
ranges over "sourcehood of a tree"; the undecidable question is asked
of the forced generator, the one object it is well-posed for.
Evaluator: proof (Rice's theorem, by citation; the exclusion is a
well-definedness argument).

### 8.3 The witness (non-vacuity)

"Forces a generator" would be vacuous if no laundering attempt
provably reached the generator stage. One does:

> **Witness (compressed-blob generator).** Target: a ~100 KB ELF
> binary `B`. The launderer ships an atom declaring source class C and
> committing one file `gen.c` containing a
> `static const unsigned char z[]` holding the zlib-compressed bytes
> of `B`, and a `main` that inflates `z` to stdout. The build plan
> compiles and runs it; the captured stdout — `B` — is the output.
>
> Gate accounting: **(a)** passes — the tree contains no
> executable-container magic bytes; `z` is a zlib stream, not a
> container. **(b)** passes — `gen.c` is valid C. **(c)** does not
> hard-fail — `B`'s bytes are not present as a literal (only the
> compressed form is; `B` is the _computed_ output of `inflate`), so
> the emission-correlation clause has no literal to correlate, and the
> opacity flag is soft. The only channel through which `B` entered is
> the committed `inflate(z)` program: the forced generator is real,
> and the residual question — genuine parameterized decompressor, or a
> hand-rigged one-shot emitter with finite range — is exactly the
> degeneracy property. Non-triviality holds on the witness's own
> program family: the rigged emitter is degenerate, stock `inflate` is
> not, and both are expressible in C.
>
> The witness also closes a dilemma: to catch it, the
> emission-correlation check would have to _execute_ `inflate(z)` and
> diff the result against the output — but executing the tree's
> program to recover the output is running the generator, which
> concedes that the generator is the object under adjudication. Either
> horn delivers the launderer to the generator stage.

Under the vouch mechanism the witness's fate sharpens: either no
admitted voucher signs its class claim — it sits in
`SourceClassResidue` and never presents as `Total` — or one does, and
the undecidable residue is pinned to a named, permanent, czd-addressed
signature in `B(a)`. The undecidability does not move; the
accountability for accepting it does. Both outcomes are exercised in
the machine-checked model (§10).

## 9. The ceiling theorems

Four claims in three epistemic registers, deliberately kept apart:
one impossibility theorem, one conditional maximality theorem, one
completeness invariant, one economic argument. The two optimality axes
(decidable rejection; forced attributability) are non-comparable — a
statement in recursion theory and a finite relational invariant share
no scale — and are never bundled into one claim.

### 9.1 Theorem 1 — generator impossibility

> **Theorem 1.** Let `c` be a declared source class whose language is
> Turing-complete and whose generators are admitted without
> restriction. Then:
>
> **(i)** `{ G : range([G]) is finite }` is undecidable — no algorithm
> decides, for an arbitrary committed generator, whether it is a
> degenerate finite-table emitter or a genuine parameterized
> transformation; and
>
> **(ii) (promise refinement)** undecidability persists on the
> verifier's actual epistemic position: it remains undecidable on the
> promise subclass `{ G : G(z) = B }` of generators already observed
> to emit the atom's output `B` on the committed input `z`. Knowing
> the one input/output pair the build exhibited buys the verifier
> nothing.

Proof: (i) is Rice's theorem applied to the index set of the
finite-range partial computable functions (extensionality is immediate
from the definition; non-triviality from §8.3). (ii) is a reduction
from the halting problem into the promise class: given machine `M` and
input `x`, define

```
G_{M,x}(w)  ≜  if w = z then B
               else: simulate M(x) for |w| steps;
                     if halted, output B, else output w
```

Then `G_{M,x}(z) = B` always, so the promise holds; if `M` halts on
`x`, `range([G_{M,x}])` is finite (degenerate); if `M` never halts,
`[G_{M,x}]` is the identity off `z` — infinite range. A degeneracy
decider on the promise class therefore decides halting. Evaluator:
proof — (i) by citation, (ii) by the reduction above.

Consequence for the construction: gate (c) cannot be promoted to a
hard gate (§5.1), and what the construction does instead is force `G`
into existence as a permanent, signed, czd-addressed, inspectable
artifact — converting an undecidable question into preserved evidence
(Theorem 3). Falsification signpost: a decision procedure for
finite-range degeneracy over the promise class refutes Theorem 1 as
stated.

### 9.2 Theorem 2 — maximal sound decidable rejection (conditional)

Named premises, assumed not proven, each with a falsification
signpost:

> **P1 (gate decidability).** Gates (a)/(b) are decidable for
> gate-executable classes: container-format detection and parsing are
> algorithmic. _Signpost: a gate-executable class with undecidable
> parse membership falsifies P1 for that class (fail-closure already
> excludes non-gate-executable classes, so this degrades scope, not
> soundness)._

> **A1 (hermeticity).** An atom's output bytes are a computable
> function of its committed closure bytes and plan alone — no side
> channel injects bytes the closure does not carry. This is what the
> build substrate enforces; it is an assumption about the executor.
> _Signpost: a build demonstrably producing output bytes not derivable
> from committed inputs falsifies A1 (an executor defect, not a
> theorem defect)._

> **A2 (provenance-realizability).** Every admissible atom — every
> `(t, p, B)` with tree `t ∈ L_c` and `B` the hermetic result of
> running plan `p` over `t` — is realizable as genuinely authored:
> some possible history honestly wrote exactly these bytes as source
> in `c` and honestly ran this plan. Equivalently: genuineness is not
> a computable function of the atom's bytes. _Signpost: exhibiting an
> admissible hermetic triple that provably cannot be genuine would
> falsify A2 — and would simultaneously license a new hard gate on
> exactly that set, advancing the syntactic frontier without changing
> the theorem's form._

> **Theorem 2.** Call a verifier **sound** when it never rejects a
> realizable-genuine atom, and **unrestricted** when it admits every
> generator expressible in the declared Turing-complete class. Under
> P1, A1, A2:
>
> **(i) (upper bound)** for every unrestricted sound verifier, every
> decidable rejection predicate rejects only atoms whose committed
> source tree is not in the declared class's language `L_c`. The
> quantification is over atoms rejected _by inspection of the
> committed tree_: A1 makes the output a function of (tree, plan), so
> rejection for output mismatch against the committed plan is an
> executor-integrity check outside this space, not a counterexample
> inside it.
>
> **(ii) (achievement)** gates (a)/(b) decide membership in the
> complement of `L_c` exactly.
>
> Hence the hard-gate tier achieves the maximum sound decidable
> rejection available to any unrestricted verifier: every
> syntactically detectable laundering fails a hard gate, and nothing
> more is syntactically detectable without unsoundness.

Proof of (i): a decidable predicate rejecting an admissible atom
rejects, by A2, a realizable-genuine atom, hence is unsound; the
entire load is A2. (ii) is P1 plus the gate definitions. Evaluator:
proof (one-paragraph set-inclusion argument over named axioms).

**The condition is part of the statement, never implicit.** Maximality
holds only among _unrestricted_ constructions. A construction that
restricts admissible generators to a class with decidable
finiteness-of-range soundly decides degeneracy on its restricted
domain and hence decides strictly more wherever the generator falls in
the subset. The concrete witness class is **finite-state
transducers**: the image of a regular language under a finite-state
transduction is effectively regular, and finiteness of a regular
language is decidable. Totality of the generator language is NOT
sufficient for this and must not be cited as the witness: degeneracy
remains undecidable even for primitive-recursive generators (take
`f_M(n) = B` if `M` halts within `n` steps, else `n` — primitive
recursive, degenerate iff `M` halts), so "run a total program to see
if it ignores its input" decides nothing about the unbounded domain.

**Division of labor with Theorem 1.** A2 closes the byte-level exit
(no sound rejection inside `L_c` by looking at bytes); the one
remaining bytes-grounded signal inside `L_c` is the committed
generator's behavior, and Theorem 1 closes that behavior-level exit.
The two results are complements: without A2, Theorem 1 alone would
permit sound syntactic rejection inside `L_c`; without Theorem 1, A2
alone would permit a sound behavioral adjudicator. The ceiling is
their conjunction.

### 9.3 Theorem 3 — forced attributability (two registers)

> **Theorem 3a (completeness of the accounting — formal, finite,
> machine-checkable).** For every atom `a` and every `(P, σ)`: every
> member of `depclosure(a)` is booked in exactly one bucket
> (`Exhaustive`, §2), and if `Total(a)` then `T(a) ⊆ GenesisSeeds` and
> every admission-dependent grain of evidence the verdict rests on is
> enumerated in `B(a)` — no positive classification rests on an
> unenumerated signature. "Maximal forced attributability" means
> exactly this universality: there is no third status — every closure
> member is either mechanically closed with its closing evidence
> enumerated, or booked residue. Nothing is silently trusted.

Evaluator: the Alloy model checker, in bounded scope (§10) — not
prose. Falsification signpost: in the model, basis-completeness holds
by construction, so the model itself cannot refute it; the refutation
surface is an _implementation_ of the closure walk whose computed
`B(a)` omits a signature the classification counted. Any conforming
implementation must expose `B(a)` so that check is possible.

> **Theorem 3b (attributability forcing — economic argument, not a
> theorem).** Any laundering (§8.1 sense) that survives the hard gates
> faces a forced dilemma: remain in residue forever — never presenting
> as `Total`, visible in `T(a)` — or purchase establishment, which
> costs permanent, non-repudiable, czd-addressed signatures entering
> `B(a)`: the committed generator (signed, inspectable, preserved) and
> an admitted voucher's source-class-vouch. Evasion past the decidable
> frontier is not prevented; it is priced in permanent attributable
> evidence, and the price is levied automatically by the
> classification, not by vigilance.

Evaluator: prose argument, deliberately. No evidence metric is
defined, so no "maximal evidence" or dominance claim is made; what is
claimed formally is carried by 3a (the forcing is universal) and by
the construction (the forced evidence is permanent and attributable).
The cost asymmetry — the launderer signs forever, the verifier walks
once — is design rationale, not mathematics.

### 9.4 The frontier map

| Frontier                                    | What bounds it                                                                                                                                                                  | What the construction does at it                                                                                              |
| :------------------------------------------ | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :---------------------------------------------------------------------------------------------------------------------------- |
| **Syntactic** (artifact form)               | A2 — genuineness is not bytes-determined; sound rejection stops at `L_c`                                                                                                        | Gates (a)/(b) decide to exactly this line (Theorem 2); gate (c) contributes evidence, never decisions                         |
| **Semantic** (artifact behavior)            | Theorem 1 — degeneracy of the forced generator is undecidable, even under the observed-pair promise                                                                             | Forces `G` into permanent signed existence; books the atom in residue unless establishment is purchased attributably          |
| **Substrate** (the verifier)                | The trusting-trust regress: the walk's own toolchain closure regresses; diverse double-compilation _detects_ divergence under stated assumptions — it reduces, never eliminates | The regress terminates in named, permanent, in-protocol trust-imports: the genesis seeds, inside `T(a)` — counted, not hidden |
| **Governance** (who says the class is true) | Not an impossibility at all — an authorization gap                                                                                                                              | Closed by mechanism: gate-executability fail-closure + the source-class-vouch; the closing signature is booked in `B(a)`      |

The two impossibility axes are distinct in kind: the semantic frontier
is Rice's theorem applied to an artifact the construction itself
forces into existence; the substrate frontier is the trusting-trust
result about the verifier's own toolchain. The full ceiling, in one
sentence: decidable detection to the syntactic frontier; attributable,
permanent evidence at the semantic frontier; diversity-based
reduction, never elimination, at the substrate frontier; and beyond
all three, nothing but explicit, named, signed trust — the seeds in
`T(a)`, the admitted evidence in `B(a)`, the admission choices in `P`.
Above the protocol's verification floor, no grain of trust the verdict
admits is unlocated — that, and not "no trust", is the theorem-shaped
content of `Total`.

## 10. The machine-checked safety result

The classification law of §2–§7 is encoded as a relational model —
[`surety_classification.als`](../specs/alloy/surety_classification.als),
which `open`s the shared core
[`surety_core.als`](../specs/alloy/surety_core.als), with the acyclicity
differential carried in
[`surety_no_f1.als`](../specs/alloy/surety_no_f1.als) — and checked with
the Alloy Analyzer (version 5.1.0, SAT4J solver, bounded-exhaustive
search). Both entry modules are checked in continuous integration on
every push and pull request (the repository's `model-check` workflow,
`.github/workflows/model-check.yml`), so the results below are
re-verified against the committed model on each change rather than
asserted once. The model carries the artifact/evidence
sorts, the four-bucket classification with its precedence cascade and
the `Established_RCAS` biconditional, `T(a)`, `B(a)`, `Total`, the
acyclicity axiom on the input relation, and the admission policy as
free signer sets (the checker explores all valuations, including
empty). Laundering is rendered by its structural signature — a member
with no build record, or sourcehood resting on self-declaration alone
— because ground-truth "laundered" is not machine-representable, by
this model's own §8.2 argument. Results, all at two bounded scopes (up
to 8 and up to 10 artifacts and evidence records):

- **Safety (sense 1) holds — no counterexample found.** No
  laundered-shaped member ever classifies `ReproducibleCASource`, and
  no atom whose dependency closure contains one presents as `Total`
  (`NoSilentLaundering`, `LaunderedNeverPresentsAsTotal`).
  Specifically encoded and held: a member with no build record — a
  promoted fetch pin included — is forced into `TrustImport` and
  defeats `Total` anywhere in a closure; no testimony-only path
  reaches the closed bucket (declaration alone never closes); vouches
  from non-admitted signers change no classification; every real
  `Total` verdict carries its admitted vouch enumerated in `B(a)`. A
  vacuity guard (an instance inhabiting all four buckets
  simultaneously) confirms the checks are not green over a starved
  theory.
- **Satisfiability (sense 2) holds and is vouch-dependent.** A
  non-seed atom _can_ be `Total`, grounded through a genesis seed at
  the closure base; and with the admitted-voucher set empty, no
  non-seed atom is ever `Total` (unsatisfiable) — mechanically
  confirming that the vouch mechanism alone makes `Total` non-vacuous
  for real software, and that safety never needed it. The §8.3
  witness's fate is exercised directly: gate-evading, corroborated,
  content-addressed, but unvouched, it sits in `SourceClassResidue`.
- **The acyclicity axiom is essential.** With it, no
  circular-justification instance exists (two members classified
  closed by citing each other). With it removed, the checker exhibits
  both the admitted cycle and an ungrounded cyclic closure that
  presents as `Total` with no seed anywhere — exactly the spurious
  fixed point the axiom exists to exclude.

Honest bounds of the result: bounded-scope model checking is
exhaustive within scope and silent beyond it; the safety property
follows from the §2 biconditional by a short logical chain, so the
check's real content is that the biconditional, the precedence
cascade, the recursive condition (v), acyclicity, and the
well-formedness constraints are jointly consistent and jointly deliver
it, plus the closure-level and differential results, which are not
one-step consequences. The one case outside the machine-checked
property is the policy boundary §7 names: a laundering tree that
passes the hard gates and obtains an admitted voucher's signature and
corroboration classifies closed — for that consumer, by that
consumer's own admission — and the compensation (the laundering is
pinned to the voucher's permanent signature in `B(a)`, never silent)
is itself machine-checked. The forced generator appears in the model
only through its structural consequence (a witness-shaped atom is
decided purely by vouch admission); its degeneracy is never computed —
the model does not decide the undecidable.
