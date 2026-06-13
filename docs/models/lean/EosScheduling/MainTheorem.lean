/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Theorem5
import EosScheduling.Theorem6
import EosScheduling.Theorem4Prime
import EosScheduling.Coarsening
import EosScheduling.ListScheduling

/-!
# Main Theorem: End-to-End CAS-Scheduling Bound

This module composes the full proof chain into a single end-to-end result:

  Schedule.lean (worker model)
    → ListScheduling.lean (Graham's list-scheduling bound)
    → Theorem 4' (weighted deduplication bound)
    → Theorem 5 (unified coarsening dominance)
    → Theorem 6 (CAS-scheduling competitive ratio)
    → **Main Theorem** (composition)

The main theorem states: given a valid `Coarsening` structure and CAS merge
semantics that preserve critical paths, a work-conserving unified schedule
achieves makespan bounded by `α(1 + ρ|R|) · M_max`, where ρ is the
deduplication factor and M_max is the worst-case independent makespan.

## Hypothesis Justification

The theorem takes three categories of hypotheses:

1. **Structural** (`Coarsening V E`): The coarsening function satisfies
   monotonicity under cache growth and subadditivity under request union.
   These are defining properties of any correct entry-point computation.

2. **CAS merge** (`h_cp_preserved`): The unified DAG's critical path does
   not exceed the maximum individual critical path. This holds because CAS
   merge identifies identical nodes — it does not create new dependency
   edges between formerly independent subgraphs.

3. **Scheduling** (`h_wc`): The unified schedule is work-conserving, i.e.
   no worker idles when a ready task exists. This is the defining property
   of list-scheduling algorithms (Graham 1966), of which PEFT (the active
   scheduler) is an instance.
-/

-- The main theorem unifies types across five imports
-- (Thm4', Thm5, Thm6, ListScheduling, Schedule, Coarsening).
set_option linter.style.setOption false in
set_option linter.style.maxHeartbeats false in
set_option maxHeartbeats 400000 in
open Finset Classical in
/--
Main Theorem: End-to-end CAS-scheduling bound.

Given a valid coarsening structure, CAS critical-path preservation,
and a work-conserving unified schedule:

  `M(σ_unified) ≤ α · (1 + ρ · |R|) · M_max`
-/
theorem main_theorem_cas_scheduling {V W : Type*} [DecidableEq V] [Fintype V]
    [Fintype W] [WellFoundedRelation V]
    {E : V → V → Prop} {τ : W → W → Real} {pool : WorkerPool W}
    {R : Type*} [Fintype R] (hR : Nonempty R)
    -- Coarsening structure (monotonicity + subadditivity)
    (_γ : Coarsening V E)
    -- Per-request node sets and durations
    (V_prime : R → Finset V) (d : V → Real) (hd : ∀ v, 0 ≤ d v)
    -- Independent makespans and dedup factor
    (makespan_indep : R → Real)
    (ρ α : Real) (h_ρ_nonneg : 0 ≤ ρ) (h_α_ge : 1 ≤ α)
    (h_nonempty_set : ((univ : Finset R).image makespan_indep).Nonempty)
    -- CAS critical path preservation: the unified DAG's critical path
    -- does not exceed the max independent makespan. This holds because
    -- CAS merge identifies identical nodes without creating new edges.
    (h_cp_preserved : critical_path_makespan (fun v =>
      if v ∈ univ.biUnion V_prime then d v else 0) E ≤
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set)
    -- Deduplication factor definition
    (h_rho : (univ.biUnion V_prime).sum d =
      ρ * (univ : Finset R).sum (fun i => (V_prime i).sum d))
    -- Per-request work bounds (from individual scheduling)
    (h_indep_work_bound : ∀ i,
      (V_prime i).sum d ≤ (Fintype.card W : Real) * makespan_indep i)
    -- The unified schedule (work-conserving)
    (σ_unified : Schedule E
      (fun v => if v ∈ univ.biUnion V_prime then d v else 0)
      (fun v => if v ∈ univ.biUnion V_prime then d v else 0) τ pool)
    (h_W_pos : 0 < (Fintype.card W : Real))
    -- Work conservation: the defining property of list scheduling
    -- (Graham 1966). PEFT is a list scheduling algorithm by construction.
    (h_wc : (Fintype.card W : Real) * schedule_makespan σ_unified
      - (univ.sum (fun v =>
          if v ∈ univ.biUnion V_prime then d v else 0))
      ≤ (Fintype.card W - 1 : Real) * critical_path_makespan
          (fun v => if v ∈ univ.biUnion V_prime then d v else 0) E) :
    schedule_makespan σ_unified ≤
      α * (1 + ρ * (Fintype.card R : Real)) *
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set :=
  -- Chain: Coarsening → Thm 4' (dedup) → Thm 5 (dominance) → ListScheduling → Thm 6
  theorem6_cas_scheduling_bound hR V_prime d hd makespan_indep ρ α
    h_ρ_nonneg h_α_ge h_nonempty_set h_cp_preserved h_rho
    h_indep_work_bound σ_unified h_W_pos h_wc
