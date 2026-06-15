+++
title = "What Eos's formal proofs guarantee"
description = "A plain-language tour of the safety, fairness, and performance properties proven about the Eos scheduler in TLA+ and Lean — no formal-methods background required"
quadrant = "Explanation"
tags = ["eos"]
audience = "Developers evaluating Eos who understand systems concepts like head-of-line blocking and deadlock but are not formal-methods or scheduling-theory experts"
+++

Most schedulers tell you they're fast and correct, then hand you a pile of benchmarks. Eos's design has been checked a level deeper: the core properties are either **exhaustively model-checked** (every possible ordering of events on small setups, with no bad state ever reached) or **machine-proven** (a mathematical proof, verified by a computer, that holds for any size). This page explains what that work actually buys you, in ordinary terms.

## Two kinds of guarantee

The proofs come in two flavors, and the distinction matters when you decide how much to trust each claim:

- **Model-checked (TLA+).** A tool enumerated *every possible* sequence of events — dispatches, completions, failures, merges, in every interleaving — on small scheduler configurations, and confirmed the bad thing never happens. Exhaustive, but on bounded examples.
- **Machine-proven (Lean).** A theorem proven for *all* inputs and all sizes, with a proof a computer checked line by line. Every proof in the Eos development is complete — there are no gaps, no "trust me here" placeholders. These hold for any scale, under explicitly stated assumptions.

Neither one is "we ran it and it seemed fine." Both are stronger than that. What follows is grouped into **correctness** (the scheduler never does the wrong thing) and **performance** (it's provably fast, not just fast-on-our-laptop).

## Correctness: it never does the wrong thing

These are model-checked: across every reachable state of the system, on every topology we tested (linear chains, diamonds, fan-in, independent tasks, and dynamically merged requests), the property holds.

**No head-of-line blocking.** This is the headline. A task that's ready to run, and has a worker with spare capacity, *will* be dispatched — and the decision to dispatch it looks **only** at that task and that worker, never at any other task. So a slow or stuck task sitting on one worker can't wedge an unrelated ready task waiting elsewhere. We proved this two ways: that the unrelated task *can* always move (a safety property), and that it *eventually does* move (a liveness property). A queue-of-one slow job can't hold up everyone behind it.

**No deadlocks, ever.** At every reachable state, either all work is finished or *some* action can still make progress. The system can't paint itself into a corner where work remains but nothing can advance. There is no reachable "everything is waiting on everything else" state.

**Dependencies are always respected.** A task never starts or finishes until everything it depends on has completed. "Ran the build before its inputs were ready" is not a reachable bug.

**Workers never get overloaded.** Assigned load never exceeds a worker's declared capacity. This isn't a runtime check that might occasionally fail — it's an invariant that holds in every reachable state.

**Failures stay contained.** A task is only ever marked failed for a legitimate reason: it genuinely failed, or a dependency it needed failed and the failure cascaded along a real edge. Unrelated tasks can't get spuriously marked failed. And a task hit by a transient failure (a worker crash, an infrastructure blip) always recovers to a real terminal state instead of hanging forever.

**Merging duplicate work across live requests can't corrupt anything.** When two in-flight requests share work and Eos merges their dependency graphs at runtime, the result stays acyclic and still deadlock-free. This is genuinely hard — most systems either don't deduplicate live requests or do it unsafely. It's provably safe here because identity is content-addressed: two requests can never disagree about which direction a dependency edge points, so merging graphs that share nodes can't introduce a cycle.

**Once dispatched, a task stays put.** No mid-flight work-stealing or reassignment. A dispatched task keeps its worker until it terminates. Predictable, no thrash.

**Everything finishes.** Every request's tasks all eventually reach done-or-failed. No silently abandoned work.

**Starvation is impossible — and the mechanism that prevents it is load-bearing.** Even under an *infinite* stream of high-priority jobs, a low-priority job still eventually runs. It works because a waiting task's priority grows with how long it has waited (a "delay credit") until it overtakes fresh arrivals. The sharp part: we deliberately broke it. Turn the delay credit off, and the proof *fails* — the checker produces an actual starvation trace. So the fairness mechanism isn't decorative; it is exactly and provably what prevents starvation.

## Performance: and it's provably fast

These are machine-proven in Lean, so they hold at any scale — under the stated assumptions, which are noted inline.

**Bad time-estimates degrade you gracefully, not catastrophically.** Eos uses learned predictions of how long tasks take. The consistency theorem proves that if predictions are within a fraction $\varepsilon$ of reality, your actual finish time (makespan) stays within a factor of

$$\frac{1 + \varepsilon}{1 - \varepsilon}$$

of the prediction-perfect schedule. In plain terms: a predictor that's 10% off costs you roughly 22% in worst-case finish time — not a blowup, not a cliff. Error stays bounded and proportional. (An adaptive variant proves the same shape when the scheduler widens its safety margin as error grows.)

**The predictor provably self-corrects.** The scheduling decision stays stable as long as the gap between the best and second-best choice is wider than the prediction noise, and the error-tracking estimate is proven to converge toward the true error at an exponential rate. The system gets *more* reliable over time, with a proven convergence rate rather than a hope.

**Sharing work across requests is provably a win, never a loss.** When multiple requests overlap, Eos deduplicates the shared work. It's proven that the merged graph is never bigger than the sum of the separate requests — with equality *exactly* when the requests share nothing in common (proven in both directions). And the merged schedule's finish time is proven to be **always less than or equal to** running the requests separately. You cannot lose by unifying.

**There's a hard, interpretable ceiling on total finish time.** The main theorem bounds the whole unified scheduler's makespan by

$$\alpha \,\bigl(1 + \rho \,|R|\bigr)\, M_{\max}$$

where $\alpha$ is how good the underlying list-scheduling is, $\rho$ is how much the requests actually overlap, $|R|$ is the number of requests, and $M_{\max}$ is the worst single-request finish time. Every term is something you can reason about. And $\alpha$ is pinned to **Graham's classic $2 - 1/|W|$ bound** — the same well-known guarantee general-purpose schedulers have relied on for decades. So Eos is never worse than the textbook baseline, and you get the deduplication savings on top of it.

**The cache provably converges.** As work gets cached, the set of tasks the scheduler still has to launch shrinks monotonically, and a fully-warmed cache is reached in a bounded number of steps. Incremental re-coarsening doesn't oscillate or stall — it provably settles.

## The two-sentence version

We mathematically proved the scheduler can't deadlock, can't head-of-line-block, can't starve a job, and can't violate dependencies or worker capacity — and that merging duplicate work across requests is always safe and never slower. Separately, we proved its finish time stays within a known constant factor of optimal even when its time-predictions are wrong, degrading smoothly instead of falling off a cliff.

## The honest caveat

The correctness properties are exhaustively *checked on small configurations* — every interleaving, but bounded in size. The performance properties are *proven for all sizes*, but under explicit assumptions (bounded prediction error, work-conserving dispatch, and so on). This is dramatically stronger than benchmark-and-hope, but it is not "the entire production binary is proven correct." We'd rather you know exactly where the line is than discover it later.

The proofs themselves live in the repository under `docs/models/tla` (the model-checked properties) and `docs/models/lean` (the machine-proven theorems), with the scheduler design written up in [the architecture overview](architecture.md).
