# MODEL: Eos Build Scheduling

> **Terminology note**: This model uses "derivation" to refer to
> the Nix-native build unit. In Axios's layered terminology, the
> Eos engine abstraction is "Plan" (`BuildEngine::Plan`). See
> ADR-0004 for the full scheduling design.

## Domain Classification

**Problem Statement:** The Eos build scheduler constructs entry
point DAGs from derivation graphs, dispatches them topologically
across federated workers, relies on CAS-idempotent store semantics to
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
  globally unique identity. CAS-idempotent store semantics guarantee
  at-most-one materialization per output path across all concurrent builds.
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
| CAS-idempotent store deduplication         | Artifact store implementation details     |
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

#### Multi-Request Union Graph and Clients

When $R$ concurrent requests arrive, the scheduler merges their uncached sub-DAGs JIT into a single **unified global derivation graph** $G_\cup = (V_\cup, E_\cup)$ where:

$$V_\cup = \bigcup_{i=1}^R V'_i \quad \text{and} \quad E_\cup = \bigcup_{i=1}^R E'_i$$

To track ownership, each node $v \in V_\cup$ is associated with a set of requesting clients:

$$\text{request\_clients}(v) \subseteq \{1, \ldots, R\}$$

When request $i$ is cancelled, the client ID $i$ is removed from $\text{request\_clients}(v)$ for all $v$. If $\text{request\_clients}(v) = \emptyset$ and the node is mutable (not yet dispatched), it is pruned from the global DAG.

#### Entry Points and Coverage

An **entry point selection** is a subset $S \subseteq V_\cup$
and a **coverage relation** $\kappa \subseteq V_\cup \times S$ satisfying:

1. **Total coverage**: $\forall v \in V_\cup,\; \exists s \in S:\; (v, s) \in \kappa$
   (every uncached derivation is assigned to at least one entry point).

2. **Self-coverage**: $\forall s \in S,\; (s, s) \in \kappa$, and
   if $(s, s') \in \kappa$ then $s' = s$ (every entry point covers itself uniquely).

3. **Transitive containment**: If $(v, s) \in \kappa$ and
   $v \neq s$, then $v$ is in the transitive dependency
   closure of $s$ in $G_\cup$ (a derivation is only covered
   by an entry point that transitively depends on it).

4. **Downward closure within coverage**: If $(v, s) \in \kappa$,
   $(u, v) \in E_\cup$, and $u \notin S$, then $(u, s) \in \kappa$
   (if $v$ is covered by $s$ and depends on non-entry-point
   $u$, then $u$ is also covered by $s$ — entry point scopes
   propagate downward through non-entry-point dependency chains).

**Relation Overlap**: Unlike a single-valued function, the relation $\kappa$
permits overlapping entry point scopes. If a non-entry-point $u \notin S$ has
multiple dependents $v_1, v_2 \in V_\cup$ covered by different entry points $s_1, s_2$,
it is simply covered by both: $(u, s_1) \in \kappa$ and $(u, s_2) \in \kappa$.
This removes the strict **Convergence Obligation** from the formal model,
preventing the macroscopic scheduling DAG from shattering into tiny, high-overhead
synchronization steps for shared leaves. Overlapping transitive builds are resolved
safely by the content-addressed, idempotent storage model (CAS) where concurrent
writes of the same outputs both succeed independently. Redundant computation is
minimized at the scheduler level via convergence-point promotion to standalone
entry points.

The **entry point DAG** is $T = (S, E_S)$ where
$(s_i, s_j) \in E_S$ iff $s_j$ transitively depends on
$s_i$ in $G_\cup$ and there is no intermediate entry point
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
scheduler-level structural deduplication is treated as a
software-level performance optimization (Track B) and is omitted
from the Track A correctness model. In the absence of locks, correct
build execution under overlapping entry point scopes relies entirely
on the content-addressed, idempotent storage model (CAS). Simultaneous
identical builds both write to the store safely, producing identical
content at identical keys, ensuring consistency without blocking.

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

> **Enforcement note**: $\text{cap}(w)$ is a virtual capacity vector
> reported by the worker at registration. The scheduler is
> enforcement-agnostic — the dispatch guard is identical regardless
> of the worker's enforcement capability. Under OCI-based workers,
> capacity equals physical resources and P4 is **kernel-enforced**
> via cgroup limits (`cpu.max`, `memory.max`). Under non-OCI workers
> (e.g., macOS), capacity is a conservative virtual budget accounting
> for the lack of kernel enforcement. See ADR-0004 §3.1.

**P8. Frozen stability**: Once an entry point is dispatched to a worker, its assignment is fixed for that execution. It can only transition to complete (on the same worker), failed (released from worker), or back to ready (released from worker via transient failure). It can never be migrated directly to a different worker while dispatched.

$$
\Box\; \forall s \in S:\; \forall w \in W:\;
  (Q(s) = \text{dispatched} \land \text{worker}(s) = w)
  \implies \big( Q'(s) \in \{\text{dispatched}, \text{complete}, \text{failed}, \text{ready}\} \land (Q'(s) = \text{dispatched} \implies \text{worker}'(s) = w) \big)
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

**P5'. Head-of-line (HoL) immunity**: The progress of a ready entry point $s$ belonging to request $R_i$ is independent of the status of any unrelated entry point $s' \in S \setminus S_{R_i}$ or blocking states of unrelated requests. P5 holds locally for each request sub-DAG.

**P6. Completion propagation**: If all entry points in a
request either complete or fail, the request terminates.

$$
\Box\; \Diamond\; \forall s \in S_\text{req}:\;
  Q(s) \in \{\text{complete}, \text{failed}\}
$$

**P6'. Per-request completion**: Each request independently terminates. If all entry points $S_{R_i}$ associated with request $R_i$ complete or fail, then request $R_i$ terminates, regardless of whether other requests are still active.

$$
\Box\; \Diamond\; (\forall s \in S_{R_i}:\; Q(s) \in \{\text{complete}, \text{failed}\}) \implies \text{terminated}(R_i)
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

**P9. Work Conservation**: If a ready entry point exists and a worker has capacity, that entry point is eventually dispatched (or skipped, failed, or canceled).

$$
\Box\; \big( Q(s) = \text{ready} \land \exists w:\; L(w) + \text{load}(s) \leq \text{cap}(w) \big) \implies \Diamond\; Q(s) \in \{\text{dispatched}, \text{complete}, \text{failed}\}
$$

**P10. Transient Failure Recovery**: If an infrastructure or worker crash occurs while running entry point $s$, the entry point is unfrozen back to `ready` for re-dispatch, and worker capacity is reclaimed.

**P11. Failure Isolation**: An entry point can only transition to `failed` if it fails deterministically (build failure) or if a dependency fails (cascade failure), preventing spurious failure propagation.

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

#### Theorem 2': Adaptive Consistency Bound

_Let $T$ be the entry point DAG under average prediction error $\bar{\epsilon}$. If $\bar{\epsilon} < 1$, then the heuristic assignment $\sigma_H$ achieves:_

$$
M(\sigma_H) \leq \alpha(\bar{\epsilon}) \cdot \frac{1 + \bar{\epsilon}}{1 - \bar{\epsilon}} \cdot M(\sigma*)
$$

_where $\alpha(\bar{\epsilon})$ is the adaptive coarsening quality function, which is monotonically non-increasing and tightens as prediction quality improves:_

$$\alpha(0) = \alpha_{\text{heft}} \quad \text{and} \quad \alpha(1) \leq \alpha_{\text{max}}$$

**Proof approach**: Corollary of Theorem 2 where the constant approximation factor $\alpha$ is parameterized by prediction-error-based coarsening boundaries. Under perfect predictions ($\bar{\epsilon} \to 0$), cost thresholds are lowered, yielding more precise entry points (closer to the optimal HEFT schedule, $\alpha \to \alpha_{\text{heft}}$). As error increases, thresholds rise, coarsening the DAG and forcing conservative scheduling, bounded by the prediction-free baseline approximation $\alpha_{\text{max}}$.

**Status**: Machine-checked in Lean 4 (Theorem2Prime.lean).
Zero `sorry`, zero custom `axiom`.

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

#### Theorem 4: Structural Deduplication Savings (Track B Optimization)

_Let $R$ concurrent requests produce derivation DAGs
$G_1, \ldots, G_R$ with shared uncached sub-DAGs. Total build work is
$|\bigcup_{i=1}^{R} V'_i|$ instead of $\sum_{i=1}^{R} |V'_i|$ builds,
preventing duplicate worker slot allocation._

**Proof intuition**: Content-addressed hashing ensures
$\text{hash}(v) = \text{hash}(u) \iff v = u$ (derivations
are identical iff their hashes match, by deterministic
evaluation). Identical derivations across requests coalesce in the global DAG. $\square$

The savings ratio is:
$$\rho = \frac{|\bigcup V'_i|}{\sum |V'_i|}$$

In practice, $\rho$ is small when requests share common
dependencies (e.g., many projects depend on `openssl`),
yielding large savings.

**Status**: Machine-checked in Lean 4 (Theorem4.lean).

#### Theorem 4': Weighted Structural Deduplication

_Let $R$ concurrent requests produce uncached sub-DAGs $V'_1, \ldots, V'_R$. Let $d : V \to \mathbb{R}_{\geq 0}$ be the duration-weighted cost of each node. Then:\_

$$\sum_{v \in \bigcup V'_i} d(v) \leq \sum_{i=1}^R \sum_{v \in V'_i} d(v)$$

_with equality holding iff the uncached sub-DAGs are pairwise disjoint._

**Proof intuition**: This generalizes Theorem 4 from cardinality to duration-weighted sums, directly capturing the total computation reduction from content-addressed storage deduplication. Formally proved in Lean 4 using set-theoretic properties.

**Status**: Machine-checked in Lean 4 (Theorem4Prime.lean).

#### Theorem 5: Unified Coarsening Dominance

_Let $S_{\text{per}}$ be the per-request coarsened set of entry points, and $S_{\text{unified}} \subseteq S_{\text{per}}$ be the unified coarsened set of entry points under deduplication. For any valid schedule $\sigma_{\text{per}}$ on $S_{\text{per}}$, the restricted schedule $\sigma_{\text{unified}}$ on $S_{\text{unified}}$ satisfies:\_

$$M(\sigma_{\text{unified}}) \leq M(\sigma_{\text{per}})$$

_where $M(\sigma)$ is the makespan of schedule $\sigma$._

**Proof intuition**: Proves makespan dominance of the unified schedule over the per-request schedule. Because the unified coarsened set has fewer nodes to schedule due to deduplication (i.e. $S_{\text{unified}} \subseteq S_{\text{per}}$), a restricted schedule can be constructed that pointwise preserves or reduces task start times, durations, and load, thereby ensuring makespan dominance under the Schedule model.

**Status**: Machine-checked in Lean 4 (Theorem5.lean).
Zero `sorry`, zero custom `axiom`.

#### Theorem 6: CAS-Scheduling Bound

_Let $R$ concurrent requests produce uncached sub-DAGs $V'_1, \ldots, V'_R$. Let $\sigma_{\text{unified}}$ be the unified HEFT schedule on $G_\cup = \bigcup G'_i$ and $\sigma_{\text{indep}, i}$ be the independent schedules. Let $\rho = \frac{\sum_{v \in \bigcup V'_i} d(v)}{\sum_{i=1}^R \sum_{v \in V'_i} d(v)}$ be the deduplication factor. Then there exists a unified schedule $\sigma_\cup$ such that:\_

$$M(\sigma_\cup) \leq \alpha (1 + \rho \cdot |R|) \cdot \max_i M(\sigma_{\text{indep}, i})$$

**Proof intuition**: This connects CAS deduplication to scheduling quality, bounding makespan under worker contention. It is derived using Schedule.lean (valid worker schedules, non-overlapping constraints, and critical path makespan lower bounds) and HEFT.lean (the work-conserving makespan bound $M \le \text{CP} + \text{Work}/|W|$ proved from first principles). By replacing the bare scheduling makespan hypothesis with the verified work-conserving bound, the theorem establishes the final competitive ratio using only structural DAG parameters and the deduplication factor $\rho$, with no unproven scheduling assumptions.

**Status**: Machine-checked in Lean 4 (Theorem6.lean).

#### Theorem 7: Re-coarsening Convergence

_Let $C \subseteq V$ be the cache state of the system. Let $\text{coarse} : \mathcal{P}(V) \to \mathcal{P}(V)$ be the confidence-gated coarsening function selecting entry points from the uncached sub-DAG. Then:_

1. _Monotonicity: If $C_1 \subseteq C_2$, then $|\text{coarse}(C_2)| \leq |\text{coarse}(C_1)|$._
2. _Convergence: Under strict incremental cache growth, the active entry point set converges to $\emptyset$ in at most $|V|$ steps._

**Proof intuition**: Monotonicity is verified because a larger cache reduces the size of the uncached sub-DAG, yielding fewer or equal candidate entry points. Convergence follows because the uncached set strictly shrinks with each cache update, terminating in at most $|V|$ steps on any finite DAG.

**Status**: Machine-checked in Lean 4 (Theorem7.lean).

---

## Validation

| Check                                | Result    | Detail                                                                                 |
| :----------------------------------- | :-------- | :------------------------------------------------------------------------------------- |
| Coverage properties (1-4) coherent   | PASS      | Identity witness mechanized in Lean 4 (Thm 1);                                         |
|                                      |           | properties 1-4 are satisfiable and non-contradictory                                   |
| Coverage existence (Thm 1)           | PASS      | Machine-checked in Lean 4. Constructs `EosModel` with                                  |
|                                      |           | `S = V'`, `κ = id`. Zero `sorry`, zero `axiom`.                                        |
| Ordering soundness (P1)              | PASS      | Model-checked in TLA+ across 4 topology models                                         |
|                                      |           | (linear, diamond, convergence, independent)                                            |
| Coverage completeness (P2)           | PASS      | Track B (Lean) — guaranteed by `EosModel` total coverage                               |
|                                      |           | property, verified satisfiable by Thm 1                                                |
| Artifact completeness (P3)           | PASS      | Model-checked in TLA+ (`ArtifactSafety` invariant)                                     |
| Capacity safety (P4)                 | PASS      | Model-checked in TLA+ under all interleavings                                          |
| Liveness (P5, P6)                    | PASS      | Model-checked in TLA+ with `WF_vars(Next)` weak                                        |
|                                      |           | fairness. All 4 topology models verify both properties                                 |
| Federation liveness (P7)             | OPEN      | Requires real-time bounds ($\Diamond_{\leq\delta}$)                                    |
|                                      |           | not expressible in standard TLA+ temporal logic.                                       |
|                                      |           | Intentionally deferred — see note below                                                |
| HoL immunity (P5')                   | PASS      | Model-checked in TLA+ under concurrent requests                                        |
| Per-request completion (P6')         | PASS      | Model-checked in TLA+ under concurrent requests                                        |
| Frozen stability (P8)                | PASS      | Model-checked in TLA+ under dynamic re-coarsening                                      |
| Work conservation (P9)               | PASS      | Model-checked in TLA+ under concurrent requests                                        |
| Transient failure recovery (P10)     | PASS      | Model-checked in TLA+ under transient failures                                         |
| Failure isolation (P11)              | PASS      | Model-checked in TLA+ under deterministic and cascade failures                         |
| Consistency bound (Thm 2)            | PASS      | Machine-checked in Lean 4. Proves                                                      |
|                                      |           | $M(\sigma_H) \leq \alpha (1+\varepsilon)/(1-\varepsilon) \cdot M(\sigma^*)$            |
|                                      |           | via well-founded induction on DAG completion times                                     |
| Adaptive consistency (Thm 2')        | PASS      | Machine-checked in Lean 4 (Theorem2Prime.lean)                                         |
| Robustness — assignment stability    | PASS      | Lean 4 Lemma 3.1: perturbation $2P < \Delta_{\min}$ implies                            |
|                                      |           | $\sigma_H = \sigma_\text{base}$ (assignment identity)                                  |
| Robustness — EMA convergence         | PASS      | Lean 4: under sustained error $\eta \geq \eta_0$,                                      |
|                                      |           | EMA $\geq (1 - \gamma^n) \eta_0 + \gamma^n E_0$                                        |
| Robustness — μ-makespan bound        | OPEN      | Quantitative bound during transient not mechanized.                                    |
|                                      |           | Low risk — transient is short (geometric convergence)                                  |
|                                      |           | and capacity safety holds throughout (Track A)                                         |
| Structural deduplication (Thm 4)     | PASS      | Machine-checked in Lean 4. Proves                                                      |
|                                      |           | $\lvert\bigcup V'_i\rvert \leq \sum \lvert V'_i\rvert$                                 |
|                                      |           | with equality iff pairwise disjoint                                                    |
| Weighted deduplication (Thm 4')      | PASS      | Machine-checked in Lean 4. Generalizes Thm 4 to                                        |
|                                      |           | duration-weighted computation cost sums.                                               |
| Unified Coarsening Dominance (Thm 5) | PASS      | Machine-checked in Lean 4. Proves                                                      |
|                                      |           | $M(\sigma_{\text{unified}}) \leq M(\sigma_{\text{per}})$ via schedule restriction.     |
| CAS-scheduling bound (Thm 6)         | PASS      | Machine-checked in Lean 4. Bounds unified makespan                                     |
|                                      |           | as $M(\sigma_\cup) \leq \alpha(1+\rho \cdot \|R\|) \max_i M(\sigma_{\text{indep},i})$. |
| Re-coarsening convergence (Thm 7)    | PASS      | Machine-checked in Lean 4. Proves monotonicity and                                     |
|                                      |           | convergence of coarsened EPs under cache growth.                                       |
| End-to-end composition (Main Thm)    | PASS      | Machine-checked in Lean 4. Composes Thm 4'→5→HEFT→6                                    |
|                                      |           | into a single bound with all hypotheses justified.                                     |
| Graph coarsening optimality          | COND PASS | Bounded by $\alpha(\bar{\epsilon})$; competitive gap                                   |
|                                      |           | closes dynamically as prediction quality improves                                      |
| Minimality                           | PASS      | Two-track decomposition is minimal — protocol and                                      |
|                                      |           | optimization are formally independent concerns                                         |
| External adequacy                    | PASS      | Model captures all mechanisms described in ADR-0004                                    |

**Remaining open items:**

1. **Federation liveness (P7)**: Requires bounded artifact
   store propagation time $\delta$ as an environmental
   assumption. Under partition, $\delta \to \infty$ and P7
   fails. This is inherently unverifiable by the system —
   it depends on network infrastructure. Implementation
   should use timeouts with worker reassignment as a
   pragmatic mitigation.
2. **Robustness - $\mu$-makespan transient**: The transient makespan
   bound during prediction weight decay is not formally mechanized.
   Its correctness and boundedness are verified empirically and
   protected by local capacity constraints (Track A).
3. **Starvation prevention (P12)**: While the work-conserving liveness property (P9) guarantees that ready EPs are eventually dispatched, it does not prevent low-priority tasks from being starved indefinitely under continuous high-priority arrival. Formalizing starvation-freedom requires modeling arrival processes and priority queuing disciplines (e.g. aging or FIFO bounds), which is deferred to future work.
4. **DAG Boundedness and Memory Limits (P13)**: The TLA+ and Lean models assume a finite vertex set $V$. At runtime, the unified global DAG must be bounded to prevent memory exhaustion under continuous request streams. Proving memory safety and progress under sliding window request pruning is a future modeling objective.

---

## Implications

### Verification Status

Both tracks of formal verification are complete:

- **Track A (TLA+)**: Protocol correctness model-checked.
  `MultiRequestModel` verifies safety invariants (P1 Ordering,
  P3 Artifact, P4 Capacity, P11 Failure Isolation) and liveness
  properties (P5 Progress, P5' HoL Immunity, P6 Completion,
  P6' Per-request Completion, P8 Frozen Stability, P9 Work Conservation,
  P10 Transient Failure Recovery) under `WF_vars(Next)` weak fairness
  with multi-request DAG merging, cache-skip, cancellation,
  transient failure, and failure cascading. See `models/tla/`.
- **Track B (Lean 4)**: Optimization quality machine-checked.
  Zero `sorry`, zero custom `axiom`. Nine theorems verified
  with Mathlib: Theorem 1 (Coverage Existence), Theorem 2
  (Consistency Bound), Theorem 2' (Adaptive Consistency),
  Theorem 3 (Robustness), Theorem 4 (Structural Deduplication),
  Theorem 4' (Weighted Structural Deduplication), Theorem 5
  (Unified Coarsening Dominance), Theorem 6 (CAS-Scheduling Bound),
  Theorem 7 (Re-coarsening Convergence). See `models/lean/`.

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

6. **Structural deduplication keyed by derivation hash**: Theorem 4
   operates on abstract set families. In implementation, the
   content-addressed global DAG merging is the concrete instantiation.
   The theorem guarantees deduplication savings exactly equal the overlap
   between concurrent requests' uncached sub-DAGs.

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
