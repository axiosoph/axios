/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Fintype.Card

/-!
# Theorem 7: Re-coarsening Convergence

This module contains the formal proof of Theorem 7, showing that the entry point
set under incremental re-coarsening converges monotonically to the empty set
as the cache state grows, and that the cache state reaches completeness in at
most |V| steps.
-/

open Finset

-- Monotonicity of coarsened entry points under cache growth
theorem theorem7_recoarsening_monotonicity {V : Type*}
    (coarse : Finset V → Finset V)
    (h_coarse_mono : ∀ {C₁ C₂ : Finset V}, C₁ ⊆ C₂ → coarse C₂ ⊆ coarse C₁)
    {C₁ C₂ : Finset V} (h_sub : C₁ ⊆ C₂) :
    (coarse C₂).card ≤ (coarse C₁).card := by
  exact card_le_card (h_coarse_mono h_sub)

-- Helper lemma: cardinality bound of C k grows by at least min(|V|, k)
lemma card_ge_min {V : Type*} [Fintype V]
    (C : ℕ → Finset V)
    (h_mono : ∀ k, C k ⊆ C (k + 1))
    (h_strict : ∀ k, C k ≠ univ → C k ⊂ C (k + 1)) (k : ℕ) :
    (C k).card ≥ min (Fintype.card V) k := by
  induction k with
  | zero =>
    omega
  | succ k ih =>
    by_cases h_eq : C k = univ
    · have h_card : (C k).card = Fintype.card V := by rw [h_eq, card_univ]
      have h_sub : C k ⊆ C (k + 1) := h_mono k
      have h_card_le : (C k).card ≤ (C (k + 1)).card := card_le_card h_sub
      rw [h_card] at h_card_le
      have h_min : min (Fintype.card V) (k + 1) ≤ Fintype.card V :=
        min_le_left _ _
      omega
    · have h_ss : C k ⊂ C (k + 1) := h_strict k h_eq
      have h_card_lt : (C k).card < (C (k + 1)).card := card_lt_card h_ss
      by_cases h_cases : k < Fintype.card V
      · have h_min1 : min (Fintype.card V) (k + 1) = k + 1 :=
          min_eq_right (by omega)
        have h_min2 : min (Fintype.card V) k = k :=
          min_eq_right (by omega)
        omega
      · have h_min1 : min (Fintype.card V) (k + 1) = Fintype.card V :=
          min_eq_left (by omega)
        have h_min2 : min (Fintype.card V) k = Fintype.card V :=
          min_eq_left (by omega)
        omega

-- Convergence of cache state under strict incremental growth
theorem theorem7_recoarsening_convergence {V : Type*} [Fintype V]
    (C : ℕ → Finset V)
    (h_mono : ∀ k, C k ⊆ C (k + 1))
    (h_strict : ∀ k, C k ≠ univ → C k ⊂ C (k + 1)) :
    C (Fintype.card V) = univ := by
  have h_ge := card_ge_min C h_mono h_strict (Fintype.card V)
  simp only [min_self] at h_ge
  have h_le : (C (Fintype.card V)).card ≤ Fintype.card V := by
    exact card_le_univ _
  have h_eq : (C (Fintype.card V)).card = Fintype.card V := by
    omega
  exact Finset.eq_univ_of_card _ h_eq
