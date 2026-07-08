# eos-sim — Eos scheduling simulator

A deterministic, single-threaded Rust simulator for the Eos
learning-augmented build scheduler. It evaluates the entry-point (EP)
coarsening heuristic variants **H1–H4** and **PEFT** dispatch under bounded
dispatch windows against plan DAGs with duration ground truth.

`eos-sim` is a **standalone** crate: it depends on no `eos` runtime crate and
consumes plain JSON data files. It is the compensating control that makes the
scheduling heuristics — invisible to the formal TLA+/Lean verification tracks —
empirically measurable (campaign `eos-scheduler-validation`, finding F14).

The binding algorithm is **ADR-0004** (`docs/adr/0004-learning-augmented-scheduling.md`,
§2a coarsening, §2b PEFT/OCT + delay credit, §3 Option-C duration model); the
data model and invariants are **`docs/specs/eos-scheduler.md`**; the fairness
dispatch rule mirrors **`docs/models/tla/StarvationModel.tla`**.

## Trace file format

The simulator's sole input is a _trace_: a JSON document with four top-level
keys. It is the data contract for node P9 (corpus extraction) and node P10
(heuristic evaluation).

```jsonc
{
  "nodes": [
    {
      "id": "top", // opaque plan digest, unique within the trace
      "duration": 1.0, // isolated build duration, seconds (d(v))
      "peak_mem": 1000000000, // optional: predicted peak memory, bytes
      "is_atom": false, // optional: synthetic atom marker (atom-seeded variant)
      "plan_name": "top-1.0", // optional: version-stable profile key (corpus fidelity)
      "confidence": 0.9, // optional: prediction confidence [0,1] (default 0.5)
      "arrival": 0.0, // optional: system-entry time (RequestArrival staggering; default 0)
    },
  ],
  "edges": [
    { "from": "top", "to": "a" }, // "from" depends on "to": "to" builds first
  ],
  "workers": [
    {
      "id": "w0", // opaque worker identity, unique
      "speed": 1.0, // optional: duration multiplier (<1 faster)
      "capacity": { "mem": 8000000000 }, // optional: abstract capacity vector
      "cached": ["leaf"], // optional: plan ids cached locally at t=0
    },
  ],
  "store_cached": ["some-prebuilt-plan"], // optional: globally cached; filtered pre-coarsening
}
```

**Edge orientation.** An edge `{ "from": X, "to": Y }` means _X depends on Y_,
so Y must be built before X. Equivalently, Y is a dependency of X and X is a
dependent of Y. The graph must be acyclic (validated when the graph is built).

**Validation.** A trace is rejected for: duplicate node or worker ids, edges
referencing unknown nodes, self-edges, an empty worker pool, or non-finite /
negative durations.

## CLI

```
eos-sim --trace <FILE> [OPTIONS]
```

| Flag                                    | Default        | Meaning                                                  |
| :-------------------------------------- | :------------- | :------------------------------------------------------- |
| `--seed <U64>`                          | `0`            | Deterministic tie-break seed.                            |
| `--variant <H1\|H2\|H3\|H4>`            | `H1`           | Promotion variant (ADR §2a).                             |
| `--seeding <from-scratch\|atom-seeded>` | `from-scratch` | Initial-cover seeding axis.                              |
| `--theta-critical <F>`                  | `30`           | Critical-path-cut threshold.                             |
| `--theta-redundancy <F>`                | `20`           | Cost-gated convergence threshold.                        |
| `--theta-cost <F>`                      | `60`           | Troublesome-node threshold.                              |
| `--theta-scale <F>`                     | `1`            | Confidence-gating scale (`0` disables gating).           |
| `--theta-subgraph <F>`                  | `120`          | H4 subgraph-cost threshold.                              |
| `--theta-fanin <N>`                     | `2`            | H4 fan-in threshold.                                     |
| `--theta-trivial <F>`                   | `10`           | Atom-absorption threshold (atom-seeded).                 |
| `--cache-speedup <F>`                   | `0.5`          | Cache-affinity speedup factor (Option C).                |
| `--beta <F>`                            | `0.5`          | Resource-fit penalty weight (Option C).                  |
| `--delta <F>`                           | `0`            | Bounded dispatch window Δ, seconds (P9′).                |
| `--gamma <F>`                           | `0`            | Delay-credit weight γ (P12 fairness).                    |
| `--lambda <F>`                          | `1`            | Redundant-work weight in the objective.                  |
| `--json`                                | off            | Emit the metrics JSON line instead of the human summary. |

On a successful run the simulator prints the two contract lines consumed by
node P10 / constraint C2, around the metrics block:

```
Loaded <N> plans
<metrics summary or --json line>
Simulation completed
```

`N` is the number of plan nodes loaded from the trace.

### Metrics

`makespan`, `redundant_work` (CPU-time on plan nodes built concurrently by more
than one EP), `ep_count`, `mean_utilization`, `critical_path_accuracy`
(predicted EP-DAG critical path / actual makespan), `max_dispatch_wait` (the
fairness / starvation indicator), and `objective` (`makespan + λ·redundant_work`).

### Determinism

A fixed `--seed` reproduces **byte-identical** metrics output. The seed drives
only tie-breaking among genuinely-equal priorities or placements; the rest of
the engine is fully deterministic.

## Development

```
cargo test  -p eos-sim
cargo clippy -p eos-sim -- -D warnings
```

Fixtures live in `fixtures/` (the diamond DAG and the documented H1-vs-H4
divergence DAG). The bounded-window and starvation-contention scenarios are
constructed programmatically in the integration tests, since they parameterise
over Δ, γ, and stream length.
