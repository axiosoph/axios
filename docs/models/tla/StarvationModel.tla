--------------------------- MODULE StarvationModel ---------------------------
\* Starvation-freedom of the delay-cost fairness discipline (P12).
\*
\* A single worker (maximal contention), a HIGH-priority RECURRING stream H
\* (it completes and immediately re-arrives, modelling a continuous stream of
\* high-priority arrivals) and one LOW-priority job L. The scheduler dispatches
\* the ready EP of top priority, where priority = a static base rank IMPROVED by
\* an accrued delay credit  Gamma * age  (age = clock - readySince).
\*
\* This delay credit IS the shared cost-model fairness term: the greedy PEFT tier
\* reads it as a priority/score adjustment; the future MCMF tier reads the SAME
\* term as the cost on a task's "unscheduled/delay" edge (ADR-0004 Future Work).
\* One cost model, two solvers, one property.
\*
\* With Gamma = 0 the delay credit vanishes (pure static priority) and L starves
\* forever behind the H stream -> StarvationFreedom FAILS. That is the
\* non-vacuity mutation (verified manually; not part of the green suite).
\*
\* Time advances as builds COMPLETE (run-to-completion; no preemption, so we
\* measure accrued WAIT, not time-sliced service -- this is why "delay cost" is
\* the accurate primitive here, not CFS vruntime). L's age therefore grows with
\* each H build, until it outranks the fresh re-arrivals of H.
EXTENDS Naturals

EntryPoints == {"H", "L"}

CONSTANTS
    Gamma,          \* delay-credit weight (Gamma = 0 disables fairness)
    LowPriPenalty,  \* static priority gap H enjoys over L
    MaxClock        \* clock bound for finite model checking

ASSUME Gamma \in Nat /\ LowPriPenalty \in Nat /\ MaxClock \in Nat

\* Lower base rank = higher static priority. H is preferred; L is penalised.
BaseRank == [s \in EntryPoints |-> IF s = "H" THEN 0 ELSE LowPriPenalty]

VARIABLES
    epStatus,    \* "ready" | "dispatched" | "complete" (H never "complete": it recurs)
    busy,        \* TRUE iff the single worker is occupied
    clock,
    readySince   \* per-EP tick at which it last became ready

vars == <<epStatus, busy, clock, readySince>>

Init ==
    /\ epStatus = [s \in EntryPoints |-> "ready"]
    /\ busy = FALSE
    /\ clock = 0
    /\ readySince = [s \in EntryPoints |-> 0]

Age(s) == clock - readySince[s]

\* s is at least as preferred as t: more delay credit and/or lower base rank.
\* Cross-multiplied to stay within the naturals (avoids signed costs).
Prefers(s, t) == Gamma * Age(s) + BaseRank[t] >= Gamma * Age(t) + BaseRank[s]

\* s is a top-priority ready EP under the delay-cost discipline.
TopPriority(s) ==
    /\ epStatus[s] = "ready"
    /\ \A t \in EntryPoints : epStatus[t] = "ready" => Prefers(s, t)

Dispatch(s) ==
    /\ ~busy
    /\ TopPriority(s)
    /\ epStatus' = [epStatus EXCEPT ![s] = "dispatched"]
    /\ busy' = TRUE
    /\ UNCHANGED <<clock, readySince>>

Tick(c) == IF c < MaxClock THEN c + 1 ELSE c

\* Completing a build advances the clock (time passes as work runs). H re-arrives
\* as fresh ready work (a new high-priority arrival); L becomes terminal.
Complete(s) ==
    /\ busy
    /\ epStatus[s] = "dispatched"
    /\ clock' = Tick(clock)
    /\ busy' = FALSE
    /\ epStatus' = [epStatus EXCEPT ![s] = IF s = "H" THEN "ready" ELSE "complete"]
    /\ readySince' = [readySince EXCEPT ![s] = IF s = "H" THEN Tick(clock) ELSE @]

Next ==
    \/ \E s \in EntryPoints : Dispatch(s)
    \/ \E s \in EntryPoints : Complete(s)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Clock bound for finite checking. Dispatch never consumes a tick, so bounding
\* the clock only truncates the (already-won) tail and cannot mask a starvation.
ClockBound == clock <= MaxClock

-----------------------------------------------------------------------------

TypeOK ==
    /\ epStatus \in [EntryPoints -> {"ready", "dispatched", "complete"}]
    /\ busy \in BOOLEAN
    /\ clock \in 0..MaxClock
    /\ readySince \in [EntryPoints -> 0..MaxClock]

\* Work-conservation under priority: a free worker never idles while ready work
\* exists (the top-priority ready EP is always dispatchable). Holds even at
\* Gamma = 0, so it is NOT the starvation discriminator -- it shows the delay-cost
\* discipline introduces no idling (the no-HoL-blocking facet of bounded-fair-
\* dispatch, under contention).
NoIdle ==
    (~busy /\ \E s \in EntryPoints : epStatus[s] = "ready")
        => \E s \in EntryPoints : ENABLED Dispatch(s)

\* P12 -- Starvation-freedom: the low-priority job is eventually dispatched, even
\* under the unbounded high-priority stream H. The teeth of bounded-fair-dispatch.
\* FAILS at Gamma = 0 (mutation): without the delay credit L is starved forever.
StarvationFreedom == <> (epStatus["L"] \in {"dispatched", "complete"})

=============================================================================
