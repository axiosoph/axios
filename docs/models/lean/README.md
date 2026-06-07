# Eos Scheduling: Lean 4 Formal Proofs

Machine-checked proofs for the optimization quality guarantees of the
Eos build scheduler. These complement the TLA+ protocol-correctness
model in `../tla/` — together they form the two-track formal
verification strategy described in
[ADR-0004](../../adr/0004-learning-augmented-scheduling.md).

## Prerequisites

| Dependency | Version / Source                                                     |
| :--------- | :------------------------------------------------------------------- |
| Lean 4     | `v4.31.0-rc1` (pinned in [`lean-toolchain`](lean-toolchain))         |
| Mathlib    | Fetched automatically by Lake (see [`lakefile.toml`](lakefile.toml)) |

Nix users: a nested `shell.nix` is not currently provided. Use
[elan](https://github.com/leanprover/elan) to install the pinned
toolchain, or enter the repo-root Nix shell which provides elan.

## Building

```sh
lake build          # from this directory
```

The first build fetches and compiles Mathlib (~15–30 minutes).
Subsequent builds are incremental.

## Project Structure

```
EosScheduling.lean            Root module — imports all theorem files
EosScheduling/
  Defs.lean                   Shared definitions
  Schedule.lean               Scheduling model infrastructure
  Theorem1.lean               Coverage Existence
  Theorem2.lean               Consistency Bound
  Theorem2Prime.lean          Adaptive Consistency
  Theorem3.lean               Robustness
  Theorem4.lean               Structural Deduplication
  Theorem4Prime.lean          Weighted Structural Deduplication
  Theorem5.lean               Makespan Subadditivity
  Theorem6.lean               CAS-Scheduling Bound
  Theorem7.lean               Re-coarsening Convergence
```

**`Defs.lean`** — DAG path reachability (`PathNoS`), the `EosModel`
structure encoding the four coverage properties (total coverage,
self-coverage, transitive containment, downward closure), and the
`WellFounded` DAG edge relation.

**`Schedule.lean`** — Defines `WorkerPool` and `Schedule` structures encoding resource capacity and dependency constraints, defines `schedule_makespan`, and proves makespan lower bounds under worker contention.

**`Theorem1.lean`** — _Coverage Existence._ Constructs the identity
witness (`S = univ, κ = id`) proving a valid `EosModel` exists for
any finite DAG.

**`Theorem2.lean`** — _Consistency Bound._ Proves
`M(σ_H) ≤ α · (1+ε)/(1-ε) · M(σ*)` under ε-accurate
predictions via well-founded induction on completion times.

**`Theorem3.lean`** — _Robustness._ Lemma 3.1 (scoring perturbation
stability — bounded perturbation doesn't change the greedy
assignment) and EMA lower bound (geometric convergence under
sustained prediction error).

**`Theorem4.lean`** — _Structural Deduplication._ Proves
`|⋃ V'_i| ≤ Σ|V'_i|` with equality iff pairwise disjoint (both
directions).

**`Theorem4Prime.lean`** — _Weighted Structural Deduplication._ Generalizes
Theorem 4 from cardinality to duration-weighted sums over union,
with equality iff pairwise disjoint.

**`Theorem2Prime.lean`** — _Adaptive Consistency._ Proves that
the coarsening factor `α(ε̄) → 1` as the mean prediction error
`ε̄ → 0`, conditionally closing the coarsening gap from
Theorem 2 when predictions are accurate.

**`Theorem5.lean`** — _Unified Coarsening Dominance._ Proves that the unified coarsening schedule (which has fewer scheduled nodes due to deduplication) achieves equal or better makespan than the per-request coarsening schedule under the Schedule model.

**`Theorem6.lean`** — _CAS-Scheduling Bound._ Proves makespan bound
under CAS deduplication with competitive ratio bounded by
`α(1 + ρ |R|)` against the optimal independent makespan.

**`Theorem7.lean`** — _Re-coarsening Convergence._ Proves monotonicity and
convergence of the coarsened entry point set under incremental cache growth.

## Relationship to the Project

These proofs are **Track B** of a two-track formal verification
strategy:

- **Track A** — TLA+ model checking (`../tla/`): verifies protocol
  correctness under all interleavings.
- **Track B** — Lean 4 proofs (this directory): verifies optimization
  quality bounds.

See the formal model at [`../eos-scheduling.md`](../eos-scheduling.md)
and [ADR-0004](../../adr/0004-learning-augmented-scheduling.md) for
full context.

## Verification Status

Zero `sorry` placeholders. Zero custom `axiom` declarations. All
assumptions enter as explicit hypotheses on theorem signatures or
type-level constraints (e.g. `[Fintype V]`, `[DecidableEq V]`).
