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
  Theorem1.lean               Coverage Existence
  Theorem2.lean               Consistency Bound
  Theorem3.lean               Robustness
  Theorem4.lean               Singleflight Deduplication
```

**`Defs.lean`** — DAG path reachability (`PathNoS`), the `EosModel`
structure encoding the four coverage properties (total coverage,
self-coverage, transitive containment, downward closure), and the
`WellFounded` DAG edge relation.

**`Theorem1.lean`** — _Coverage Existence._ Constructs the identity
witness (`S = univ, κ = id`) proving a valid `EosModel` exists for
any finite DAG.

**`Theorem2.lean`** — _Consistency Bound._ Proves
`makespan(d, τ) ≤ α · (1+ε)/(1-ε) · makespan(d*, τ)` under
ε-accurate predictions via well-founded induction on completion
times.

**`Theorem3.lean`** — _Robustness._ Lemma 3.1 (scoring perturbation
stability — bounded perturbation doesn't change the greedy
assignment) and EMA lower bound (geometric convergence under
sustained prediction error).

**`Theorem4.lean`** — _Singleflight Deduplication._ Proves
`|⋃ V'_i| ≤ Σ|V'_i|` with equality iff pairwise disjoint (both
directions).

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
