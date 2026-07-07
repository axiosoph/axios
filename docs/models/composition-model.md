# MODEL: Composition — the Second Primitive

_2026-07-07. Status: v0.1 — the composition primitive given its own
formal home, completing the substrate trichotomy (nrd's directive: the
three founding primitives — storage, composition, execution — should be
primitives **in the formalism**, not only in the narrative). §§1–5, 7–8
are re-homed from the [Execution Model](execution-model.md) v0.9 (its
§0.5, §1.1 in part, §1.2, §1.4, §1.5, §5.3, §6.4), where they had
accumulated as request-formation machinery; that content passed two
zero-context adversarial reviews inside the execution model and is moved,
not re-litigated — cross-references remapped, substance unchanged. New
material: §0 (the trichotomy and the promotion cycle), §6 (the fact-set),
§9–§10 (boundary and obligations P8–P9). The execution model consumes
`Composition` as an abstract type whose semantics this document owns._

---

## 0. The substrate trichotomy

The substrate's founding analysis named three primitives. Each answers a
different question, and each has its own formal model:

| Primitive       | Question                                                  | Model                                 |
| :-------------- | :-------------------------------------------------------- | :------------------------------------ |
| **storage**     | identity — what content _is_                              | [Storage Model](storage-model.md)     |
| **composition** | structure — how content is arranged into a world          | this document                         |
| **execution**   | dynamics — what happens when a process runs in that world | [Execution Model](execution-model.md) |

```
store   : Bytes → Digest                    (static: identity)
compose : Intent × FactSet → Composition    (static: structure; pure, §6)
execute : Request × World → Record          (dynamic; policy-stratified)
```

**The cycle, and the one arrow that closes it.** The three primitives are
not a stack but a loop:

```
compose ──▶ execute ──▶ record ──promotion──▶ fact-set ──▶ compose ...
```

Composition forms views; execution runs opaque commands in them and emits
records; **promotion** (execution model §3.3, §3.5) — reified monadic
bind, priced at a signature — is the unique arrow by which execution's
outputs re-enter composition's inputs as signed intent or accumulated
facts. Every dynamic behavior of the substrate (iterated builds,
fetch-discovery, closure refinement) is a traversal of this cycle;
"pipelines cross strata, steps do not" (§5) is the law that each
traversal step lives in exactly one primitive.

**The evaluator-exclusion law.** The composition algebra is deliberately
**sub-Turing**: elaboration is a terminating computation over declared
data (P8), never general recursion. Anything Turing-complete is
execution — a _generator_ is not a composition-level program but an
**action** whose outputs re-enter composition only through promotion.
This is the structural guarantee behind ADR-0006 §3: the evaluator
cannot regrow in the trusted core, because the only place a program can
run is the executor, and the executor's results cross back only through
signed intent.

**Deliberate non-primitives.** Two things are conspicuously absent from
the trichotomy, both on purpose:

- **Naming and trust** (atom identity, `publish_czd`, signatures, trust
  anchors) are the protocol plane _above_ the substrate. The primitives
  answer "what is computed"; trust answers "who believes what"
  (execution model §3.4). The omission is a decision, not an oversight.
- **The fact-set** is not a fourth primitive but the substrate's only
  state — the shared knowledge both static primitives read and execution
  writes. It is defined once, in §6, and both sibling models state their
  obligations against that definition.

## 1. The language-free purity thesis

Nix's purity is folklorically attributed to its purely functional
language — and at the binding level the attribution is correct: the
language's semantics is what guarantees that package _composition_
combines purely (referential transparency of the generator ⇒ coherence of
the generated graph). Removing the language therefore creates a real
obligation, and this model is how it is discharged:

> **Purity moves from a property of the generator to a property of the
> artifact.** Nix guarantees "this graph was produced by a pure function"
> — a _provenance_ claim, which evaporates the moment an object crosses a
> trust boundary (a foreign drv must be re-evaluated or trusted). This
> model guarantees "this object satisfies checkable laws, whoever produced
> it" — digest identity for referential transparency (a composition root
> _is_ its meaning; substitution of interface-equals is the β-rule, §4),
> the merge monoid for combination (§3), certificates for linking
> coherence (§4), signatures for authorship (execution model §3.4).
> Validity is carried _by the object_, which is the only kind of purity
> that survives decentralization.

In embedding terms: Nix embeds composition **shallowly** in a host
language and inherits purity from the metalanguage; this model embeds it
**deeply** — compositions are first-order data with algebraic laws — and
trades the language's expressiveness for analyzability (you cannot ask a
Nix closure _why_ it contains a path; it is the residue of an arbitrary
Turing-complete evaluation. You can ask a certificate). In _Build Systems
à la Carte_ terms: the model deliberately occupies the **applicative
fragment** — dependency structure is static data, never the output of
arbitrary computation — and the monadic escape (Nix's import-from-
derivation) is deleted with the eval stage, its legitimate use-cases
re-entering through exactly one audited door: trial → promotion → signed
intent (execution model §3.3). Where Nix evaluation may diverge and
resists analysis, environment formation is a terminating fixpoint (P8)
over declared data.

The generating _language_ thereby becomes optional, untrusted sugar:
anything may emit compositions — a CLI, a script, someday a DSL — because
soundness is enforced at the object level, not the generator level. The
system is "implicitly functional": it has functional programming's
combinatorial guarantees with no functional language in the trusted core.

## 2. The composition domain

All values are content-addressed; identity is digest (storage model A2).
Blobs and Merkle trees are the storage primitive's values; composition
adds one value of its own:

- **Composition** `c` — a finite map from conventional paths to content
  entries, signed. Its _denotation_ is a partial function:

  ```
  ⟦c⟧ : Path ⇀ Content        (Content = blob | tree-node | symlink)
  ```

  A composition is a **value that denotes an environment**. This is the
  precise sense in which the system is "purely functional": a process
  under this model runs against an environment that is itself a
  first-class, content-addressed, signed value — not against an ambient
  mutable world.

  The denotation includes **directory enumeration order** (castore trees
  are name-sorted, storage model A3; executors MUST present that order).
  Without this, two conforming executors backed by different mount
  technologies could return different `readdir` orders for one view, and
  a build sensitive to enumeration order would break action-stratum
  bisimulation (execution model §6.1).

## 3. Composition merge is a partial commutative monoid

Define `c₁ ⊕ c₂` (merge) as the union of the two maps, **defined only when
they agree on the intersection of their domains** (byte-identical entries).
Disagreement is a _conflict_ — merge is undefined, surfaced as an explicit
error at compose time (never resolved silently — the collision point is
exactly where ABI reality lives).

Laws (where defined):

```
c ⊕ ∅ = c                      (identity: the empty composition)
c₁ ⊕ c₂ = c₂ ⊕ c₁              (commutative)
(c₁ ⊕ c₂) ⊕ c₃ = c₁ ⊕ (c₂ ⊕ c₃)  (associative)
c ⊕ c = c                      (idempotent)
```

`(Comp, ⊕, ∅)` is a **partial commutative idempotent monoid**. This tiny
structure carries real weight: it is what makes "compose the toolchain with
the dep trees with the fetch blobs" order-independent and deterministic, and
it is the merge over which independent subgraphs compose in the action DAG
(execution model §5.1).

**Conflict is defined over denotations, not keys** (correction F5, second
adversarial review): entries include subtree grafts (`Dir{tree}`), so
`{/usr ↦ Dir t}` and `{/usr/lib/x ↦ File b}` have disjoint keys but
overlapping denotations — key-level conflict detection would either miss
the collision or silently shadow. Two compositions conflict iff their
_denotations_ disagree on any path in the intersection of their
denotation domains (equivalently: mandate a prefix-free normal form and
check keys there). _Proof obligation P1 (cheap, Alloy-able): the concrete
merge implementation satisfies the laws AND detects exactly the
denotational conflicts — the graft case is in P1's scope explicitly._

## 4. Interfaces: the typing of compositions

The merge monoid (§3) says when compositions _can_ coexist (path
disjointness). It says nothing about whether the result _works_. That is
the job of the **interface layer** — and it is the seam the whole
substitution story hangs on, so it enters the formal core rather than
remaining prose.

**The judgment.** Interface manifests (htc-sad §2.2) assign each tree its
provides/requires per namespace, with `iface_digest = H(canonical
interface description)`. For a composition `c`, lift pointwise:

```
Prov(c) = ⋃ provides of member trees
Req(c)  = ⋃ requires of member trees NOT satisfied within c
          (the composition's free variables — its unbound obligations)
```

A **binding** is a justified edge `required ↦ provider` with
`satisfies(needs, provides)` holding (htc-sad §6.1's relation). The
`iface_digest` is the substrate's **chosen identity proxy for "same
contract"** — normalized, hashed, unforgeable, _decidable by
construction_ where semantic ABI equality is not. (Honesty is inherited
from htc-sad §6.2's precision statement: symbol-level satisfaction is
necessary, not sufficient; strict mode remains the guarantee floor. The
proxy is load-bearing precisely because it is a proxy we can check.)

**This is a module system.** Compositions are modules; interface
manifests are their signatures; binding is linking. Substitution
soundness — the LEGO thesis, "swap the OpenSSL blob, nothing rebuilds" —
is exactly signature-preserving relinking, and `iface_digest` is what
makes "signature-preserving" checkable. In this precise sense **the
runtime is more than an optimized runtime: the executor and materializer
are the interpreters of the value algebra, and the interface layer is its
type discipline** — the support contract under every claim the model
makes about pure computation.

**The granularity strata** (explicit, where the derivation blurred them):

```
package      an atom's output tree + its interface manifest
             (typed but not linked: Req ≠ ∅ is normal and healthy)

environment  a composition equipped with a COHERENCE CERTIFICATE:
             every internal require bound, at most one provider chosen
             per (ns, name) per scope (no diamonds), the choice function
             recorded (defaults ∪ user overrides), and the residual
             Req(c) — the declared ambient base (kernel ABI, loader) —
             stated, not silent. An environment is a LINKED module.

system       a composition of environments, cross-environment
             obligations discharged or scoped. Per-composition scoping
             (ADR-0005's store-path role decomposition: conflict-free
             co-installation) is what permits two environments to bind
             DIFFERENT versions of one provider: ⊕'s conflict rule
             forces explicit prefix/namespace separation, so
             co-installation is a theorem of the merge monoid, not a
             convention.
```

**Flat is the normal form; a scope boundary must earn its existence.**
The default composition is a single scope: everything merged by `⊕`, one
provider chosen per (ns, name), coherence certified across the whole.
Scope boundaries — layers — are introduced only when meaning demands
them: a genuine conflict (one member requiring a divergent provider —
the co-installation escape hatch above), or a genuine intent boundary
(an OS scope vs. a project dev shell whose choice function is a
recorded _delta_ over its parent's, inheriting every choice it does not
override; vs. a single package's quirk view inside that). Because a
scope is itself just a composition with its own choice function, a
layer is expressible **inside the algebra** — never an external
packaging artifact — which is the precise difference from OCI layering:
there a layer is an accident of build-script ordering; here it is a
certified linking boundary with a reason. Pragmatics align with the
semantics: every scope boundary buys indirection overhead, so the
preference for flatness is a performance norm and a coherence norm at
once.

**Environment formation IS resolution.** The coherence certificate is the
recorded output of a fixpoint + choice function over interface bindings —
ion's version-resolution algebra, one layer down. Bindings all the way
down, again: this is the third instantiation (lock, composition, and now
environment linkage) of the same name→signed-pointer discipline.

**Repair happens at the package contract, not the environment.** The
environment is the unit of _linking and choice_ — assembling packages,
selecting providers — never a dumping ground for fixes. A closure fault
(execution model §4.4) — a missing runtime dep that every detector
missed — is evidence about the _package_: its requires-set was
incomplete. The durable repair is therefore a **declared require in the
package's interface contract** (execution model §4.2's `Declared` kind —
an author- or third-party-asserted fact, keyed by declarer exactly as
analyzer facts are keyed by analyzer, so it works even for atoms the
repairer does not own). Every environment containing the package then
picks up the binding automatically at re-formation: new composition
root, zero rebuilds, the package's _content_ untouched. Patching a
single environment instead would strand the repair in one place while
every other environment with the same package stays broken — and the
formalism agrees: the evidence sets of the justified closure
(`R_declared(a)`, execution model §4.3) are indexed by the artifact,
not by any environment.

**Versioning.** An environment's _identity_ is its composition root; its
_contract_ is its interface (Prov/residual Req and their digests). A
semantic version is neither — it is human-facing naming, and it applies
exactly when an environment is _published as an atom_ (signed intent
whose content is the composition), at which point ion's existing version
machinery covers it with nothing new invented. Whether environments
routinely publish as atoms is an open question (execution model §9.10),
but the model makes it a choice, not a gap. Composition of environments
into an OCI image (one layer per environment, each with its certificate)
is an Export-tier mapping that gives layering _meaning_ — the layer
boundary is a certified linking boundary, not an accident of Dockerfile
ordering.

## 5. The two strata of intent

Atoms are the unit of publishable, versioned, signed intent. The question
"should package-atoms and environment/generator-atoms be distinct types?"
cuts at the right joint only when asked about _laws_, not _schemas_ — and
there the answer is yes, there are exactly two strata of intent, mirroring
the action/trial stratification of the execution primitive:

```
executable intent   elaboration REQUIRES execution: a package atom
                    denotes actions; realizing it crosses into the
                    execution model's world (sandbox, cache, records,
                    signer trust — execution model §2, §3.4).
                    Verifiable only by building.

algebraic intent    elaboration is pure computation in the value
                    algebra — precisely: PURE IN (intent, fact-set)
                    (correction F6, second adversarial review: v0.6's
                    "from the intent alone" was an overclaim, since
                    interface manifests are analyzer OUTPUTS — i.e.
                    executions — so even base-case formation is
                    fact-conditioned, and version resolution pins a
                    worldly discovery snapshot just as the lock does).
                    An environment atom denotes a composition +
                    certificate via the formation fixpoint (P8); a
                    generator atom denotes an Env → Env operator
                    pipeline (§7). Given the pinned fact snapshot,
                    elaboration needs no executor, no sandbox, no cache,
                    and is SELF-VERIFYING BY RECOMPUTATION: a registry
                    can check an environment's coherence RELATIVE TO ITS
                    CLAIMED MANIFESTS at publish time (the manifests'
                    own truth rides signer trust or re-analysis).
                    No registry can refuse a package that won't build
                    without building it.
```

**Why this helps rather than decorates:** several results already proven
quietly depend on "the composition side never touches the execution
layer" — repair-without-rebuild (§4), the rebind stratum of overrides
(§7), install-without-execute (§8), transactional update. The strata
give that recurring precondition a name and a single home. They also
settle the earlier versioning question with symmetry rather than fiat:

```
package atom      manifest ──resolution──▶ lock         (pinned intent)
environment atom  manifest ──formation───▶ certificate  (pinned intent)
```

The certificate is the environment's lockfile — the same
intent→pinned-elaboration shape, fourth instantiation of the binding
algebra. Both strata version and resolve through ion's existing
machinery; environment members ARE references (set/label/version
constraints) — what environments lack is not dependency _declaration_ but
build-input _semantics_ (no toolchain, no fetch set, no build/runtime dep
split).

**Kinds are open; strata are closed.** Package, environment, generator —
and future kinds (test intent is already a manifest section per the
execution model §5.2) — form an open, schema-level family. Each kind
belongs to exactly one stratum, and the stratum determines its laws
(execution-realized and trust-mediated, vs. purely elaborated and
recomputation-verified). A mixed atom (one publishing intent in both
strata) breaks no law; whether the schema permits or discourages it is a
convention call (execution model §9.13).

**Pipelines cross strata; steps do not.** The strata classify elaboration
_steps_; an atom denotes an elaboration _pipeline_, and pipelines may
interleave strata freely — a system atom is the canonical case: members
composed (algebraic) → config intent rendered by actions (execution) →
rendered blobs bound (algebraic) → certificate. Boot and running services
are executions _of_ the finished artifact at use time, outside elaboration
entirely (the atom ships the activation program as content). The
prohibited object is a mixed **step** — a computation partly pure, partly
world: Nix's import-from-derivation is precisely that violation, and
promotion (execution model §3.3) is its lawful replacement. Cross-strata
data flows only through CAS values, records, and promotion.

**Fact-conditioned formation stays algebraic.** Formation may consume
execution records and attestations as _inputs_ ("admit P only if its
tests pass", "members must carry k reproducibility attestations") and
remain in the algebraic stratum, **provided the certificate pins the fact
snapshot consumed**: formation is pure in `(intent, fact-set)`,
recomputable by anyone holding both. Publication gating (execution model
§3.2) is the degenerate case; staged rollouts and policy-gated
environments are the general one.

## 6. The fact-set: the substrate's only state

Everything in the substrate is an immutable value except one thing: what
is _known_. The **fact-set** is that knowledge — the accumulating store
of signed execution records and attestations (execution model §3.1),
indexed by `action_id` and `req_digest` — and it is deliberately not a
fourth primitive but the shared state at the seam: **execution writes
it, composition reads it, and promotion is the only arrow that turns its
contents back into intent.**

```
FactSet = finite set of signed records/attestations
          (indexed: action_id ↦ Set<record>, req_digest ↦ Set<record>)
```

Laws:

- **Monotone accumulation.** The fact-set only grows; insertion is
  commutative and idempotent (set-union shape). This is the
  witness-accumulation semantics of the execution model §2.2 stated as
  the state's own law: several distinct output digests for one action
  from several signers is a legitimate state, and no reconciliation,
  tie-break, or canonical winner exists at the state level.
- **Read-side selection is a recorded choice.** Any consumer that acts
  on the fact-set (cache lookup, witness pick at request formation,
  fact-conditioned formation §5) records _which snapshot and which
  choice_ it consumed. Determinism of every algebraic elaboration is
  relative to `(intent, fact-set snapshot, choice policy)` — never to a
  global state the system nowhere maintains.
- **Snapshot pinning.** A certificate (or lock) that consumed facts pins
  the snapshot: formation is pure in `(intent, fact-set)` and
  recomputable by anyone holding both (§5). The pin is what keeps
  fact-conditioned formation inside the algebraic stratum.
- **Fact-hood is signer-relative at read time.** Which records _count_
  is the trust layer's judgment (execution model §3.4), applied on read;
  the state itself stores evidence from any signer.

The concrete realization is atom metadata (appended records, htc-sad
§6.10) — decentralized, per-atom, no global store. That the substrate's
only state is an append-only, commutative, signer-attributed set is what
makes the whole design tractable under decentralization: replicas
converge by union, and nothing anyone writes can invalidate what another
party already consumed (they pinned their snapshot).

## 7. The override algebra: generators live inside the model

What Nix does with an overlay — an untyped function over the package
universe ("give everything Python 3.4") — this model does with a **typed
edit over the binding graph**, and the difference is the model's central
economic claim. Define the substitution operator:

```
subst[n ↦ p′] : Comp → Comp     -- rebind every binding of name n to p′
```

Its validity is _checked, per consumer_: for each require previously bound
to the old provider, `satisfies(needs, provides(p′))` must hold (§4;
under Strict policy only digest-equality passes; under Compat, an
interface-satisfying swap passes with a recorded proof — the
`SubstitutionRecord` htc-sad §2.1/§6.2 already anticipated). This one
operator splits every override into its two true strata:

- **Rebinding (runtime stratum, cheap).** Consumers whose interface
  contracts survive the swap are _relinked_: composition edit, recertify,
  re-sign. Zero rebuilds. This is the security-patch case — the
  Guix-grafts use case with proofs instead of binary patching.
- **Rebuild frontier (intent stratum, bounded).** Consumers where
  satisfaction _fails_ form the frontier `F` — they genuinely need their
  intent edited (build against the new provider ⇒ new `action_id`s), or
  they stay on the old provider in their own scope (co-installation,
  §4, is the lawful answer to the diamond). Downstream of `F`, early
  cutoff (execution model §2.4) prunes every descendant whose rebuilt
  output is digest-identical.

**Theorem (bounded blast radius — relative to the interface proxy).** An
override's rebuild set is exactly `F` plus its non-cutoff descendants,
_where `F` is computed against `iface_digest` satisfaction_ — and per
htc-sad §6.2's precision statement that proxy is necessary-not-sufficient,
so real breakage escaping it (same-symbol struct-layout changes) makes
`F` an under-approximation; strict mode remains the guarantee floor
(correction F10, second adversarial review). Contrast the system
being replaced: Nix rebuilds the _entire reverse-dependency closure_ of
the overridden node, unconditionally — input-addressed identity with no
interface layer means it cannot certify that anything survived. The
interface certificate is precisely the instrument that turns "rebuild the
world below the change" into "rebuild what the change (checkably) broke."

An **overlay** is then a reusable, named sequence of algebra operations
(`subst`, `⊕`-extend, prune, choice-function override) — data, so it can
itself be content-addressed and signed like everything else. No language
in the trusted core; the expressive power Nix gets from functions, this
model gets from operators whose blast radius the type layer computes.

**Property (generator normal form).** Environments are freely generated
from packages by `⊕` and binding; therefore every generator's effect —
whatever its surface abstraction — has a normal form as a package-level
edit set: `(remove R, add A, rebind B, re-certify)`. A Nix overlay is an
opaque function whose effect is learned by evaluating the world twice and
diffing; a generator here is _statically diffable_ as a certificate
delta before it is ever applied. The package is the primitive of the
generator/environment relation; environments are states; generators are
transitions compiled to the operator algebra.

## 8. Materialization, installation, and transactional update

The runtime has a second operation beside `execute` — the interpreter of
the value layer itself:

```
materialize : CompositionRoot → View
```

Obligation: the view presents exactly `⟦c⟧` (§2, enumeration order
included), tamper-evident where the mechanism allows (composefs +
fs-verity: the kernel refuses corrupted content _at read time_ — a
guarantee Nix does not have; its store verifies at substitution and
executes tampered bytes happily). Materialization is the seam arrow from
the static primitives into execution's world: the executor's conformance
obligation to present exactly `⟦view⟧` (execution model §6.3) is this
obligation, restated from the consuming side.

**Installation is the dogfood test, and the model passes it with zero new
concepts.** Installing the system (bootstrap binary included) is:

```
1. obtain the signed composition        →  trust anchors verify the
                                           SIGNER (execution model §3.4);
                                           nothing else is trusted
2. fetch blobs by digest, any channel   →  the substitution principle
                                           (storage model §3): the
                                           transport is untrusted BY
                                           CONSTRUCTION — mirrors, CDNs,
                                           peers are all equally fine
3. materialize, persistently            →  the composefs layer
```

Note what installation is **not**: it is not an `execute` — no sandbox, no
policy stratum, no record. Fetching during install needs no trial
machinery because nothing here has identity to corrupt: a blob either
matches its digest or is rejected. (This sharpens the deposit/bind law,
execution model §5.2: fetch-by-digest is _substitution into an existing
binding_, the third and most trivial way content moves — deposit needs
promotion to bind; substitution needs nothing because the binding,
signed, already exists.) The bootstrap binary itself ships as a tree
pinned by a composition: **the system installs itself with the two
static primitives alone — storage and composition; no execution
occurs** — and the only bootstrap-specific artifact is the initial
trust-anchor set.

**Update is a root swap; transactionality is structural.** A new system
state is a new composition root; switching a mount (or a symlink to a
mounted image) is atomic. The old root remains valid — rollback is
keeping it; GC is dropping unreferenced blobs (storage model §4).
"Combining and cheaply updating pieces of a layer, transactionally" is
then: recompose (edit bindings, `⊕` in a new environment), re-certify
(§4), re-sign, swap. No package rebuilds anywhere in the loop unless
intent changed — the separation of the action DAG (rebuild world) from
the composition algebra (rebind world) is the entire point of the
trichotomy, exercised end to end by the system's own installation.

## 9. What this model deliberately does not own

The same boundary discipline as the execution model's §7.3, from the
composition side:

- **Execution semantics** — policy strata, records, witness accumulation,
  the sandbox conformance table: the execution model. This document
  consumes `Record` and `FactSet` entries as opaque signed values.
- **Acceptance and trust** — which signers count, how many record-equal
  attestations upgrade confidence: the trust layer (execution model
  §3.4), never new invariants here.
- **Naming and version resolution** — atom identity and ion's resolution
  algebra sit above; §4's "environment formation IS resolution" borrows
  the algebra's _shape_, not its ownership.
- **The intent schema** — the manifest/lock redesign (ADR-0006's flagged
  open surface: action params, test params, the kind discriminator,
  requires-edge generalization, adopted-lockfile entries) is design work
  this model grounds but does not perform. `Intent` here is the abstract
  input type of `compose`; its concrete schema is that redesign's
  deliverable.
- **Composition-side open questions** remain queued in the execution
  model §9 (its items 8–13: ambient base, choice-function defaults,
  environments-as-atoms, override surface, env-var namespace, kind
  schema) — one queue, deliberately not split across documents.

## 10. Proof obligations

Owned here (continuing the substrate-wide P-numbering; P1–P7 are homed in
the execution model, P10 in the storage model):

- **P1** _(re-homed from the execution model)_ — `⊕` partial-monoid laws
  - exact denotational conflict detection, the graft case in scope
    (Alloy; disciplines the composer implementation).
- **P8** — **the formation fixpoint**: environment formation terminates
  (Knaster–Tarski over the finite candidate set — the same mathematics
  as P2's justified closure, applied to declared bindings rather than
  observed evidence), is deterministic in `(intent, fact-set snapshot,
choice policy)`, and its certificate is **recomputable**: any holder
  of intent + snapshot re-derives a byte-identical certificate
  (self-verification by recomputation, §5).
- **P9** — **override soundness**: `subst[n ↦ p′]` preserves certificate
  coherence for every consumer whose satisfaction survives the swap;
  the rebuild frontier `F` is exactly the satisfaction-failure set
  relative to the `iface_digest` proxy; recertification after rebind
  yields a valid certificate (§7). Testable against the composer
  implementation.

Seam obligations shared with the execution model, homed there: **P6**
(promotion coherence — the arrow of §0's cycle), **P7** (`resolve`
determinism and totality — the arrow from intent into requests).
