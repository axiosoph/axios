/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Theorem4Prime
import EosScheduling.Schedule
import EosScheduling.ListScheduling
import Mathlib.Data.Finset.Max
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring

/-!
# Theorem 6: CAS-Scheduling Bound

This module contains the formal proof of Theorem 6, connecting content-addressed
storage deduplication (parameterized by the deduplication factor ρ) to the
makespan bound of the unified scheduler.
-/

set_option linter.unusedSectionVars false

open Finset

lemma sum_indicator_eq_sum_subset {V : Type*} [DecidableEq V] [Fintype V]
    (S : Finset V) (d : V → Real) :
    (univ.sum (fun v => if v ∈ S then d v else 0)) = S.sum d := by
  classical
  have h1 : S.sum (fun v => if v ∈ S then d v else 0) = S.sum d := by
    apply sum_congr rfl
    intro x hx
    simp only [hx, if_true]
  rw [← h1]
  symm
  apply sum_subset (s₁ := S) (s₂ := univ) (subset_univ S)
  intro x _ hx
  simp only [hx, if_false]

open Classical in
theorem theorem6_cas_scheduling_bound {V W : Type*} [DecidableEq V] [Fintype V]
    [Fintype W] [wf : WellFoundedRelation V]
    {E : V → V → Prop} {τ : W → W → Real} {pool : WorkerPool W}
    {R : Type*} [Fintype R] (_ : Nonempty R)
    (V_prime : R → Finset V) (d : V → Real) (hd : ∀ v, 0 ≤ d v)
    (makespan_indep : R → Real)
    (ρ α : Real) (h_ρ_nonneg : 0 ≤ ρ) (h_α_ge : 1 ≤ α)
    (h_nonempty_set : ((univ : Finset R).image makespan_indep).Nonempty)
    (h_critical_path : critical_path_makespan (fun v =>
      if v ∈ univ.biUnion V_prime then d v else 0) E ≤
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set)
    (h_rho : (univ.biUnion V_prime).sum d = ρ * (univ : Finset R).sum (fun i => (V_prime i).sum d))
    (h_indep_work_bound : ∀ i, (V_prime i).sum d ≤ (Fintype.card W : Real) * makespan_indep i)
    (σ_unified : Schedule E (fun v => if v ∈ univ.biUnion V_prime then d v else 0)
      (fun v => if v ∈ univ.biUnion V_prime then d v else 0) τ pool)
    (h_W_pos : 0 < (Fintype.card W : Real))
    (h_wc : (Fintype.card W : Real) * schedule_makespan σ_unified
      - (univ.sum (fun v => if v ∈ univ.biUnion V_prime then d v else 0))
      ≤ (Fintype.card W - 1 : Real) * critical_path_makespan (fun v =>
        if v ∈ univ.biUnion V_prime then d v else 0) E) :
    schedule_makespan σ_unified ≤ α * (1 + ρ * (Fintype.card R : Real)) *
      ((univ : Finset R).image makespan_indep).max' h_nonempty_set := by
  -- Let M_max be the maximum independent makespan
  let M_max := ((univ : Finset R).image makespan_indep).max' h_nonempty_set
  let W_card : Real := Fintype.card W
  let R_card : Real := Fintype.card R
  let total_work_u := (univ.biUnion V_prime).sum d
  let critical_path_u := critical_path_makespan (fun v =>
    if v ∈ univ.biUnion V_prime then d v else 0) E
  -- Deriving the unified list-scheduling bound from ListScheduling.lean
  have h_list_sched_bound : schedule_makespan σ_unified ≤ critical_path_u +
      (univ.sum (fun v => if v ∈ univ.biUnion V_prime then d v else 0)) / W_card := by
    apply work_conserving_makespan_bound σ_unified _ h_W_pos h_wc
    intro v
    split_ifs
    · exact hd v
    · rfl
  have h_sum_eq := sum_indicator_eq_sum_subset (univ.biUnion V_prime) d
  have h_cp_u_nonneg : 0 ≤ critical_path_u := by
    classical
    apply critical_path_makespan_nonneg (E := E)
    intro v
    split_ifs
    · exact hd v
    · rfl
  have h_unified_bound : schedule_makespan σ_unified ≤
      α * (critical_path_u + total_work_u / W_card) := by
    have h_sum_nonneg : 0 ≤ total_work_u := sum_nonneg (fun v _ => hd v)
    have h_term_nonneg : 0 ≤ critical_path_u + total_work_u / W_card := by
      have h_div_nonneg : 0 ≤ total_work_u / W_card :=
        div_nonneg h_sum_nonneg (le_of_lt h_W_pos)
      linarith
    have h_rw : (univ.sum (fun v => if v ∈ univ.biUnion V_prime then d v else 0))
        = total_work_u := h_sum_eq
    rw [h_rw] at h_list_sched_bound
    have h_le_mul : critical_path_u + total_work_u / W_card ≤
        α * (critical_path_u + total_work_u / W_card) := by
      nth_rw 1 [← one_mul (critical_path_u + total_work_u / W_card)]
      apply mul_le_mul_of_nonneg_right h_α_ge h_term_nonneg
    linarith
  -- Bound the work of each request by W_card * M_max
  have h_M_max_ge : ∀ i, makespan_indep i ≤ M_max := by
    intro i
    have hi : makespan_indep i ∈ ((univ : Finset R).image makespan_indep) := by
      simp only [mem_image, mem_univ, true_and]
      use i
    exact le_max' ((univ : Finset R).image makespan_indep) (makespan_indep i) hi
  have h_work_le : ∀ i, (V_prime i).sum d ≤ W_card * M_max := by
    intro i
    have h1 := h_indep_work_bound i
    have h2 := h_M_max_ge i
    have h3 : 0 ≤ W_card := le_of_lt h_W_pos
    nlinarith
  -- Sum of work of all requests is ≤ |R| * W_card * M_max
  have h_sum_work_le : (univ : Finset R).sum (fun i => (V_prime i).sum d) ≤
      R_card * (W_card * M_max) := by
    have h_card_sum : (univ : Finset R).sum (fun i => (V_prime i).sum d) ≤
        (univ : Finset R).sum (fun (_i : R) => W_card * M_max) := by
      apply sum_le_sum
      intro i _
      exact h_work_le i
    simp only [sum_const, nsmul_eq_mul] at h_card_sum
    exact h_card_sum
  -- Total work of the union is ≤ ρ * |R| * W_card * M_max
  have h_total_work_le : (univ.biUnion V_prime).sum d ≤
      ρ * (R_card * (W_card * M_max)) := by
    rw [h_rho]
    have h3 : 0 ≤ ρ := h_ρ_nonneg
    nlinarith
  have h_total_work_le_u : total_work_u ≤ ρ * (R_card * (W_card * M_max)) :=
    h_total_work_le
  -- Divide total work by W_card to get the average makespan bound:
  -- total_work_u / W_card ≤ ρ * |R| * M_max
  have h_avg_work_le : total_work_u / W_card ≤ ρ * R_card * M_max := by
    have h_div : total_work_u / W_card ≤
        (ρ * (R_card * (W_card * M_max))) / W_card := by
      apply div_le_div_of_nonneg_right h_total_work_le_u (le_of_lt h_W_pos)
    have h_cancel : (ρ * (R_card * (W_card * M_max))) / W_card =
        ρ * R_card * M_max := by
      have h_pos : W_card ≠ 0 := ne_of_gt h_W_pos
      calc (ρ * (R_card * (W_card * M_max))) / W_card
        _ = (ρ * R_card * M_max * W_card) / W_card := by ring
        _ = ρ * R_card * M_max := mul_div_cancel_right₀ _ h_pos
    linarith
  -- Substitute into the unified bound:
  -- makespan_u ≤ α * (M_max + ρ * |R| * M_max) = α * (1 + ρ * |R|) * M_max
  have h_makespan_le_temp : schedule_makespan σ_unified ≤
      α * (M_max + ρ * R_card * M_max) := by
    have h_step : α * (critical_path_u + total_work_u / W_card) ≤
        α * (M_max + ρ * R_card * M_max) := by
      have h_add : critical_path_u + total_work_u / W_card ≤
          M_max + ρ * R_card * M_max := by
        linarith
      have h_alpha_nonneg : 0 ≤ α := by linarith
      nlinarith
    linarith
  -- Rewrite the right hand side to match the goal
  have h_rewrite : α * (M_max + ρ * R_card * M_max) =
      α * (1 + ρ * R_card) * M_max := by
    ring
  linarith
