+++
title = "What the Eos scheduling simulation found"
description = "A plain-language account of the simulation study that determined how Eos selects build entry points — what we tested, what the data showed, and why the result holds under realistic conditions"
quadrant = "Explanation"
audience = "Developers evaluating Eos who are comfortable with concepts like dependency graphs and parallelism but are not scheduling-theory specialists"
+++

Before the Eos scheduler dispatches a single task, it makes a decision that shapes everything that follows: which nodes in the build graph to expose as **entry points** — independently-dispatchable units of work. Too few and the scheduler idles workers while a single thread of tasks unwinds. Too many and you pay coordination overhead on work that would naturally chain anyway.

Getting this right is not obvious. A build graph for a real package contains thousands of nodes spanning a deep toolchain bootstrap, and the heuristic must decide which of them merit their own scheduling slot without knowing in advance which tasks will be slow, which workers will free up, or what else will be building concurrently. The choice has measurable consequences for build time. This page explains what a 3,700-cell simulation study over 13 real packages found about how to make that choice well.

## What the simulation covers

We ran the Eos simulator against traces extracted from real nixpkgs derivation closures at a fixed anchor commit, covering thirteen packages ranging from small utilities (jq, fd) through medium-sized tools (curl, git, openssh) to large and extreme-scale builds (ffmpeg, libreoffice, chromium). Each trace captures the full recursive dependency closure — every derivation that needs to be built, with its estimated duration and dependency edges.

Across those traces, we swept:

- **Six EP-selection heuristics** (H0 through H4 and H6), including a parameter sweep over thresholds and weight configurations
- **All four cache states** per package: cold (nothing cached), base (normal state), partial (shared infrastructure cached), and warm (only the top layer uncached)
- **Worker pool sizes from 4 to 64** homogeneous workers, to check whether rankings change with parallelism
- **The actual scheduler surface** — a single unified DAG formed by merging all 13 package closures with shared nodes deduplicated, exactly as the real scheduler would encounter them when building multiple packages at once

The unified DAG test matters, and its design requires a word of clarification. Because all 13 packages come from the same nixpkgs anchor commit, their derivation closures share store-path hashes for common bootstrap infrastructure — 974 nodes appear identically in every closure, so the merged graph of all 13 packages contains only 4,323 distinct nodes despite per-package traces totalling 26,419 nodes before deduplication. This is an accurate model of a specific and important scenario: the first build of these packages from a fresh nixpkgs checkout. Packages from different nixpkgs revisions would not share store paths even if structurally similar, so the merged graph in that case would be larger and the shared-node fan-in would not inflate in the same way.

In practice, the bootstrap chain is typically cached after its first build and reused indefinitely. Our warm cache state — where 80% of nodes are already in the store — models exactly this more common scenario, and the heuristic rankings are consistent across all four cache states (cold through warm). The per-package results (which are agnostic to cross-package sharing) also show the same directional ranking. Both lines of evidence point to the same conclusion, which is why we are confident it holds under realistic deployment conditions and not only in the same-checkout first-build scenario.

## What no entry point selection costs you

H0 is the degenerate baseline: select only the package root as an entry point, producing exactly one independently-schedulable unit for the entire closure. Everything else runs sequentially, chained off that one root.

The cost is severe — and it gets worse as the worker pool grows. At 8 workers, H0 finishes the build 2.9× slower than the best heuristic (median across packages). At 64 workers, the chromium closure takes 6.4× longer under H0 than under H1. This is not surprising: more idle workers make a bottlenecked serial chain proportionally more expensive. The simulation quantifies it, but the mechanism is what matters — **any deployment that scales the worker pool makes entry point selection more valuable, not less**.

## What the winner does differently

The simulation tested four substantive heuristics. In order of measured performance:

**H4 — cost and fan-in thresholds.** Promote a node if its estimated duration, fan-in count, or subgraph cost exceeds a fixed limit. Simple, but it misses an important class of bottleneck: nodes that aren't expensive in isolation but are expensive *because many things depend on them*. A node that costs 60 seconds and has ten packages waiting on it holds up ten threads of work simultaneously. H4's absolute thresholds can miss this if it doesn't happen to cross any single threshold.

**H1 — three independent criteria.** Promote a node if *any* of these hold:
- Its contribution to the longest dependency chain (critical path) exceeds a threshold
- Its **convergence value** — `(fan_in − 1) × duration` — exceeds a threshold
- Its cost alone exceeds a threshold

The convergence criterion is what gives H1 its consistent edge over H4. A node with ten packages depending on it and a 60-second build time has convergence 540 — and the criterion fires regardless of whether that cost alone would cross H4's thresholds. In the unified 13-package DAG, where the shared bootstrap chain gives many nodes fan-in in the tens or hundreds, H1 correctly identifies 97.8% of all nodes as entry points. The shared infrastructure is almost entirely parallelized automatically.

**H2 (tuned) — a weighted combined score.** Promote a node if `w_c × CP + w_r × convergence + w_d × duration > θ_combined`. With equal weights and a threshold of 30, this catches a class of nodes H1 misses: cheap isolated leaf nodes (duration ≈ 38s, fan-in of 1, no critical-path contribution) that don't trigger any of H1's three separate gates but are worth parallelizing when a worker would otherwise sit idle. H2 at this setting consistently beats H1 across all packages, all cache states, and all worker pool sizes tested.

## The ranking

| Configuration | Measured gap vs H1 | Stable across conditions? |
| :------------ | :----------------: | :-----------------------: |
| H2 (θ_combined = 30) | −0.28 to −0.50% | Yes |
| H1 (default parameters) | — | reference |
| H2 (θ_combined = 60, default) | ≈ 0% | Ties H1 |
| H4 | +0.12 to +0.43% | Yes |
| H0 | +190 to +308% | Worsens with worker count |

The H2(θ=30) advantage is smaller on the actual unified DAG (0.28%) than in per-package testing (up to 0.50%). This is expected: when packages share a deep bootstrap chain, H1's convergence criterion already fires on most shared nodes, leaving less work for H2's combined score to improve upon. The advantage is real but more modest in production than the isolated-package numbers suggest.

The H1 > H4 gap is stable in the opposite direction: it holds at +0.12% on the cold unified DAG and widens to +0.43% on the warm cache state. It did not reverse under any configuration tested.

## Scale properties

Two findings from the worker-pool sweep deserve separate emphasis.

H0's penalty scales with workers in exactly the wrong direction — the more capacity you add, the more wasteful serial scheduling becomes. This is the strongest argument for taking entry point selection seriously at build infrastructure scale.

Every other ranking is **worker-count invariant**. The H1 > H4 gap holds at 0.3% across P = 4, 8, 16, 32, and 64. The H2(θ=30) advantage over H1 holds across the same range. You don't need to re-tune the heuristic as you scale the pool.

## The two-sentence version

H2 with equal weights and θ_combined = 30 is the best-measured entry point selection heuristic, beating H1 by 0.28–0.50% across all tested conditions including the real multi-package unified DAG. H1 is a strong documented alternative with more interpretable criteria; H4 consistently underperforms; and the cost of using no selection at all (H0) compounds with every worker you add.

## The honest scope

The corpus is 13 packages from a single nixpkgs anchor commit. The unified DAG result — where shared nodes are deduplicated and fan-in inflates — applies specifically to building those packages together from the same checkout; it is not a claim about multi-package builds in general. The per-package results do not depend on cross-package sharing and apply more broadly to any single-package build against a nixpkgs-like bootstrap graph.

The quantitative gaps (0.28%, 0.43%) are specific to this graph family and its estimated build durations. The directional rankings are structural and expected to hold across nixpkgs versions, since the deep bootstrap topology is a stable property of how nixpkgs packages are built — but have not been validated on fundamentally different graph structures (sparse graphs with few shared nodes, or build systems without a deep common toolchain), and a separate study would be needed to make claims there.

The simulation binary, corpus traces, sweep harness, and raw result files are all in the repository under `tools/eos-sim`, `tools/eos-sim-traces`, and `tools/eos-sweep` respectively.
