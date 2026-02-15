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

## Formalism Selection

| Aspect                  | Detail                                                           |
| :---------------------- | :--------------------------------------------------------------- |
| **Primary Formalism**   | Coalgebra (behavior endofunctors, bisimulation)                  |
| **Supporting Tools**    | Session types (protocol ordering), Olog (domain ontology)        |
| **Decision Matrix Row** | State + observation → Coalgebra; Protocol → Session types        |
| **Rationale**           | Traits-as-observers is the dominant pattern; bisimulation is the |
|                         | correct equivalence for implementation interchangeability.       |
|                         | Session types capture the protocol ordering that coalgebra       |
|                         | alone cannot express. Olog grounds the static ontology.          |

**Alternatives Considered:**

- **Pure functorial migration:** Too narrow — trait boundaries involve
  behavioral contracts, not just data restructuring.
- **Linear logic as primary:** Atoms are freely duplicable and
  discoverable; no linear resource discipline on data flow.
- **Hyperdoctrine:** No quantified predicates over type families
  needed — associated types are simple type-level programming.

## Model

### 1. Olog — Domain Ontology

**Objects:**

| Object   | Description                                               | Layer |
| :------- | :-------------------------------------------------------- | :---- |
| Atom     | Fundamental unit of publishing                            | L1    |
| Atom-id  | Content-addressed digest: czd(anchor, label)              | L1    |
| Label    | Human-readable name                                       | L1    |
| Anchor   | Genesis commit hash establishing atom-set identity        | L1    |
| Atom-set | Collection of atoms sharing a common anchor               | L1    |
| Version  | Abstract version (via VersionScheme — ecosystem-agnostic) | L1    |
| Revision | A specific commit in source history                       | L1    |
| Manifest | Metadata: label, version, deps, composer config           | L1    |
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

### 2. Coalgebras — Behavioral Observation

Each trait defines a coalgebra c: X → F(X) where X is the state space
(any implementation) and F is the behavior endofunctor.

#### 2.1. AtomSource (atom-core, L1)

```
F_source(S) = (AtomId → Option<AtomEntry>)    -- resolve
            × (Query → Set<AtomId>)            -- discover
```

Bisimulation: s₁ ~ s₂ iff resolve and discover agree pointwise. Two
implementations containing the same atoms are interchangeable.

#### 2.2. AtomRegistry (atom-core, L1)

```
F_registry(R) = F_source(R)                   -- inherits AtomSource
              × (ClaimReq → Result<AtomId>)    -- claim
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
!ClaimReq . ?Result<AtomId> . !PublishReq . ?Result<()> . end
```

Claim MUST succeed before publish. The session type enforces this —
the client cannot send `PublishReq` without first receiving `AtomId`.

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
| External adequacy    | PASS    | Concurrency, errors, async modeled; dev workspace deferred  |

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

- **Dev workspace semantics:** `import_path` stamps dev prerelease
  versions — a side effect not captured in the coalgebra. Implementation
  concern, not architectural.
- **Protocol specification:** When Atom SPEC §4–9 mature, a companion
  session type model should formalize the full interaction protocol.
