/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Theorem2
import Mathlib.Algebra.Order.BigOperators.Group.Finset

/-!
# Theorem 5: Unified Coarsening Dominance

This module formalizes and proves the Unified Coarsening Dominance theorem:
the makespan of the unified global DAG is bounded by the sum of makespans
of individual requests.
-/

open Finset

variable {V : Type*} [Fintype V] [wf : WellFoundedRelation V]

-- Definition of makespan of a set of nodes
noncomputable def makespan_of_set (d : V → Real) (τ : V → V → Real) (U : Finset V) : Real :=
  WithBot.unbotD 0 (U.image (completion_time d τ)).max

-- Helper: completion time of any node is non-negative
lemma completion_time_nonneg_global {d : V → Real} {τ : V → V → Real}
    (hd : ∀ v, 0 ≤ d v) (hτ : ∀ u v, 0 ≤ τ u v) (v : V) :
    0 ≤ completion_time d τ v := by
  exact completion_time_nonneg hd hτ v

-- Helper: makespan of a set is non-negative
lemma makespan_of_set_nonneg {d : V → Real} {τ : V → V → Real}
    (hd : ∀ v, 0 ≤ d v) (hτ : ∀ u v, 0 ≤ τ u v) (U : Finset V) :
    0 ≤ makespan_of_set d τ U := by
  dsimp [makespan_of_set]
  apply unbotD_max_nonneg
  intro x hx
  simp only [mem_image] at hx
  rcases hx with ⟨v, _, rfl⟩
  exact completion_time_nonneg_global hd hτ v

-- Theorem 5: Unified Coarsening Dominance Inequality
theorem theorem5_unified_dominance [DecidableEq V] {R : Type*} [Fintype R]
    (V_prime : R → Finset V) (d : V → Real) (τ : V → V → Real)
    (hd : ∀ v, 0 ≤ d v) (hτ : ∀ u v, 0 ≤ τ u v) :
    makespan_of_set d τ (univ.biUnion V_prime)
      ≤ univ.sum (fun i => makespan_of_set d τ (V_prime i)) := by
  classical
  have h_sum_nonneg : 0 ≤ univ.sum (fun i => makespan_of_set d τ (V_prime i)) := by
    apply Finset.sum_nonneg
    intro i _
    exact makespan_of_set_nonneg hd hτ (V_prime i)
  dsimp [makespan_of_set] at *
  rw [unbotD_max_le_iff h_sum_nonneg]
  intro x hx
  simp only [mem_image, mem_biUnion, mem_univ, true_and] at hx
  rcases hx with ⟨v, ⟨i, hv⟩, rfl⟩
  have h_mem_image : completion_time d τ v ∈ (V_prime i).image (completion_time d τ) := by
    simp only [mem_image]
    exact ⟨v, hv, rfl⟩
  have h_le_ms : completion_time d τ v
      ≤ WithBot.unbotD 0 ((V_prime i).image (completion_time d τ)).max := by
    exact unbotD_max_ge_self h_mem_image
  have h_le_sum : WithBot.unbotD 0 ((V_prime i).image (completion_time d τ)).max
      ≤ univ.sum (fun j => WithBot.unbotD 0 ((V_prime j).image (completion_time d τ)).max) :=
    Finset.single_le_sum (fun j _ => makespan_of_set_nonneg hd hτ (V_prime j)) (mem_univ i)
  linarith
