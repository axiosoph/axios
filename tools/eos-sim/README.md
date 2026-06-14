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

The simulator's sole input is a *trace*: a JSON document with four top-level
keys. It is the data contract for node P9 (corpus extraction) and node P10
(heuristic evaluation).

```jsonc
{
  "nodes": [
    {
      "id": "top",            // opaque plan digest, unique within the trace
      "duration": 1.0,        // isolated build duration, seconds (d(v))
      "peak_mem": 1000000000, // optional: predicted peak memory, bytes
      "is_atom": false,       // optional: synthetic atom marker (atom-seeded variant)
      "plan_name": "top-1.0", // optional: version-stable profile key (corpus fidelity)
      "confidence": 0.9       // optional: prediction confidence [0,1] (default 0.5)
    }
  ],
  "edges": [
    { "from": "top", "to": "a" }  // "from" depends on "to": "to" builds first
  ],
  "workers": [
    {
      "id": "w0",                          // opaque worker identity, unique
      "speed": 1.0,                        // optional: duration multiplier (<1 faster)
      "capacity": { "mem": 8000000000 },   // optional: abstract capacity vector
      "cached": ["leaf"]                   // optional: plan ids cached locally at t=0
    }
  ],
  "store_cached": ["some-prebuilt-plan"]   // optional: globally cached; filtered pre-coarsening
}
```

**Edge orientation.** An edge `{ "from": X, "to": Y }` means *X depends on Y*,
so Y must be built before X. Equivalently, Y is a dependency of X and X is a
dependent of Y. The graph must be acyclic (validated when the graph is built).

**Validation.** A trace is rejected for: duplicate node or worker ids, edges
referencing unknown nodes, self-edges, an empty worker pool, or non-finite /
negative durations.

## CLI

```
eos-sim --trace <FILE> [--seed <U64>]
```

(Heuristic flags — `--variant`, thresholds, `--gamma`, `--delta`, `--lambda` —
are documented as they land. See `eos-sim --help`.)

On a successful run the simulator prints the two contract lines consumed by
node P10 / constraint C2:

```
Loaded <N> plans
Simulation completed
```

`N` is the number of plan nodes loaded from the trace.

### Determinism

A fixed `--seed` reproduces **byte-identical** metrics output. The seed drives
only tie-breaking among genuinely-equal priorities or placements; the rest of
the engine is fully deterministic.

## Development

```
cargo test  -p eos-sim
cargo clippy -p eos-sim -- -D warnings
```

Fixtures live in `fixtures/`. Each is a self-contained trace used by the
integration tests (diamond DAG, H1-vs-H4 divergence, starvation contention).
