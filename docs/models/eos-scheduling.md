# MODEL: Eos Build Scheduling

## Domain Classification

**Problem Statement:** The Eos build scheduler constructs entry
point DAGs from derivation graphs, dispatches them topologically
across federated workers, relies on builder-level locks to
deduplicate concurrent transitive builds, and uses historical
predictions keyed by derivation name to improve placement. These
mechanisms interact concurrently under nondeterministic request
arrival and worker failure. Before implementation, the scheduling
algorithm needs formal validation to confirm: (1) the dispatch
protocol is correct — ordering and liveness invariants hold under
all interleavings; (2) the optimization heuristics achieve
bounded performance relative to an optimal offline algorithm.

**Domain Characteristics:**

- **Concurrent state machine** — the scheduler maintains mutable
  state (entry point status, worker loads, artifact store)
  accessed by concurrent request handlers and completion
  callbacks. Correctness depends on interleaving behavior.
- **DAG-structured workflow** — derivation dependencies form a
  DAG. Entry point selection partitions this DAG into covering
  subgraphs. Dispatch order must respect the induced entry point
  dependency DAG.
- **Content-addressed deduplication** — derivation hashes provide
  globally unique identity. Builder-level locks guarantee at-most-one
  execution per output path across all concurrent builds.
- **Prediction-augmented optimization** — historical build
  profiles (keyed by derivation name) augment a baseline
  scheduling algorithm. Quality guarantees must hold when
  predictions are accurate (consistency) and when they are
  arbitrarily wrong (robustness).
- **Federated scale** — workers may span multiple clusters with
  varying latency and trust boundaries. Liveness must hold under
  network partitions and artifact store propagation delays.

**Model Scope:**

| In Scope                                   | Out of Scope                              |
| :----------------------------------------- | :---------------------------------------- |
| Entry point DAG construction correctness   | Snix evaluation internals                 |
| Dispatch protocol ordering and liveness    | Builder-internal dependency resolution    |
| Builder-level store locking correctness    | Artifact store implementation details     |
| Coverage property (partition completeness) | Wire protocol (Cap'n Proto serialization) |
| Consistency/robustness competitive bounds  | Authentication/authorization middleware   |
| Federated liveness under partition         | Specific hash algorithm (BLAKE3, etc.)    |
| Historical profile EMA convergence         | Operator tuning parameter selection       |

## Formalism Selection

| Aspect                  | Detail                                                     |
| :---------------------- | :--------------------------------------------------------- |
| **Primary Formalism**   | Two-track: State machine + temporal logic (Track A);       |
|                         | Online scheduling on DAGs + competitive analysis (Track B) |
| **Supporting Tools**    | Graph theory (coverage), probability (EMA convergence)     |
| **Decision Matrix Row** | Row 4 (protocol-heavy concurrency) → Track A;              |
|                         | Outside SDMA matrix (combinatorial optimization) → Track B |
| **Rationale**           | The protocol and optimization concerns are formally        |
|                         | independent. A correct-but-naive scheduler satisfies A.    |
|                         | A near-optimal-but-buggy scheduler satisfies B. They       |
|                         | compose but don't interact at the formal level.            |

**Verification Tooling Plan:**

| Track | Concern              | Tool       | Justification                          |
| :---- | :------------------- | :--------- | :------------------------------------- |
| A     | Protocol correctness | **TLA+**   | Designed for concurrent state machines |
|       |                      |            | with temporal invariants. Model        |
|       |                      |            | checking finds interleaving bugs that  |
|       |                      |            | inspection misses. Fast iteration.     |
| B     | Optimization quality | **Lean 4** | Machine-checked proofs for competitive |
|       |                      |            | analysis bounds. Paper proofs first to |
|       |                      |            | stabilize definitions, then mechanize. |
|       |                      |            | Valuable if CAS+scheduling is a novel  |
|       |                      |            | contribution.                          |

**Alternatives Considered:**

- **Single-tool (Lean 4 for everything)**: Possible but
  encoding nondeterministic protocol interleavings in a theorem
  prover is 5-10× slower than TLA+ for equivalent confidence.
  Lean excels at mathematical proofs, not state space
  exploration.
- **Petri nets**: Natural for concurrency but lack TLA+'s
  temporal logic operators and mature tooling for liveness.
- **Alloy**: Good for structural/relational properties but
  weak on temporal and quantitative bounds.
- **Coalgebraic bisimulation (SDMA §3)**: More relevant for
  implementation interchangeability (covered by the existing
  publishing-stack-layers model). The scheduling protocol is
  better modeled as an explicit state machine than as
  observations over hidden state.

---

## Model

### Shared Definitions

The following definitions are shared between Track A and
Track B. They constitute the mathematical universe in which
both the protocol and the optimization operate.

#### Derivation DAG

Let $G = (V, E)$ be a directed acyclic graph where:

- $V$ is the set of **derivations** (nodes)
- $E \subseteq V \times V$ is the dependency relation:
  $(u, v) \in E$ means derivation $v$ depends on the
  output of derivation $u$ (i.e., $u$ must be built before
  $v$ can start)

Each derivation $v \in V$ has:

- $\text{hash}(v)$: content-addressed derivation hash
  (globally unique, deterministic)
- $\text{name}(v)$: derivation name from `StorePath`
  (human-readable, structurally stable across versions)
- $\text{cached}(v) \in \{\text{true}, \text{false}\}$:
  whether $v$'s output exists in the artifact store at
  the time of DAG construction

#### Uncached Sub-DAG

$G' = (V', E')$ where $V' = \{v \in V : \neg\text{cached}(v)\}$
and $E' = E \cap (V' \times V')$.

This is the sub-DAG of derivations that must be built.
Cached nodes and their edges to other cached nodes are
removed. Edges from uncached nodes to cached nodes are
removed (the dependency is already satisfied).

#### Entry Points and Coverage

An **entry point selection** is a subset $S \subseteq V'$
and a **coverage relation** $\kappa \subseteq V' \times S$ satisfying:

1. **Total coverage**: $\forall v \in V',\; \exists s \in S:\; (v, s) \in \kappa$
   (every uncached derivation is assigned to at least one entry point).

2. **Self-coverage**: $\forall s \in S,\; (s, s) \in \kappa$, and
   if $(s, s') \in \kappa$ then $s' = s$ (every entry point covers itself uniquely).

3. **Transitive containment**: If $(v, s) \in \kappa$ and
   $v \neq s$, then $v$ is in the transitive dependency
   closure of $s$ in $G'$ (a derivation is only covered
   by an entry point that transitively depends on it).

4. **Downward closure within coverage**: If $(v, s) \in \kappa$,
   $(u, v) \in E'$, and $u \notin S$, then $(u, s) \in \kappa$
   (if $v$ is covered by $s$ and depends on non-entry-point
   $u$, then $u$ is also covered by $s$ — entry point scopes
   propagate downward through non-entry-point dependency chains).

**Relation Overlap**: Unlike a single-valued function, the relation $\kappa$
permits overlapping entry point scopes. If a non-entry-point $u \notin S$ has
multiple dependents $v_1, v_2 \in V'$ covered by different entry points $s_1, s_2$,
it is simply covered by both: $(u, s_1) \in \kappa$ and $(u, s_2) \in \kappa$.
This removes the strict **Convergence Obligation** from the formal model,
preventing the macroscopic scheduling DAG from shattering into tiny, high-overhead
synchronization steps for shared leaves. Instead, overlapping transitive builds
are resolved safely and transparently by the builder's store-path locks at runtime.
(While the scheduling heuristic may still choose to promote high fan-out convergence
points to $S$ as a performance optimization to avoid redundant worker allocation,
it is not a formal correctness constraint).

The **entry point DAG** is $T = (S, E_S)$ where
$(s_i, s_j) \in E_S$ iff $s_j$ transitively depends on
$s_i$ in $G'$ and there is no intermediate entry point
$s_k \in S$ on the path.

#### Workers and Assignment

$W = \{w_1, \ldots, w_m\}$ is the set of available workers.
Each worker $w$ has:

- $\text{tags}(w)$: set of capability tags
- $\text{cap}(w)$: resource capacity vector
  $(c_\text{cpu}, c_\text{mem}, c_\text{disk})$
- $\text{load}(w, t)$: current load vector at time $t$
- $\text{cluster}(w)$: federation cluster identifier

An **assignment** $\sigma: S \to W$ maps each entry point
to a worker, subject to:

- **Feasibility**: $\forall s \in S,\;
  s.\text{required\_tags} \subseteq \text{tags}(\sigma(s))$
- **Capacity**: The aggregate predicted resource demand of
  all entry points assigned to $w$ does not exceed
  $\text{cap}(w)$

#### Historical Profiles and Predictions

$P: \text{Names} \to \text{Profiles}$ maps derivation names
to historical profiles:

$$P[\text{name}(v)] = (\hat{d}(v),\; \hat{m}(v),\; \hat{o}(v))$$

where $\hat{d}(v)$ is predicted build duration,
$\hat{m}(v)$ is predicted peak memory, and $\hat{o}(v)$ is
predicted output size.

For derivations with no history, $P[\text{name}(v)]$
falls back to developer metadata (if the derivation is an
atom) or system defaults.

The **prediction error** for a specific execution is:
$$\eta(v) = \frac{|\hat{d}(v) - d(v)|}{d(v)}$$

where $d(v)$ is the actual duration revealed on completion.

---

### Track A: Protocol Correctness (→ TLA+)

This track formalizes the dispatch protocol as a state
machine and specifies the temporal properties it must
satisfy.

**Scope note**: This track models entry-point-level scheduling and
dependency constraints. To simplify the correctness state space,
scheduler-level singleflight deduplication is treated as a
software-level performance optimization (Track B) and is omitted
from the Track A correctness model. Instead, correct build execution
under overlapping entry point scopes relies entirely on builder-level
store path locks (via snix's `PathInfoService` locks) which block
redundant execution and ensure consistency.

#### State Space

The system state is a tuple:

$$\text{State} = (Q, A, L)$$

where:

- $Q: S \to \{\text{pending}, \text{ready}, \text{dispatched},
  \text{complete}, \text{failed}\}$ — entry point status
- $A \subseteq \text{Hashes}$ — the artifact store contents
  (set of completed derivation hashes)
- $L: W \to \text{LoadVectors}$ — per-worker load state

#### Initial State

$$
Q_0(s) = \begin{cases}
  \text{ready} & \text{if } \forall (s', s) \in E_S,\;
                  s' \notin S \text{ (no EP dependencies)} \\
  \text{pending} & \text{otherwise}
\end{cases}
$$

$A_0$ = set of cached derivation hashes,
$L_0(w) = \mathbf{0}$ for all $w$.

#### Transitions

**Dispatch** $(s, w)$ — assign ready entry point $s$ to
worker $w$:

- **Guard**: $Q(s) = \text{ready}$ and $\sigma(s) = w$ and worker $w$ has capacity
- **Effect**:
  - $Q(s) \gets \text{dispatched}$
  - $L(w) \gets L(w) + \text{predicted\_load}(s)$

**Complete** $(s)$ — entry point $s$ finishes building:

- **Guard**: $Q(s) = \text{dispatched}$
- **Effect**:
  - $Q(s) \gets \text{complete}$
  - $A \gets A \cup \text{outputs}(s)$ (outputs enter store)
  - $L(\sigma(s)) \gets L(\sigma(s)) - \text{predicted\_load}(s)$
  - For each $(s, s') \in E_S$: if $\forall (s'', s') \in E_S,\;
    Q(s'') = \text{complete}$, then $Q(s') \gets \text{ready}$

**Fail** $(s)$ — entry point $s$ fails:

- **Guard**: $Q(s) = \text{dispatched}$
- **Effect**:
  - $Q(s) \gets \text{failed}$
  - $L(\sigma(s)) \gets L(\sigma(s)) - \text{predicted\_load}(s)$
  - (Retry policy is orthogonal — may transition back to
    $\text{ready}$)

**CascadeFail** $(s)$ — propagate failure to dependent entry point:

- **Guard**: $Q(s) \in \{\text{pending}, \text{ready}\}$ and
  $\exists (s', s) \in E_S$ such that $Q(s') = \text{failed}$
- **Effect**:
  - $Q(s) \gets \text{failed}$

#### Safety Properties (□ — must always hold)

**P1. Ordering soundness**: No entry point is dispatched
before all its dependency entry points have completed.

$$
\Box\; \forall (s_i, s_j) \in E_S:\;
  Q(s_j) \in \{\text{dispatched}, \text{complete}\}
  \implies Q(s_i) = \text{complete}
$$

**P2. Coverage completeness**: Every uncached derivation is
covered by at least one in-progress, completed, or failed
entry point (no derivation is lost).

$$
\Box\; \forall v \in V':\;
  |\{s \in S : v \in \text{scope}(s)\}| \ge 1
$$

**P4. Capacity safety**: No worker is assigned load
exceeding its capacity.

$$
\Box\; \forall w \in W:\;
  L(w) \leq \text{cap}(w) \quad \text{(component-wise)}
$$

#### Liveness Properties (◇ — must eventually hold)

**P5. Progress**: If a ready entry point exists and a
feasible worker has capacity, the entry point is eventually
dispatched.

$$
\Box\; (Q(s) = \text{ready} \land \exists w:\;
  \text{feasible}(s, w))
  \implies \Diamond\; Q(s) \in \{\text{dispatched},
    \text{complete}\}
$$

**P6. Completion propagation**: If all entry points in a
request either complete or fail, the request terminates.

$$
\Box\; \Diamond\; \forall s \in S_\text{req}:\;
  Q(s) \in \{\text{complete}, \text{failed}\}
$$

**P7. Federation liveness**: In a federated deployment,
after an entry point completes, its outputs are reachable
by any worker within bounded time $\delta$ (artifact store
propagation).

$$
\Box\; (Q(s) = \text{complete})
  \implies \Diamond_{\leq \delta}\;
  \forall w \in W:\; \text{outputs}(s) \subseteq A_w
$$

where $A_w$ is the artifact store view at worker $w$.

---

### Track B: Optimization Quality (→ Paper Proofs → Lean 4)

This track formalizes the optimization properties of the
entry point selection and worker assignment algorithms.

#### Coverage Optimality

**Definition (Makespan)**: Given entry point DAG $T = (S, E_S)$,
assignment $\sigma: S \to W$, and actual durations $d$, the
makespan is:

$$M(\sigma) = \max_{s \in \text{sinks}(T)} C(s)$$

where $C(s)$ is the completion time of entry point $s$:

$$C(s) = \max_{(s', s) \in E_S} C(s') + \tau(s', s) + d_\sigma(s)$$

- $\tau(s', s)$ is the artifact transfer time from
  $\sigma(s')$ to $\sigma(s)$ (zero if same worker)
- $d_\sigma(s)$ is the build duration of $s$ on worker
  $\sigma(s)$, which includes all transitive builds within
  $s$'s coverage scope

#### Theorem 1: Coverage Existence

_For any uncached sub-DAG $G' = (V', E')$, a valid entry
point selection $(S, \kappa)$ satisfying properties 1-4
exists._

**Proof sketch**: The trivial selection $S = \{r\}$ where
$r$ is the top-level derivation, with $\kappa(v) = r$ for
all $v \in V'$, satisfies all four properties. (Every
derivation is covered by the single entry point, which is
the top-level derivation.) $\square$

This is the degenerate case (zero parallelism). The
optimization problem is finding $S$ that minimizes makespan.

#### Theorem 2: Consistency Bound

_Given a fixed entry point DAG $T = (S, E_S)$ constructed from
uncached sub-DAG $G'$, if predictions are accurate ($\eta(v) \leq \epsilon$ for
all $v$) and the overlap between entry point transitive scopes in the coverage
relation is negligible (ensuring task duration independence), then the heuristic
assignment $\sigma_H$ achieves:_

$$M(\sigma_H) \leq (1 + O(\epsilon)) \cdot M(\sigma^*)$$

_where $\sigma^*$ is the optimal offline assignment for $T$._

**Concession on Coarsening**: We explicitly narrow this bound to the assignment phase
on a _fixed_ entry point DAG $T$. The entry point _selection_ (graph coarsening)
phase, which groups $G'$ into $T$, currently lacks a formal competitive bound.
While greedy heuristic selection performs well in practice by isolating troublesome
nodes and convergence points, mathematically bounding the makespan penalty of graph
coarsening against arbitrary offline schedules remains open.

**Proof approach**: Instantiate the framework of Gupta et
al. (arXiv:1703.01634). Their result shows greedy scheduling
with stochastic duration estimates on a fixed set of tasks is competitive, with
ratio depending on the squared coefficient of variation.
When $\epsilon$ is small (predictions are accurate), the
coefficient of variation is $O(\epsilon)$, giving the bound on $T$.

**Status**: Proof sketch. Full proof requires formalizing the
mapping from our entry point DAG model to their unrelated
machine scheduling model. Candidate for Lean 4 mechanization.

#### Theorem 3: Robustness Bound

_For any prediction quality (including $\eta \to \infty$),
the heuristic assignment $\sigma_H$ achieves:_

$$M(\sigma_H) \leq \alpha \cdot M(\sigma_\text{base})$$

_where $\sigma_\text{base}$ is the prediction-free baseline
(tag matching with LRH affinity and availability only) and $\alpha \geq 1$
is a small constant.\_

**Proof approach**: We normalize the `resource_fit` term using capacity-relative fractions:
$$\text{resource\_fit}(w, e) = \sum_{i} \left( \frac{r_{e,i}}{c_{w,i}} \cdot \frac{a_{w,i}}{c_{w,i}} \right)$$
bounding it to match the range of the cache `affinity` and `availability` terms while preserving task magnitude and resolving dimensional mismatch.
Furthermore, the scheduler dynamically decays the weight $\beta_e$ of the `resource_fit`
term based on the exponential moving average (EMA) of absolute relative prediction error:
$$\beta_e = \beta \cdot e^{-\lambda \cdot \text{EMA}(|\eta_e|)}$$
where $\text{EMA}(|\eta_e|)$ is the running average of relative prediction error magnitude.
If predictions are systematically incorrect (even with zero variance), $\text{EMA}(|\eta_e|) \to \infty$,
causing $\beta_e \to 0$. The scoring function thus dynamically filters out the noisy prediction
term, mathematically collapsing the assignment algorithm back to the prediction-free baseline.
Because the degradation scales with the decayed weight and the normalized terms, the makespan
under incorrect predictions is bounded within a small constant factor $\alpha$ of the baseline
placement.

**Status**: Conjecture. Requires formalization of the scoring
function's degradation behavior. The learning-augmented
framework (Lindermayr & Megow, arXiv:2202.10199) provides
the template: define consistency and robustness as functions
of prediction error, then prove the tradeoff is tight.

#### Theorem 4: Singleflight Deduplication Savings (Track B Optimization)

_Let $R$ concurrent requests produce derivation DAGs
$G_1, \ldots, G_R$ with shared uncached sub-DAGs. If the optional
scheduler-level singleflight optimization is enabled, total build work is
$|\bigcup_{i=1}^{R} V'_i|$ instead of $\sum_{i=1}^{R} |V'_i|$ builds,
preventing duplicate worker slot allocation._

**Proof sketch**: Content-addressed hashing ensures
$\text{hash}(v) = \text{hash}(u) \iff v = u$ (derivations
are identical iff their hashes match, by deterministic
evaluation). The singleflight map keys on hash, so identical
derivations across requests coalesce. $\square$

The savings ratio is:
$$\rho = \frac{|\bigcup V'_i|}{\sum |V'_i|}$$

In practice, $\rho$ is small when requests share common
dependencies (e.g., many projects depend on `openssl`),
yielding large savings.

---

## Validation

| Check                                  | Result  | Detail                                                 |
| :------------------------------------- | :------ | :----------------------------------------------------- |
| Coverage properties (1-4) coherent     | PASS    | Trivial selection exists (Thm 1); properties 1-4 are   |
|                                        |         | consistent and non-contradictory                       |
| Ordering soundness (P1) well-formed    | PASS    | The entry point DAG is acyclic (induced from a DAG);   |
|                                        |         | topological dispatch is well-defined                   |
| Coverage completeness (P2) well-formed | PASS    | Guaranteed by entry point selection algorithm;         |
|                                        |         | relation-based coverage maps all uncached nodes        |
| Liveness (P5, P6) depend on fairness   | PARTIAL | P5 requires a fairness assumption (the scheduler       |
|                                        |         | eventually considers every ready entry point). Must be |
|                                        |         | explicitly stated in TLA+ as a fairness constraint.    |
| Federation liveness (P7) depends on    | PARTIAL | Requires bounded artifact store propagation time δ as  |
| network model                          |         | an assumption. Under partition, δ → ∞ and P7 fails.    |
|                                        |         | Need to specify partition behavior explicitly.         |
| Consistency bound (Thm 2) sketch       | PARTIAL | Mapping to Gupta et al. framework not yet formalized.  |
|                                        |         | The entry point DAG structure adds complexity beyond   |
|                                        |         | their unrelated-machines model.                        |
| Robustness bound (Thm 3)               | PARTIAL | Conjecture only. Scoring function degradation behavior |
|                                        |         | needs formal characterization.                         |
| Minimality                             | PASS    | Two-track decomposition is minimal — protocol and      |
|                                        |         | optimization are formally independent concerns.        |
| External adequacy                      | PASS    | Model captures all mechanisms described in ADR-0004.   |
|                                        |         | No ADR mechanism is unmodeled.                         |

**Validation gaps to address:**

1. **Fairness**: P5 (progress) requires weak fairness on the
   dispatch action. The TLA+ spec must include this as
   `WF_vars(Dispatch)`.
2. **Partition model**: P7 assumes bounded propagation. Under
   network partition, the model must specify degraded behavior
   (entry points on unreachable workers are retried on
   reachable workers after timeout).
3. **Consistency bound formalization**: The mapping from entry
   point DAGs with coverage to unrelated-machine scheduling
   requires a reduction proof showing that the coverage
   structure does not violate the preconditions of Gupta et al.

---

## Implications

### For TLA+ Specification (Track A — Next Step)

The state machine defined above translates directly to a
TLA+ module. Key modeling decisions:

- **State variables**: `epStatus` (function $S \to$ status),
  `artifactStore` (set of hashes), `workerLoad` (function $W \to$ load vectors)
- **Actions**: `Dispatch(s, w)`, `Complete(s)`, `Fail(s)`,
  `CascadeFail(s)`
- **Invariants**: P1, P2, P4 as `INVARIANT` declarations
- **Liveness**: P5, P6 as `PROPERTY` declarations with
  fairness via `WF_vars`
- **Model checking**: Finite instances (e.g., 3 workers,
  5 entry points) to exhaustively verify invariants for a
  single request DAG.

### For Lean 4 Proofs (Track B — After Paper Proofs Stabilize)

- **Coverage**: Formalize the entry point selection as a
  function on finite DAGs in Mathlib's `Combinatorics.SimpleGraph`
  or `Order.PartialOrder`. Prove existence (Thm 1) and
  uniqueness of coverage assignment.
- **Consistency/Robustness**: Formalize the scoring function
  and makespan computation. Instantiate the learning-augmented
  framework. This likely requires formalizing a fragment of
  competitive analysis in Lean — potentially reusable beyond
  this project.
- **Dedup savings**: Straightforward set theory proof (Thm 4).
  Good "warmup" for the Lean formalization.

### For Implementation

- The state machine structure maps to a Rust `enum` for entry
  point status and a `tokio::sync::watch` for completion
  broadcast. If the optional scheduler-level singleflight
  optimization is implemented, it uses a `DashMap<DrvHash, SharedFuture>`
  to track in-flight builds.
- The coverage relation $\kappa$ is computed once during entry
  point selection and stored as a lookup table (e.g., mapping each
  derivation hash to its set of covering entry point hashes).
- The ordering soundness invariant (P1) translates to a runtime
  assertion: before dispatching entry point $s$, verify all
  $(s', s) \in E_S$ have $Q(s') = \text{complete}$.

### Design Insights Revealed

1. **Coverage property 4 (downward closure) constrains entry
   point selection**: You cannot split a dependency chain
   across two entry points unless the split point is itself
   an entry point. This means the entry point set must form
   an **antichain cover** of the DAG — a set of nodes such
   that every maximal chain passes through at least one entry
   point, and non-entry-point subchains are fully contained
   within one entry point's scope.

2. **Cascade failure propagation prevents deadlocks**: If a dependency
   entry point fails, dependent entry points cannot be built. The state
   machine must actively propagate this failure via `CascadeFail` to
   prevent dependent tasks from hanging in `pending` or `ready`
   independently, satisfying liveness (P6).

3. **Federation liveness (P7) is the weakest property**: It
   depends on an environmental assumption (bounded propagation
   $\delta$) that the system cannot enforce. Under partition,
   the system must degrade gracefully — the formal model
   should specify what "graceful" means (e.g., entry points
   are reassigned to reachable workers after timeout $> \delta$).
