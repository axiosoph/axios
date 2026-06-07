# ADR-0004: Learning-Augmented Build Scheduling

- **Status**: PROPOSED (DRAFT)
- **Date**: 2026-06-05
- **Deciders**: nrd
- **Source**: [Eos SAD §7](../architecture/eos-sad.md) |
  [Eos Scheduler Spec](../specs/eos-scheduler.md) |
  [Formal Model](../models/publishing-stack-layers.md)

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

> **Terminology note**: This ADR and its companion formal model
> use "derivation" to refer to the Nix-native build unit. In
> Axios's layered terminology, the Eos engine abstraction is
> "Plan" (`BuildEngine::Plan`). The scheduling documents
> operate at the Nix/snix integration layer where "derivation"
> is the precise domain term.

The Eos scheduler currently uses tag-based set-containment
matching with Rendezvous hashing for cache affinity (SAD §7).
This is a correct baseline but leaves significant performance
on the table:

1. **No historical awareness**: Every build of atom X is
   treated identically regardless of whether X has been built
   a thousand times or never. The scheduler cannot distinguish
   a 2-second leaf compilation from a 30-minute monolithic
   link step until it's already running.

2. **No derivation DAG awareness**: When evaluation produces a
   derivation DAG, the scheduler sees a flat set of uncached
   derivations. It does not consider the graph structure when
   selecting which derivations to schedule as top-level build
   entries — missing opportunities to colocate tightly coupled
   subgraphs on the same machine.

3. **No multi-resource awareness**: Jobs are dispatched based
   on tag match and capacity, without considering whether the
   job's resource profile aligns with the worker's currently
   available resources. This causes fragmentation.

### The Derivation DAG Problem

This is the core scheduling challenge. A Nix evaluation
produces not a single derivation but a **directed acyclic
graph** of derivations — the top-level derivation and all
its transitive build dependencies:

```
top-level.drv
├── dep-a.drv
│   ├── dep-c.drv  (cached ✓)
│   └── dep-d.drv
├── dep-b.drv
│   ├── dep-d.drv  (shared with dep-a)
│   └── dep-e.drv  (cached ✓)
└── dep-f.drv
```

After filtering cached derivations, the remaining uncached
nodes form the **uncached sub-DAG**. The key insight from
Nix/snix's execution model is:

> **Scheduling a derivation for build automatically builds
> all its transitive dependencies.** The builder resolves
> the full dependency chain internally — there is no need
> to explicitly schedule each transitive node.

This means the scheduler's job is not to partition every
uncached node into groups. Its job is to **select optimal
entry points** (peaks) into the uncached DAG:

- Each entry point, when scheduled to a worker, causes the
  builder to transitively build everything below it on that
  same machine — with full locality (no cross-machine
  artifact transfers for transitive outputs).
- The scheduler tracks only the entry points, not every
  individual derivation in the graph.
- Parallelism comes from scheduling multiple entry points
  to different workers simultaneously.

The scheduling question becomes: **which derivations should
be the entry points, and which workers should execute them?**

#### Entry Point Selection

Naive approach (schedule only the top-level derivation):

- One worker builds the entire DAG sequentially
- Zero parallelism — all transitive dependencies serialize

Naive approach (schedule every uncached leaf independently):

- Maximum parallelism but redundant work — multiple workers
  may attempt to build the same shared dependency (dep-d
  appears under both dep-a and dep-b)
- Requires synchronization to avoid duplicate builds
- No locality benefit: outputs of shared dependencies must
  transfer through the global store

Optimal approach (select strategic entry points):

- Identify subgraph roots that capture useful amounts of
  transitive work without excessive overlap
- Schedule entry points that share dependencies to the same
  worker when possible (colocating dep-a and dep-b means
  dep-d builds once, locally)
- Assign heavy entry points to capable workers based on
  historical profiles or developer metadata

### The Atom-Id Advantage

Unlike generic build systems where task identity is fragile,
atom-id = `digest(anchor, label)` is **cryptographically
stable across versions**. Build #1 and build #1000 of atom X
share the same atom-id. This gives us a high-quality
prediction oracle:

- **Duration**: atom X consistently takes ~45s to evaluate,
  ~120s to build
- **Resources**: atom X consistently uses ~2GB RAM during
  build
- **DAG shape**: atom X's derivation DAG is structurally
  similar across versions (same dependencies, same depth)
- **Cache behavior**: atom X's dependencies are 90% cached
  after the first build

No ML training is needed — the atom protocol provides the
stable identifier that makes historical tracking trivially
reliable.

### Developer Metadata (Cold-Start Signal)

For atoms the scheduler has never encountered, developers
MAY provide scheduling hints via atom metadata tags:

```toml
[atom.metadata.scheduling]
expected-build-duration = "30m"
expected-build-memory   = "8GiB"
build-weight            = "heavy"
requires                = ["big-parallel", "kvm"]
```

This fills the cold-start gap. A developer publishing a
Chrome-scale build can signal "this is an extremely heavy
build that needs a capable machine" before the scheduler
has ever seen it. After the first build, historical profiles
take over and the metadata becomes a fallback.

**Priority order**: Historical profile (most data) >
developer metadata (domain knowledge) > system defaults
(conservative fallback).

---

## Decision

Augment the tag-based scheduler with four capabilities, each
grounded in academic prior art:

### 1. Historical Build Profiles (Unified Derivation Model)

Everything is a derivation. An atom is a derivation with
extra metadata (atom-id, developer scheduling hints), not a
separate classification. The profile store reflects this:

```
P[drv_name] → {
    build_duration:  ExponentialMovingAverage,
    build_memory:    ExponentialMovingAverage,
    build_cpu_cores: ExponentialMovingAverage,  // average or peak cpu cores
    output_size:     ExponentialMovingAverage,
    confidence:      f64,                        // computed as 1 - EMA(|η_e|) from error history

    // enrichment (present when derivation is also an atom)
    atom_id:        Option<AtomId>,
    atom_metadata:  Option<SchedulingMetadata>,
}
```

Profiles are keyed by **derivation name** (the `name` field
from `StorePath`, e.g., `openssl-3.0.12`). Derivation names
are human-readable and structurally stable — the same
package produces similarly-named derivations across versions.

When a derivation that appears as a transitive dependency in
one atom's DAG is also an independently-published atom, the
scheduler recognizes this: the derivation name matches an
existing profile, and the atom-id (if present) provides
cross-version stability and access to developer-provided
scheduling metadata.

After each completed build, update `P[drv_name]` with
observed metrics.

**Prediction resolution** for a derivation:

1. **Exact match**: `P[drv_name]` — exact derivation name match (historical).
2. **Cross-version aggregate**: Aggregate EMA of the `atom_id` group — if the derivation is an atom, query the secondary index to aggregate historical profiles from other versions of the same atom (calibrating prediction for version bumps like `openssl-3.0.12` → `openssl-3.0.13`).
3. **Developer metadata**: `P[drv_name].atom_metadata` — developer-provided hints (if derivation is an atom).
4. **Defaults**: System defaults — conservative fallback.

**Cross-version querying**: The `atom_id` field is a
secondary index, not just a passive annotation. Grouping
profiles by `atom_id` gives the full historical trajectory
of an atom across versions — how build duration, memory, CPU,
and DAG shape have evolved over time. This is essential for
trend detection (is this atom's build getting heavier?),
EMA calibration, and operator visibility. Derivation names
change with versions (e.g., `openssl-3.0.12` →
`openssl-3.0.13`), but the atom-id groups them coherently.

This unified model avoids the complexity of maintaining
separate atom-level and derivation-level stores. A
derivation that is also an atom simply has richer metadata
in the same profile entry.

**Prior art**: Learning-augmented algorithms framework
(Mitzenmacher & Vassilvitskii, arXiv:2006.09123). Historical
profiles and developer metadata serve as the "prediction" in
the consistency/robustness/smoothness framework.

### 2. Entry Point DAG Construction and Dispatch

#### 2a. Construction of the Unified Global DAG and Coarsening

To prevent redundant computation across concurrent requests and minimize myopic scheduling, all requests contribute to a **unified global derivation graph** $G_\cup = (V_\cup, E_\cup)$ keyed by derivation hash (N1). When a new build request arrives, its derivation sub-graph is merged JIT into the global DAG in $O(|V_{\text{new}}| + |E_{\text{new}}|)$ time. Derivations whose outputs are already cached in the artifact store are immediately filtered out.

The scheduler partitions the mutable portion of the global DAG into coarsened **entry points** ($S$). Entry points are selected top-down to cover the uncached sub-DAG using a greedy walk governed by the following promotion criteria:

A node $v$ is promoted to a standalone entry point ($v \in S$) if:
$$\operatorname{predicted_cost}(v) > \frac{\theta_{\text{cost}}}{1 + \operatorname{conf}(v) \cdot \theta_{\text{scale}}} \;\lor\; \operatorname{fan_in}(v) > \theta_{\text{fanin}} \;\lor\; \operatorname{subgraph_cost}(v) > \frac{\theta_{\text{subgraph}}}{1 + \operatorname{conf}(v) \cdot \theta_{\text{scale}}}$$

Where:

- **Troublesomeness**: High predicted cost or resource footprint.
- **Convergence (Fan-in)**: High fan-in node (many direct dependents).
- **Subgraph Volume**: Large aggregate transitive cost below $v$.
- **Confidence Gate**: $\text{conf}(v) = 1 - \text{EMA}(|\eta_v|)$ modulates the cost thresholds based on past prediction error. When confidence is low ($\text{conf} \to 0$), cost thresholds remain high (conservative coarsening: fewer EPs, fewer scheduling decisions). When confidence is high ($\text{conf} \to 1$), cost thresholds are lowered, allowing selective promotion for fine-grained HEFT optimization.

**Frozen/Mutable Partition & Re-Coarsening**:
The global scheduling state is partitioned into two regions:

1. **FROZEN partition**: $\{ ep \mid ep.\text{status} \in \{\text{dispatched}, \text{complete}, \text{failed}\} \}$. This boundary is sacred. Dispatched EPs are immutable: their coverage scope $\kappa(v, s)$ and worker assignment can never change (Frozen Stability invariant).
2. **MUTABLE partition**: $\{ ep \mid ep.\text{status} \in \{\text{pending}, \text{ready}\} \} \cup \{ \text{unassigned nodes} \}$. These EPs can be freely restructured (merged, split, promoted, demoted) by incremental re-coarsening when new requests arrive or cache updates occur (Mutable Freedom invariant).

#### 2b. Event-Driven HEFT Dispatch Protocol

Instead of sequential topological dispatch or epoch batching, the scheduler employs **event-driven HEFT re-planning** on the **full EP DAG** (pending + ready + frozen). HEFT computes a complete time-slotted assignment plan in $O(|S|^2 \cdot |W|)$ time. Ready EPs with feasible workers are dispatched immediately, while pending EPs are tentatively scheduled to future time slots. These tentative assignments do NOT lock workers; workers remain available for ready work.

To avoid scheduler thrashing, the engine uses an **event coalescing** loop (draining the non-blocking event channel completely before running a single HEFT pass).

The state transitions are governed by the following event handlers:

1. **RequestArrival(dag, request_id)**:
   - Cache filter: Remove cached derivations from incoming DAG.
   - Merge: JIT merge uncached nodes/edges into $G_\cup$.
   - Request tracking: Add `request_id` to `request_clients` set on each node.
   - Incremental re-coarsening: Re-coarsen the MUTABLE partition.
   - HEFT re-plan: Run HEFT on full EP DAG.
   - Dispatch: Dispatch ready EPs to feasible workers.
2. **EPComplete(ep)**:
   - Freeze: Mark `ep` as completed (frozen).
   - Cache update: Populate artifact store with `ep` outputs.
   - EMA update: Refine prediction error and update profile.
   - Cache-skip scan: For each MUTABLE EP whose scope overlaps `ep`: if all outputs are now in the store, mark it complete (cache-skipped); otherwise, update its predicted duration.
   - Dependency cascade: Downstream pending EPs whose dependencies are satisfied transition to `ready`.
   - HEFT re-plan & Dispatch: Run HEFT and dispatch ready EPs.
3. **EPFail(ep, failure_kind)**:
   - If `deterministic`: Mark failed, fail downstream dependents, and notify clients.
   - If `transient` (infra failure): Revert `ep` to `ready`, run HEFT, and re-dispatch.
4. **WorkerHealthChange(worker, unhealthy)**:
   - Revert affected running EPs on `worker` to `ready`.
   - Run HEFT excluding `worker` and dispatch.
5. **RequestCancellation(request_id)**:
   - For each EP, remove `request_id` from `request_clients`.
   - If `request_clients` is empty and EP is MUTABLE, prune it.

**Two-Level Deduplication & CAS Idempotency**:
Deduplication is achieved at two distinct levels:

1. **Entry Point Level (Unified DAG)**: Structural deduplication. Because requests merge into $G_\cup$, identical derivation hashes map to the same node. This is the primary defense against redundant computation.
2. **Derivation Level (CAS Idempotency)**: Unlike Nix's legacy model which relies on store-level locks, Snix store operations (gRPC `BlobService`, `DirectoryService`, `PathInfoService`) are content-addressed and **purely idempotent**. Concurrent writes of the same outputs both succeed. If two builders race on the same derivation, they both execute the computation, incurring **redundant CPU/resource cost** rather than lock contention. The builder's internal `has()` check provides a partial defense by skipping a build if it finishes after another, but the scheduler's convergence-point promotion is the primary mechanism to prevent overlapping scopes from triggering wasted computation.

**Prior art**: Graphene/DagPS (Grandl et al., OSDI 2016) — troublesome task identification and pre-allocation in multi-resource space-time.

### 3. Multi-Criteria Placement Scoring

For each feasible worker `w` and entry point `e`, compute:

```
score(w, e) = α · affinity(w, e)
            + β · resource_fit(w, e)
            + γ · availability(w)
```

Where:

- **affinity(w, e)**: Local Rendezvous Hash score for atom
  content locality. Measures likelihood that worker `w`
  already has entry point `e`'s source tree and transitive
  inputs cached locally.

- **resource_fit(w, e)**: Capacity-normalized resource alignment
  between the entry point's predicted resource vector $\mathbf{r}_e$ and
  the worker's available capacity vector $\mathbf{a}_w$, normalized by the
  worker's total capacity vector $\mathbf{c}_w$ (as established in Tetris):
  $$\operatorname{resource_fit}(w, e) = \sum_{i \in \{\text{cpu}, \text{mem}, \text{disk}\}} \left( \frac{r_{e,i}}{c_{w,i}} \cdot \frac{a_{w,i}}{c_{w,i}} \right)$$
  Normalizing each dimension by total worker capacity $c_{w,i}$ converts
  raw resource values (e.g., CPU cores vs. memory bytes) into unitless, comparable
  fractions in $[0,1]$ before combination. This prevents large byte ranges from
  dominating CPU count while preserving the absolute magnitude of tasks to steer
  heavy builds to capable workers (preventing the magnitude erasure of cosine similarity).

- **availability(w)**: Headroom ratio:
  `1 - (w.current_load / w.max_capacity)`
  Prefers workers with more spare capacity (dimensionless $\in [0,1]$).

Weights `α`, `β`, `γ` are operator-tunable. To protect against adversarial or
highly inaccurate predictions, the effective weight of the resource fit term
is dynamically decayed based on the exponential moving average (EMA) of the absolute
relative prediction error:
$$\beta_e = \beta \cdot e^{-\lambda \cdot \text{EMA}(|\eta_e|)}$$
where $\text{EMA}(|\eta_e|)$ is the running average of the absolute relative prediction
error magnitude for that atom/derivation group, and $\lambda > 0$ is a decay
constant. If predictions are systematically incorrect (even if stable and having zero variance),
$\text{EMA}(|\eta_e|)$ grows, causing $\beta_e \to 0$ and safely falling back to the
prediction-free baseline. Operators running homogeneous clusters may set $\beta = 0$.

**Symbol note**: The scoring weights $\alpha, \beta, \gamma$
are operator-tunable parameters distinct from the identically
named symbols in the formal guarantees: $\alpha$ in Theorem 2
is the heuristic's base approximation ratio (a bound, not a
knob), and $\gamma$ in Theorem 3 is the EMA smoothing factor
(a convergence parameter). Context disambiguates.

**Prior art**: Tetris (Grandl et al., SIGCOMM 2014) —
multi-resource dot-product alignment heuristic.

#### 3a. Operational Tuning and Sensitivity Analysis

Production scheduling performance is highly sensitive to the prediction decay constant ($\lambda$) and the entry-point coarsening thresholds.

##### Prediction Decay Constant ($\lambda$) Tuning

The decay constant $\lambda$ governs the rate at which the resource-fit optimization term degrades under prediction errors. A primary design goal is balancing responsiveness to systematic prediction error against resilience to ambient noise (e.g., I/O jitter, network drops, or CPU throttling):

- **High Sensitivity ($\lambda \gg 1$)**: Setting $\lambda$ too high causes the scheduler to treat minor ambient variance as a systematic failure. A transient network slowdown during a build will temporarily inflate $\text{EMA}(|\eta_e|)$, causing $\beta_e \to 0$ rapidly. The scheduler will discard valuable historical profiles and fall back to the prediction-free baseline prematurely, resulting in suboptimal multi-resource bin-packing and increased cluster fragmentation.
- **Low Sensitivity ($\lambda \approx 0$)**: Setting $\lambda$ too low makes the scheduler sluggish to react to true systematic prediction errors (e.g., when a package version bump drastically changes its dependency tree or memory usage, or when developer-provided cold-start metadata is highly inaccurate). The scheduler will continue to place tasks using incorrect estimates, leading to resource overloading, queue stalls, and load imbalance.

To tune $\lambda$, operators should align it with the expected coefficient of variation ($CV$) of healthy build execution times. Let $CV = \sigma_{\text{ambient}} / \mu_{\text{duration}}$ represent the standard deviation of ambient duration jitter under nominal conditions. The decay constant should satisfy:

- $e^{-\lambda \cdot CV} \approx 1$ (nominal jitter does not decay weight).
- $e^{-\lambda \cdot E_{\text{sys}}} \approx 0$ for a systematic error magnitude $E_{\text{sys}}$ (e.g., $E_{\text{sys}} \ge 1.0$, corresponding to a $100\%$ or greater estimation error).

##### Entry-Point Coarsening Thresholds

The entry-point selection heuristic determines which nodes in the uncached sub-DAG are promoted to standalone entry points ($S$) and which are merged into transitive parent scopes. The thresholds for promote-vs-merge (subgraph cost threshold, convergence/fan-in threshold, and resource limits) dictate the scheduling granularity:

- **Aggressive Coarsening (High Thresholds)**: Too few entry points are promoted. Trivial dependencies, convergence points, and even heavy derivations are absorbed into a small number of top-level tasks. This serializes execution, forcing individual workers to build large subgraphs sequentially and starving the rest of the cluster of parallel execution opportunities.
- **Weak Coarsening (Low Thresholds)**: Too many entry points are promoted. The scheduler shatters the DAG into a high volume of tiny tasks (e.g., short compile tasks, small script runs). This leads to:
  1. High scheduling queue overhead.
  2. Destruction of input cache locality, as sub-DAG nodes are scattered across different workers rather than executing on the same machine.
  3. Redundant computation, as multiple workers simultaneously execute the same shared transient dependencies due to overlapping entry point scopes.

Production environments should calibrate coarsening thresholds to the _scheduling latency_ of the cluster. The minimum subgraph cost threshold must be significantly larger than the average round-trip scheduling latency (dispatch + worker pickup overhead) to ensure that the parallel execution benefit outweighs the coordination cost.

### 3b. Strategy Trait and JIT Solver

The scheduler decouples the mathematical properties of the assignment phase from the concrete solver implementation through a strategy trait boundary:

```rust
pub trait SchedulerStrategy {
    fn plan(&self, ep_dag: &EpDag, workers: &[Worker], predictions: &Profiles)
        -> Assignment;

    // Invariants any strategy must satisfy:
    // 1. Total coverage: every EP is assigned to some worker
    // 2. Dependency ordering: if A -> B in EP DAG, B does not start before A completes
    // 3. Worker capacity: peak memory/CPU does not exceed worker limits

    // Competitive Makespan Bound:
    fn alpha_bound(&self) -> f64;
}
```

The Eos scheduling daemon utilizes this trait boundary to support three swappable strategy backends:

1. **HEFT (Primary)**: The default active strategy. HEFT constructs a time-slotted execution plan. It is highly efficient ($O(|S|^2 \cdot |W|)$) and provides a provable competitive bound ($\alpha_{\text{heft}}$) mechanized in Lean 4.
2. **Greedy (Fallback)**: A low-overhead fallback strategy activated under high prediction error or missing profiles. It assigns tasks greedily to the highest scoring cache-affinity worker, satisfying basic capacity safety.
3. **MILP/MCMF (Deferred)**: A global solver strategy reserved for federated scale ($|W| > 1000$). It solves the JIT assignment globally via mixed-integer programming or min-cost max-flow, operating over the coarsened Atom DAG. Because the graph size is naturally bounded ($|S| \leq 20$), the solver completes in sub-millisecond time, bypassing NP-hardness in practice.

### 3.1 Virtual Capacity and Enforcement

The scheduler's capacity model is **enforcement-agnostic**.
`WorkerCap` is an abstract capacity vector reported by the
worker at registration. The scheduler treats it as a budget
and applies a single dispatch guard uniformly:

$$
L(w) + \text{PredictedLoad}(s) \leq \text{WorkerCap}(w)
$$

How that capacity is determined is a worker-side concern:

- **With OCI cgroups**: The worker reports its full physical
  capacity (e.g., 128 cores, 256 GiB RAM) because enforcement
  is kernel-guaranteed. Each dispatched build receives a cgroup
  with limits matching `PredictedLoad` — the OCI runtime spec
  sets `linux.resources.cpu.max` and `linux.resources.memory.max`.
  Builds physically cannot exceed their allocation. If a
  prediction underestimates, the build is throttled by the
  kernel, not the machine exhausted. P4 is **kernel-enforced**.

- **Without OCI (macOS, disabled)**: The worker reports a
  **conservative virtual capacity** — smaller than hardware
  reality, accounting for the lack of enforcement and
  shared-resource interference. A 128-core Mac might report
  effective capacity equivalent to 2–3 concurrent max-jobs.
  P4 is **scheduler-enforced** with conservative margins.

The scheduler algorithm is identical in both cases. No
conditional branches, no tier detection, no capability
queries. The abstraction boundary is clean:

```
┌────────────────────────────────┐
│   Scheduler                    │
│   Sees only: WorkerCap[w]      │
│   Guard: L[w] + P[s] <= C[w]  │
└──────────────┬─────────────────┘
               │ abstract capacity vector
┌──────────────▼─────────────────┐
│   Worker Registration          │
│   ┌─────────┐  ┌─────────────┐ │
│   │ OCI:    │  │ Non-OCI:    │ │
│   │ report  │  │ report      │ │
│   │ physical│  │ conservative│ │
│   │ capacity│  │ virtual cap │ │
│   └─────────┘  └─────────────┘ │
└────────────────────────────────┘
```

**Snix seam**: `BuildConstraints::MinMemory(u64)` already
exists in the build request protocol
(`snix/build/src/buildservice/build_request.rs`). Extending
with CPU resource constraints is a clean addition.
`snix/build/src/oci/spec.rs` generates OCI runtime specs
with PID/IPC/UTS/Mount namespaces; cgroup namespace support
is architecturally prepared.

### 4. Local Rendezvous Hashing (LRH)

Replace standard Highest Random Weight (HRW) hashing with
Local Rendezvous Hashing, which restricts candidate
selection to a cache-local window of `C` neighbors on a
virtual ring:

- Near-optimal load balance (comparable to multi-probe
  consistent hashing)
- ~6.8× higher throughput than standard HRW by exploiting
  CPU cache locality during hash computation
- 0% excess churn under topology-fixed failures
- O(log|R| + C) lookup complexity

**Prior art**: Local Rendezvous Hashing
(arXiv:2512.23434, 2025).

### 5. Computational Complexity

The entire scheduling pipeline runs in **polynomial time**,
linear in the DAG size plus the product of entry points
and workers. No phase has exponential or super-polynomial
cost.

#### Per-Event Cost (Coalesced Batch)

Each coalesced event batch triggers the following phases.
Not all phases run on every event type — `RequestArrival`
triggers DAG merge + re-coarsening + HEFT; `EPComplete`
triggers cache-skip scan + dependency cascade + HEFT;
failure/health events trigger only HEFT re-planning.

| Phase                   | Operation                               | Complexity                                     | Trigger Events |
| :---------------------- | :-------------------------------------- | :--------------------------------------------- | :------------- |
| DAG merge (incremental) | JIT merge new nodes/edges into $G_\cup$ | $O(\|V_{\text{new}}\| + \|E_{\text{new}}\|)$   | RequestArrival |
| Cache filtering         | Check store for each new node           | $O(\|V_{\text{new}}\|)$                        | RequestArrival |
| Re-coarsening           | Greedy walk over MUTABLE partition      | $O(\|V'_{\text{mut}}\| + \|E'_{\text{mut}}\|)$ | RequestArrival |
| Cache-skip scan         | Check scope overlap for mutable EPs     | $O(\|S_{\text{mut}}\| \cdot \bar\kappa)$       | EPComplete     |
| Dependency cascade      | Update ready status of pending EPs      | $O(\|S\|)$                                     | EPComplete     |
| EP DAG derivation       | Transitive closure on selected set      | $O(\|S\|^2)$ worst case                        | RequestArrival |
| HEFT re-planning        | Full EP DAG scheduling                  | $O(\|S\|^2 \cdot \|W\|)$                       | All events     |
| EMA update              | Per-completion profile update           | $O(1)$                                         | EPComplete     |

Where $\bar\kappa$ is the average entry-point scope size
(number of derivations in a typical EP's transitive closure).

The **dominant per-event cost** is HEFT re-planning at
$O(\|S\|^2 \cdot \|W\|)$. Event coalescing amortizes this:
$k$ closely-spaced events trigger one HEFT pass, not $k$.

#### Initial Construction Cost

The first `RequestArrival` for a given request incurs the
full DAG construction cost:

| Phase                 | Operation                                    | Complexity                                 |
| :-------------------- | :------------------------------------------- | :----------------------------------------- |
| DAG construction      | Topological sort + cache filtering           | $O(\|V'\| + \|E'\|)$                       |
| Entry point selection | Greedy top-down walk with fan-in/cost checks | $O(\|V'\| + \|E'\|)$                       |
| EP DAG derivation     | Transitive closure on selected set           | $O(\|S\|^2)$ worst case                    |
| HEFT planning         | Initial full EP DAG scheduling               | $O(\|S\|^2 \cdot \|W\|)$                   |
| LRH lookup (per EP)   | Cache affinity scoring                       | $O(\log\|R\| + C)$                         |
| **Total (initial)**   | **First scheduling pass**                    | $O(\|V'\| + \|E'\| + \|S\|^2 \cdot \|W\|)$ |

#### Lifecycle Cost

Over the full lifetime of a request (from arrival through
all EP completions), the scheduler runs approximately
$\|S\|$ HEFT re-plans (one per EP completion, amortized by
coalescing). The total lifecycle cost is:

$$O(\|V'\| + \|E'\| + \|S\|^3 \cdot \|W\|)$$

The $\|S\|^3$ term arises from $\|S\|$ re-plans each
costing $O(\|S\|^2 \cdot \|W\|)$. In practice, coalescing
reduces this significantly — concurrent completions
trigger a single re-plan. The $\|S\|^3$ bound is
conservative.

Where:

- $\|V'\|, \|E'\|$ = uncached sub-DAG nodes and edges
- $\|S\|$ = selected entry points ($\|S\| \leq \|V'\|$,
  typically $\|S\| \ll \|V'\|$ due to coarsening)
- $\|W\|$ = number of candidate workers
- $d$ = resource dimensions (constant, typically 3–4:
  CPU, memory, disk, optionally egress bandwidth — see note #12)
- $\|R\|$ = LRH ring size, $C$ = cache-local window
- EP DAG derivation is $O(\|S\|^2)$ in the worst case
  (all-pairs reachability on the coarsened set) but
  typically much smaller since $\|S\| \ll \|V'\|$
- **Single-machine degenerate case**: When $\|W\| = 1$,
  HEFT degenerates to a topological sort and coarsening
  is irrelevant — all EPs are assigned to the single
  worker regardless of the partition chosen

**Key property**: The NP-hard optimal entry point
selection is sidestepped by the greedy heuristic, which
runs in $O(\|V'\| + \|E'\|)$. The tradeoff is between
selection quality (captured by $\alpha$ in Theorem 2) and
computational cost. A global MILP solver could potentially
find a better $\alpha$ but at exponential cost —
unacceptable when the scheduling decision itself must
complete in sub-second time.

**Implementation note**: The dominant cost per event is
HEFT at $O(\|S\|^2 \cdot \|W\|)$. For a cluster of 100
workers and a DAG coarsened to 50 entry points, this is
250,000 operations per HEFT pass — trivially fast. At
federated scale ($\|W\| > 1000$), the min-cost flow
formulation (§Future Work) replaces HEFT with a global
flow optimization.

---

## Guarantees

The learning-augmented framework provides three formal
properties, now backed by machine-checked proofs
(Lean 4, Track B) and model-checked protocol verification
(TLA+, Track A). See `docs/models/lean/` and
`docs/models/tla/`.

### Consistency (Machine-Checked — Theorem 2)

When historical predictions are $\varepsilon$-accurate
($|d(s) - \hat{d}(s)| \leq \varepsilon \cdot \hat{d}(s)$),
the heuristic assignment $\sigma_H$ satisfies:

$$
M(\sigma_H) \leq \alpha \cdot \frac{1 + \varepsilon}
{1 - \varepsilon} \cdot M(\sigma^*)
$$

where $\alpha$ is the heuristic's base approximation ratio
on perfectly-predicted inputs, and $\sigma^*$ is the optimal
offline assignment. For small $\varepsilon$, this simplifies
to $\alpha \cdot (1 + 2\varepsilon + O(\varepsilon^2))$.

This bound cleanly separates two concerns: heuristic quality
($\alpha$, a function of DAG structure and scoring function)
and prediction degradation ($(1+\varepsilon)/(1-\varepsilon)$,
a function of profile accuracy). The implementation can
independently tune each axis.

**Implementation note**: After each build, compute observed
$\varepsilon$ from `|d_actual - d_predicted| / d_predicted`
and track it via EMA. This provides a live monitorable
metric for how close the system is to the proven bound.

### Adaptive Consistency (Machine-Checked — Theorem 2')

As prediction error increases gradually, scheduling quality degrades proportionally through the parameterized approximation function $\alpha(\bar{\epsilon})$:

$$
M(\sigma_H) \leq \alpha(\bar{\epsilon}) \cdot \frac{1 + \bar{\epsilon}}{1 - \bar{\epsilon}} \cdot M(\sigma^*)
$$

Where:

- $\bar{\epsilon}$ is the average prediction error magnitude.
- $\alpha(\bar{\epsilon})$ is a monotonically non-increasing function representing the quality of the selected entry-point DAG coarsening:
  $$\alpha(0) = \alpha_{\text{heft}} \quad \text{and} \quad \alpha(1) \leq \alpha_{\text{max}}$$

This theorem bounds the combined performance of both graph coarsening and worker placement. Under accurate predictions ($\bar{\epsilon} \to 0$), cost thresholds are safely lowered, yielding fine-grained entry points that HEFT schedules optimally ($\alpha \to \alpha_{\text{heft}}$). When error is high ($\bar{\epsilon} \to 1$), thresholds rise, yielding a coarse DAG with fewer entry points, falling back to the prediction-free baseline bound $\alpha_{\text{max}}$.

### Robustness (Machine-Checked — Theorem 3)

When predictions are arbitrarily wrong ($\eta \to \infty$),
the system self-heals:

1. **Assignment stability** (Lemma 3.1): If the prediction
   perturbation satisfies $2P < \Delta_{\min}$ (twice the
   perturbation is less than the baseline scoring gap), then
   $\sigma_H = \sigma_{\text{base}}$ — the heuristic makes
   the same assignment as the prediction-free baseline.

2. **EMA convergence**: Under sustained prediction error
   $\eta \geq \eta_0 > 0$, the EMA of error magnitudes
   satisfies $\text{EMA}_n \geq (1 - \gamma^n) \eta_0 +
   \gamma^n E_0$, causing the prediction weight $\beta_e$
   to decay geometrically toward zero.

Together: bad predictions → EMA grows → $\beta_e$ decays →
perturbation shrinks → eventually $2P < \Delta_{\min}$ →
assignment equals baseline. The convergence is automatic
and requires $O(\ln(\beta R_{\max}/\Delta_{\min}))$
observations.

### Smoothness (Corollary of Theorem 2)

As prediction error increases gradually (incremental version
changes, slowly shifting build profiles), scheduling quality
degrades proportionally via the $(1+\varepsilon)/(1-\varepsilon)$
factor — not catastrophically. This follows directly from
Theorem 2's bound: the consistency ratio is a smooth, monotone
function of $\varepsilon$. When error becomes sustained, the
EMA decay mechanism (Theorem 3) automatically collapses the
scoring function to the prediction-free baseline.

---

## Optimality Assessment

The formal proofs (§Appendix) establish that the algorithm
is **correct** (Track A) and achieves **bounded quality**
relative to an optimal offline schedule on a fixed DAG
(Track B). But "bounded" is not "optimal." This section
documents the specific tradeoffs.

### Where the Algorithm Departs from Optimality

To achieve $O(\|V'\| + \|E'\| + \|S\|^2 \cdot \|W\|)$
polynomial-time scheduling (§5), the design makes four
intentional mathematical compromises relative to a
theoretical omniscient MILP solver:

1. **Graph coarsening blindspot**: Finding the optimal
   antichain cover of a weighted DAG to minimize
   distributed makespan is NP-hard. The entry point
   selection uses a greedy heuristic — it cannot foresee
   if bundling a subgraph will accidentally serialize
   tasks that could have been parallelized, or fragment
   what should be colocated. This gap is captured by
   $\alpha(\bar\varepsilon)$ in Theorem 2': the coarsening
   gap closes adaptively as the mean prediction error
   $\bar\varepsilon \to 0$, because higher-confidence
   predictions enable finer-grained partitions.

2. **Cross-event residual myopia**: HEFT on the full EP
   DAG replaces the earlier myopic greedy dispatch,
   computing a global priority ordering across all ready
   entry points within a single scheduling event.
   Residual sub-optimality is bounded to cross-event
   myopia — decisions made between successive HEFT
   re-plans, where a completion event reveals new ready
   EPs that could not be anticipated. Event coalescing
   (batching closely-spaced events into a single HEFT
   invocation) further reduces this gap.
   Example: a 4-worker cluster with 4 available cores
   each receives 5 concurrent 4-core entry points.
   Capacity is exhausted (4 × 4 = 16 cores, 5 × 4 = 20
   required). The 5th EP must wait for a completion
   event, and HEFT cannot anticipate which of the 4
   running EPs will finish first. An omniscient scheduler
   with perfect completion-time knowledge would pre-assign
   the 5th EP to the worker whose current task finishes
   soonest.

3. **Submodular redundant computation**: The relaxed coverage
   mapping permits non-entry-point derivations to exist in
   multiple entry points' transitive scopes. When two
   workers concurrently build entry points with shared
   transitives (e.g., `openssl`), they will both redundantly
   build that shared dependency if their executions overlap.
   The builder's `has()` check will only skip the build if
   the other builder completes and publishes the output before
   the second builder starts the same derivation. A global
   solver could calculate submodular overlap costs and perfectly
   stagger or route tasks to eliminate redundant computation.

4. **Federation topology blindness**: LRH measures logical
   hash-ring distance, not physical WAN bandwidth or
   inter-cluster latency. Transferring a 50GB artifact
   from Tokyo to Virginia incurs massive wall-clock
   penalties invisible to the scoring function. The
   deferred min-cost flow formulation (§Future Work)
   addresses this by encoding federation topology as edge
   costs.

### Why It Is Highly Efficacious for This Domain

The algorithm exploits three domain-specific realities
that close most of the gap to theoretical optimality:

1. **Nix granularity solution**: Nix derivation DAGs are
   wildly bimodal — a few colossal linchpin tasks plus
   thousands of trivial micro-tasks (patches, fetches,
   hooks). Academic schedulers choke on this; scheduling
   10,000 micro-tasks across a network incurs RPC overhead
   that dwarfs compute time. Entry point coarsening forces
   the distributed scheduler to handle only macroscopic
   peaks, offloading trivial leaves to the builder's
   native transitive execution with perfect local data
   locality and zero network overhead.

2. **Atom-id prediction oracle**: The hardest part of DAG
   scheduling is predicting task durations. Advanced
   systems (Decima) use graph neural networks requiring
   expensive training. Because Nix evaluation is pure and
   `atom-id = digest(anchor, label)` is cryptographically
   stable across versions, a cheap EMA over historical
   observations achieves the performance of a sophisticated
   learning system. The atom protocol IS the prediction
   oracle.

3. **Fail-safe packing**: The $\beta_e$ decay mechanism
   (§3) guarantees that when predictions fail, the scoring
   function smoothly degrades to a cache-affinity baseline
   rather than actively making adversarial placements that
   crash workers via OOM kills — the primary cause of
   CI/CD bottlenecking in production.

### The Simulation Gap

Formal proofs validate bounds on **fixed, abstract DAGs**
but cannot validate that the heuristic thresholds
(troublesome node cost, convergence fan-in limit, subgraph
cost threshold) are well-calibrated for **real Nix
derivation shapes**. The $\alpha$ factor in Theorem 2
absorbs heuristic quality — the proofs guarantee bounded
quality for any $\alpha$ but do not tell us what $\alpha$
is in practice.

Bridging this gap requires trace-driven simulation:

1. Extract real derivation DAGs from `nixpkgs` (e.g.,
   `chromium`, `kde-plasma`, `linux`) as trace corpus
2. Extract actual build durations and resource profiles
   from Hydra/Cachix as ground truth
3. Implement the entry point selection + scoring in a
   single-threaded Rust simulator
4. Sweep heuristic thresholds and measure simulated
   makespan against an LRH-only baseline (cache-affinity
   dispatch without coarsening or prediction)

If the simulator demonstrates order-of-magnitude DAG
reduction without serializing the critical path, the
algorithm's practical efficacy is confirmed beyond what
formal proofs alone can establish.

---

## Rejected Alternatives

### GNN + Reinforcement Learning (Decima)

Decima (Mao et al., SIGCOMM 2019, arXiv:1810.01963) uses
graph neural networks to embed DAG structure and an RL agent
to make scheduling decisions. Rejected because:

1. **No simulator**: Decima requires a faithful cluster
   simulator for RL training. Building one is substantial
   engineering effort outside our core mission.
2. **No production deployments**: Even the authors only
   demonstrated on a 25-node prototype. No known production
   use.
3. **Opaque scheduling**: RL policies are non-interpretable.
   Debugging "why was this job placed here?" requires
   inspecting neural network weights.
4. **Generalization failure**: RL policies trained on one
   workload distribution don't generalize to another.
   Workload shifts require retraining.
5. **Unnecessary**: Atom-id stability gives us the prediction
   oracle that Decima needs a simulator to learn. Historical
   profiles with the learning-augmented framework provide
   the same adaptive benefit with provable guarantees and
   deterministic, debuggable behavior.

### Peer-to-Peer Work Stealing

No production build system uses inter-worker work stealing
for coarse-grained build tasks. Builds are non-migratable —
the cost of abort-and-re-execute exceeds the benefit of
rebalancing. The locality-aware gain function from SLAW
(Guo et al., IPDPS 2010) is valuable as a placement
heuristic (the `resource_fit` scoring term), not as a
runtime stealing mechanism.

Additionally, Nix's execution model (builder transitively
resolves dependencies internally) means there is no
meaningful "steal" granularity — you cannot steal a
transitive dependency mid-build.

### MILP Solver (TetriSched)

TetriSched (Tumanov et al., EuroSys 2016) uses Mixed Integer
Linear Programming for global rescheduling. Adds a solver
dependency (CPLEX/Gurobi/SCIP) and solving time may exceed
scheduling time at smaller scales. Rejected for the initial
implementation but may be revisited if the MILP formulation
aligns well with the formal model (§Appendix).

### Min-Cost Flow (Firmament) — Deferred, Not Rejected

Firmament (Gog et al., OSDI 2016) models scheduling as
min-cost max-flow on a directed graph. Flow network encodes
tasks, machines, racks, locality, fairness, and anti-affinity
as edge costs. Incremental MCMF solving achieves sub-second
placement at 10,000+ machines.

This was initially dismissed on the assumption of small
cluster sizes. That assumption is wrong. The target
architecture includes **federated build sharing between
clusters** — potentially global-scale decentralized build
distribution for open-source ecosystems. At that scale,
min-cost flow's ability to encode complex placement
constraints (data locality, trust boundaries, federation
policies) as edge costs in a single optimization is
compelling.

**Status**: Deferred to a follow-up ADR. The initial
implementation uses the heuristic scoring function (§3).
Min-cost flow is a candidate replacement for the scoring
function when:

- Cluster sizes exceed ~1000 workers
- Federation introduces cross-cluster placement decisions
- The formal model (§Appendix) identifies properties that MCMF
  can guarantee and the heuristic cannot

---

## Consequences

### Positive

- **Predictable improvement**: Repeated builds of the same
  atoms (the common case in CI/CD) benefit from historical
  awareness without any configuration.
- **DAG-aware locality**: Entry point selection colocates
  related subgraphs on the same worker. The builder handles
  all transitive work internally with full locality.
- **Reduced scheduler tracking**: The scheduler tracks entry
  points, not individual derivations. The builder's internal
  dependency resolution handles the rest.
- **Resource efficiency**: Multi-resource scoring reduces
  fragmentation, improving effective cluster utilization.
- **Graceful degradation**: Cold starts with developer
  metadata degrade to informed estimates. Cold starts without
  metadata degrade to conservative defaults. Never worse than
  the current baseline.
- **Deterministic and debuggable**: No ML inference in the
  scheduling loop. Historical profiles are inspectable.
  Scheduling decisions are reproducible given the same state.
- **Cross-request optimization**: The unified global DAG
  shares cached dependencies across concurrent requests.
  HoL immunity (P5') ensures no request can block another's
  progress.
- **Adaptive coarsening**: Coarsening quality adjusts
  automatically to prediction accuracy via the confidence
  gate.

### Negative

- **Storage overhead**: Historical build profiles must be
  persisted. EMA per atom-id is small (~100 bytes), but
  scales with the number of distinct atoms.
- **Entry point quality is heuristic**: The selection
  algorithm uses greedy heuristics, not a provably optimal
  algorithm. Quality depends on prediction accuracy and
  DAG structure.
- **Tuning surface**: Three operator-tunable weights (α, β,
  γ) plus entry point selection thresholds. Sensible defaults
  should cover most cases.
- **Global DAG memory**: The unified global DAG's memory
  footprint is proportional to the sum of concurrent request
  DAG sizes. Under many concurrent requests with large DAGs,
  this may require memory budgeting.

### Risks

- **Redundant computation on overlapping scopes**: Two entry points on
  different workers whose transitive scopes share derivations will both
  attempt to build the shared derivations. Because Snix uses a
  CAS-idempotent store model rather than store-level locks, both workers
  will redundantly compute the shared dependency if their executions overlap.
  The builder's `has()` check only prevents redundant work if the first
  builder finishes before the second builder starts the shared derivation.
  Mitigation: the convergence point promotion rule in entry point selection
  promotes high fan-in nodes to standalone entry points, ensuring they are
  scheduled and built exactly once before downstream dependents start,
  minimizing redundant computation.
- **EMA lag**: Exponential moving average adapts slowly to
  sudden changes in build characteristics. Mitigation:
  configurable decay factor; operators can flush profiles.
- **Chain pathology**: A deep linear dependency chain has
  only one useful entry point (the top). This serializes
  the build on one worker. Mitigation: this is inherent to
  the dependency structure — there IS no parallelism to
  extract from a linear chain. The scheduler correctly
  identifies this.
- **Deduplication-amplified failure cascade**: A shared EP's
  deterministic failure cascades to all requests that depend
  on it. Independent scheduling would isolate this failure.
  Mitigation: transient failure detection + re-dispatch (P10).

### Implementation Notes

Derived from formal verification (§Appendix) and the
optimality assessment above:

1. **Critical path priority**: When multiple entry points
   become `ready` simultaneously, the dispatcher should
   sort them by estimated critical-path contribution
   (longest weighted path to the DAG root) before assigning.
   Greedy FIFO readiness ordering risks assigning trivial
   entry points to capable workers, blocking imminent
   critical tasks. The critical path computation is
   $O(\|S\| + \|E_S\|)$ on the coarsened DAG (a single
   reverse topological pass with max-aggregation over EP
   edges).

2. **Redundant computation monitoring**: When overlapping
   coverage causes workers to redundantly compute the same derivations,
   resources are wasted. The implementation should track the frequency
   of redundant builds (e.g., when a worker publishes outputs that
   were already populated by another worker during its execution)
   as a per-entry-point metric, and feed it back into the
   convergence-point promotion threshold — high redundant computation
   on a specific transitive dependency is a signal to promote it to
   an explicit entry point.

3. **Federation-aware transfer cost**: The $\tau(s', s)$
   transfer time in the formal model should be populated
   from actual inter-cluster RTT measurements (e.g., via
   periodic pings or artifact store latency probes), not
   from a constant default. For intra-cluster assignments,
   $\tau \approx 0$. For cross-region assignments, $\tau$
   may dominate makespan — the scoring function's
   `affinity` term partially captures this via LRH but
   does not model WAN bandwidth constraints.

4. **Coverage properties as debug assertions**: The four
   `EosModel` properties (total coverage, self-coverage,
   transitive containment, downward closure) from the Lean
   proof should be implemented as `debug_assert!` checks on
   the output of the entry point selection algorithm. These
   are $O(\|S\| \cdot \|V'\|)$ and should be gated behind
   a debug or test build flag.

5. **ε-monitoring**: After each completed build, compute
   the observed prediction error $\varepsilon = |d - \hat{d}|
   / \hat{d}$ and report it as a metric. The consistency
   bound $(1+\varepsilon)/(1-\varepsilon)$ is a live,
   monitorable quantity — operators can directly observe how
   close the system is to the proven worst-case ratio.

6. **Artifact upload before completion**: Workers **must**
   ensure all output artifacts are uploaded to the store and
   globally visible **before** reporting `EPComplete` to the
   scheduler. If the scheduler dispatches a dependent EP
   before the parent's artifacts are available, the dependent
   worker will encounter missing inputs. The completion
   protocol is: build → upload all outputs → confirm store
   acknowledgment → report `EPComplete`. This ordering is
   critical for the correctness of the dependency cascade in
   the event handler (§2b, step 2.5: "Downstream pending EPs
   whose dependencies are satisfied transition to `ready`").

7. **Opportunistic artifact salvage on failure**: When a
   build fails, the worker should upload any intermediate
   artifacts that were successfully produced before the
   failure point. Many Nix builds consist of deep transitive
   dependency chains where early phases (fetching sources,
   building dependencies) succeed before the final
   compilation step fails. By uploading the successful
   intermediate outputs, subsequent re-dispatch attempts —
   or entirely different requests that share those
   dependencies — can skip the already-completed work. The
   failure protocol is: detect failure → upload all
   successfully produced store paths → report `EPFail` with
   failure kind. This is best-effort: if the failure is a
   worker crash or network partition, salvage may not be
   possible. The scheduler must not block on salvage — if
   `EPFail` arrives without salvage, it proceeds normally.

8. **Prediction inflation on resource exhaustion**: When a
   worker reports a sandbox failure caused by resource
   exhaustion (e.g., OOM exit code 137, disk quota exceeded),
   the scheduler **must** immediately inflate the historical
   profile for the failing EP before returning it to the
   `ready` pool. Without this inflation, the scheduler will
   re-dispatch the EP with the same faulty resource estimate,
   inducing a dispatch-fail-redispatch loop. The inflation
   should set the resource estimate to at least the worker's
   capacity limit for the exhausted resource dimension, and
   the failure should be classified as `transient` (infra
   failure) rather than `deterministic` (build error) to
   allow re-dispatch to a higher-capacity worker. The formal
   model's EMA decay (Theorem 3) handles gradual prediction
   drift; this note addresses the acute case where a single
   catastrophic misprediction needs immediate correction.

9. **Convergence point cost floor**: The fan-in promotion
   rule ($\operatorname{fan_in}(v) > \theta_{\text{fanin}}$) should
   be gated with an absolute cost floor to prevent trivial
   high-convergence nodes (e.g., a 20ms header copy with
   many dependents) from being promoted to standalone entry
   points. Promoting such nodes inserts a synchronization
   barrier whose coordination overhead (RPC dispatch,
   worker pickup, result reporting) exceeds the node's actual
   computation time. The cost floor should be calibrated to
   the cluster's average round-trip scheduling latency
   $\Omega_{\text{latency}}$:
   $\operatorname{fan_in}(v) > \theta_{\text{fanin}} \;\land\;
   \operatorname{predicted_cost}(v) \ge \Omega_{\text{latency}}$.
   This aligns with the general guidance in §3a that
   coarsening thresholds must exceed scheduling latency.

10. **Resource-fit normalization**: The multi-criteria
    `resource_fit` dot product (§3) should be normalized to
    a bounded interval $[0, 1]$ before applying the decayed
    weight $\beta_e$. Without normalization, heterogeneous
    clusters (small edge nodes vs. large builder nodes)
    produce magnitude skew: the same task yields vastly
    different raw dot products on different worker types.
    This distorts the scoring function's ability to route
    heavy tasks to capable workers. Normalize by dividing
    by the norm of the resource vectors, or equivalently,
    use cosine similarity instead of the raw dot product.

11. **Non-blocking ingress draining**: The scheduler's
    event channel **must not** block on HEFT planning or
    graph coarsening. Because entry-point selection involves
    graph searches over potentially large derivation trees,
    inline processing would cause backpressure that stalls
    request ingress. The implementation should use a
    non-blocking `try_recv` drain loop that immediately
    buffers all pending events into an in-memory queue,
    then runs a single coalesced HEFT pass over the
    accumulated events. This matches the event coalescing
    model described in §2b and prevents the ingress channel
    from filling under burst request load. Reference
    pattern: `eka-ci`'s `poll_for_builds` loop uses
    `try_recv` + `VecDeque` to completely insulate its
    ingress from downstream processing latency.

12. **Fixed-output derivation (FOD) locality**: FODs
    (source fetches, git checkouts, tarball downloads) have
    a fundamentally different resource profile from compute
    tasks: they are network-bound, I/O-heavy, and produce
    unpredictable latency. Some CI systems (e.g., `eka-ci`)
    isolate FODs onto dedicated worker pools to prevent
    network I/O storms from starving compute slots. However,
    in the coarsened EP model this advice does **not**
    translate directly. Because FODs are typically source
    inputs absorbed into a parent EP's transitive scope, the
    build worker fetches them locally with perfect data
    locality. Routing FODs to a dedicated node would add a
    pointless store round-trip: the dedicated node fetches
    the source, uploads it to the store, and then the actual
    build worker downloads it again as an input — strictly
    more work than letting the builder fetch directly. The
    coarsening heuristic already handles this correctly: a
    trivial FOD is absorbed into its parent EP scope rather
    than promoted to a standalone entry point (see note #9,
    cost floor). FOD isolation may be worth considering only
    in extreme deployment topologies where dedicated caching
    proxies or mirrors serve as intermediaries for expensive
    upstream fetches (e.g., large git monorepos), but this
    is an infrastructure concern rather than a scheduling
    decision. What **is** relevant for FOD-aware placement
    is **worker internet egress bandwidth**: an EP whose
    scope contains many FODs will have its predicted duration
    dominated by fetch time, which varies dramatically with
    worker bandwidth (100 Mbps vs. 10 Gbps). The resource
    vector (§3) should include `egress_bandwidth` as a
    dimension so that the scoring function can steer
    FOD-heavy EPs toward high-bandwidth workers. Historical
    profiles can track FOD fetch durations per derivation
    hash to refine bandwidth-adjusted predictions.

13. **Worker reconnection protocol**: When a worker is
    marked unhealthy (heartbeat timeout), the scheduler
    reverts its running EPs to `ready` and re-dispatches
    them. However, the worker may still be alive and
    actively building — the health check failure may be a
    transient network blip. If the worker reconnects while
    a duplicate build is in flight, the cluster wastes
    resources. The implementation should use a **grace
    period** before rescheduling: when a worker misses a
    heartbeat, enter a `suspect` state for a configurable
    cooldown (e.g., 30–60 seconds) before transitioning to
    `unhealthy`. During the cooldown, no rescheduling
    occurs. If the worker reconnects during the cooldown,
    its EPs remain assigned and the incident is logged as
    a transient network event. If the cooldown expires, the
    worker transitions to `unhealthy` and rescheduling
    proceeds normally. Additionally, if a worker reconnects
    _after_ rescheduling has already occurred and reports
    that it is still actively building the same EP, the
    scheduler should allow the original build to continue
    (with its own short monitoring window in case the
    network issue recurs) and cancel the duplicate. CAS
    idempotency guarantees correctness regardless of which
    build "wins," but minimizing redundant work is a
    resource efficiency concern.

14. **Causal prediction error attribution**: The EMA-based
    prediction system tracks _that_ predictions are wrong
    (magnitude of |η|) but not _why_. For faster
    convergence, the implementation should track
    per-worker-class performance deltas: if derivation X
    takes 120s on a 64GB worker but 45s on a 256GB worker,
    the prediction error is attributable to memory pressure
    (swapping or cache thrashing), not to an inherent
    change in the derivation. The profile store can
    maintain resource-conditioned predictions — e.g.,
    separate EMA tracks for "duration on workers with
    ≥128GB RAM" vs. "duration on workers with <128GB RAM."
    When a prediction error exceeds a significance
    threshold (e.g., >50% deviation), the scheduler should
    compare the worker's resource profile to the historical
    median worker profile for this derivation and attribute
    the error to the most divergent resource dimension.
    This attribution can then drive targeted prediction
    inflation (note #8) and inform the resource-fit
    scoring (§3) without waiting for the EMA to accumulate
    enough samples to self-correct.

15. **Heuristic refinement stability**: The formal model
    is explicitly designed so that heuristic tuning does
    not invalidate the proofs. The `SchedulerStrategy`
    trait (§3b) abstracts over the specific solver; the
    `Coarsening` structure (Lean 4) abstracts over the
    specific entry-point selection criteria; and the
    competitive ratio α absorbs heuristic quality — worse
    heuristics produce a larger α, but the bound structure
    remains valid. Implementors should treat the
    coarsening thresholds, scoring weights, EMA decay
    constants, and cooldown timers as continuously
    tunable parameters. Steady refinement of these
    heuristics through production telemetry is expected
    and encouraged — the architecture's formal seams
    guarantee that tuning cannot violate safety or
    liveness properties.

---

## Prior Art

| Technique                         | Source                                  | Application                            |
| :-------------------------------- | :-------------------------------------- | :------------------------------------- |
| Troublesome task pre-allocation   | Graphene (OSDI 2016)                    | Entry point identification             |
| Multi-resource dot-product        | Tetris (SIGCOMM 2014)                   | Placement scoring                      |
| Learning-augmented framework      | Mitzenmacher & Vassilvitskii (2020)     | Consistency/robustness guarantees      |
| Greedy stochastic scheduling      | Gupta et al. (Math OR 2020)             | Greedy with predictions is competitive |
| Permutation predictions           | Lindermayr & Megow (SPAA 2022)          | Robustness bounds                      |
| Local Rendezvous Hashing          | arXiv:2512.23434 (2025)                 | Cache-local HRW replacement            |
| Multi-resource DAG bounds         | Kedad-Sidhoum et al. (arXiv:2106.07059) | Approximation ratio bounds             |
| Request coalescing (singleflight) | Fitzpatrick (groupcache), BuildBuddy    | Cross-request dedup (prior art)        |
| Build systems formalization       | Mokhov et al. (ICFP 2018)               | Cloud build memoization model          |

### Open Research Area

No academic work addresses scheduling with content-addressed
storage semantics, where:

- Build outputs are globally shared via a content-addressed
  store (output locality is trivial)
- Build inputs (atom source trees) have per-worker locality
  (input locality is the scheduling concern)
- Task identity is content-addressed and stable across
  versions (atom-id)
- The builder internally resolves transitive dependencies,
  reducing the scheduler's responsibility to entry point
  selection

This intersection of CAS, stable identity, and DAG entry
point selection is a potential novel contribution.

---

## Open Questions

### Resolved

1. **DAG visibility** — RESOLVED. The snix evaluator
   accumulates the full derivation DAG as a side-effect in
   `KnownPaths` (owned by `SnixStoreIO`). After evaluation:
   - `known_paths.get_derivations()` iterates all derivations
   - `Derivation.input_derivations` provides dependency edges
     (`BTreeMap<StorePath, BTreeSet<String>>`)
   - `PathInfoService.get(digest)` provides per-derivation
     cache checks
   - `KnownPaths` enforces a topological ordering invariant
     (all input derivations registered before dependents)

   The Eos scheduler can therefore introspect the full DAG
   after evaluation completes: walk `input_derivations` edges,
   query `PathInfoService` for each node, and construct the
   uncached sub-DAG and entry point DAG proactively.

2. **Dynamic re-evaluation** — RESOLVED (unnecessary).
   Because evaluation is deterministic (invariant
   `[eos-eval-pure-eval]`) and derivation hashes are
   content-addressed, the uncached sub-DAG is fully
   determined at construction time. When entry point D
   completes, the downstream cache state is exactly what
   was predicted — D's outputs are now cached, and this was
   already factored into the entry point DAG's dependency
   edges. Re-evaluation would produce the same result.
   The entry point DAG is constructed once and dispatched
   topologically without re-computation.

3. **Entry point granularity** — RESOLVED (algorithmic, not
   a static parameter). Granularity is determined per
   derivation by analyzing the subgraph below each candidate
   entry point:
   - **Aggregate predicted cost**: sum historical build
     durations for all uncached transitive dependencies
     below the candidate (using both atom-level and
     derivation-level profiles — see Q5 resolution)
   - **Subgraph shape**: depth (critical path length) and
     width (max parallelism within the subgraph)
   - A candidate becomes an explicit entry point if:
     - Its aggregated cost exceeds a threshold (it's
       "worth" scheduling independently)
     - It has high fan-in (convergence point — many
       downstream nodes depend on it)
     - It is a troublesome node (heavy resource profile)
     - Its subgraph is deep enough that internal parallelism
       would be wasted on a single builder

   Small, tightly-coupled subgraphs are absorbed into their
   parent entry point. Large, loosely-coupled subgraphs are
   split at convergence points.

4. **Historical profile schema** — RESOLVED (unified
   derivation model). Everything is a derivation at the
   base layer. An atom is a derivation with extra metadata
   (atom-id, developer scheduling hints), not a separate
   classification. One profile store keyed by derivation
   name (`P[drv_name]`), with optional atom enrichment.

   This eliminates the two-table split (atom-level vs
   derivation-level) that added classification complexity
   without clear benefit. A derivation appearing as a
   transitive dep in one DAG and as an independent atom
   in another context shares the same profile entry —
   the scheduler recognizes it automatically via name
   match and uses the atom metadata if available.

5. **Cross-request deduplication** — RESOLVED (structural
   deduplication as a Track B optimization). This is a well-understood,
   production-proven pattern called **request coalescing** (inspired by Go's "singleflight",
   Bazel backends: "action merging", CDN: "request collapsing").

   Mechanism: maintain a concurrent map of in-flight entry
   point builds keyed by derivation hash:

   ```
   in_flight: Map<DrvHash, SharedFuture<BuildResult>>
   ```

   When dispatching an entry point:
   1. Check artifact store → HIT: skip, already built
   2. Check `in_flight` map → IN-FLIGHT: subscribe to the
      existing future, await the result
   3. Neither: insert a new future into `in_flight`, dispatch
      the build to a worker, broadcast the result to all
      subscribers on completion, remove from map

   **Failure Domain Isolation (Crucial)**: To preserve sequential request
   equivalence (bisimulation), the coalesced future must differentiate between
   deterministic build failures and transient infrastructure/worker failures. If a
   build owner fails due to an infrastructure issue (e.g. worker network dropout,
   hardware failure, or disk crash), the scheduler must not propagate this failure
   to subscribers. Instead, the subscribers must safely transition back to
   `ready` and be re-dispatched independently to a healthy worker.

   **Prior art**: BuildBuddy's action merging (Bazel RBE),
   Nix's store path deduplication, Go's
   `golang.org/x/sync/singleflight`, Varnish's request
   coalescing, Dask's automatic DAG deduplication. Also
   formalized in Mokhov, Mitchell & Peyton Jones,
   "Build Systems à la Carte" (ICFP 2018) under "cloud
   builds" — shared computation via content-addressed
   memoization.

   **Key design decisions**:
   - **Scope**: Single-node in-memory map (sufficient with
     centralized scheduler — our architecture)
   - **Failure**: If in-flight build fails deterministically, all waiters get
     the error. If it fails due to infrastructure, waiters are re-dispatched.
   - **Cancellation**: If original requester cancels but
     other waiters exist, the build continues (reference
     counting on the shared future)
   - **Eviction**: Entries removed from `in_flight`
     immediately on completion/resolution — this is not a cache, only
     concurrent dedup.

---

## Relationship to Other ADRs

- **ADR-0002**: Defines the worker boundary. This ADR assumes
  workers are independent processes communicating via Cap'n
  Proto (per ADR-0002). The scheduling algorithm runs in
  the daemon.
- **ADR-0003**: Defines deployment modes. In monolithic mode,
  the DAG entry point selection still applies (scheduling
  across threads), but cross-machine transfer costs are zero.
  The algorithm degrades gracefully: `affinity` and
  `resource_fit` still provide value; only the transfer cost
  component becomes irrelevant.

---

## Appendix: Formal Verification Results

The scheduling model has been formally verified through a
two-track approach. The formal model is defined in
`docs/models/eos-scheduling.md`.

### Track A: Protocol Correctness (TLA+)

The dispatch protocol was model-checked across four DAG
topologies (linear chain, diamond fork-join, convergence,
independent) using TLC with weak fairness.

**Verified properties:**

| Property                     | Type     | Status |
| :--------------------------- | :------- | :----- |
| Ordering soundness (P1)      | Safety   | ✅     |
| Capacity safety (P4)         | Safety   | ✅     |
| Artifact completeness (P3)   | Safety   | ✅     |
| Progress (P5)                | Liveness | ✅     |
| Completion prop. (P6)        | Liveness | ✅     |
| HoL immunity (P5')           | Liveness | ✅     |
| Per-request completion (P6') | Liveness | ✅     |
| Frozen stability (P8)        | Safety   | ✅     |
| Work conservation (P9)       | Liveness | ✅     |
| Transient recovery (P10)     | Liveness | ✅     |
| Failure isolation (P11)      | Safety   | ✅     |

Key finding: `CascadeFail` is **required** for liveness.
Without active failure propagation, dependent tasks hang
in `pending`/`ready` indefinitely after a dependency fails.
This must be implemented as a mandatory transition, not an
optional recovery mechanism.

See `docs/models/tla/` for specifications and topology
model instantiations.

### Track B: Optimization Quality (Lean 4)

Nine theorems machine-checked with Mathlib. Zero `sorry`
placeholders, zero custom `axiom` declarations.

| Theorem | Statement                                                                                     | Status |
| :------ | :-------------------------------------------------------------------------------------------- | :----- |
| Thm 1   | Valid entry point selection exists (identity witness)                                         | ✅     |
| Thm 2   | $M(\sigma_H) \leq \alpha \cdot \frac{1+\varepsilon}{1-\varepsilon} \cdot M(\sigma^*)$         | ✅     |
| Thm 2'  | Adaptive bound: $\alpha(\bar\varepsilon) \to 1$ as $\bar\varepsilon \to 0$                    | ✅     |
| Thm 3   | Assignment stability under perturbation; EMA convergence                                      | ✅     |
| Thm 4   | Structural: $\lvert\bigcup V'_i\rvert \leq \sum \lvert V'_i\rvert$, equality iff disjoint     | ✅     |
| Thm 4'  | Weighted: $\sum_{v \in \bigcup V'_i} d(v) \leq \sum_i \sum_{v \in V'_i} d(v)$, disjoint Eq    | ✅     |
| Thm 5   | Unified Coarsening Dominance: $M(\sigma_{\text{unified}}) \leq M(\sigma_{\text{per}})$        | ✅     |
| Thm 6   | CAS-scheduling makespan competitive ratio bounded by $\alpha(1 + \rho \cdot \lvert R \rvert)$ | ✅     |
| Thm 7   | Re-coarsening convergence: monotonicity and finite-step cache convergence                     | ✅     |

All assumptions enter as explicit hypotheses on theorem
signatures (non-negative durations, $\varepsilon < 1$,
well-founded DAG order) or as type-level constraints
(`Fintype`, `DecidableEq`). These correspond to inherent
properties of the Rust type system and physical
non-negativity.

Key finding: Theorem 2's inductive step requires
$\tau(s', s) \geq 0$ (transfer times are non-negative).
The implementation must not encode cache time savings as
negative transfer times — these are separate concerns.

See `docs/models/lean/` for proof sources.

### Remaining Open Items

1. **Federation liveness (P7)**: Requires real-time bounds
   not expressible in TLA+ temporal logic. Mitigate with
   configurable timeouts and worker reassignment.
2. **Graph coarsening optimality**: The entry point
   selection problem is NP-hard. Theorem 2 bounds
   assignment quality on any fixed DAG, cleanly separating
   it from DAG decomposition quality
   ($\alpha(\bar\varepsilon)$). Theorem 2' conditionally
   closes this gap: as mean prediction error
   $\bar\varepsilon \to 0$, $\alpha \to 1$ and the
   coarsening penalty vanishes.
3. **μ-makespan transient bound**: During EMA convergence,
   the quantitative makespan penalty is not mechanized.
   Low risk: the transient is geometrically short and
   capacity safety (Track A) holds throughout.
4. **Starvation prevention (P12)**: While the work-conserving liveness property (P9) guarantees that ready EPs are eventually dispatched, it does not prevent low-priority tasks from being starved indefinitely under continuous high-priority arrival. Formalizing starvation-freedom requires modeling arrival processes and priority queuing disciplines (e.g. aging or FIFO bounds), which is deferred to future work.
5. **DAG Boundedness and Memory Limits (P13)**: The TLA+ and Lean models assume a finite vertex set $V$. At runtime, the unified global DAG must be bounded to prevent memory exhaustion under continuous request streams. Proving memory safety and progress under sliding window request pruning is a future modeling objective.

## Future Work: Federated Scale via Min-Cost Max-Flow (MCMF)

While the multi-criteria placement heuristic (§3) is highly suitable for medium-sized, homogeneous clusters, large-scale federated deployments (scaling past 1,000+ nodes across disparate administrative or geographical boundaries) introduce complex constraints. At this scale, we propose exploring a flow-based scheduling model based on the Quincy (SOSP 2009) and Firmament (OSDI 2016) architectures as a successor to the heuristic scoring model.

### Graph Formulation

Instead of evaluating independent scores for each task-worker pair, scheduling is modeled as a Min-Cost Max-Flow (MCMF) optimization problem on a directed graph $F = (V_F, E_F)$. The network is constructed dynamically at each scheduling iteration:

1. **Sources and Sinks**: A global source node $s$ injects flow, and a global sink node $t$ consumes it.
2. **Task Nodes**: Each ready entry point $e \in S$ is represented by a node. Edges from the source $s$ to each task node $e$ have capacity $1$ and cost $0$.
3. **Worker Nodes**: Each worker $w \in W$ is represented by a node.
4. **Placement Edges**: Edges are added from task nodes to candidate worker nodes. The capacity is $1$. The cost of this edge encodes all scheduling preferences:
   - **Locality Cost**: Lower cost is assigned if the candidate worker $w$ has cached input files (LRH ring affinity).
   - **Resource Fit Cost**: Tetris-style resource alignment is encoded as a cost term (higher alignment = lower cost).
   - **Federation latency cost**: Higher costs are assigned to edges crossing geographic or regional network boundaries.
5. **Cluster and Federation Nodes**: Intermediate aggregator nodes represent local clusters, racks, or geographical regions. They can enforce hierarchy-aware placement and bound cross-region bandwidth.
6. **Trust and Policy Boundaries**: Trust constraints can be modeled directly in the topology. For example, if a task $e$ requires a high-trust builder (e.g., for signed artifact compilation), placement edges are only constructed to workers belonging to the trusted sub-network. Edges to untrusted public peer nodes are omitted or assigned a penalty cost representing verification overhead.
7. **Unschedulable/Delay Nodes**: To allow tasks to wait for local resources rather than being immediately dispatched to a suboptimal remote worker, tasks connect to the sink $t$ via "unscheduled" nodes. The cost on these edges represents the penalty of delaying the task.

```mermaid
graph TD
    s["Source (s)"] --> T1["Task (EP-1)"]
    s --> T2["Task (EP-2)"]

    T1 -- "Cost: Cache Affinity" --> W1["Worker 1 (Local)"]
    T1 -- "Cost: Remote Latency" --> W2["Worker 2 (Remote)"]
    T2 -- "Cost: Resource Fit" --> W2

    T1 -- "Delay Penalty" --> U1["Unscheduled Node"]
    T2 -- "Delay Penalty" --> U2["Unscheduled Node"]

    W1 --> Sink["Sink (t)"]
    W2 --> Sink
    U1 --> Sink
    U2 --> Sink
```

### Advantages of MCMF at Federated Scale

- **Global Optimization**: Rather than scheduling tasks greedily one-by-one (which can lead to bad placements for later tasks due to capacity exhaustion), MCMF solves placement globally across all ready tasks in a single optimization pass.
- **Expressive Policy Encoding**: Locality, resource capacity, federation topology, and trust boundaries are unified into a single mathematical abstraction (costs and capacities) rather than balancing separate, competing heuristic terms.
- **Incremental Solver Performance**: Firmament demonstrated that by using incremental MCMF solvers (e.g., cost-scaling algorithms like CS2), the solver can reuse the previous iteration's solution, achieving sub-second scheduling times for tens of thousands of tasks on 10,000+ machines. This eliminates the scalability bottlenecks typically associated with flow network solvers.
