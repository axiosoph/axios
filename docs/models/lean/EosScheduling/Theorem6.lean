/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Theorem4Prime
import Mathlib.Data.Finset.Max
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring


/-!
# Theorem 6: CAS-Scheduling Bound

This module contains the formal proof of Theorem 6, connecting content-addressed
storage deduplication (parameterized by the deduplication factor ρ) to the
makespan bound of the unified scheduler.
-/

open Finset

open Classical in
theorem theorem6_cas_scheduling_bound {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (_ : Nonempty R)
    (V_prime : R → Finset V) (d : V → ℝ) (_hd : ∀ v, 0 ≤ d v)
    (makespan_indep : R → ℝ)
    (critical_path_u total_work_u : ℝ)
    (W_card : ℝ) (h_W_pos : 0 < W_card)
    (ρ α : ℝ) (h_ρ_nonneg : 0 ≤ ρ) (h_α_nonneg : 0 ≤ α)
    (h_nonempty_set : ((univ : Finset R).image makespan_indep).Nonempty)
    (h_critical_path : critical_path_u ≤
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set)
    (h_total_work : total_work_u = (univ.biUnion V_prime).sum d)
    (h_rho : (univ.biUnion V_prime).sum d = ρ * (univ : Finset R).sum (fun i => (V_prime i).sum d))
    (h_indep_work_bound : ∀ i, (V_prime i).sum d ≤ W_card * makespan_indep i)
    (makespan_u : ℝ)
    (h_unified_bound : makespan_u ≤ α * (critical_path_u + total_work_u / W_card)) :
    makespan_u ≤ α * (1 + ρ * (univ : Finset R).card) *
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set := by
  -- Let M_max be the maximum independent makespan
  let M_max := ((univ : Finset R).image makespan_indep).max'
    h_nonempty_set
  have h_M_max_ge : ∀ i, makespan_indep i ≤ M_max := by
    intro i
    have hi : makespan_indep i ∈ ((univ : Finset R).image makespan_indep) := by
      simp only [mem_image, mem_univ, true_and]
      use i
    exact le_max' ((univ : Finset R).image makespan_indep) (makespan_indep i) hi
  -- Bound the work of each request by W_card * M_max
  have h_work_le : ∀ i, (V_prime i).sum d ≤ W_card * M_max := by
    intro i
    have h1 := h_indep_work_bound i
    have h2 := h_M_max_ge i
    have h3 : 0 ≤ W_card := le_of_lt h_W_pos
    nlinarith
  -- Sum of work of all requests is ≤ |R| * W_card * M_max
  have h_sum_work_le : (univ : Finset R).sum (fun i => (V_prime i).sum d) ≤
      (univ : Finset R).card * (W_card * M_max) := by
    have h_card_sum : (univ : Finset R).sum (fun i => (V_prime i).sum d) ≤
        (univ : Finset R).sum (fun (_i : R) => W_card * M_max) := by
      apply sum_le_sum
      intro i _
      exact h_work_le i
    simp only [sum_const, nsmul_eq_mul] at h_card_sum
    linarith
  -- Total work of the union is ≤ ρ * |R| * W_card * M_max
  have h_total_work_le : total_work_u ≤ ρ * ((univ : Finset R).card * (W_card * M_max)) := by
    rw [h_total_work, h_rho]
    have h3 : 0 ≤ ρ := h_ρ_nonneg
    nlinarith
  -- Divide total work by W_card to get the average makespan bound:
  -- total_work_u / W_card ≤ ρ * |R| * M_max
  have h_avg_work_le : total_work_u / W_card ≤
      ρ * (univ : Finset R).card * M_max := by
    have h_div : total_work_u / W_card ≤
        (ρ * ((univ : Finset R).card * (W_card * M_max))) / W_card := by
      apply div_le_div_of_nonneg_right h_total_work_le (le_of_lt h_W_pos)
    have h_cancel : (ρ * ((univ : Finset R).card * (W_card * M_max))) / W_card =
        ρ * (univ : Finset R).card * M_max := by
      have h_pos : W_card ≠ 0 := ne_of_gt h_W_pos
      calc (ρ * ((univ : Finset R).card * (W_card * M_max))) / W_card
        _ = (ρ * (univ : Finset R).card * M_max * W_card) / W_card := by ring
        _ = ρ * (univ : Finset R).card * M_max := mul_div_cancel_right₀ _ h_pos
    linarith
  -- Substitute into the unified bound:
  -- makespan_u ≤ α * (M_max + ρ * |R| * M_max) = α * (1 + ρ * |R|) * M_max
  have h_makespan_le_temp : makespan_u ≤
      α * (M_max + ρ * (univ : Finset R).card * M_max) := by
    have h_step : α * (critical_path_u + total_work_u / W_card) ≤
        α * (M_max + ρ * (univ : Finset R).card * M_max) := by
      have h_add : critical_path_u + total_work_u / W_card ≤
          M_max + ρ * (univ : Finset R).card * M_max := by
        linarith
      nlinarith
    linarith
  -- Rewrite the right hand side to matches the goal
  have h_rewrite : α * (M_max + ρ * (univ : Finset R).card * M_max) =
      α * (1 + ρ * (univ : Finset R).card) * M_max := by
    ring
  linarith





