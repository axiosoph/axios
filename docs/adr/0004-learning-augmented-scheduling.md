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

When evaluation produces a derivation DAG, the scheduler
constructs an **entry point DAG** — a coarsened dependency
graph where each node is an entry point (a derivation that
will be scheduled as a top-level build invocation) and edges
represent inter-entry-point ordering constraints.

#### 2a. Construction

```
Derivation DAG
    → (filter cached) →
Uncached Sub-DAG
    → (select entry points) →
Entry Point DAG
    → (topological dispatch) →
Worker Assignments
```

1. **Receive** the uncached sub-DAG from the evaluator
   (derivations whose outputs are not in the artifact store)

2. **Select entry points** — identify derivations that will
   serve as top-level build invocations. Entry points are
   selected **top-down** to cover the uncached sub-DAG:

   Nix derivation DAGs are highly detailed — many leaves
   are trivial (patches, source fetches, fixed-output
   derivations). Scheduling every leaf as an entry point
   would create a flood of tiny jobs whose completion
   blocks higher-level entry points, causing excessive
   scheduling overhead with no locality benefit. Instead,
   entry points are selected to **cover** the leaves
   beneath them:
   - **Start from the top-level derivation** and walk
     downward through the uncached sub-DAG
   - **Split at strategic points**: a node becomes a
     separate entry point (rather than being absorbed into
     its parent's transitive scope) if:
     - It is a **troublesome node** — predicted duration
       or resource usage above threshold (from `P[drv_name]`,
       developer metadata, or `requiredSystemFeatures`)
     - It is a **convergence point** — high fan-in node
       where many dependency paths converge. Making it an
       explicit entry point prevents multiple downstream
       builders from redundantly building it.
     - Its **subgraph cost** (aggregate predicted duration
       of all uncached transitives below it) exceeds a
       threshold, meaning it represents enough work to
       justify independent scheduling
   - **Everything else** is absorbed into the nearest
     covering entry point's transitive scope — the builder
     handles trivial leaves, patches, and fetches
     internally as part of the entry point's build.

     **Merging constraint (Relaxed)**: Since the formal model relaxes
     entry-point coverage to a relation, a non-entry-point derivation is
     permitted to exist in multiple entry points' transitive scopes
     simultaneously. At runtime, the builder's store-path locks deduplicate
     the build of this shared node. To optimize scheduling efficiency and
     avoid redundant worker load reservations, the selection heuristic
     _may_ choose to promote high fan-in convergence points to standalone
     entry points. However, there is no mathematical constraint forcing
     this promotion, preventing the macroscopic scheduling DAG from
     shattering under dense dependencies.

3. **Derive inter-entry-point dependencies** — if entry
   point A's transitive subgraph depends on the output of
   entry point B's subgraph, then A depends on B in the
   entry point DAG. This includes the user's top-level
   derivation: it depends on ALL sub-entry-points whose
   outputs it transitively needs.

   ```
   Entry Point DAG (derived from uncached sub-DAG):

   EP-top (user's top-level derivation)
   ├── depends on: EP-a (troublesome: heavy link step)
   │   └── depends on: EP-d (convergence point)
   ├── depends on: EP-b
   │   └── depends on: EP-d (shared)
   └── (EP-f absorbed into EP-top — trivial fetch)

   Dispatch order: {EP-d} → {EP-a, EP-b} → {EP-top}
   ```

4. **Assign entry points to workers** using the scoring
   function (§3). Entry points that share dependencies are
   preferentially colocated on the same worker to avoid
   redundant transitive builds.

#### 2b. Dispatch Protocol

Entry points are dispatched in **topological order** of the
entry point DAG:

1. **Root entry points** (no dependency on other entry
   points in the EP DAG) are dispatched immediately.

2. **When an entry point completes**, the scheduler:
   - Records its outputs as available in the artifact store
   - Checks which downstream entry points now have ALL their
     dependency entry points completed
   - Dispatches newly-unblocked entry points to workers

3. **The user's top-level derivation** is dispatched last —
   only after all its sub-entry-points have completed and
   their outputs are available. This ensures the top-level
   builder does not start building things already being
   worked on by other builders.

4. **Shared dependency protection**: If entry points A and B
   both depend on entry point D, D is built exactly once.
   Neither A nor B is dispatched until D completes. This
   prevents the scenario where A's and B's builders both
   independently attempt to build D's subgraph.

The scheduler tracks only the entry point DAG — not
individual derivations within each entry point's transitive
scope. The builder handles all transitive work below each
entry point internally, on the assigned worker, with full
locality.

**Two-level deduplication**: Dedup operates at two independent levels:

- **Entry point level (singleflight - Track B optimization)**: The scheduler's
  in-flight map deduplicates across concurrent requests that select the same
  entry point (same derivation hash). Since this is a pure software-level
  optimization rather than a protocol correctness invariant, it is omitted
  from the formal Track A correctness model. If implemented in software, the
  coalescing logic must isolate failure domains: it must distinguish between
  deterministic build failures (which can be safely broadcast to subscribers)
  and transient infrastructure failures (e.g., worker crash, disk failure).
  If the build owner fails due to infrastructure, subscribers must not inherit
  the failure; they must be re-dispatched to a healthy worker.
- **Derivation level (store locks - Track A correctness)**: Within a builder,
  snix's `PathInfoService` acquires exclusive locks on output store paths.
  If two builders (from different entry points or different requests) attempt
  to build the same transitive derivation, the second blocks on the lock
  and uses the first's result. This catches overlaps that the entry-point-level
  singleflight cannot — e.g., different entry point selections with shared
  transitives.

**Prior art**: Graphene/DagPS (Grandl et al., OSDI 2016) —
troublesome task identification and pre-allocation in
multi-resource space-time.

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
  $$\text{resource\_fit}(w, e) = \sum_{i \in \{\text{cpu}, \text{mem}, \text{disk}\}} \left( \frac{r_{e,i}}{c_{w,i}} \cdot \frac{a_{w,i}}{c_{w,i}} \right)$$
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

**Prior art**: Tetris (Grandl et al., SIGCOMM 2014) —
multi-resource dot-product alignment heuristic.

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

---

## Guarantees

The learning-augmented framework provides three formal
properties:

### Consistency

When historical predictions are accurate (same atom, similar
version), the scheduler approaches the quality of an optimal
offline algorithm that knows all durations and resource
profiles in advance. Per Kedad-Sidhoum et al.
(arXiv:2106.07059), DAG scheduling with heterogeneous tasks
achieves bounded approximation ratios. When prediction error
is low, the greedy entry point assignment with scoring
approximates these bounds — the ratio depends on DAG
structure (depth/width) and prediction accuracy.

### Robustness

When predictions are wrong (new atom with no metadata,
radical version change), the algorithm degrades to baseline
tag-matching with LRH affinity. The scoring function still
works — the `resource_fit` term becomes noise, but `affinity`
and `availability` terms remain valid. Per Lindermayr & Megow
(arXiv:2202.10199), the algorithm achieves a bounded
competitive ratio independent of prediction quality.

### Smoothness

As prediction error increases gradually (incremental version
changes, slowly shifting build profiles), scheduling quality
degrades proportionally, not catastrophically. The EMA update
rule adapts the historical profile over time, tracking drift.

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
aligns well with the formal model (§6).

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
- The formal model (§6) identifies properties that MCMF
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

### Risks

- **Store-level transitive dedup**: Two entry points on
  different workers whose transitive scopes share
  derivations will both attempt to build the shared
  derivations. The store-level lock mechanism
  (snix `PathInfoService`) prevents redundant work — the
  second builder blocks until the first completes and uses
  the cached result. This is correct but adds latency
  (blocking wait). Mitigation: the convergence point
  promotion rule in entry point selection minimizes shared
  non-EP derivations, reducing how often store-level dedup
  is needed.
- **EMA lag**: Exponential moving average adapts slowly to
  sudden changes in build characteristics. Mitigation:
  configurable decay factor; operators can flush profiles.
- **Chain pathology**: A deep linear dependency chain has
  only one useful entry point (the top). This serializes
  the build on one worker. Mitigation: this is inherent to
  the dependency structure — there IS no parallelism to
  extract from a linear chain. The scheduler correctly
  identifies this.

---

## Prior Art

| Technique                       | Source                                  | Application                            |
| :------------------------------ | :-------------------------------------- | :------------------------------------- |
| Troublesome task pre-allocation | Graphene (OSDI 2016)                    | Entry point identification             |
| Multi-resource dot-product      | Tetris (SIGCOMM 2014)                   | Placement scoring                      |
| Learning-augmented framework    | Mitzenmacher & Vassilvitskii (2020)     | Consistency/robustness guarantees      |
| Greedy stochastic scheduling    | Gupta et al. (Math OR 2020)             | Greedy with predictions is competitive |
| Permutation predictions         | Lindermayr & Megow (SPAA 2022)          | Robustness bounds                      |
| Local Rendezvous Hashing        | arXiv:2512.23434 (2025)                 | Cache-local HRW replacement            |
| Multi-resource DAG bounds       | Kedad-Sidhoum et al. (arXiv:2106.07059) | Approximation ratio bounds             |
| Request coalescing/singleflight | Fitzpatrick (groupcache), BuildBuddy    | Cross-request deduplication            |
| Build systems formalization     | Mokhov et al. (ICFP 2018)               | Cloud build memoization model          |

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

5. **Cross-request deduplication** — RESOLVED (singleflight
   pattern as a Track B optimization). This is a well-understood,
   production-proven pattern called **request coalescing** (Go: "singleflight",
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
   Nix's store-level output path locks, Go's
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

## Appendix: Toward a Formal Model

Scheduling with entry point DAGs, content-addressed
deduplication, federated workers, and historical predictions
is inherently complex. The heuristic algorithm described
above is a practical starting point, but key properties
should be formally modeled and proven sound.

### Proposed Formalization

**System model**:

- Let $G = (V, E)$ be the uncached derivation sub-DAG
- Let $S \subseteq V$ be the selected entry points
- Let $T_S = (S, E_S)$ be the induced entry point DAG
- Let $W = \{w_1, ..., w_m\}$ be the set of workers
- Let $\sigma: S \to W$ be the assignment function
- Let $\hat{d}(v)$ be the predicted duration for derivation
  $v$ (from `P[drv_name]`)
- Let $d(v)$ be the actual duration (revealed on completion)

**Properties to prove**:

1. **Coverage**: Every uncached derivation in $G$ is
   covered by at least one entry point in $S$. Formally:
   $\forall v \in V, \exists s \in S$ such that $v$ is in
   the transitive closure of $s$ in $G$, or $v \in S$ itself.
   This ensures no derivation is missed (total coverage).

2. **Ordering soundness**: The dispatch protocol respects
   data dependencies. No entry point is dispatched to a
   worker before all its dependency entry points have
   completed and their outputs are available.
   Formally: if $(s_i, s_j) \in E_S$ (entry point $s_j$
   depends on $s_i$), then $s_j$ is dispatched only after
   $s_i$'s build result is in the artifact store.

3. **Store-level lock correctness**: Under concurrent execution
   of overlapping entry points, builder-level store locks ensure
   at-most-one execution per output path.

4. **Consistency bound**: _Given a fixed entry point DAG $T$_,
   when predictions are accurate ($\hat{d}(v) \approx d(v)$
   for all $v$), the makespan of the entry point DAG schedule
   achieved by greedy assignment $\sigma_H$ is within a bounded
   factor of the optimal offline schedule of $T$. We concede that
   the entry point selection (graph coarsening) phase currently
   lacks a formal competitive bound.

5. **Robustness bound**: When predictions are arbitrarily
   wrong ($\eta \to \infty$), the schedule makespan is no worse
   than a small constant factor of a baseline algorithm that ignores
   predictions (tag matching with LRH affinity only), thanks to
   the dynamic decay of the resource fit term's weight ($\beta_e \to 0$).

6. **Liveness under federation**: In a federated deployment
   where workers span multiple clusters with varying
   latency, the dispatch protocol must not deadlock. If all
   dependency entry points have completed, their outputs
   must be reachable by the assigned worker within bounded
   time (artifact store propagation latency). Failure propagation
   via `CascadeFail` ensures that failed tasks do not cause
   dependent entry points to hang indefinitely.

### Formalization Approach

The formal model should be developed as a companion document
to this ADR, potentially using:

- **TLA+ or Alloy** for verifying ordering soundness,
  liveness, and failure cascade propagation (Track A)
- **Competitive analysis** (from learning-augmented
  algorithms theory) for consistency and robustness bounds
  on a fixed DAG (Track B)
- **Graph theory** for coverage relations

This is deferred to a follow-up effort but is considered
essential for a system targeting global-scale deployment.
