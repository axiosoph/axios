/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Schedule
import Mathlib.Algebra.Order.BigOperators.Group.Finset
import Mathlib.Tactic.Linarith


/-!
# Theorem 5: Unified Coarsening Dominance

This module formalizes and proves the Unified Coarsening Dominance theorem:
under the Schedule model, a unified coarsening (which has fewer scheduled nodes
due to deduplication) achieves equal or better makespan than the per-request coarsening.
-/

set_option linter.unusedSectionVars false

open Finset

variable {V W : Type*} [DecidableEq V]
variable {E : V → V → Prop} {τ : W → W → Real} {pool : WorkerPool W}
variable {d load : V → Real}


/--
Helper lemma to bound duration of unified coarsening by per-request coarsening.
-/
lemma d_unified_le_d_per (hd : ∀ v, 0 ≤ d v) {S_per S_unified : Finset V}
    (h_sub : S_unified ⊆ S_per) (v : V) :
    (if v ∈ S_unified then d v else 0) ≤ (if v ∈ S_per then d v else 0) := by
  split_ifs with h1 h2 h3
  · rfl
  · exfalso; exact h2 (h_sub h1)
  · exact hd v
  · rfl

/--
Helper lemma to bound load of unified coarsening by per-request coarsening.
-/
lemma load_unified_le_load_per (hload : ∀ v, 0 ≤ load v) {S_per S_unified : Finset V}
    (h_sub : S_unified ⊆ S_per) (v : V) :
    (if v ∈ S_unified then load v else 0) ≤ (if v ∈ S_per then load v else 0) := by
  split_ifs with h1 h2 h3
  · rfl
  · exfalso; exact h2 (h_sub h1)
  · exact hload v
  · rfl

/--
Restrict schedule helper.
-/
def restrict_schedule [Fintype V] (hd : ∀ v, 0 ≤ d v) (hload : ∀ v, 0 ≤ load v)
    {S_per S_unified : Finset V} (h_sub : S_unified ⊆ S_per)
    (σ_per : Schedule E (fun v => if v ∈ S_per then d v else 0)
      (fun v => if v ∈ S_per then load v else 0) τ pool) :
    Schedule E (fun v => if v ∈ S_unified then d v else 0)
      (fun v => if v ∈ S_unified then load v else 0) τ pool where
  worker := σ_per.worker
  start := σ_per.start
  h_start_nonneg := σ_per.h_start_nonneg
  h_dep := by
    intro u v h_edge
    have h_dep_per := σ_per.h_dep u v h_edge
    have h_d_le := d_unified_le_d_per hd h_sub u
    linarith
  h_cap := by
    classical
    intro w t
    have h_cap_per := σ_per.h_cap w t
    have h_sub_filter :
      (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_unified then d v else 0))
        ⊆ (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_per then d v else 0)) := by
      intro v hv
      simp only [mem_filter, mem_univ, true_and] at hv ⊢
      rcases hv with ⟨h_w, h_st, h_dt⟩
      have h_d_le := d_unified_le_d_per hd h_sub v
      exact ⟨h_w, h_st, by linarith⟩
    have h_sum_le1 :
      (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_unified then d v else 0)).sum
        (fun v => if v ∈ S_unified then load v else 0)
      ≤ (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_unified then d v else 0)).sum
        (fun v => if v ∈ S_per then load v else 0) := by
      apply sum_le_sum
      intro v _
      exact load_unified_le_load_per hload h_sub v
    have h_sum_le2 :
      (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_unified then d v else 0)).sum
        (fun v => if v ∈ S_per then load v else 0)
      ≤ (univ.filter (fun v => σ_per.worker v = w ∧ σ_per.start v ≤ t ∧
        t < σ_per.start v + if v ∈ S_per then d v else 0)).sum
        (fun v => if v ∈ S_per then load v else 0) := by
      apply sum_le_sum_of_subset_of_nonneg h_sub_filter
      intro v _ _
      split_ifs with h_mem
      · exact hload v
      · linarith
    linarith

/--
Theorem 5: Unified Coarsening Dominance
-/
theorem theorem5_unified_coarsening_dominance [Fintype V]
    (hd : ∀ v, 0 ≤ d v) (hload : ∀ v, 0 ≤ load v)
    {S_per S_unified : Finset V} (h_sub : S_unified ⊆ S_per)
    (σ_per : Schedule E (fun v => if v ∈ S_per then d v else 0)
      (fun v => if v ∈ S_per then load v else 0) τ pool) :
    schedule_makespan (restrict_schedule hd hload h_sub σ_per)
      ≤ schedule_makespan σ_per := by
  classical
  dsimp [schedule_makespan, restrict_schedule]
  have h_nonneg : 0 ≤ WithBot.unbotD 0 (image (fun v => σ_per.start v +
    (if v ∈ S_per then d v else 0)) univ).max := by
    have h_ms_nonneg := schedule_makespan_nonneg σ_per
      (fun v => by
        split_ifs
        · exact hd v
        · rfl)
    exact h_ms_nonneg
  apply (unbotD_max_le_iff h_nonneg).mpr
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨v, rfl⟩
  have h_mem : σ_per.start v + (if v ∈ S_per then d v else 0) ∈
    image (fun v => σ_per.start v + if v ∈ S_per then d v else 0) univ := by
    simp
  have h_max := unbotD_max_ge_self h_mem
  have h_d_le := d_unified_le_d_per hd h_sub v
  linarith
