+++
title = "How the Eos scheduler works"
description = "A cohesive end-to-end walkthrough of the Eos build scheduler: what it does, how each piece is designed, and why — for engineers who want to understand the system well enough to use it"
quadrant = "Explanation"
tags = ["eos"]
audience = "Engineers evaluating or operating Eos who are comfortable with distributed systems concepts and want to understand the scheduler's design without reading a formal specification"
+++

Most build systems treat parallelism as a property of the build definition: if you declare two jobs independent, they can run in parallel. Eos treats parallelism as a property of the scheduler's understanding of the dependency graph. Even in a single build request, the scheduler decides in real time which parts of the dependency closure deserve their own dedicated execution slot, how finely to slice the work, where on the cluster each slice should run, and how to share work across concurrent requests from different users. None of this is configured by hand.

This page explains how each piece of that works, and why it's designed the way it is.

## The stack Eos sits in

Eos is the L2 layer of the Axios stack. Requests arrive from **Ion** (L3), the CLI that resolves your package manifest into a concrete dependency graph and hands it off. Eos schedules and coordinates the actual builds, delegating sandboxed execution to **Snix** workers — Nix-compatible build environments that write their outputs to a content-addressed artifact store. The **Atom** protocol (L1) provides the identity and signing layer underneath everything.

Eos's specific job is everything between "here is a dependency graph" and "here are the build outputs": cache checking, deduplication across requests, parallelism decisions, worker assignment, failure recovery, and re-planning as new requests arrive or builds complete.

One property of this stack deserves early mention because it's load-bearing for the entire scheduling model: Snix's blob store performs intra-file chunk-level deduplication. When a package produces similar outputs across versions — as Nix packages almost always do, since output hashes change completely on any input change while the actual binary content changes only partially — Snix transfers only the novel chunks, not the full archive. Transfer cost is proportional to content novelty, not output size, and declines as the store fills with chunks from prior builds. Without this property, cross-worker artifact dependencies would impose transfer costs that make aggressive parallelism prohibitively expensive; with it, the scheduling model is viable. This document describes the build scheduling layer, which is fully specified but not yet fully implemented. The evaluation scheduling layer — which handles the Nix evaluation step that produces derivation DAGs from atom expressions — is still being specified.

> **Status:** Eos is a work in progress. The build scheduling design described here is the formal basis; the evaluation scheduling layer and full implementation are ongoing.

## One graph for everything

When a request arrives, Eos does not isolate it in its own queue. It merges the incoming dependency graph into a single **unified global graph** shared across all currently active requests. Every node in that graph is keyed by a content-addressed plan hash — the same plan, requested by two different users simultaneously, maps to the same node and is scheduled exactly once. This is structural deduplication, not opportunistic: it is impossible for two requests in the same global graph to produce separate builds of the same content-addressed plan.

Before the graph enters scheduling, every node is checked against the artifact store. Plans whose outputs are already cached are pruned immediately — they never become scheduling decisions. What remains is the uncached subgraph: the work that actually needs to happen.

## Deciding how finely to slice the work

The uncached subgraph could contain thousands of nodes. Scheduling each as an independent distributed task sounds maximally parallel, but it causes two concrete problems.

**Locality destruction.** When package A depends on package B, the natural thing is to build them on the same worker — B's outputs are already local, and A can start the moment B finishes without any data movement. Fragment them onto different workers and you've introduced a network transfer that wouldn't have existed otherwise. Multiplied across a graph with thousands of edges, fine-grained per-node dispatch can spend more time moving data than building.

**Redundant computation.** Build graphs have shared nodes — the same base library required by dozens of packages. If those packages are dispatched to different workers and start building concurrently, every worker will start building the shared dependency independently. The first to finish is useful; the others are wasted.

Eos addresses both with **coarsening**: before dispatch, the scheduler groups the uncached subgraph into a small set of **entry points** — independently-schedulable units, each covering a coherent subgraph. A worker dispatched an entry point builds everything in its scope locally, with full data locality and no cross-machine transfers for any transitive dependency. The scheduler tracks entry points — typically dozens — rather than individual plans — typically thousands.

The coarsening algorithm selects which nodes become entry points from three signals:

- A node on the **critical path** — the longest dependency chain through the graph — becomes its own entry point so that chain can start on a dedicated worker immediately, rather than being blocked behind unrelated work.
- A **high-fan-in node** — one that many independent packages all depend on — becomes its own entry point, scheduled and built exactly once before any dependent can start, eliminating the window for redundant computation.
- A **heavy isolated node** — an expensive build step with no upstream contention — becomes its own entry point so it can be routed to the most capable available worker.

A simulation study across 13 real nixpkgs packages quantified the consequences of getting this wrong: naive coarsening (one entry point for the entire closure) is 3–6× slower than well-chosen coarsening, and the gap widens as the worker pool grows. The [scheduling simulation page](eos-scheduling-simulation.md) covers that study in detail.

## Routing entry points to workers

Once coarsened, the scheduler assigns each entry point to a worker using **PEFT** (Predict Earliest Finish Time). PEFT works backward through the entry-point graph, computing for every (entry-point, worker) pair the optimistic time until all remaining downstream work completes. This is the key distinction from greedy assignment: the question is not "when does this entry point finish on this worker?" but "if I put this entry point here, what's the best-case time until the entire build finishes?"

An entry point that looks cheap in isolation but sits above ten critical downstream steps gets higher priority than one that's expensive but has no dependents. The look-ahead prevents the common failure mode of greedy scheduling: filling all workers with visible work and leaving the critical path waiting.

Two placement signals enter the decision through the predicted duration estimate, not as a separate scoring layer:

**Cache affinity.** A worker that already holds an entry point's input artifacts can build it faster — no fetch required. Eos uses Local Rendezvous Hashing to compute affinity scores: a content-routing scheme that achieves near-optimal load balance while keeping per-worker score comparisons efficient. An affinity advantage shrinks the predicted duration for that worker, and PEFT naturally favors it.

**Resource fit.** A worker whose free CPU and memory closely match the entry point's predicted resource demand runs it faster than a poorly-fitted worker. Resource fit inflates or reduces the predicted duration accordingly. This is what steers heavy builds toward capable workers and prevents memory overcommit — a predictive placement signal rather than a runtime enforcement mechanism.

When prediction confidence is high, the scheduler may hold a ready entry point briefly — waiting for a better-fitting worker to free up — before dispatching. When confidence is low, it dispatches immediately. The mechanism for ensuring this hold is always bounded is described in the next section.

## Learning from history

Duration and resource estimates come from **historical build profiles** maintained for every plan the scheduler has seen. After each completed build, the profile for that plan is updated via exponential moving average. Subsequent requests for the same plan arrive with a calibrated estimate rather than a conservative default.

What makes this work reliable is the combination of two properties — one from Nix, one from Atom. Nix's derivation model is content-addressed: the same derivation inputs always produce the same output hash, so a profile keyed to a derivation hash is a precise claim — *this exact package, built this exact way*. When the same hash reappears, the profile applies without qualification. This is the foundation: historical profiles are reliable because Nix derivation identity is deterministic.

The limitation of derivation hashes alone is that they change completely on any input change, including minor version bumps. Every version bump looks like an entirely new package to a derivation-hash-keyed system, discarding accumulated profile history. A scheduler without cross-version identity must either reset profiles on every bump or rely on a learned model to infer that openssl-3.0.13 is probably similar to openssl-3.0.12 — a non-trivial inference problem with its own training data requirements and failure modes. **Atom** — the identity protocol paired with this stack — addresses this directly: the atom-id is derived from the atom's label and structural role rather than its content hash, so it is stable *across versions*. When openssl bumps from 3.0.12 to 3.0.13, the atom-id doesn't change — the scheduler's historical profile carries forward, and the relevant build characteristics — duration, memory footprint, parallelism headroom — are typically stable enough across minor versions to make that profile a better prior than any conservative default. The atom-id is a structural claim of continuity; the profile lookup replaces inference entirely.

For packages the scheduler has never seen, developers can supply scheduling hints in atom metadata: expected build duration, memory requirements, whether the task needs specialized hardware. After the first build, historical data takes over and the metadata becomes a fallback.

When predictions are wrong, the system degrades gracefully. The resource-fit signal is weighted by a confidence factor derived from observed prediction error. Sustained inaccuracy causes that weight to decay exponentially until placement falls back to pure cache affinity — the prediction-free baseline — and waits for profiles to recalibrate. The scheduler never makes actively bad placements on the basis of wrong predictions; it retreats to conservative behavior.

## Staying fair under load

PEFT's look-ahead rank determines dispatch priority, but every entry point also accrues a **delay credit** proportional to how long it has been waiting. That credit rises without bound. It is, specifically, what prevents starvation.

Under continuous high-priority load — a steady stream of new, high-priority requests — a low-priority entry point's credit eventually grows large enough to overtake every fresh arrival. There is no timeout or quota; the bound follows from the mathematics of the credit growth rate relative to the range of possible priorities. The TLA+ formal model has a counterexample: set the credit to zero and a low-priority entry point can wait forever. The credit is not a heuristic tweak — it is the mechanism.

The delay credit also makes the bounded dispatch window work without risking starvation. The scheduler can hold a ready entry point for a bounded period, waiting for an affinity-matched worker to free up, because the credit guarantees the hold ends: as the credit grows, the entry point eventually outranks whatever it's waiting for.

## Handling the unexpected

**Dispatched entry points are immutable.** Once an entry point is running on a worker, its assignment is fixed. Re-coarsening and re-planning triggered by new requests or cache updates only touch the pending and ready portions of the graph — the mutable partition. Dispatched and completed entry points are frozen inputs that PEFT treats as fixed constraints.

This partition is what makes failure recovery local: when a worker fails, only that worker's dispatched entry points need to be reverted to ready and re-dispatched. Everything else continues undisturbed. Transient failures (worker crash, network partition) trigger automatic re-dispatch with no manual intervention. Deterministic failures propagate to downstream dependents and ultimately to the requesting client.

New requests arriving mid-flight are handled identically to initial requests: merge into the unified graph, run the cache filter, re-coarsen the mutable portion, re-run the dispatch algorithm. Frozen work is untouched. The scheduler never needs to know whether a request is "new" or "concurrent" — the unified graph structure handles the rest.

## The evaluation gap

The build scheduling layer described above has a predecessor step that is not yet fully specified: evaluation scheduling. Before builds can be dispatched, Eos must evaluate the Nix expressions that produce the derivation DAG — the actual build graph the scheduler operates on. Evaluation is a separate CPU-bound step that must run before the build scheduler has a graph to work with.

The atom lock file — a per-project manifest of resolved atom versions — is a coarser representation of the same dependency structure the derivation DAG captures. When multiple requests arrive concurrently, their lock files will typically share substantial overlap: the same atoms at the same versions, producing identical derivation subgraphs. An evaluation scheduler that exploits this — grouping concurrent requests by lock-file similarity to evaluate overlapping atoms once — avoids redundant evaluation work before derivations ever reach the build scheduler. The build scheduler already coalesces concurrent requests into a unified global DAG (described in "One graph for everything" above), so the eval scheduler's contribution is complementary rather than duplicative: it reduces the cost of producing the derivations the build scheduler will merge. Atoms whose derivations are already known from a prior evaluation can skip the step entirely, their derivations pushed directly to the build queue — the same pattern as build-side artifact pruning, one layer up.

The shape of this optimization is clear; the exact evaluation scheduling semantics are not yet specified, but it is expected to be substantially simpler than build scheduling: it is structurally shallow, CPU-bound rather than I/O-bound, and produces the build graph rather than executing it. The build scheduler, the harder of the two problems, was specified first.

## The two-sentence version

Eos merges all build requests into a single deduplicated graph, coarsens the uncached work into a small set of entry points that preserve locality and prevent redundant computation, then assigns those entry points to workers using look-ahead scheduling that accounts for cache affinity, resource fit, and the priority of downstream work — all while learning from history, staying fair under load, and recovering from failures without human intervention.

The formal proofs that these properties hold — no deadlock, no starvation, bounded makespan relative to optimal — are covered in the [formal guarantees page](eos-formal-guarantees.md); the simulation study that determined the entry-point coarsening algorithm is covered in the [scheduling simulation page](eos-scheduling-simulation.md).
