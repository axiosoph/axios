# MODEL: Execution — the Third Primitive

_2026-07-06. Status: **APPROVED FOR PROMOTION** (nrd, 2026-07-07 — core
terminology and observation posture ratified; remaining §9 items are
tracked open questions, not blockers). Extends the algebra of
`docs/models/publishing-stack-layers.md` (olog / coalgebra / session
types); where this document and prior prose (ADR-0005, htc-sad) conflict,
this document is the correction — the reconciliation sweep amends the
older documents accordingly._

_v0.2 (2026-07-07): incorporates all ten corrections from a zero-context
adversarial review by an independent architect instance, which reached
ADOPT-WITH-CORRECTIONS — having independently formulated the layer without
the execution unification and then conceded to it on engineering economy,
REAPI precedent, and the test-edit property. Sharpest corrections: the
determinism claim restated relationally (§2.2), strata made policy-derived
so hermetic tests cache (§2.3), signer-trust semantics for facts (§3.4),
request-digest completeness (§2.1), the deposit-vs-bind law (§5.2), and
the μ/ν and monoidal-category decorations struck (§4.4, §5.1)._

_v0.3 (2026-07-07): extensions from nrd's expansion — interfaces as the
typing of compositions (§1.4: packages / environments / systems made
explicit, the granularity derivations blur); the human escape hatch as a
third justification kind, `Declared`, with its epistemics kept honest
(§4.2–4.3); materialization/installation/transactional update as the value
layer's own operation, proving install needs zero new concepts (§6.4)._

_v0.4 (2026-07-07): the language-free purity thesis promoted to §0.5 (the
model's framing result); the override algebra added as §5.3 — overlays are
operations inside the model (typed edits over the binding graph, blast
radius computed by the interface layer) rather than an external generator
gap; §9.11 rewritten accordingly._

_v0.5 (2026-07-07): the two strata of intent (§1.5) — executable intent
(packages: elaboration requires execution) vs algebraic intent
(environments, generators: elaboration is pure, terminating, self-verifying
by recomputation) — mirroring the action/trial stratification at the intent
layer; kinds open at schema level, strata closed at law level; the
certificate identified as the environment's lockfile._

_v0.6 (2026-07-07): §3.5 — promotion as reified monadic bind (the IFD
answer: monadic builds as iterated applicative generations, signatures at
the join points); mixed-atom law (pipelines cross strata, steps do not)
and fact-conditioned formation added to §1.5; P6 (promotion coherence:
identity, idempotence, confluence of independent promotions — the lock
must merge)._

_v0.7 (2026-07-07): corrections from the SECOND zero-context adversarial
review (cross-formalism audit; verdict RATIFIABLE AFTER NAMED CORRECTIONS).
F1: the certified interval's quantification repaired (Dep indexed per view;
build-view sufficiency made relative — runtime-only deps are real, the
Nix containment coincidence is not inherited). F2: theory-transfer claim
split (Lean bounds transfer; Track A's overlap-safety needs a
witness-reconciliation rule — P4 extended, eos amendments queued). F3:
`resolve`-is-a-function demoted to obligation P7 (determinism +
materializer versioning + totality). F4: record_core defined; all
equality claims quantify over it. F5: ⊕ conflicts defined denotationally
(grafts in P1 scope). F6: algebraic stratum's law restated as pure in
(intent, fact-set). F8: flaky-verdict caveat on hermetic-test caching.
F9: BuildRecord.observed_read_set_digest added to the correction sweep.
F10: blast radius qualified by the interface proxy. F7 (lock inadequacy:
no merge op, single-valued owner, purge rules, Either-order gap) recorded
in §9.14 as the lock campaign's acceptance criteria._

_v0.8 (2026-07-07): whole-system corrections from nrd — the model was
overcorrecting for problems sibling layers already solve. The single-slot
"first-witness" cache (imported in v0.2 from the centralized Nix/REAPI
worldview, and the sole reason v0.7 demanded a witness-reconciliation
rule) replaced by the multi-valued record store the trust design always
implied: witnesses accumulate, cache hits serve any trust-acceptable one,
selection at request formation is a recorded choice, multiplicity is
reproducibility evidence. F2 shrinks to two wording amendments in eos
docs. P6's concurrent-merge demand struck (write topology is phased by
design); reorder-invariance laws kept. §9.14's owner-set patch superseded
by consumer-side requires-edge generalization. New §7.3: the boundary
discipline — what this model deliberately does not own._

_v0.10 (2026-07-07): the substrate trichotomy made explicit in the
formalism (nrd's directive: the three founding primitives should be
primitives in the formalism, not only in the narrative). The composition
algebra — §0.5, the composition value of §1.1, §1.2, §1.4, §1.5, §5.3,
§6.4 — is re-homed to the [Composition Model](composition-model.md);
storage is axiomatized in the [Storage Model](storage-model.md). The
moved sections are retained below as stubs so section numbering and
inbound references stay stable. This document now consumes `Composition`
as an abstract type and owns the dynamic primitive alone. P1 moves with
the merge monoid; obligation numbering (P1–P11) is unique across the
three models._

_Trigger: early substrate implementation work treated **build** as the
primitive and bolted observation into its materialization layer (a FUSE
read-set inside `BuildResult`). The substrate's founding analysis had
already named three primitives — storage (CAS), composition, and
EXECUTION — with execution never given its own formal definition. This model gives execution its missing formal
definition, from which build, test, fetch-recording, and closure capture all
derive as policy strata — rather than as separately engineered features._

---

## 0. The one-sentence model

> **Execution is a single operation, `execute : Request × World → Record`,
> stratified _by the request's policy_ — never by workload kind — into
> _actions_ (all ambient channels discharged, hence cacheable) and _trials_
> (some channel open, hence signed witnesses); a build is an action, a test
> that needs the world is a trial, a hermetic test is an action and caches
> like one, and every other capability of the substrate — caching, fetch
> recording, runtime-closure capture, publication gating,
> no-rebuild-on-test-edit — is a consequence of this stratification rather
> than a feature.**

---

## 0.5 The language-free purity thesis

_Re-homed: [Composition Model §1](composition-model.md#1-the-language-free-purity-thesis)
— the thesis is the composition primitive's framing result. The clause
that is execution's own is restated where it binds: the executor never
interprets `command`; there is no interpreted language in the trusted
core (§2.1)._

## 1. The semantic domain

### 1.1 Values

All values are content-addressed; identity is digest.

- **Blob** `b` — bytes. Identity `H(b)`.
- **Tree** `t` — a Merkle tree of named blobs/trees (castore-shaped).
  Identity: root digest.
- **Composition** `c` — the second primitive's value: a signed finite
  map from conventional paths to content entries, with denotation
  `⟦c⟧ : Path ⇀ Content` (directory enumeration order included). Its
  full algebra — merge, interfaces, strata of intent, overrides,
  materialization — is owned by the
  [Composition Model](composition-model.md) (§2 there); this document
  consumes the denotation and nothing else.

### 1.2 Composition merge is a partial commutative monoid

_Re-homed: [Composition Model §3](composition-model.md#3-composition-merge-is-a-partial-commutative-monoid),
together with P1. Execution uses exactly one fact from it: independent
subgraphs of the action DAG merge views by the partial commutative
idempotent monoid `(Comp, ⊕, ∅)` (§5.1)._

### 1.3 The world and its channels

Everything a process can be influenced by that is _not_ in its view is an
**ambient channel**. The finite channel set (extensible, but honest — list
what exists):

```
C = { net, clock, entropy, ids(uid/gid), locale/tz, cpu-features, ... }
```

A **world** `w ∈ W` is an assignment of concrete behavior to channels. The
world is the enemy of reproducibility; the whole design is a discipline for
discharging it.

Each channel, per execution, is in one of three states:

```
closed  — the channel does not exist for the process (e.g. no network ns)
pinned  — the channel exists but its content is a declared function of the
          request (e.g. replay-proxy map; SOURCE_DATE_EPOCH; fixed uid map)
open    — the channel passes through to the real world
```

A **policy** `p ∈ P` assigns a state to every channel. This is a
**classification, not an order**: no result below uses a policy ordering
(and the tempting monotonicity claim — "less access ⇒ same-or-fewer
records" — is vacuous anyway, since different policies produce different
request digests and hence different cache slots). The only structure the
model needs is the **deterministic stratum**:

```
Det = { p ∈ P | no channel is open under p }
```

### 1.4 Interfaces: the typing of compositions

_Re-homed: [Composition Model §4](composition-model.md#4-interfaces-the-typing-of-compositions).
What execution borrows: `satisfies`/`iface_digest` as the checkable
contract proxy (htc-sad §6.1–6.2), and the package contract — not
per-environment patching — as the durable home of closure-fault
repairs (§4.4; the environment is the unit of linking and choice)._

### 1.5 The two strata of intent

_Re-homed: [Composition Model §5](composition-model.md#5-the-two-strata-of-intent).
The strata mirror this document's action/trial stratification at the
intent layer; the executable stratum is realized here (§2–§3), and
cross-strata data flows only through CAS values, records, and promotion
(§3.3)._

## 2. Execution

### 2.1 The request

```
ExecutionRequest = {
  view    : CompositionRoot      -- the entire visible universe of the process
  command : { argv, env, cwd }   -- opaque; the executor NEVER interprets it
  outputs : [Path]               -- declared scratch/output paths to ingest
  policy  : P                    -- channel states, per §1.3
}
req_digest = H(canonical serialization)
```

Three invariants inherited from the substrate's doctrine, now stated as
type-level facts:

- **No interpreted language.** `command` is opaque argv. The executor is a
  universal machine runner, not an evaluator. (This is what died with the
  eval stage in ADR-0005 §6; the request type keeps it dead.)
- **The view is the whole world.** Under any `p ∈ Det`, the process's
  observable universe is exactly `⟦view⟧` plus pinned channel content. There
  is no second place inputs can come from.
- **Request completeness.** Every pinned channel carries a _payload_ (the
  replay map's digest, the `SOURCE_DATE_EPOCH` value, the uid/gid mapping)
  — and every pin payload MUST be inside the canonically serialized,
  digested request. Otherwise two executions differing in pin content
  share a cache slot: unsound. This is not hypothetical — Gate A's
  "seed toolchain provenance untracked" debt is a live instance of the
  defect class (the seed enters via `view`, so it _is_ digested, but the
  lesson generalizes: anything that can influence the record must be
  reachable from `req_digest`). _Proof obligation P5: an
  injectivity/completeness audit of the canonical request serialization —
  every field the executor consults appears in the digest preimage._

### 2.2 The semantics

```
execute : ExecutionRequest × W → ExecutionRecord
```

`execute` is implemented by a sandbox; §6.3 states the obligations that make
the following true _by construction_ rather than by trust.

**The semantics is relational, not functional — deliberately.** Even with
every ambient channel closed or pinned, _residual nondeterminism_ remains
inside the sandbox: thread scheduling, ASLR, the interleavings of a
parallel `make`. So `execute` denotes, for each request, an **admissible
record set**:

```
⟦req⟧ ⊆ ExecutionRecord     (non-empty when execution terminates; failure
                             is itself admissible — a record with exit ≠ 0)
```

**Property (hermeticity / world-discharge).** If `req.policy ∈ Det`, the
admissible set is a function of the request alone:
`Adm(req, w) = Adm(req, w′)` for all `w, w′`. The world argument is
discharged; what is _not_ claimed is that the set is a singleton. The
discharge distributes over channels: for each channel, either the
namespace construction removes it (closed) or the pinning mechanism
determines its content from the request (pinned). §6.3's audit table is
the per-channel discharge — a _conformance checklist for executor
implementations_, not a metaphysical claim.

**The record store is multi-valued; caching is witness selection, not
canonicalization.** (v0.8 correction — nrd, from the whole-system view:
v0.2–v0.7 carried a single-slot "first witness wins" cache imported from
the centralized Nix/REAPI worldview, an invariant this system never
needed.) Records of `⟦req⟧` **accumulate** — signed facts in the atom's
metadata, possibly several distinct output digests from several trusted
builders ("a trusted key has seen this action produce these three
hashes"). A _cache hit_ is: ∃ a witness acceptable under the consumer's
trust anchors (§3.4) — serve it. The executor neither knows nor cares
that other witnesses exist. Coherence needs no tie-break because
consumers bind to the concrete output digests of whichever witness they
consumed — every downstream branch is internally consistent by
content-addressing. The one genuine obligation: when a downstream
request is formed, the witness _pick_ is a **recorded choice over the
fact snapshot** (the same fact-conditioned machinery as the fact-set discipline,
[Composition Model §6](composition-model.md#6-the-fact-set-the-substrates-only-state)), so P7's
determinism holds relative to (intent, fact-set, choice policy).
Multiplicity is not an anomaly: distinct witnesses of one request are
free reproducibility evidence (§2.3), surfaced, never reconciled away.

Reproducibility (§2.3) is then precisely the empirical claim that
`⟦req⟧` is observed to be a singleton — which is why it can only ever be
attested, not proven.

### 2.3 The stratification: actions and trials

```
action  =  request with policy ∈ Det        (world-independent)
trial   =  request with some channel open   (world-dependent)
```

**Stratum membership is a property of the request's policy — never of the
workload kind.** There is no "build stratum" and "test stratum"; there are
only requests that discharge the world and requests that do not:

- An **action**'s record is a **fact**: a signed witness of `⟦req⟧`,
  accumulating alongside any others in the record store (§2.2); a cache
  hit serves any trust-acceptable witness. `build` is an action. So is a **hermetic test** — a test that opens no channel is
  cacheable exactly like a build, and skipping its re-execution on cache
  hit is sound (this is Bazel's cached-test-results behavior, recovered
  here as a stratum consequence rather than a feature). **One epistemic
  caveat (F8, second adversarial review): a test's exit code is its
  payload.** For a hermetically _flaky_ test (racy threads), first-
  witness caching converts "∃ run that passed" into "passes" — sound for
  build outputs (any witness is a valid build), weaker for verdicts that
  gates (§3.2) consume. Acceptance policy may therefore demand n
  core-equal samples before a cached verdict feeds a gate — a policy
  hook, not a structural change.
- A **trial**'s record is an **attestation**: a signed _witness_ that "at
  time τ, under executor E, with world summary σ, this outcome occurred."
  It is evidence, never a cache value. Whether existing evidence is
  _accepted_ in lieu of re-running is an explicit acceptance policy
  (staleness, required count, required signers) — a policy decision, not an
  identity judgment. A **networked test** is a trial. So is **record-mode
  fetch** (§3.3).

**Hermetic ≠ reproducible — two predicates, deliberately separated:**

- **Hermetic** (§2.2): input surface fully determined by the request.
  Auditable per channel; holds by construction for actions.
- **Reproducible**: two _executions_ of the same action yield byte-identical
  output trees. This is **empirical**, not provable — a toolchain may
  consume a pinned-but-present channel (a clock we haven't virtualized, ASLR
  interacting with a buggy linker) in ways we don't model. Reproducibility
  is therefore an **accumulating attestation on an action** (n independent
  executions, k distinct executors, all record-equal — exactly Gate A's
  two-run check, formalized). Nix conflates these two predicates; we will
  not. Cache correctness needs only hermeticity; _trust_ policies (who may
  substitute a binary) may demand reproducibility attestations on top.

### 2.4 Identity discipline (three identities, never conflated)

```
action_id   = H(atom_czd_closure_root, toolchain_composition_root, params)
              -- identity of INTENT (signed); the scheduler/user-facing key
req_digest  = H(ExecutionRequest)
              -- identity of the concrete execution; the executor-level key
record czd  = signed content digest of the record/attestation object
              -- identity of the FACT/WITNESS as durable data
```

Resolution and materialization map intent to request:
`resolve : action → ExecutionRequest`. **That `resolve` is a function is an
OBLIGATION (P7), not an established fact** (correction F3, second
adversarial review — v0.6 asserted it while §9.14 admitted it open; worse,
ion's `[resolution-deterministic]` governs a different arrow entirely,
manifest→lock, and `resolve` is not even _total_ from lock data today:
the toolchain composition has no lock entry type yet, an already-
registered ADR-0005 open item). P7's content: (a) determinism of the full
action→request map — closure materialization layout, command assembly,
pin/policy assembly; (b) **the materializer's layout algorithm must be
versioned inside the identity** (as an `action_params` component or an
explicit term) — otherwise a layout change silently serves stale
action-level cache entries against a changed request; (c) totality,
gated on the toolchain-pin lock entry. Granted P7, caching at either
level is coherent and the two-level scheme buys something real:

**Corollary (cross-intent deduplication).** Two distinct atoms whose
resolution produces the same `ExecutionRequest` share one executor-level
cache entry. Dedup happens at the request layer even when intent differs.

**Corollary (early cutoff).** Records address outputs by content. If a
re-executed action yields an output tree with an unchanged digest, every
downstream request (whose views embed that digest) is byte-identical to its
cached form — the rebuild cascade stops. This is the "constructive traces"
rebuilder of _Build Systems à la Carte_, obtained structurally. (À la carte
decomposes build systems as scheduler × rebuilder; ours is eos × CAS-traces,
and this model adds the axis their taxonomy lacks: observation policy.)

## 3. Facts, witnesses, and promotion

### 3.1 The record

```
ExecutionRecord = {
  req_digest, exit_code,
  outputs   : [tree_digest],          -- ingested from declared output paths
  stdio     : blob digests,
  observed  : ObservationDigest?,     -- iff policy.observe = Trace (§4)
  context   : { executor id, time, world summary σ }   -- trials only
  signature : executor identity
}

record_core(r) = (req_digest, exit_code, outputs)
```

**All record-equality claims in this model quantify over `record_core`,
never the full record** — the signature and context differ across
executors _by construction_, and stdio is not bit-stable even for
reproducible builds (timestamps, parallel interleaving). Reproducibility
(§2.3), k-record-equal attestation counting (§3.4), and golden-request
conformance (§6.1) all mean _core_-equality. (Correction F4, second
adversarial review: v0.6 let three inequivalent equalities drift across
those sections.)

Note what is _absent_: `BuildResult` carrying a read-set (Gate 0's design)
is mis-homed under this model. Observation is an execution-policy output,
attached to the record of whichever execution was traced — typically a
trial, never mandatory for builds.

### 3.2 Publication gating

A build fact is _advertised_ (published into the fact channel — appended
atom metadata per htc-sad §6.10) according to a **gating policy**: e.g.
"advertise `BuildRecord` iff a passing test attestation for `req_digest`
exists." The build's fact-hood is never conditional on the test — the cache
entry exists the moment the action completes (nrd's requirement: a failed
test never forces a rebuild) — only its _advertisement_ is gated. Gating is
configuration on the fact channel, not scheduler structure.

### 3.3 Promotion: attestation → intent (the cargo-update law)

Record-mode fetch is a trial (`net: open` through the recording proxy).
Its attestation contains the discovered map `request → blob digest`. The
tool then **promotes** that attestation into _intent_: fetch-set entries
written into the lock (plugin-typed, per ADR-0005 §7). Promotion is the
unique arrow from the trial stratum back into the deterministic one:

```
trial (record-mode fetch) ──attestation──▶ promotion ──▶ lock entries
                                                          (intent)
⇒ all subsequent builds run with net: pinned(replay map) ∈ Det
```

This is the same shape as `cargo update` writing `Cargo.lock`, and the same
epistemics as a Nix FOD hash bump — formalized as the _only_ sanctioned way
world-dependence enters the deterministic stratum: **through signed,
reviewed intent, never at execution time.**

**Adopted lockfiles: promotion's door is not always needed** (v0.9, nrd).
For ecosystems that already ship a pinned, checksummed lockfile
(Cargo.lock, package-lock.json, go.sum), the net channel's pin payload
can be **adopted** rather than discovered: the language lockfile is
already reviewed, pinned intent — and it lives _inside the atom's
sources_, hence inside the closure, hence inside `action_id`, so request
completeness (§2.1) holds with zero extra machinery. A tiny
per-ecosystem **proxy adapter** enumerates the lockfile's (url, digest)
pairs as the replay map — the build fetches exactly what the language
lockfile declares and nothing else. No record trial, no promotion, no
redundant re-declaration in the atom lock. This reduces the \*2nix
translators' entire job to its essence: not IR translation into build
instructions, just fetch-set enumeration through a small compatibility
interface. Record-mode → promotion remains the door for lockfile-less
ecosystems; both doors end at the same place — signed, digested intent
pinning the channel.

### 3.4 Whose facts are facts: signer trust

This is a _decentralized_ stack: records arrive signed by executors you do
not control. Hermeticity is a property of _honest_ execution — it does
nothing to make a remote builder honest. So **fact-hood is relative to a
trust anchor set**: a record is accepted _as a fact_ (cache-usable,
substitution-grade) only when its signer lies within the consumer's
configured anchors; outside them, the very same record is epistemically an
attestation — evidence from a foreign party, subject to acceptance policy
like any trial's witness. Reproducibility attestations (§2.3) are the
hardening mechanism: `k` record-equal executions from independent signers
upgrade confidence without requiring local rebuild. This is the classical
substituter problem (Nix's binary-cache trust), placed where it belongs:
**the action/trial stratification governs what a record _can_ be; signer
trust governs what it _is, to you_.** The concrete anchor format and
policy language ride on atom's existing signing machinery (Coz identities)
and are deliberately left to the trust-model spec, not fixed here.

### 3.5 Promotion is reified bind (the IFD answer)

Moggi's value/computation distinction is this model's strata: an
`ExecutionRequest` is a **reified effect description** — a pure,
content-addressed value describing a worldly computation without
performing it. The algebraic stratum constructs and composes descriptions
purely; the executor is the boundary that runs them; results re-enter the
pure universe as data. The system state is always pure (one Merkle
universe holding sources, requests, records, compositions, certificates
alike); impurity exists only in the transition `execute`, never in the
state — effects are edges, nodes are values.

In _Build Systems à la Carte_ terms, dynamic dependency — computing the
next build from a previous build's _result_ — is monadic `bind`, and
Nix's import-from-derivation is bind **hidden inside an evaluator that
claims purity**: evaluation blocks on builds, analyzability and caching
collapse, and the pain is structural. This model has bind too:

> **Promotion is bind, reified and priced at a signature.** The
> continuation runs only after the result is a first-class fact promoted
> into signed intent; monadic builds are realized as an _iteration of
> applicative generations_ with signatures at the join points. Each
> generation remains fully static and analyzable; the dynamic step is
> explicit, audited, resumable.

The frame is adopted for its laws, not its vocabulary (stated as
obligations, P6, not as assumed structure): _left identity_ — promoting
an already-known fact is equivalent to having declared it as intent;
_idempotence_ — re-promoting without change is a no-op; _confluence of
independent promotions_ — concurrent promotions touching disjoint intent
(two record-mode fetches of different deps; a fetch promotion racing a
closure-fault promotion) MUST merge without conflict. The third is a
genuine engineering requirement surfaced by the frame: **the lock must be
confluent under independent promotion**, conflicting only on genuine
overlap. And the frame yields a standing rejection rule: any future
proposal that computes intent mid-elaboration without a signature is a
hidden bind — recognizable and refusable on sight.

## 4. The two closures

### 4.1 The true dependency set is not computable

For an artifact `a` (an output tree), the dependency notion must be
**indexed by view** — v0.6 carried one symbol for two quantifications,
which was a genuine formal error (caught by the second adversarial
review, F1):

```
Dep_v(a) = { p ∈ Path | some execution of a within view v reads p }
Dep(a)   = ⋃_v Dep_v(a)        (the liberal union, over all views)
```

`Dep(a)` is undecidable (Rice's theorem; the classic witness is
`dlopen`-by-computed-string — htc-sad §6.3's motivating case). **No component of this system ever claims to compute it.**
Everything below is about maintaining _certified bounds_ — and the two
bounds quantify over _different_ dependency sets, which is why the index
matters.

### 4.2 The certified interval

```
R_static ∪ R_observed ∪ …   ⊆   Dep(a)          (evidence: liberal union)
Dep_v(a)                    ⊆   dom(⟦v⟧)         (structure: per view)
```

- **Upper bound — structural, free, and per-view.** The namespace bounds
  the observable universe: a process cannot read what does not exist in
  its view, so `Dep_v(a) ⊆ dom(⟦v⟧)` holds _by construction_, for each
  view separately — no tracing, no checking, no FUSE required. What does
  **not** hold is `Dep(a) ⊆ dom(⟦v⟧)` for any particular `v`: an
  execution in a _larger_ view can read paths this view lacks. That is
  not a defect; it is the definition of a closure fault (§4.4), and the
  reason runtime-only dependencies (lazily-loaded plugins, CA
  certificates, the `awk` a script shells out to — things no build-time
  execution ever touches) are _real_ in this model. Nix never had to say
  this because hash-scanning made runtime references a subset of
  build-time presence by construction; this model deliberately does not
  inherit that coincidence, and the package-contract repair discipline of
  [Composition Model §4](composition-model.md#4-interfaces-the-typing-of-compositions) exists
  precisely because of it.
- **Lower bound — evidential.** `R_static`: name references extracted
  structurally (ELF `DT_NEEDED`, shebangs, import syntax — the Debian/RPM
  lineage, htc-sad §6.2), each resolved binding carrying a satisfaction
  proof. `R_observed`: reads captured by traced executions (trials with
  `observe: Trace`), each an unforgeable member of `Dep(a)` — observation
  is sound (every observed read really happened) and incomplete
  (unexercised branches), the exact dual of static analysis (complete over
  syntax, blind to computed loads). The two compose because they
  under-approximate from independent directions.

There is a **third justification kind, and it is not evidence**:

- **`Declared` — the human escape hatch.** Because `Dep(a)` is
  uncomputable, detection _will_ miss; a human may declare a runtime
  dependency directly (signed intent, like any lock entry). Epistemics
  kept honest: a declaration asserts nothing about `Dep(a)` — the human
  may be wrong in the fat direction, and that is _safe_ (an unused
  binding is closure bloat inside the sound upper bound, prunable later
  by evidence). Declarations therefore participate in `J`'s generating
  set (§4.3) but **never in the certified lower bound** — the interval
  above remains evidence-only. A declaration must resolve within the
  candidate set (containment is preserved); it is the runtime analogue of
  a lock entry, and it rides the same signed-intent discipline (§3.3).

### 4.3 The justified closure

The runtime composition is computed as the least fixpoint (htc-sad §6.4)
over the evidence:

```
J = μX. bind(R_static(a) ∪ R_observed(a) ∪ R_declared(a) ∪ requires(X))
```

_Proof obligation P2 (real, small, Lean-able): `J` is monotone in its
evidence and terminates (Knaster–Tarski on the finite powerset of the
candidate set); refinement by new observations only grows `J` toward
`Dep(a)`, never oscillates._ Every entry of `J` carries a **justification
object** — a binding proof or an observation record — so the closure is not
merely computed but _explainable_, entry by entry (contrast: Nix's closure
is "every path whose hash appears in the output bytes," unexplainable).

### 4.4 Graceful degradation and the refinement loop

**Property (build-view sufficiency — relative, not absolute).** v0.6
claimed the full build composition is "always a sound runtime
composition"; that is false as an absolute (runtime-only dependencies,
§4.2, are the counterexample — and the model's own repair story
contemplates exactly them). The true statement: `dom(⟦build view⟧)` is
sound _with respect to every behavior witnessable within it_ — shipping
fat covers everything build-time evidence could ever justify, and the
residue (runtime-only deps) is a **completeness** gap, never a
**soundness** gap: minimization only prunes entries with no
justification, so nothing evidence supports is ever lost. The residue is
handled by the two mechanisms built for it — `Declared` bindings (§4.2)
and fail-closed closure faults feeding refinement (below). An artifact
with no trials run is still shippable fat; it simply carries the honest
caveat that its runtime-only residue is undiscovered. The core-vs-hook
resolution stands: **the capability is core, its exercise optional per
artifact.**

**Property (fail-closed refinement).** Running under `J` in production, a
read outside `J` is a **closure fault**: denied, logged _with the name that
missed_. The fault is itself an observation — a new justification candidate
— and by P2's monotonicity, incorporating it strictly grows `J` toward
`Dep(a)`. The runtime closure is thus **refined over the artifact's
operational life**: never assumed complete, always improvable, always
inside the sound upper bound. Formally this is _one_ mechanism, applied
twice: a least fixpoint — over declared lock structure for the build
closure, over a monotonically accumulating evidence set (a Kleene chain in
the evidence parameter) for the runtime closure. The two closures differ
in _where their generating sets come from_ (declaration vs. observation),
not in their fixpoint mathematics. (v0 dressed this as an algebra/
coalgebra μ/ν duality; the adversarial review correctly struck that — no
coinduction is constructed anywhere, and the operational content needs
none.)

## 5. Compositional structure

### 5.1 The action DAG over the merge monoid

Deterministic executions compose: action `r₂` may embed `r₁`'s output
digests in its view (via `⊕` and binding). The structure actually used is:

- **The action DAG** — nodes are requests, edges are output-digest flows
  into downstream views. Sequential dependency is edge order.
- **Independent subgraphs** merge views by `⊕` (the partial monoid of
  [Composition Model §3](composition-model.md#3-composition-merge-is-a-partial-commutative-monoid)) —
  and eos's parallel dispatch is exactly the evaluation of independent
  subgraphs.

**Theory transfer, split honestly (correction F2, second adversarial
review — v0.6 overclaimed this as "unchanged"):**

- The **Lean bounds** (Theorems 1–7 + Main) are genuinely node-agnostic
  and transfer to requests-as-nodes without amendment.
- **Track A's overlap-safety argument transfers after a wording repair,
  not a new mechanism.** (v0.8 — nrd's correction dissolved v0.7's
  demand for a "witness-reconciliation rule," which only existed because
  of the single-slot cache invariant this model wrongly imported.) The
  eos scope note justifies concurrent-completion safety via "identical
  content at identical output-tree digests" — a determinism premise §2.2
  denies. The _correct_ justification needs no determinism: witnesses
  accumulate (set-insert, commutative), each downstream branch coheres by
  binding the concrete digests it consumed, and witness selection at
  request formation is a recorded choice (§2.2). Queued doc amendments
  only: the eos scope note's justification, and eos-sad §6.6's "exactly
  one BuildRecord per action" → one per _build event_ (accumulating).
  P4's scope (§8) is correspondingly modest: check that set-accumulation
  under concurrent completion preserves the dispatch state machine's
  invariants — expected to be near-trivial.

(v0 claimed a symmetric monoidal category here; the adversarial review
correctly struck it — morphism composition was not well-defined as stated
(binding data is not carried by the morphism), `⊕`'s partiality breaks the
monoidal laws without machinery this model neither invokes nor needs, and
no result below used the categorical structure. The partial monoid and the
DAG carry everything.)

### 5.2 Tests are not build nodes; trials deposit but never bind

Tests (of either stratum) do not appear in the _build_ DAG: no build
request's view embeds a test's output. They decorate the DAG — attached to
the nodes whose outputs they exercise.

**The deposit/bind law.** A trial MAY deposit values into the CAS (record-
mode fetch deposits every response body as a blob — the draft's own
flagship trial does this, so "trials produce no outputs" would be false).
What a trial MUST NOT do is **bind** those values into any view: binding
requires **promotion through signed intent** (§3.3 — lock entries, written
by the tool, reviewed as intent). CAS deposit is inert — an unbound blob
influences nothing; binding is the only door into the deterministic
stratum, and it is guarded by signatures. This law is also the answer to
v0's open question 5: benchmark artifacts and coverage reports are CAS
blobs referenced _by the attestation_, never view-entering outputs.

**Theorem (no-rebuild-on-test-edit).** Test parameters occur only in test
requests (of either stratum). `action_id` and every _build_ `req_digest`
are computed from objects that contain no test parameters. Hence editing
test configuration changes no build identity, invalidates no build cache
entry, and triggers no rebuild — _structurally_, not by scheduler courtesy;
a hermetic test's own cached result is invalidated (its `req_digest`
changed), which is exactly the desired scope of re-execution. (Manifest
support: test/check params must live in a manifest section that feeds test
requests only — an atom-API obligation this model imposes on L1/L4.)

The nixpkgs pathology this kills, named precisely: coupling build and test
into one derivation makes the _pair_ the unit of caching, so a sandbox-
induced test failure poisons a perfectly good build. Here the build fact
stands alone; the trial's failure gates advertisement (§3.2), and its fix
(test-flag edit, network policy change) re-runs _only the trial_.

### 5.3 The override algebra: generators live inside the model

_Re-homed: [Composition Model §7](composition-model.md#7-the-override-algebra-generators-live-inside-the-model).
Execution's stake is the rebuild frontier: consumers whose interface
satisfaction fails under a `subst` need intent edits (new `action_id`s),
and early cutoff (§2.4) prunes every descendant whose rebuilt output is
digest-identical._

## 6. The executor

### 6.1 The coalgebra and its bisimulation

```
F_exec(X) = ExecutionRequest → X × Result<ExecutionRecord>
```

Two executors are equivalent iff:

- on **actions**: every record either produces lies in `⟦req⟧` — i.e.
  their admissible sets coincide (consistent with §2.2's relational
  semantics: conforming executors may return _different witnesses_ of a
  non-singleton set, and the cache's first-witness fiat absorbs that). The
  _testable_ form: on requests empirically known reproducible (singleton
  `⟦req⟧` attested per §2.3), conforming executors MUST produce
  digest-identical records — golden-request conformance uses exactly
  these;
- on **trials**: they produce _valid_ attestations for identical requests
  (agreement up to the policy's licensed nondeterminism — a test may pass
  on one run and fail on another _only_ through open channels or residual
  nondeterminism).

This is the deployment-interchangeability story (local runner ≅ remote
worker ≅ adapter-bridged worker), and — more practically — the
**conformance criterion** for any executor implementation: bisimilarity on
the action stratum is directly testable (golden request → expected record
digest). The existing `BuildEngine` bisimulation (publishing-stack-layers
§2.4) is this criterion restricted to builds.

### 6.2 The session type

```
ExecuteSession (client → Executor):
  !ExecutionRequest .
  & { Known:     ?ExecutionRecord . end     -- action: cache hit
                                            -- trial: accepted attestation
    , Scheduled: ?ExecutionRecord . end     -- executed now
    , Refused:   ?PolicyError . end }       -- policy unsatisfiable here
```

- `BuildSession` (existing, ion→eos) is the restriction of `ExecuteSession`
  to actions derived from atoms: `Known = BuildPlan::Cached`,
  `Scheduled = NeedsBuild`-then-apply. The existing finding
  `CacheSession ≅ BuildSession` is the action-stratum instance of a general
  law: **the `Known` branch is inhabited by identity on actions and by
  acceptance-policy on trials** — same protocol shape, different epistemic
  license.
- `TestSession` = `BuildSession ; ExecuteSession(trial)` — a composite: the
  trial's view is assembled from the build's output ∪ build closure.
- Progress/duality for `ExecuteSession` is _proof obligation P3_ (cheap,
  mechanical — same treatment BuildSession got).

### 6.3 Enforcement obligations (the per-channel audit)

What makes §2.2's theorem true. For each channel, the executor MUST
implement the policy state by the named mechanism — this table is the
conformance checklist, and it is honest about today's gaps:

| Channel           | closed                     | pinned                                       | status today                                                                                                                                        |
| :---------------- | :------------------------- | :------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------- |
| filesystem        | mount ns: view is the root | — (view IS the pin)                          | **exists** (bind/OCI); composefs pending privilege story                                                                                            |
| net               | empty netns                | replay proxy over bound socket (`ProxyOnly`) | **exists** (Gate 0)                                                                                                                                 |
| ids               | userns fixed mapping       | fixed uid/gid in spec                        | **exists**                                                                                                                                          |
| clock             | — (cannot close)           | `SOURCE_DATE_EPOCH` env; **not virtualized** | **gap, honest**: env-pinning is convention, not enforcement; full pinning needs time-ns/seccomp — decide if we care (Nix doesn't enforce it either) |
| entropy           | —                          | seeded/deterministic `/dev/urandom`?         | **gap, honest**: unpinned; same posture as every mainstream build system                                                                            |
| cpu-features      | —                          | pinned `-march`/target in params             | convention via action params                                                                                                                        |
| nproc / CPU count | —                          | pinned via cgroup cpuset or params           | **unpinned today**: `make -j$(nproc)` reads it (Gate A/B literally did); affects parallelism (usually not bytes, but _usually_ is not a proof)      |
| readdir order     | —                          | pinned by ⟦view⟧'s sorted denotation (§1.1)  | obligation on every mount mechanism                                                                                                                 |
| /proc contents    | partially maskable         | —                                            | **partially open**: OCI masks some paths; a build reading `/proc/meminfo` sees the world — audit and extend masking                                 |

Two further absolute obligations: the executor **materializes exactly
`⟦view⟧`** (no more — the containment bound depends on it) and **never
interprets `command`**. Everything else (which mount tech, which trace
tech, fork daemon vs native binary) is implementation freedom below the
bisimulation line.

**Observation instruments and their honesty conditions.** The model
requires only _soundness_ of observed reads (every logged read really
happened); instruments differ in _coverage_, and coverage is an empirical
property that MUST be named per instrument: ptrace+seccomp (~25% measured
overhead at zlib scale) misses reads issued through syscall surfaces the
filter doesn't trap — `io_uring` is the known live hazard, direct syscalls
a second — so an observed read-set from ptrace is a lower bound _relative
to its trapped surface_. FUSE had the complementary profile: structurally
complete over the mount (every read flows through the daemon) at ~5.9×
cost. Missed reads are survivable by construction — they surface later as
fail-closed closure faults (§4.4) and refine `J` — but the coverage gap
must be recorded with the observation, not assumed away. eBPF is a future
instrument with its own privilege/coverage trade.

### 6.4 Materialization, installation, and transactional update

_Re-homed: [Composition Model §8](composition-model.md#8-materialization-installation-and-transactional-update).
The executor-side residue is already stated in §6.3: the executor
materializes exactly `⟦view⟧`. Installation and transactional update
involve no `execute` at all — they are the static primitives' own
operations._

## 7. Boundary mappings

### 7.1 REAPI: adapter, not core

REAPI (`build.bazel.remote.execution.v2`) validates this factorization at
industrial scale — its Action = (Command, input-root Merkle digest,
platform) → ActionResult is our request/record restricted to the action
stratum, and Bazel already runs both builds and tests over it. We adopt the
**shape natively** and treat literal REAPI as a **lossy adapter functor**
at the boundary: their Directory protos ↔ castore trees (re-hash), their
stringly platform properties ↔ our policy classification (lossy — _the policy
semantics is precisely our novelty_), their unsigned actions ↔ our signed
intent (lossy). Lossy in both load-bearing dimensions ⇒ edge tier (like
Export materialization), never the core contract. What the adapter buys,
when wanted: off-the-shelf worker fleets (BuildBarn/NativeLink-class) as
dumb action-stratum capacity, and foreign clients.

### 7.2 What this model corrects in the existing corpus

| Where                                                 | Today                                         | Under this model                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| :---------------------------------------------------- | :-------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| ADR-0005 §11 three "materialization tiers"            | Observe/Fast/Export as mount tiers            | **Materialization and observation are orthogonal axes**; "Observe" as a mount tier dissolves; Fast (composefs) is the default mount, tracing is a policy                                                                                                                                                                                                                                                                                                                                                                        |
| Fork: `BuildResult.read_set`, FUSE recording (Gate 0) | observation welded into build materialization | mis-homed: observation is execution-policy output; build records carry no read-set. FUSE is **demoted from mechanism to instrument** — removal from the fork is sequenced on the ptrace observer proving coverage at openssl scale (io_uring included, §6.3), not decreed by this model. This reverses an earlier design preference for FUSE-based observation (chosen for needing zero ptrace/seccomp machinery) on measured cost evidence — **ratified by nrd 2026-07-07**: ptrace is the runtime-closure capture instrument. |
| htc-sad §2.3 `BuildRecord.observed_read_set_digest`   | read-set as a standing BuildRecord field      | same defect class as the row above, at the _doctrinal_ home (F9, second review — v0.6's sweep missed it): becomes optional/absent for untraced executions; populated only when policy.observe was on                                                                                                                                                                                                                                                                                                                            |
| htc-sad §3.5 executor trait `build(...)`              | build-shaped                                  | specializes `execute(...)`; the trait generalizes, build becomes the Det instance                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| eos DAG                                               | build nodes                                   | nodes are requests; trials attach as non-morphism decorations; gating policies on the fact channel                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Fork trajectory                                       | castore + build daemon                        | once a native execution runtime exists, `OCIBuildService` is subsumed; fork shrinks toward castore — tightening the GPL seam                                                                                                                                                                                                                                                                                                                                                                                                    |

### 7.3 What this model deliberately does not own

Added v0.8, after two corrections in one review cycle traced to the same
root: reviewers (and the formalizer) importing obligations the _system_
already discharges at sibling layers. The standing boundary, for every
future reader and reviewer of this document:

- **Acceptance is the trust layer's job.** Which witnesses count, whose
  signatures suffice, how many record-equal executions upgrade
  confidence — §3.4 hooks, atom's signing machinery, never new
  invariants here. In particular: the record store is multi-valued and
  needs no canonical winner; do not re-import a centralized
  single-slot cache.
- **Write concurrency is dissolved by topology, not solved by algebra.**
  Lock writes are phased within one atom's lifecycle and land in
  per-atom files; environment-level intent lands in certificates. Do not
  demand merge operators for races the design never runs.
- **Sharing relations live consumer-side.** `requires`-style edges
  express many-to-many naturally; do not patch provider-side
  back-pointers into sets.
- **Human-facing concurrency (two people editing one atom) is VCS
  territory**, same as every lockfile ecosystem today — out of scope.

## 8. Proof obligations — and deliberate non-obligations

Worth proving (small, load-bearing, mechanical):

- **P1** _(re-homed v0.10)_ — owned by the
  [Composition Model §10](composition-model.md#10-proof-obligations):
  `⊕` partial-monoid laws + exact denotational conflict detection.
- **P2** — justified-closure fixpoint: monotone, terminating, refinement-
  monotone (Lean; disciplines the closure computer — this is the algorithm
  the whole minimal-composition story rests on).
- **P3** — `ExecuteSession` progress/duality (mechanical; same as
  BuildSession's treatment).
- **P4** _(re-scoped in v0.2)_ — a small state-machine specification of
  the **relational semantics and cache confluence** (§2.2): channels as
  nondeterministic inputs, the sandbox as a filter, residual
  nondeterminism as internal choice; show (a) under Det the admissible
  record set is invariant across channel valuations, and (b) first-witness
  caching preserves DAG coherence (downstream always builds against the
  witness that won). TLA⁺-able at modest scope. Valuable as the
  _specification_ of the executor, not as mathematical news.
- **P5** _(added in v0.2)_ — request-serialization completeness (§2.1):
  every input the executor consults is reachable from the digest preimage;
  pin payloads included. An audit obligation with a checkable inventory
  more than a theorem.
- **P6** _(added in v0.6; re-scoped in v0.8 per nrd)_ — promotion
  coherence laws (§3.5): left identity, idempotence, and
  **reorder-invariance of independent sequential promotions** (running
  two independent promotions in either order yields the same lock).
  v0.6–v0.7 additionally demanded a concurrent lock-merge operator —
  an overcorrection: the system's write topology is _phased by design_
  (resolution pre-build; fetch pins once at trial end; runtime-dep
  discovery post-build; repairs land as declared contract facts and
  re-formed certificates, not package locks), so no two writers race
  one file. Whole-file atomic
  write stands. Testable directly against the lock tooling; the one
  genuine residual spec gap is `[lock-dep-ordering]`'s undefined
  `Either<AtomId, Name>` sort order.
- **P7** _(added in v0.7, from F3; extended v0.8)_ — `resolve` is a
  function and total: determinism of the action→request map (layout,
  command, pin assembly, **and witness selection over the fact
  snapshot** per §2.2), materializer-version inclusion in the identity,
  totality gated on the toolchain-pin lock entry. §2.4's two-level cache
  coherence and cross-intent dedup are conditional on P7 until
  discharged.
- **P4 (extended v0.7; shrunk v0.8)** — covers concurrent completion as
  _set-accumulation_: check that commutative witness insertion preserves
  the dispatch state machine's invariants. No reconciliation rule (v0.8
  struck it); the eos amendments were wording-level and have landed (§5.1).

Deliberately NOT proof targets, with reasons:

- **Bit-reproducibility** — empirical by nature (§2.3); the artifact is an
  accumulating attestation, and pretending to prove it would re-conflate
  hermetic/reproducible.
- **Completeness of `Dep(a)`** — undecidable (Rice); the design is
  structured around never needing it (certified interval + fail-closed
  refinement).
- **The scheduler** — already proven; transfers unchanged.

## 9. Open questions for nrd

1. ~~Naming~~ — **RATIFIED (nrd, 2026-07-07)**: `action`/`trial` for the
   two strata; `record`/`attestation` for their outputs. Canonical.
2. **Clock/entropy posture.** Accept the honest gap (Nix-parity,
   convention-pinned) or commit to enforcement (time-ns, seccomp on
   clock_gettime, deterministic urandom) as a differentiator? Cost is real;
   my lean: record the gap, defer enforcement, revisit when reproducibility
   attestations exist to measure whether it matters in practice.
3. **Trial acceptance policy.** _(Narrowed in v0.2: hermetic tests cache
   as actions — no policy needed there.)_ For genuine trials (networked
   tests): what makes an attestation "fresh enough" to skip re-running?
   (Time-bound, executor-set-bound, per-atom policy?) Needs a decision
   before the scheduler can gate on trial evidence.
4. **Executor binary placement** (deliberately unconstrained by the model):
   extend the fork's daemon near-term vs. a fresh axios-native executor
   crate. The §7.2 trajectory (fork shrinks toward castore) suggests where
   it ends; the question is the path.
5. ~~Trial output scope~~ — _resolved in v0.2 by the deposit/bind law
   (§5.2): trials deposit CAS values referenced by their attestations;
   binding into views happens only via promotion through signed intent._
6. ~~Observation-posture reversal~~ — **RATIFIED (nrd, 2026-07-07)**:
   ptrace is the runtime-closure capture instrument, superseding the
   earlier FUSE-observation preference on measured cost evidence (5.9×
   vs ~25%). The §7.2 sequencing rule stands: FUSE machinery is removed
   from the fork only after ptrace proves coverage at openssl scale.
7. **Trust-anchor policy language** _(added in v0.2, from §3.4)_: fact-hood
   is signer-relative; the anchor set format and the acceptance policy for
   foreign records (reproducibility-attestation thresholds included) need
   their own spec, riding on atom's Coz identities.
8. **The ambient base** _(added in v0.3, from §1.4)_: an environment's
   coherence certificate must state its residual `Req` — the declared
   base it stands on (kernel ABI, dynamic loader, VDSO). What is the
   canonical ambient set, and who owns its definition? (This is the
   substrate's version of "what does glibc get to assume.")
9. **Coherence choice function defaults** _(added in v0.3)_: "supported
   shared versions, user-selectable with sane defaults" — the default
   policy for choosing one provider per (ns, name) at environment
   formation needs an owner and a spec (prefer-same-atom, then lock
   order, is ion's existing determinism rule; does it lift unchanged?).
10. **Environments as atoms** _(added in v0.3)_: publishing an
    environment as a signed atom (content = the composition) would give
    versioning/resolution over environments with zero new machinery — my
    lean is yes, but it makes environments _publishable intent_, which
    deserves a deliberate decision rather than a default.
11. **The override operator set** _(rewritten in v0.4 — v0.3 misfiled
    this as a "generator gap")_: overlays are not a missing language
    feature; they are operations inside the algebra (Composition Model §7 — `subst`,
    extend, prune, choice-function override), with blast radius computed
    by the interface layer instead of assumed total. What remains open
    is only the _surface_: the canonical operator set, whether overlays
    are first-classed as signed content-addressed objects, and the CLI
    verbs (eka) that apply them. Pure-build-flag variants ("everything
    with clang") still route through intent edits (action params) and
    enjoy only early-cutoff pruning, not interface pruning — worth
    saying honestly in any paper: the LEGO win is at the binding layer;
    intent-layer changes still rebuild what they touch, merely with a
    tighter bound than input-addressing gives.
12. **Env vars as an interface namespace** _(added in v0.4, from the
    generator audit)_: dev shells and some runtimes need
    `JAVA_HOME`-style bindings the FHS view doesn't eliminate. Candidate
    answer with zero core changes: `ns = "env"` as a namespace plugin —
    an env-var requirement is an interface fact, satisfied at environment
    formation, values rendered at materialization. Decide when the
    dev-environment surface is designed.
13. **Atom kind schema policy** _(added in v0.5, from §1.5; narrowed same
    day)_: strata are closed and law-bearing; kinds are open and
    schema-level. The mixed-atom question is _answered in the model_ —
    lawful, with the system atom as canonical case, governed by
    pipelines-cross/steps-don't and the promotion door. What remains is
    only schema: the kind discriminator's shape in the manifest
    (`[package]` / `[environment]` / `[generator]` tables? a `kind`
    field?) and whether hygiene wants mixed atoms discouraged despite
    their legality.
14. **The lock format** _(added v0.6; re-assessed v0.8 after nrd's
    whole-system corrections)_: the lock is binding-algebra instantiation
    #1 and the promotion target (§3.3); its formal treatment is an
    operational TOML spec, and _some_ elaboration is owed — but less
    than the second review claimed. Dissolved by the phased write
    topology (P6 re-scope): the demands for a concurrent merge operator
    and field-level merge granularity. Superseded by a better design
    direction (nrd): rather than pluralizing the single-valued `owner`
    back-pointer on fetch entries, **generalize consumer-side
    `requires`-style edges to cover fetch entries** — shared ownership
    becomes many-to-many for free, GC becomes reference-counting over
    edges, `owner` becomes derived or deleted. Still genuinely owed:
    the `Either<AtomId, Name>` sort-order definition; a
    tool-authored-entry liveness class (the reconcile/sanitization purge
    rules must not eat promoted fetch entries — pre-existing P4 flag);
    whether lock and certificate share one formal treatment (Composition
    Model §5's
    symmetry); and P7's resolution-determinism elaboration.
