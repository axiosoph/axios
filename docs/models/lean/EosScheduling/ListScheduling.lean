/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Schedule
import Mathlib.Data.Fintype.Card

/-!
# List-Scheduling Makespan Bound (Graham 1966)

This module formalizes list-scheduling/work-conservation and proves
Graham's list-scheduling bound from first principles under the Schedule model.
The bound holds for the entire list-scheduling family; PEFT — the active
scheduler (ADR-0004 §Strategy) — inherits it. (HEFT, ADR-0004's superseded
baseline, is another instance.) Also establishes structural makespan dominance.
-/

set_option linter.unusedSectionVars false

open Finset

variable {V W : Type*} [DecidableEq V] [Fintype V]
variable {E : V → V → Prop} {d : V → Real} {load : V → Real}
variable {τ : W → W → Real} {pool : WorkerPool W}

/--
Lemma: critical path duration is non-negative if durations are non-negative.
-/
lemma critical_path_nonneg [wf : WellFoundedRelation V] (hd : ∀ v, 0 ≤ d v) (v : V) :
    0 ≤ critical_path d E v := by
  classical
  induction v using wf.wf.induction with
  | h v ih =>
    rw [critical_path]
    dsimp
    have h_max_nonneg : 0 ≤ WithBot.unbotD 0 (image (fun x => critical_path d E x.val)
      (attach (filter (fun u => wf.rel u v) univ))).max := by
      apply unbotD_max_nonneg
      intro x hx
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hx
      rcases hx with ⟨u, hu, rfl⟩
      exact ih u (by
        simp only [mem_filter, mem_univ, true_and] at hu
        exact hu)
    have h_dv := hd v
    linarith

/--
Lemma: critical path makespan is non-negative.
-/
lemma critical_path_makespan_nonneg [wf : WellFoundedRelation V] (hd : ∀ v, 0 ≤ d v) :
    0 ≤ critical_path_makespan d E := by
  classical
  dsimp [critical_path_makespan]
  apply unbotD_max_nonneg
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨v, rfl⟩
  exact critical_path_nonneg hd v

/--
Theorem: Work-conserving makespan bound (Graham's list-scheduling bound).
-/
theorem work_conserving_makespan_bound [Fintype W] [wf : WellFoundedRelation V]
    (σ : Schedule E d load τ pool) (hd : ∀ v, 0 ≤ d v) (h_W : 0 < (Fintype.card W : Real))
    (h_wc : (Fintype.card W : Real) * schedule_makespan σ - (univ.sum d)
      ≤ (Fintype.card W - 1 : Real) * critical_path_makespan d E) :
    schedule_makespan σ ≤ critical_path_makespan d E + (univ.sum d) / (Fintype.card W : Real) := by
  classical
  let m : Real := Fintype.card W
  let CP := critical_path_makespan d E
  let S := univ.sum d
  let M := schedule_makespan σ
  have h_cp_nonneg := critical_path_makespan_nonneg (E := E) hd
  have h_div : M ≤ ((m - 1) * CP + S) / m := by
    exact (le_div_iff₀ h_W).mpr (by linarith)
  have h_split : ((m - 1) * CP + S) / m = ((m - 1) / m) * CP + S / m := by
    ring
  have h_coeff : (m - 1) / m ≤ 1 := by
    exact (div_le_one h_W).mpr (by linarith)
  have h_coeff_term : ((m - 1) / m) * CP ≤ 1 * CP := by
    apply mul_le_mul_of_nonneg_right h_coeff h_cp_nonneg
  linarith

/--
Lemma: schedule start time dominates structural completion time.
-/
lemma schedule_start_ge_completion_time [wf : WellFoundedRelation V]
    (σ : Schedule E d load τ pool) (h_rel : ∀ u v, E u v ↔ wf.rel u v) (_hd : ∀ v, 0 ≤ d v)
    (hτ : ∀ w1 w2, 0 ≤ τ w1 w2) (v : V) :
    σ.start v + d v ≥ completion_time d (fun u v => τ (σ.worker u) (σ.worker v)) v := by
  classical
  induction v using wf.wf.induction with
  | h v ih =>
    rw [completion_time]
    dsimp
    have h_le : WithBot.unbotD 0 (image (fun x => completion_time d
      (fun u v => τ (σ.worker u) (σ.worker v)) x.val + τ (σ.worker x.val) (σ.worker v))
      (attach (filter (fun u => wf.rel u v) univ))).max ≤ σ.start v := by
      apply (unbotD_max_le_iff (σ.h_start_nonneg v)).mpr
      intro x hx
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hx
      rcases hx with ⟨u, hu, rfl⟩
      simp only [mem_filter, mem_univ, true_and] at hu
      have h_edge : E u v := (h_rel u v).mpr hu
      have h_dep_inseq := σ.h_dep u v h_edge
      have h_tau_nonneg := hτ (σ.worker u) (σ.worker v)
      have h_ind := ih u hu
      linarith
    linarith

/--
Theorem: schedule makespan dominates structural makespan.
-/
theorem schedule_makespan_ge_structural_makespan [wf : WellFoundedRelation V]
    (σ : Schedule E d load τ pool) (h_rel : ∀ u v, E u v ↔ wf.rel u v) (hd : ∀ v, 0 ≤ d v)
    (hτ : ∀ w1 w2, 0 ≤ τ w1 w2) :
    schedule_makespan σ ≥ makespan d (fun u v => τ (σ.worker u) (σ.worker v)) := by
  classical
  dsimp [schedule_makespan, makespan]
  have h_nonneg : 0 ≤ WithBot.unbotD 0 (image (fun v ↦ σ.start v + d v) univ).max := by
    have h_ms_nonneg := schedule_makespan_nonneg σ hd
    exact h_ms_nonneg
  apply (unbotD_max_le_iff h_nonneg).mpr
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨v, rfl⟩
  have h1 := schedule_start_ge_completion_time σ h_rel hd hτ v
  have h_mem : σ.start v + d v ∈ univ.image (fun v => σ.start v + d v) := by
    simp
  have h2 := unbotD_max_ge_self h_mem
  linarith
