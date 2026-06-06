/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring

/-!
# Theorem 3: Robustness Bound

This module formalizes the scoring stability under bounded perturbations (Lemma 3.1)
and the EMA decay convergence under sustained error.
-/

-- Lemma 3.1: Agreement Condition
theorem lemma_3_1_agreement {W : Type*}
    (score_base score : W → Real) (w_B : W) (Δ P : Real)
    (h_gap : ∀ w ≠ w_B, score_base w_B - score_base w ≥ Δ)
    (h_pert : ∀ w, |score w - score_base w| ≤ P)
    (h_cond : 2 * P < Δ)
    (w_H : W) (h_max : ∀ w, score w ≤ score w_H) :
    w_H = w_B := by
  by_contra h_ne
  have h_gap_w_H := h_gap w_H h_ne
  have h_pert_w_H := h_pert w_H
  have h_pert_w_B := h_pert w_B
  have h_max_w_B := h_max w_B
  rw [abs_le] at h_pert_w_H h_pert_w_B
  linarith

-- EMA definition
def ema (γ : Real) (η : Nat → Real) (E0 : Real) : Nat → Real
  | 0 => E0
  | n + 1 => (1 - γ) * η (n + 1) + γ * ema γ η E0 n

-- EMA lower bound under sustained error
theorem ema_lower_bound (γ : Real) (η : Nat → Real) (E0 : Real) (η_0 : Real)
    (h_gam_pos : 0 ≤ γ) (h_gam_le : γ ≤ 1) (h_eta : ∀ n, η_0 ≤ η n) (n : Nat) :
    (1 - γ^n) * η_0 + γ^n * E0 ≤ ema γ η E0 n := by
  induction n with
  | zero =>
    simp [ema]
  | succ n ih =>
    rw [ema]
    have h1 : 0 ≤ 1 - γ := by linarith
    have h2 : (1 - γ) * η_0 ≤ (1 - γ) * η (n + 1) := mul_le_mul_of_nonneg_left (h_eta (n + 1)) h1
    have h3 : γ * ((1 - γ^n) * η_0 + γ^n * E0) ≤ γ * ema γ η E0 n :=
      mul_le_mul_of_nonneg_left ih h_gam_pos
    have h_calc : (1 - γ) * η_0 + γ * ((1 - γ^n) * η_0 + γ^n * E0) =
        (1 - γ^(n + 1)) * η_0 + γ^(n + 1) * E0 := by
      rw [pow_succ]
      ring
    linarith
