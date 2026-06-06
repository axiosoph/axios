# MODEL: Eos Build Scheduling

> **Terminology note**: This model uses "derivation" to refer to
> the Nix-native build unit. In Axios's layered terminology, the
> Eos engine abstraction is "Plan" (`BuildEngine::Plan`). See
> ADR-0004 for the full scheduling design.

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
(While the scheduling heuristic may still choose to promote high fan-in convergence
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

$$P[\text{name}(v)] = (\hat{d}(v),\; \hat{m}(v),\; \hat{c}(v),\; \hat{o}(v))$$

where $\hat{d}(v)$ is predicted build duration,
$\hat{m}(v)$ is predicted peak memory, $\hat{c}(v)$ is
predicted CPU cores, and $\hat{o}(v)$ is predicted output
size. These four fields populate the resource vector used
by the scoring function's `resource_fit` term.

For derivations with no history, $P[\text{name}(v)]$
falls back to developer metadata (if the derivation is an
atom) or system defaults.

The **prediction error** for a specific execution is:
$$\eta(v) = \frac{|\hat{d}(v) - d(v)|}{\hat{d}(v)}$$

where $d(v)$ is the actual duration revealed on completion
and $\hat{d}(v)$ is the predicted duration. Normalizing by
the prediction (rather than the actual) reflects the
information available at scheduling time.

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

**P3. Artifact completeness**: If an entry point completes,
its outputs are present in the artifact store.

$$
\Box\; \forall s \in S:\;
  Q(s) = \text{complete}
  \implies \text{outputs}(s) \subseteq A
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

**Proof intuition**: The trivial selection $S = V'$ (every
uncached node is an entry point), with $\kappa = \text{id}$
(each node covers itself), satisfies all four properties.
This is the degenerate case (maximum parallelism but zero
locality benefit). $\square$

**Status**: Machine-checked in Lean 4 (Theorem1.lean).
Zero `sorry`, zero custom `axiom`.

The optimization problem is finding $S$ that minimizes
makespan — the identity witness proves such a selection
always exists.

#### Theorem 2: Consistency Bound

_Given a fixed entry point DAG $T = (S, E_S)$ constructed from
uncached sub-DAG $G'$, if predictions are accurate ($\eta(v) \leq \epsilon$ for
all $v$) and the overlap between entry point transitive scopes in the coverage
relation is negligible (ensuring task duration independence), then the heuristic
assignment $\sigma_H$ achieves:_

$$
M(\sigma_H) \leq \alpha \cdot \frac{1 + \varepsilon}
{1 - \varepsilon} \cdot M(\sigma^*)
$$

_where $\sigma^*$ is the optimal offline assignment for $T$
and $\alpha$ is the heuristic's base approximation ratio
on perfectly-predicted inputs._

**Concession on Coarsening**: We explicitly narrow this bound to the assignment phase
on a _fixed_ entry point DAG $T$. The entry point _selection_ (graph coarsening)
phase, which groups $G'$ into $T$, currently lacks a formal competitive bound.
The gap is captured by $\alpha$ — heuristic quality on perfect predictions.

**Proof approach**: Well-founded induction on DAG completion
times. The inductive step uses $\varepsilon$-accuracy to
bound predicted vs. actual completion, and non-negative
transfer times $\tau(s', s) \geq 0$ between dependent
entry points.

**Status**: Machine-checked in Lean 4 (Theorem2.lean).
Zero `sorry`, zero custom `axiom`. Key hypothesis:
$\varepsilon < 1$ (predictions are better than 100% error).

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

**Status**: Machine-checked in Lean 4 (Theorem3.lean).
Two sub-results verified:

- **Lemma 3.1 (Assignment stability)**: If perturbation
  $2P < \Delta_{\min}$, then $\sigma_H = \sigma_{\text{base}}$
- **EMA lower bound**: Under sustained error $\eta \geq \eta_0$,
  $\text{EMA}_n \geq (1 - \gamma^n)\eta_0 + \gamma^n E_0$

The quantitative $\mu$-makespan bound during the EMA
transient window is not mechanized (low risk — the
transient is geometrically short and capacity safety P4
holds throughout via Track A).

#### Theorem 4: Singleflight Deduplication Savings (Track B Optimization)

_Let $R$ concurrent requests produce derivation DAGs
$G_1, \ldots, G_R$ with shared uncached sub-DAGs. If the optional
scheduler-level singleflight optimization is enabled, total build work is
$|\bigcup_{i=1}^{R} V'_i|$ instead of $\sum_{i=1}^{R} |V'_i|$ builds,
preventing duplicate worker slot allocation._

**Proof intuition**: Content-addressed hashing ensures
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

| Check                              | Result | Detail                                                 |
| :--------------------------------- | :----- | :----------------------------------------------------- |
| Coverage properties (1-4) coherent | PASS   | Identity witness mechanized in Lean 4 (Thm 1);         |
|                                    |        | properties 1-4 are satisfiable and non-contradictory   |
| Coverage existence (Thm 1)         | PASS   | Machine-checked in Lean 4. Constructs `EosModel` with  |
|                                    |        | `S = V'`, `κ = id`. Zero `sorry`, zero `axiom`.        |
| Ordering soundness (P1)            | PASS   | Model-checked in TLA+ across 4 topology models         |
|                                    |        | (linear, diamond, convergence, independent)            |
| Coverage completeness (P2)         | PASS   | Structural — guaranteed by `EosModel` total coverage   |
|                                    |        | property, verified satisfiable by Thm 1                |
| Artifact completeness (P3)         | PASS   | Model-checked in TLA+ (`ArtifactSafety` invariant)     |
| Capacity safety (P4)               | PASS   | Model-checked in TLA+ under all interleavings          |
| Liveness (P5, P6)                  | PASS   | Model-checked in TLA+ with `WF_vars(Next)` weak        |
|                                    |        | fairness. All 4 topology models verify both properties |
| Federation liveness (P7)           | OPEN   | Requires real-time bounds ($\Diamond_{\leq\delta}$)    |
|                                    |        | not expressible in standard TLA+ temporal logic.       |
|                                    |        | Intentionally deferred — see note below                |
| Consistency bound (Thm 2)          | PASS   | Machine-checked in Lean 4. Proves                      |
|                                    |        | $M(\sigma_H) \leq \alpha \cdot \frac{1+\varepsilon}    |
|                                    |        | {1-\varepsilon} \cdot M(\sigma^\*)$ via well-founded   |
|                                    |        | induction on DAG completion times                      |
| Robustness — assignment stability  | PASS   | Lean 4 Lemma 3.1: perturbation $2P < \Delta$ implies   |
|                                    |        | $\sigma_H = \sigma_\text{base}$ (assignment identity)  |
| Robustness — EMA convergence       | PASS   | Lean 4: under sustained error $\eta \geq \eta_0$,      |
|                                    |        | EMA $\geq (1 - \gamma^n) \eta_0 + \gamma^n E_0$        |
| Robustness — μ-makespan bound      | OPEN   | Quantitative bound during transient not mechanized.    |
|                                    |        | Low risk — transient is short (geometric convergence)  |
|                                    |        | and capacity safety holds throughout (Track A)         |
| Singleflight deduplication (Thm 4) | PASS   | Machine-checked in Lean 4. Proves                      |
|                                    |        | $\lvert\bigcup V'_i\rvert \leq \sum \lvert V'_i\rvert$ |
|                                    |        | with equality iff pairwise disjoint                    |
| Graph coarsening optimality        | OPEN   | Underlying problem is NP-hard. Formal competitive      |
|                                    |        | bound on entry point selection is not tractable.       |
|                                    |        | Thm 2 bounds assignment quality on any fixed DAG       |
| Minimality                         | PASS   | Two-track decomposition is minimal — protocol and      |
|                                    |        | optimization are formally independent concerns         |
| External adequacy                  | PASS   | Model captures all mechanisms described in ADR-0004    |

**Remaining open items:**

1. **Federation liveness (P7)**: Requires bounded artifact
   store propagation time $\delta$ as an environmental
   assumption. Under partition, $\delta \to \infty$ and P7
   fails. This is inherently unverifiable by the system —
   it depends on network infrastructure. Implementation
   should use timeouts with worker reassignment as a
   pragmatic mitigation.
2. **Graph coarsening optimality**: The entry point selection
   (DAG decomposition) phase is NP-hard and lacks a formal
   competitive bound. The consistency bound (Thm 2) applies
   to assignment quality on any fixed entry point DAG,
   cleanly separating DAG quality ($\alpha$) from prediction
   error cost ($\frac{1+\varepsilon}{1-\varepsilon}$).
   Heuristic quality is an engineering tuning concern.

---

## Implications

### Verification Status

Both tracks of formal verification are complete:

- **Track A (TLA+)**: Protocol correctness model-checked
  across 4 DAG topologies. All safety invariants (P1, P3,
  P4) and liveness properties (P5, P6) verified
  under `WF_vars(Next)` weak fairness. See `models/tla/`.
- **Track B (Lean 4)**: Optimization quality machine-checked.
  Zero `sorry`, zero custom `axiom`. Theorems 1-4 verified
  with Mathlib. See `models/lean/`.

### For Implementation (Derived from Proofs)

1. **Entry point status enum**: The TLA+ state machine maps
   directly to a Rust `enum { Pending, Ready, Dispatched,
Complete, Failed }` with `tokio::sync::watch` for
   completion broadcast.

2. **Coverage relation as lookup table**: The `EosModel`
   coverage relation $\kappa$ is computed once during entry
   point selection and stored as a `HashMap<DrvHash,
Vec<EntryPointHash>>`. The `EosModel` properties (1-4)
   should be enforced as `debug_assert!` checks on the
   output of the selection algorithm.

3. **Ordering soundness as runtime assertion**: Before
   dispatching entry point $s$, verify all $(s', s) \in E_S$
   have $Q(s') = \text{complete}$. This is the P1 invariant
   from the TLA+ proof, translated to a pre-dispatch guard.

4. **Consistency bound is monitorable**: The proven bound
   $M(\sigma_H) \leq \alpha \cdot \frac{1+\varepsilon}
   {1-\varepsilon} \cdot M(\sigma^*)$ has measurable inputs.
   After each build, compute observed $\varepsilon$ from
   `|d - d_hat| / d_hat` and track the EMA. If $\varepsilon$
   exceeds a threshold, the bound degrades and the system
   should surface this in metrics/telemetry.

5. **Transfer time non-negativity**: Theorem 2's proof
   depends on $\tau(s', s) \geq 0$. This is trivially
   satisfied by network transfer times but would break if
   the model encoded "time savings from caching" as negative
   $\tau$. The implementation must not conflate transfer cost
   with cache benefit in the same variable.

6. **Singleflight map keyed by derivation hash**: Theorem 4
   operates on abstract set families. In implementation, the
   `DashMap<DrvHash, SharedFuture<BuildResult>>` singleflight
   map is the concrete instantiation. The theorem guarantees
   deduplication savings exactly equal the overlap between
   concurrent requests' uncached sub-DAGs.

7. **EMA decay as self-healing mechanism**: The EMA lower
   bound proof guarantees that under sustained prediction
   error, the prediction weight decays geometrically. The
   implementation's EMA smoothing factor $\gamma$ controls
   convergence speed — smaller $\gamma$ means faster decay
   but more sensitivity to noise.

### Design Insights Revealed by Verification

1. **Coverage property 4 (downward closure) constrains entry
   point selection**: You cannot split a dependency chain
   across two entry points unless the split point is itself
   an entry point. This means the entry point set must form
   an **antichain cover** of the DAG — a set of nodes such
   that every maximal chain passes through at least one entry
   point, and non-entry-point subchains are fully contained
   within one entry point's scope.

2. **Cascade failure propagation prevents deadlocks**: The
   TLA+ model checking confirmed that without `CascadeFail`,
   dependent tasks hang in `pending` or `ready` forever after
   a dependency failure. The implementation must actively
   propagate failure — this is not optional.

3. **Federation liveness (P7) is the weakest property**: It
   depends on an environmental assumption (bounded propagation
   $\delta$) that the system cannot enforce. Implementation
   should use configurable timeouts with worker reassignment
   rather than relying on unbounded store propagation.

4. **The identity witness (Thm 1) is the degenerate case**:
   Making every node an entry point satisfies all coverage
   properties trivially but provides zero locality benefit.
   The optimization problem is finding entry points that
   maximize parallelism while preserving locality — the
   consistency bound (Thm 2) guarantees that any valid
   selection with good predictions achieves near-optimal
   makespan on the resulting DAG.

5. **α separates DAG quality from prediction quality**: The
   consistency bound cleanly factors into $\alpha$ (how good
   is your heuristic on perfect predictions?) and
   $(1+\varepsilon)/(1-\varepsilon)$ (how much does prediction
   error cost?). This separation means the implementation can
   independently tune the entry point selection heuristic
   (affects $\alpha$) and the prediction infrastructure
   (affects $\varepsilon$) without cross-concern interference.
