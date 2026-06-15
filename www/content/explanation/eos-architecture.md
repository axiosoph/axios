+++
title = "How the Eos scheduler works"
description = "A cohesive end-to-end walkthrough of the Eos build scheduler: what it does, how each piece is designed, and why — for engineers who want to understand the system well enough to use it"
quadrant = "Explanation"
audience = "Engineers evaluating or operating Eos who are comfortable with distributed systems concepts and want to understand the scheduler's design without reading a formal specification"
+++

Most build systems treat parallelism as a property of the build definition: if you declare two jobs independent, they can run in parallel. Eos treats parallelism as a property of the scheduler's understanding of the dependency graph. Even in a single build request, the scheduler decides in real time which parts of the dependency closure deserve their own dedicated execution slot, how finely to slice the work, where on the cluster each slice should run, and how to share work across concurrent requests from different users. None of this is configured by hand.

This page explains how each piece of that works, and why it's designed the way it is.

## The stack Eos sits in

Eos is the L2 layer of the Axios stack. Requests arrive from **Ion** (L3), the CLI that resolves your package manifest into a concrete dependency graph and hands it off. Eos schedules and coordinates the actual builds, delegating sandboxed execution to **Snix** workers — Nix-compatible build environments that write their outputs to a content-addressed artifact store. The **Atom** protocol (L1) provides the identity and signing layer underneath everything.

Eos's specific job is everything between "here is a dependency graph" and "here are the build outputs": cache checking, deduplication across requests, parallelism decisions, worker assignment, failure recovery, and re-planning as new requests arrive or builds complete.

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

**Cache affinity.** A worker that already holds an entry point's input artifacts can build it faster — no fetch required. Eos uses Local Rendezvous Hashing to compute affinity scores: a content-routing scheme that achieves near-optimal load balance while exploiting CPU cache locality in the hash computation itself. An affinity advantage shrinks the predicted duration for that worker, and PEFT naturally favors it.

**Resource fit.** A worker whose free CPU and memory closely match the entry point's predicted resource demand runs it faster than a poorly-fitted worker. Resource fit inflates or reduces the predicted duration accordingly. This is what steers heavy builds toward capable workers and prevents memory overcommit — a predictive placement signal rather than a runtime enforcement mechanism.

When prediction confidence is high, the scheduler may hold a ready entry point briefly — waiting for a better-fitting worker to free up — before dispatching. When confidence is low, it dispatches immediately. The mechanism for ensuring this hold is always bounded is described in the next section.

## Learning from history

Duration and resource estimates come from **historical build profiles** maintained for every plan the scheduler has seen. After each completed build, the profile for that plan is updated via exponential moving average. Subsequent requests for the same plan arrive with a calibrated estimate rather than a conservative default.

What makes this work reliably is a property specific to Nix: package identity is cryptographically stable. The content-addressed atom-id is the same for every build of the same atom at the same version. There's no need to train a model — the atom identifier is the prediction oracle. A scheduler that has built `openssl-3.0.12` a hundred times has an accurate estimate for `openssl-3.0.13` without any retraining, because the two share the same structural history.

For packages the scheduler has never seen, developers can supply scheduling hints in atom metadata: expected build duration, memory requirements, whether the task needs specialized hardware. After the first build, historical data takes over and the metadata becomes a fallback.

When predictions are wrong, the system degrades gracefully. The resource-fit signal is weighted by a confidence factor derived from observed prediction error. Sustained inaccuracy causes that weight to decay exponentially until placement falls back to pure cache affinity — the prediction-free baseline — and waits for profiles to recalibrate. The scheduler never makes actively bad placements on the basis of wrong predictions; it retreats to conservative behavior.

## Staying fair under load

PEFT's look-ahead rank determines dispatch priority, but every entry point also accrues a **delay credit** proportional to how long it has been waiting. That credit rises without bound. It is, specifically, what prevents starvation.

Under continuous high-priority load — a steady stream of new, high-priority requests — a low-priority entry point's credit eventually grows large enough to overtake every fresh arrival. There is no timeout or quota; the bound follows from the mathematics of the credit growth rate relative to the range of possible priorities. The formal model has a counterexample: set the credit to zero and a low-priority entry point can wait forever. The credit is not a heuristic tweak — it is the mechanism.

The delay credit also makes the bounded dispatch window work without risking starvation. The scheduler can hold a ready entry point for a bounded period, waiting for an affinity-matched worker to free up, because the credit guarantees the hold ends: as the credit grows, the entry point eventually outranks whatever it's waiting for.

## Handling the unexpected

**Dispatched entry points are immutable.** Once an entry point is running on a worker, its assignment is fixed. Re-coarsening and re-planning triggered by new requests or cache updates only touch the pending and ready portions of the graph — the mutable partition. Dispatched and completed entry points are frozen inputs that PEFT treats as fixed constraints.

This partition is what makes failure recovery local: when a worker fails, only that worker's dispatched entry points need to be reverted to ready and re-dispatched. Everything else continues undisturbed. Transient failures (worker crash, network partition) trigger automatic re-dispatch with no manual intervention. Deterministic failures propagate to downstream dependents and ultimately to the requesting client.

New requests arriving mid-flight are handled identically to initial requests: merge into the unified graph, run the cache filter, re-coarsen the mutable portion, re-run the dispatch algorithm. Frozen work is untouched. The scheduler never needs to know whether a request is "new" or "concurrent" — the unified graph structure handles the rest.

## The two-sentence version

Eos merges all build requests into a single deduplicated graph, coarsens the uncached work into a small set of entry points that preserve locality and prevent redundant computation, then assigns those entry points to workers using look-ahead scheduling that accounts for cache affinity, resource fit, and the priority of downstream work — all while learning from history, staying fair under load, and recovering from failures without human intervention.

The formal proofs that these properties hold — no deadlock, no starvation, bounded makespan relative to optimal — are covered in the [formal guarantees page](eos-formal-guarantees.md); the simulation study that determined the entry-point coarsening algorithm is covered in the [scheduling simulation page](eos-scheduling-simulation.md).
