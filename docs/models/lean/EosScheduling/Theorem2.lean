/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import Mathlib.Data.Finset.Max
import Mathlib.Order.WithBot
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring

/-!
# Theorem 2: Consistency Bound

This module formalizes the makespan consistency bound under epsilon-accurate predictions
for any finite DAG.
-/

open Finset

variable {S : Type*} [Fintype S] [wf : WellFoundedRelation S]

-- Completion time recurrence
open Classical in
noncomputable def completion_time (d : S → Real) (τ : S → S → Real) (s : S) : Real :=
  let preds := filter (fun s' => wf.rel s' s) univ
  let vals := preds.attach.image (fun ⟨s', h⟩ =>
    have : wf.rel s' s := by
      rcases (mem_filter.mp h) with ⟨_, h1⟩
      exact h1
    completion_time d τ s' + τ s' s)
  WithBot.unbotD 0 vals.max + d s
termination_by s

noncomputable def makespan (d : S → Real) (τ : S → S → Real) : Real :=
  WithBot.unbotD 0 (univ.image (completion_time d τ)).max

lemma unbotD_max_le_iff {α : Type*} [LinearOrder α] [Zero α] {s : Finset α} {M : α}
    (hM : 0 ≤ M) :
    WithBot.unbotD 0 s.max ≤ M ↔ ∀ x ∈ s, x ≤ M := by
  have h_cond : s.max = ⊥ → 0 ≤ M := by
    intro _
    exact hM
  rw [WithBot.unbotD_le_iff h_cond]
  rw [Finset.max_le_iff]
  simp only [WithBot.coe_le_coe]

lemma unbotD_max_eq_max' {α : Type*} [LinearOrder α] {s : Finset α} (hs : s.Nonempty) (d : α) :
    WithBot.unbotD d s.max = s.max' hs := by
  have h_eq : s.max = ↑(s.max' hs) := by
    exact (WithBot.coe_unbot s.max (Finset.max_eq_bot.not.mpr hs.ne_empty)).symm
  rw [h_eq]
  exact WithBot.unbotD_coe d (s.max' hs)

lemma unbotD_max_nonneg {α : Type*} [LinearOrder α] [Zero α] {s : Finset α}
    (h_nonneg : ∀ x ∈ s, 0 ≤ x) :
    0 ≤ WithBot.unbotD 0 s.max := by
  rcases s.eq_empty_or_nonempty with rfl | hs
  · simp
  · rw [unbotD_max_eq_max' hs]
    exact h_nonneg _ (max'_mem _ hs)

lemma unbotD_max_ge_self {α : Type*} [LinearOrder α] [Zero α] {s : Finset α} {y : α}
    (hy : y ∈ s) :
    y ≤ WithBot.unbotD 0 s.max := by
  have hs : s.Nonempty := ⟨y, hy⟩
  rw [unbotD_max_eq_max' hs]
  exact le_max' s y hy

lemma completion_time_nonneg {d : S → Real} {τ : S → S → Real}
    (hd : ∀ s, 0 ≤ d s) (hτ : ∀ s' s, 0 ≤ τ s' s) (s : S) :
    0 ≤ completion_time d τ s := by
  classical
  induction s using wf.wf.induction with
  | h s ih =>
    rw [completion_time]
    have h_nonneg : ∀ x ∈ (filter (fun s' => wf.rel s' s) univ).attach.image (fun ⟨s', h⟩ =>
      have : wf.rel s' s := by
        rcases (mem_filter.mp h) with ⟨_, h1⟩
        exact h1
      completion_time d τ s' + τ s' s), 0 ≤ x := by
      intro x hx
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hx
      rcases hx with ⟨s', h_rel, rfl⟩
      have : wf.rel s' s := by
        rcases (mem_filter.mp h_rel) with ⟨_, h1⟩
        exact h1
      have ih_s' := ih s' this
      have hτ_s' := hτ s' s
      linarith
    have h_max_nonneg := unbotD_max_nonneg h_nonneg
    have hd_s := hd s
    linarith

lemma completion_time_le_of_eps {d d_hat : S → Real} {τ : S → S → Real} {ε : Real}
    (h_eps : ∀ s, d s ≤ (1 + ε) * d_hat s)
    (h_eps_pos : 0 ≤ ε)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (s : S) :
    completion_time d τ s ≤ (1 + ε) * completion_time d_hat τ s := by
  classical
  induction s using wf.wf.induction with
  | h s ih =>
    rw [completion_time, completion_time]
    let preds := filter (fun s' => wf.rel s' s) univ
    let vals_d := preds.attach.image (fun ⟨s', h⟩ =>
      have : wf.rel s' s := by
        rcases (mem_filter.mp h) with ⟨_, h1⟩
        exact h1
      completion_time d τ s' + τ s' s)
    let vals_d_hat := preds.attach.image (fun ⟨s', h⟩ =>
      have : wf.rel s' s := by
        rcases (mem_filter.mp h) with ⟨_, h1⟩
        exact h1
      completion_time d_hat τ s' + τ s' s)
    have h_vals_d_hat_nonneg : ∀ y ∈ vals_d_hat, 0 ≤ y := by
      intro y hy
      dsimp only [vals_d_hat] at hy
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hy
      rcases hy with ⟨s', h_rel, rfl⟩
      have : wf.rel s' s := by
        rcases (mem_filter.mp h_rel) with ⟨_, h1⟩
        exact h1
      have h_ct_nonneg := completion_time_nonneg h_d_hat_nonneg h_tau_nonneg s'
      have h_t_nonneg := h_tau_nonneg s' s
      linarith
    have h_max_d_hat_nonneg := unbotD_max_nonneg h_vals_d_hat_nonneg
    have h_bound_nonneg : 0 ≤ (1 + ε) * WithBot.unbotD 0 vals_d_hat.max := by
      have : 0 ≤ 1 + ε := by linarith
      exact mul_nonneg this h_max_d_hat_nonneg
    have h_d_le := h_eps s
    have h_max_le : WithBot.unbotD 0 vals_d.max ≤ (1 + ε) * WithBot.unbotD 0 vals_d_hat.max := by
      rw [unbotD_max_le_iff h_bound_nonneg]
      intro x hx
      dsimp only [vals_d] at hx
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hx
      rcases hx with ⟨s', h_rel, rfl⟩
      have h_rel' : wf.rel s' s := by
        rcases (mem_filter.mp h_rel) with ⟨_, h1⟩
        exact h1
      have ih_s' := ih s' h_rel'
      have h_y_mem : completion_time d_hat τ s' + τ s' s ∈ vals_d_hat := by
        exact mem_image_of_mem (fun ⟨s', h⟩ => completion_time d_hat τ s' + τ s' s)
          (mem_attach preds ⟨s', h_rel⟩)
      have h_y_le := unbotD_max_ge_self h_y_mem
      have h_t_nonneg := h_tau_nonneg s' s
      have h_step1 : (1 + ε) * (completion_time d_hat τ s' + τ s' s) ≤
          (1 + ε) * WithBot.unbotD 0 vals_d_hat.max := by
        nlinarith
      have h_step2 : (1 + ε) * completion_time d_hat τ s' + τ s' s ≤
          (1 + ε) * (completion_time d_hat τ s' + τ s' s) := by
        nlinarith
      linarith
    linarith

lemma completion_time_ge_of_eps {d d_hat : S → Real} {τ : S → S → Real} {ε : Real}
    (h_eps : ∀ s, (1 - ε) * d_hat s ≤ d s)
    (h_eps_pos : 0 ≤ ε)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (s : S) :
    (1 - ε) * completion_time d_hat τ s ≤ completion_time d τ s := by
  classical
  induction s using wf.wf.induction with
  | h s ih =>
    rw [completion_time, completion_time]
    let preds := filter (fun s' => wf.rel s' s) univ
    let vals_d := preds.attach.image (fun ⟨s', h⟩ =>
      have : wf.rel s' s := by
        rcases (mem_filter.mp h) with ⟨_, h1⟩
        exact h1
      completion_time d τ s' + τ s' s)
    let vals_d_hat := preds.attach.image (fun ⟨s', h⟩ =>
      have : wf.rel s' s := by
        rcases (mem_filter.mp h) with ⟨_, h1⟩
        exact h1
      completion_time d_hat τ s' + τ s' s)
    have h_d_ge := h_eps s
    rcases vals_d_hat.eq_empty_or_nonempty with h_empty | h_nonempty
    · have h_vals_d_empty : vals_d = ∅ := by
        rw [image_eq_empty] at h_empty
        rw [image_eq_empty]
        exact h_empty
      dsimp only [vals_d, vals_d_hat] at *
      rw [h_empty, h_vals_d_empty]
      simp only [max_empty, WithBot.unbotD_bot]
      linarith
    · have h_vals_d_hat_nonneg : ∀ y ∈ vals_d_hat, 0 ≤ y := by
        intro y hy
        dsimp only [vals_d_hat] at hy
        simp only [mem_image, mem_attach, true_and, Subtype.exists] at hy
        rcases hy with ⟨s', h_rel, rfl⟩
        have : wf.rel s' s := by
          rcases (mem_filter.mp h_rel) with ⟨_, h1⟩
          exact h1
        have h_ct_nonneg := completion_time_nonneg h_d_hat_nonneg h_tau_nonneg s'
        have h_t_nonneg := h_tau_nonneg s' s
        linarith
      have h_max_d_hat_nonneg := unbotD_max_nonneg h_vals_d_hat_nonneg
      have h_max_eq : WithBot.unbotD 0 vals_d_hat.max = vals_d_hat.max' h_nonempty :=
        unbotD_max_eq_max' h_nonempty 0
      have hy_mem : vals_d_hat.max' h_nonempty ∈ vals_d_hat := max'_mem vals_d_hat h_nonempty
      dsimp only [vals_d_hat] at hy_mem
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hy_mem
      rcases hy_mem with ⟨s', h_rel, h_y_eq⟩
      have h_rel' : wf.rel s' s := by
        rcases (mem_filter.mp h_rel) with ⟨_, h1⟩
        exact h1
      have ih_s' := ih s' h_rel'
      have h_x_mem : completion_time d τ s' + τ s' s ∈ vals_d := by
        exact mem_image_of_mem (fun ⟨s', h⟩ => completion_time d τ s' + τ s' s)
          (mem_attach preds ⟨s', h_rel⟩)
      have h_x_le := unbotD_max_ge_self h_x_mem
      have h_t_nonneg := h_tau_nonneg s' s
      have h_calc : (1 - ε) * (completion_time d_hat τ s' + τ s' s) ≤ completion_time d τ s' + τ s' s := by
        have : (1 - ε) * τ s' s ≤ τ s' s := by nlinarith
        linarith
      dsimp only [vals_d, vals_d_hat] at *
      rw [h_max_eq, ← h_y_eq]
      linarith

lemma makespan_le_of_eps {d d_hat : S → Real} {τ : S → S → Real} {ε : Real}
    (h_eps : ∀ s, d s ≤ (1 + ε) * d_hat s)
    (h_eps_pos : 0 ≤ ε)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s) :
    makespan d τ ≤ (1 + ε) * makespan d_hat τ := by
  classical
  rw [makespan, makespan]
  have h_d_hat_ms_nonneg : 0 ≤ WithBot.unbotD 0 (univ.image (completion_time d_hat τ)).max := by
    have h_nonneg : ∀ x ∈ univ.image (completion_time d_hat τ), 0 ≤ x := by
      intro x hx
      simp only [mem_image, mem_univ, true_and] at hx
      rcases hx with ⟨s, rfl⟩
      exact completion_time_nonneg h_d_hat_nonneg h_tau_nonneg s
    exact unbotD_max_nonneg h_nonneg
  have h_bound_nonneg : 0 ≤ (1 + ε) * WithBot.unbotD 0 (univ.image (completion_time d_hat τ)).max := by
    have : 0 ≤ 1 + ε := by linarith
    exact mul_nonneg this h_d_hat_ms_nonneg
  rw [unbotD_max_le_iff h_bound_nonneg]
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨s, rfl⟩
  have h1 := completion_time_le_of_eps h_eps h_eps_pos h_tau_nonneg h_d_hat_nonneg s
  have h_mem : completion_time d_hat τ s ∈ univ.image (completion_time d_hat τ) := by
    simp
  have h2 := unbotD_max_ge_self h_mem
  have : 0 ≤ 1 + ε := by linarith
  nlinarith

lemma makespan_ge_of_eps {d d_hat : S → Real} {τ : S → S → Real} {ε : Real}
    (h_eps : ∀ s, (1 - ε) * d_hat s ≤ d s)
    (h_eps_pos : 0 ≤ ε)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (h_nonempty : Nonempty S) :
    (1 - ε) * makespan d_hat τ ≤ makespan d τ := by
  classical
  rw [makespan, makespan]
  let s_img := univ.image (completion_time d_hat τ)
  have h_img_nonempty : s_img.Nonempty := ⟨completion_time d_hat τ (Classical.choice h_nonempty),
    mem_image_of_mem (completion_time d_hat τ) (mem_univ _)⟩
  have h_max_eq : WithBot.unbotD 0 s_img.max = s_img.max' h_img_nonempty :=
    unbotD_max_eq_max' h_img_nonempty 0
  have hy_mem : s_img.max' h_img_nonempty ∈ s_img := max'_mem s_img h_img_nonempty
  dsimp only [s_img] at hy_mem
  simp only [mem_image, mem_univ, true_and] at hy_mem
  rcases hy_mem with ⟨s, h_y_eq⟩
  have h1 := completion_time_ge_of_eps h_eps h_eps_pos h_tau_nonneg h_d_hat_nonneg s
  have h_x_mem : completion_time d τ s ∈ univ.image (completion_time d τ) := by
    simp
  have h2 := unbotD_max_ge_self h_x_mem
  dsimp only [s_img] at h_max_eq
  rw [h_max_eq, ← h_y_eq]
  linarith

theorem theorem2_consistency_bound {d d_hat d_star d_hat_star : S → Real} {τ : S → S → Real} {ε α : Real}
    (h_eps_pos : 0 ≤ ε) (h_eps_lt : ε < 1)
    (h_alpha : 0 ≤ α)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (h_d_hat_star_nonneg : ∀ s, 0 ≤ d_hat_star s)
    (h_nonempty : Nonempty S)
    (h_err_h : ∀ s, |d s - d_hat s| ≤ ε * d_hat s)
    (h_err_star : ∀ s, |d_star s - d_hat_star s| ≤ ε * d_hat_star s)
    (h_approx : makespan d_hat τ ≤ α * makespan d_hat_star τ) :
    makespan d τ ≤ α * ((1 + ε) / (1 - ε)) * makespan d_star τ := by
  have h_eps_le : ∀ s, d s ≤ (1 + ε) * d_hat s := by
    intro s
    have h_abs := h_err_h s
    rw [abs_le] at h_abs
    linarith
  have h_eps_ge : ∀ s, (1 - ε) * d_hat_star s ≤ d_star s := by
    intro s
    have h_abs := h_err_star s
    rw [abs_le] at h_abs
    linarith
  have h_ms_le := makespan_le_of_eps h_eps_le h_eps_pos h_tau_nonneg h_d_hat_nonneg
  have h_ms_ge := makespan_ge_of_eps h_eps_ge h_eps_pos h_tau_nonneg h_d_hat_star_nonneg h_nonempty
  have h_eps_div : 0 < 1 - ε := by linarith
  have h_step1 : makespan d_hat_star τ ≤ (1 - ε)⁻¹ * makespan d_star τ := by
    have h_inv_pos : 0 ≤ (1 - ε)⁻¹ := inv_nonneg.mpr (by linarith)
    have h_mul := mul_le_mul_of_nonneg_left h_ms_ge h_inv_pos
    have h_ne : 1 - ε ≠ 0 := by linarith
    rw [← mul_assoc, inv_mul_cancel₀ h_ne, one_mul] at h_mul
    exact h_mul
  have h_step2 : makespan d_hat τ ≤ α * ((1 - ε)⁻¹ * makespan d_star τ) := by
    have h_mul := mul_le_mul_of_nonneg_left h_step1 h_alpha
    exact le_trans h_approx h_mul
  have h_step3 : makespan d τ ≤ (1 + ε) * (α * ((1 - ε)⁻¹ * makespan d_star τ)) := by
    have h_one_eps : 0 ≤ 1 + ε := by linarith
    have h_mul := mul_le_mul_of_nonneg_left h_step2 h_one_eps
    exact le_trans h_ms_le h_mul
  have h_eq : (1 + ε) * (α * ((1 - ε)⁻¹ * makespan d_star τ)) =
      α * ((1 + ε) / (1 - ε)) * makespan d_star τ := by
    ring
  rw [h_eq] at h_step3
  exact h_step3
