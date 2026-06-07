/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Theorem2

/-!
# Theorem 2': Adaptive Consistency Bound

This module proves the Adaptive Consistency Bound as a corollary of Theorem 2,
incorporating the adaptive coarsening quality function α(ε).
-/

open Finset

/-- Adaptive coarsening quality function properties.
  As prediction error ε increases, the approximation factor α(ε) is non-decreasing.
  Hence, it is non-increasing as quality improves, tightening toward α_heft at ε = 0. -/
structure AdaptiveCoarsening (α : Real → Real) (α_heft α_max : Real) : Prop where
  mono : ∀ x y, 0 ≤ x → x ≤ y → α x ≤ α y
  nonneg : ∀ x, 0 ≤ x → 0 ≤ α x
  at_zero : α 0 = α_heft
  at_one : α 1 ≤ α_max

variable {S : Type*} [Fintype S] [wf : WellFoundedRelation S]

theorem theorem2_prime_adaptive_consistency {d d_hat d_star d_hat_star : S → Real}
    {τ : S → S → Real} {ε : Real} (α : Real → Real) (α_heft α_max : Real)
    (h_ac : AdaptiveCoarsening α α_heft α_max)
    (h_eps_pos : 0 ≤ ε) (h_eps_lt : ε < 1)
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (h_d_hat_star_nonneg : ∀ s, 0 ≤ d_hat_star s)
    (h_nonempty : Nonempty S)
    (h_err_h : ∀ s, |d s - d_hat s| ≤ ε * d_hat s)
    (h_err_star : ∀ s, |d_star s - d_hat_star s| ≤ ε * d_hat_star s)
    (h_approx : makespan d_hat τ ≤ α ε * makespan d_hat_star τ) :
    makespan d τ ≤ α ε * ((1 + ε) / (1 - ε)) * makespan d_star τ := by
  have h_alpha_nonneg : 0 ≤ α ε := h_ac.nonneg ε h_eps_pos
  exact theorem2_consistency_bound h_eps_pos h_eps_lt h_alpha_nonneg
    h_tau_nonneg h_d_hat_nonneg h_d_hat_star_nonneg h_nonempty
    h_err_h h_err_star h_approx

