# MODEL: Publishing Stack Layer Boundaries

## Domain Classification

**Problem Statement:** The decentralized publishing stack is organized
as three independent workspaces (atom, eos, ion) communicating through
trait interfaces. Before implementation, the trait boundaries need
formal validation to confirm: (1) the interfaces are behaviorally
correct — implementations can be swapped without downstream breakage;
(2) the protocols between layers enforce correct operation ordering;
(3) the layered composition preserves properties end-to-end.

**Domain Characteristics:**

- **Coalgebraic observation** — traits define observers over abstract
  state (AtomSource, BuildEngine), not constructors. Identity is
  behavioral, not structural.
- **Functorial resource flow** — atoms transfer between stores with
  different capabilities while preserving identity (atom-id stable
  through ingest).
- **Protocol sequencing** — operations must occur in defined order:
  claim before publish, resolve before plan before apply.
- **Layer discipline** — strict downward dependency (ion → eos → atom).

**Model Scope — what is and is not captured:**

This model operates at the **logical trait boundary** level. It captures
the behavioral contracts between workspaces as if callers invoke trait
methods directly. This is sufficient for validating interface design,
protocol ordering, and implementation interchangeability.

| In Scope                                  | Out of Scope                              |
| :---------------------------------------- | :---------------------------------------- |
| Trait behavioral contracts (coalgebras)   | Transport mechanism (IPC, network, gRPC)  |
| Protocol ordering (session types)         | Serialization / wire format               |
| Store relationships (ingest homomorphism) | Authentication / authorization            |
| Concurrency model (parallel builds)       | Git object internals (atom-git impl)      |
| Error recovery asymmetry (plan vs apply)  | Nix/snix evaluation internals (eos impl)  |
| Static ontology (olog)                    | Dependency resolution (SAT solver, locks) |
| Scheduling correctness (bisimulation)     | Manifest format (ion.toml parsing)        |
|                                           | Network topology (distributed eos, peers) |

The model's coalgebras treat trait invocations as abstract observations
regardless of whether the call is local, crosses a process boundary via
IPC, or traverses a network. Bisimulation invariance guarantees this
abstraction is safe: if two implementations produce the same
observations, they are interchangeable — whether the observation
arrives via a function return or a deserialized RPC response. Transport,
serialization, and authentication are orthogonal concerns that layer
_on top of_ the trait boundaries modeled here, not within them.

## Formalism Selection

| Aspect                  | Detail                                                           |
| :---------------------- | :--------------------------------------------------------------- |
| **Primary Formalism**   | Coalgebra (behavior endofunctors, bisimulation)                  |
| **Supporting Tools**    | Session types (protocol ordering), Olog (domain ontology)        |
| **Decision Matrix Row** | State + observation → Coalgebra; Protocol → Session types        |
| **Rationale**           | Traits-as-observers is the dominant pattern; bisimulation is the |
| ...                     | correct equivalence for implementation interchangeability.       |
| ...                     | Session types capture the protocol ordering that coalgebra       |
| ...                     | alone cannot express. Olog grounds the static ontology.          |

**Alternatives Considered:**

- **Pure functorial migration:** Too narrow — trait boundaries involve
  behavioral contracts, not just data restructuring.
- **Linear logic as primary:** Atoms are freely duplicable and
  discoverable; no linear resource discipline on data flow.
- **Hyperdoctrine:** No quantified predicates over type families
  needed — associated types are simple type-level programming.

**Formalism Provenance:**

The formalisms above were selected by applying the Decision Matrix to
the domain (hidden state → coalgebra, sequenced protocols → session
types, static ontology → olog) independently of the trait signatures in
ADR-0001. The ADR traits and the coalgebras converge because Rust
traits-as-interfaces and coalgebras-as-observers are the same concept
in code and mathematics: both define observations on opaque state. This
is structural correspondence, not a circular derivation.

Operations with command-like character (`apply`, `store`, `ingest`)
are modeled as observations because the model captures inter-layer
boundaries — what the caller sees, not what happens internally. For
internal eos verification (cache coherence, store invariants), a future
refinement to **state-transformer coalgebras** (`c: X → F(X) × X'`)
may be warranted. The boundary coalgebras embed into state-transformer
coalgebras via projection, so the current model remains valid.

## Model

### 1. Olog — Domain Ontology

**Objects:**

| Object   | Description                                               | Layer |
| :------- | :-------------------------------------------------------- | :---- |
| Atom     | Fundamental unit of publishing                            | L1    |
| Atom-id  | Content-addressed digest: hash(anchor, label)             | L1    |
| Label    | Human-readable name                                       | L1    |
| Anchor   | Cryptographic commitment establishing atom-set identity   | L1    |
| Owner    | Opaque identity digest (e.g., Coz tmb, Cyphr PR)          | L1    |
| Atom-set | Collection of atoms sharing a common anchor               | L1    |
| Version  | Abstract version (via VersionScheme — ecosystem-agnostic) | L1    |
| Revision | A specific commit in source history                       | L1    |
| Manifest | Metadata: label + version (minimal trait surface)         | L1    |
| Plan     | Engine-specific build recipe (associated type)            | L2    |
| Output   | Engine-specific build result (associated type)            | L2    |
| Artifact | Content-addressed blob in artifact store                  | L2    |
| Digest   | Content-addressed hash                                    | L2    |

**Key morphisms:**

```
has_id:        Atom      → Atom-id     (each atom has exactly one id)
has_label:     Atom      → Label       (each atom has exactly one label)
belongs_to:    Atom      → Atom-set    (each atom belongs to one set)
identified_by: Atom-set  → Anchor      (each set has exactly one anchor)
computed_from: Atom-id   → (Anchor × Label)
claimed_by:    Atom      → Owner       (ownership via signed claim)
pins:          Revision  → Version     (each revision pins one version)
described_by:  Atom      → Manifest    (at a given version)
derives:       Revision  → Plan        (given a build engine)
produces:      Plan      → Set(Output) (a plan produces outputs)
stores_as:     Output    → Artifact    (each output stored as artifact)
addressed_by:  Artifact  → Digest      (content-addressed)
```

**Commutative diagrams:**

1. **Cryptographic chain:** `stores_as ∘ produces ∘ derives` from a
   revision yields an artifact whose digest is deterministic. Same
   revision → same plan → same output → same digest. _Formal statement
   of reproducibility._

2. **Identity stability:** `computed_from ∘ (identified_by ∘ belongs_to,
has_label) = has_id` commutes. Atom-id is computed from (anchor,
   label), neither of which changes across versions. _Formal statement
   that publishing new versions does not alter atom identity._

3. **Ownership independence:** `claimed_by` is independent of `has_id`.
   Ownership can change (claim revocation + reclaim) without altering
   the atom-id. _Formal statement that identity and ownership are
   separate concerns._

4. **Verification chain:** Given a publish CozMessage referencing a
   claim czd, `claim.anchor × claim.label → Atom-id` and
   `publish.dig → Atom-commit → Tree` commute with verification.
   _Formal statement that local verification is sufficient._

### 2. Coalgebras — Behavioral Observation

Each trait defines a coalgebra c: X → F(X) where X is the state space
(any implementation) and F is the behavior endofunctor.

#### 2.1. AtomSource (atom-core, L1)

```
F_source(S) = (AtomId → Option<AtomMeta>)      -- resolve (metadata, not entry)
            × (Query → Set<AtomId>)            -- discover
```

Bisimulation: s₁ ~ s₂ iff resolve and discover agree pointwise. Two
implementations containing the same atoms are interchangeable.

#### 2.2. AtomRegistry (atom-core, L1)

```
F_registry(R) = F_source(R)                   -- inherits AtomSource
              × (ClaimReq → Result<Czd>)       -- claim (AtomId is pre-computed;
                                               --        returns claim czd)
              × (PublishReq → Result<()>)      -- publish
```

Trait inheritance (`AtomRegistry: AtomSource`) is a coalgebra morphism —
the forgetful map dropping claim/publish observers.

#### 2.3. AtomStore (atom-core, L1)

```
F_store(W) = F_source(W)                      -- inherits AtomSource
           × (dyn AtomSource → Result<()>)     -- ingest
           × (Path → Result<AtomId>)           -- import_path
           × (AtomId → bool)                   -- contains
```

**Critical morphism — ingest as coalgebra homomorphism:**

```
∀ source, ∀ id:
  after store.ingest(source):
    resolve(store, id) ⊇ resolve(source, id)
```

The ⊇ (superset) condition is correct — the store accumulates atoms
from multiple sources. Atom-id is stable through ingest.

#### 2.4. BuildEngine (eos-core, L2)

```
F_engine(E) = (AtomRef → Result<BuildPlan<P>>)   -- plan
            × (BuildPlan<P> → Result<Vec<O>>)     -- apply
  where P = E::Plan, O = E::Output
```

`BuildPlan<P>` is a coproduct (sum type) introducing session-type
branching:

```
BuildPlan<P> = Cached { outputs: Vec<ArtifactRef> }
             | NeedsBuild { plan: P }
             | NeedsEvaluation { atom: AtomRef }
```

Bisimulation: e₁ ~ e₂ iff plans agree (variant-matching + content
equality) and apply produces digest-equivalent outputs for equivalent
plans. _Formal justification for embedded/daemon/remote deployment
modes._

#### 2.5. ArtifactStore (eos-store, L2)

```
F_artifact(A) = (Digest → Option<Blob>)        -- fetch
              × (Blob → Digest)                  -- store
              × (Digest → bool)                  -- exists
              × (Vec<Digest> → Vec<bool>)        -- check_substitute
```

**Invariant:** `∀ blob: fetch(store(blob)) = Some(blob)` (round-trip).

#### 2.6. Inter-Layer Morphisms

| Morphism   | Type                       | Direction | Mechanism                                   |
| :--------- | :------------------------- | :-------- | :------------------------------------------ |
| atom → eos | Forget: F_store → F_source | Downward  | Eos reads AtomStore via AtomSource          |
| ion → atom | Full: F_store              | Downward  | Ion exercises ingest, import_path, contains |
| ion → eos  | Full: F_engine             | Downward  | Ion dispatches plan/apply                   |

**Composition law:**

```
ion ──populate──→ AtomStore ──forget──→ AtomSource ──read──→ eos
         ↑                                                      |
    Manifest, VersionScheme                              BuildEngine
    (atom-core abstractions                              (eos-core
     implemented by ion)                                  abstraction)
```

The forgetful functor preserves bisimulation by construction: F_source
is a component of F_store.

### 3. Session Types — Protocol Ordering

Convention: `!T` = send, `?T` = receive, `⊕` = internal choice
(sender selects), `&` = external choice (receiver handles all),
`end` = termination.

#### 3.1. PublishSession (client → AtomRegistry)

```
!ClaimReq . ?Result<Czd> . !PublishReq . ?Result<()> . end
```

AtomId = hash(anchor, label) is pre-computed by the client.
ClaimReq carries (anchor, label, owner, key). On success, returns
the claim's czd. PublishReq carries (anchor, label, claim_czd,
dig, src, path, version). Claim MUST succeed before publish —
the client cannot send `PublishReq` without first receiving the
claim czd to embed in the publish payload.

#### 3.2. BuildSession (ion → BuildEngine)

```
!AtomRef .
?BuildPlan<P> .
& {
  Cached:            end,
  NeedsBuild(P):     !P . ?Vec<Output> . end,
  NeedsEvaluation:   !AtomRef . ?P . !P . ?Vec<Output> . end
}
```

The BuildPlan enum is a session type branching point. Each variant
determines the remainder of the interaction: Cached ends immediately,
NeedsBuild requires one apply round-trip, NeedsEvaluation requires
evaluate then apply.

**Finding:** CacheSession ≅ BuildSession — the three cache-skipping
levels (artifact exists, plan exists, nothing cached) are isomorphic
to the BuildPlan variants (Cached, NeedsBuild, NeedsEvaluation).
This confirms that the build protocol precisely captures the
cache-skipping decision tree.

#### 3.3. PopulateSession (ion → AtomStore)

```
⊕ {
  from_registry:  !&dyn AtomSource . ?Result<()> . end,
  from_path:      !Path . ?Result<AtomId> . end,
  from_store:     !&dyn AtomSource . ?Result<()> . end
}
```

Ion selects which population method to use. `ingest` and `import_path`
are alternative entry points, not sequential steps.

### 4. Concurrency, Scheduling, and Error Recovery

#### 4.1. Parallel Build Composition

Ion submits a batch of atoms. Each gets its own BuildSession running
in parallel:

```
BatchBuild(refs) =
  BuildSession(ref₁) | BuildSession(ref₂) | ... | BuildSession(refₙ)
```

**Non-interference property:**

```
∀ ref₁ ≠ ref₂:
  plan(e, ref₁) | plan(e, ref₂)  ≡  plan(e, ref₁) ; plan(e, ref₂)
```

Concurrent planning must equal sequential planning. Guaranteed by the
coalgebraic structure: `plan` is an observer (no shared mutable state).
Concurrent `apply` is safe because ArtifactStore writes are idempotent
(content-addressing: same blob → same digest).

#### 4.2. Session Delegation (Work-Stealing)

Delegation transfers an in-progress session from one worker to another:

```
DelegateSession<S> =
  !S .              -- source sends session continuation
  ?Ack .            -- target acknowledges
  end
```

If a worker completes `plan` and receives `NeedsBuild(P)`, its
remaining session is `!P . ?Vec<Output> . end`. Delegation transfers
this typed continuation to another worker. The target resumes the
session exactly where the source left off — no steps skipped or
reordered.

**Scheduler coalgebra (eos-internal):**

```
F_scheduler(Sch) = (Set<AtomRef> → Vec<Assignment>)    -- schedule
                 × (WorkerId → Set<InProgressSession>)  -- worker_load
                 × (WorkerId → WorkerId → Result<()>)   -- delegate
                 × (() → SchedulerMetrics)              -- observe
```

Bisimulation: two schedulers are bisimilar iff they produce the same
final outputs for the same input set, regardless of strategy. Scheduling
is an optimization; correctness is invariant.

**The scheduler is eos-internal.** Ion sees only `BuildEngine`. The
existing ion → eos morphism (F_engine) is unchanged. Scheduling
introduces no new inter-layer coupling.

#### 4.3. Error Recovery as Protocol

Errors are protocol structure, not just data. The session type must
express recovery options after failure.

**Extended BuildSession:**

```
BuildSession =
  !AtomRef .
  & {
    PlanOk:
      ?BuildPlan<P> .
      & {
        Cached: end,

        NeedsBuild(P):
          !P .
          & {
            ApplyOk:   ?Vec<Output> . end,
            ApplyFail:
              ?Error .
              ⊕ {                         -- scheduler selects recovery:
                retry:     recurse,       -- same worker retries
                delegate:  !Continuation . end,  -- work-steal
                abort:     end            -- report error
              }
          },

        NeedsEvaluation:
          !AtomRef .
          & {
            EvalOk:
              ?P . !P .
              & {
                ApplyOk:   ?Vec<Output> . end,
                ApplyFail: ?Error . ⊕ { retry | delegate | abort }
              },
            EvalFail:
              ?Error . ⊕ { retry | delegate | abort }
          }
      },

    PlanFail:
      ?Error .
      ⊕ { retry | abort }       -- no continuation to delegate
  }
```

**Key insight — plan failure ≠ apply failure:**

- **Plan failure** has no continuation to delegate. The plan hasn't been
  produced. Only retry or abort.
- **Apply failure** HAS a transferable continuation — the plan exists,
  another worker can execute it. Delegation is available.

The scheduler makes recovery decisions (`⊕` internal choice), not ion.
This preserves the architectural boundary: ion submits work, eos handles
recovery.

#### 4.4. Async Extension Property

The plan mandates sync-first traits (KD-14). The model validates that
the trait surface extends to async without structural changes:

- **Coalgebras are async-agnostic.** c: X → F(X) specifies observations,
  not execution semantics. `fn plan(...)` and `async fn plan(...)` produce
  the same observations. Bisimulation is defined on results, not timing.

- **Session types carry natively to async.** Session-typed channels are
  message-passing constructs — they map to async channels directly.
  Parallel composition (`|`) maps to `tokio::join!`. Delegation maps to
  channel ownership transfer.

**Conclusion:** KD-14 is formally validated. The sync-first design is
not a limitation — the model extends to async without restructuring.

## Validation

| Check                | Result  | Detail                                                      |
| :------------------- | :------ | :---------------------------------------------------------- |
| Olog commutativity   | PASS    | Crypto chain and identity stability diagrams commute        |
| Coalgebra structure  | PASS    | All coalgebras follow canonical c: X → F(X) form            |
| Bisimulation closure | PASS    | Ingest ⊇ condition correct; all bisimulations well-defined  |
| Session type duality | PASS    | All duals well-formed; ⊕/& inversion checks out             |
| Formalism coverage   | PARTIAL | Manifest/VersionScheme are algebraic (constructors), not    |
|                      |         | coalgebraic (observers) — correctly omitted from coalgebras |
| Minimality           | PASS    | No unused formalisms; each captures a distinct concern      |
| External adequacy    | PASS    | Concurrency, errors, async modeled; SPEC §4–9 pending       |

## Complexity Analysis

Abstract complexity of each modeled operation, parameterized by
domain-relevant quantities. Implementation-specific constants
(hash function cost, network latency) are elided.

### Coalgebra Observers

| Observer                       | Complexity  | Parameters               | Notes                                        |
| :----------------------------- | :---------- | :----------------------- | :------------------------------------------- |
| AtomSource.resolve             | O(1)        | —                        | Hash-based lookup by atom-id                 |
| AtomSource.discover            | O(n)        | n = atoms in store       | Scan; O(k) with index (k = result count)     |
| AtomRegistry.claim             | O(1)        | —                        | czd computation + Ed25519 sign               |
| AtomRegistry.publish           | O(1)        | —                        | Sign version transaction                     |
| AtomStore.ingest               | O(\|S\|)    | \|S\| = atoms in source  | Iterates source; O(\|S∖W\|) with dedup check |
| AtomStore.import_path          | O(1)        | —                        | Single atom; dominated by I/O                |
| AtomStore.contains             | O(1)        | —                        | Hash-based membership test                   |
| BuildEngine.plan               | O(1)–O(∞)   | Expression complexity    | Cached: O(1). Eval: Turing-complete (Nix)    |
| BuildEngine.apply              | O(build)    | Plan-specific            | Dominated by actual build execution          |
| ArtifactStore.fetch            | O(1)        | —                        | Content-addressed lookup; +latency if remote |
| ArtifactStore.store            | O(\|blob\|) | \|blob\| = artifact size | Must hash entire blob                        |
| ArtifactStore.exists           | O(1)        | —                        | Digest lookup                                |
| ArtifactStore.check_substitute | O(k)        | k = number of digests    | Batch existence check                        |
| Scheduler.schedule             | O(n log n)  | n = atoms in batch       | Priority-based; O(n) for round-robin         |
| Scheduler.delegate             | O(1)        | —                        | Channel transfer                             |

### Session Costs (End-to-End)

| Session         | Best Case       | Typical Case               | Worst Case                       |
| :-------------- | :-------------- | :------------------------- | :------------------------------- |
| PublishSession  | O(1)            | O(1)                       | O(1) — bounded by crypto ops     |
| BuildSession    | O(1) (Cached)   | O(build) (NeedsBuild)      | O(eval) + O(build) (NeedsEval)   |
| BatchBuild      | O(1) (all hit)  | O(max(build_i)) wall-clock | O(Σ build_i) total work          |
| PopulateSession | O(1) (one atom) | O(\|S\|) (full ingest)     | O(\|S\|) — linear in source size |
| Delegation      | O(1)            | O(1)                       | O(1) — channel transfer          |

### Performance Implications

1. **The cache cliff is real and quantified.** BuildPlan's three
   variants correspond to three distinct complexity classes:
   Cached = O(1), NeedsBuild = O(build), NeedsEvaluation = O(eval) +
   O(build). The jump from Cached to NeedsEvaluation can be orders of
   magnitude. Cache hit rate is the dominant performance lever.

2. **Ingest is the scaling bottleneck for store population.** O(|S|)
   means ingesting a large registry is expensive. Incremental or lazy
   ingestion (only ingest atoms actually needed for the current build)
   should be a priority optimization. The dedup check (`contains`)
   reduces this to O(|S∖W|) — atoms not already present.

3. **Parallelism is the dominant scheduling lever.** BatchBuild wall-clock
   is O(max(build_i)) with sufficient workers, vs O(Σ build_i) sequential.
   The non-interference property guarantees this parallelism is correct.
   Scheduling strategy affects constant factors, not asymptotic behavior.

4. **Delegation is free.** O(1) channel transfer means work-stealing
   has negligible overhead. The decision to delegate should be driven
   by load balancing, not by delegation cost.

5. **Plan is the variance hotspot.** `plan` ranges from O(1) (cache hit)
   to O(∞) (Turing-complete evaluation). All optimization effort at the
   eos layer should focus on maximizing plan cache hits and minimizing
   evaluation cost. Apply is expensive but predictable; plan is the
   wild card.

## Implications

### Architecture Validation

1. **The trait hierarchy is structurally sound.** `AtomSource` as a
   supertrait for `AtomRegistry` and `AtomStore` is a forgetful functor
   dropping role-specific observers. Preserves bisimulation by
   construction.

2. **BuildPlan is protocol structure.** CacheSession ≅ BuildSession:
   the three variants precisely encode the cache-skipping decision tree.
   Any change to caching must be reflected in `BuildPlan` and vice versa.

3. **Deployment modes are formally interchangeable.** BuildEngine
   bisimulation proves embedded, daemon, and remote engines are
   equivalent if they produce the same plans and outputs.

4. **Ingest validates store unification.** The ⊇ preservation condition
   proves atoms retain identity after transfer. Published, local, and
   cross-store atoms are indistinguishable once ingested.

5. **Scheduling is eos-internal and correctness-invariant.** Different
   scheduling strategies (round-robin, work-stealing) are bisimilar —
   ion's code is genuinely generic over scheduling.

6. **Plan failure ≠ apply failure.** Plan failure has no delegatable
   continuation; apply failure does. Eos's error handling must reflect
   this asymmetry.

7. **KD-14 (sync-first) is validated.** The model's coalgebras and
   session types extend to async without structural changes.

### Implementation Guidance

- **Test bisimulation, not structure.** Property-based tests should
  verify equivalent observations across implementations — not identical
  internal structure.

- **Session types as API contracts.** PublishSession: "claim before
  publish." BuildSession: "handle all BuildPlan variants." These are
  testable invariants.

- **The forgetful functor is a dependency firewall.** Eos seeing only
  `AtomSource` (not `AtomStore`) means eos cannot depend on mutation
  operations. Enforced structurally, not by convention.

- **Design error handling around the plan/apply asymmetry.** Apply
  failures should support delegation; plan failures should not.

### Remaining Gaps

- **Protocol specification:** When Atom SPEC §4–9 mature, a companion
  session type model should formalize the full interaction protocol.
