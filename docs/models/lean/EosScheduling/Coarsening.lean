/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Schedule
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Fintype.Card

/-!
# Coarsening Function Model

This module formalizes the coarsening function as a structure, defining its
relationship to the EosModel, monotonicity under cache growth, and union
subadditivity. It also derives the re-coarsening convergence theorem
(Theorem 7) as a corollary of the Coarsening structure's properties.
-/

open Finset

/--
A `Coarsening` structure on a global DAG type `V` with edge relation `E`.
It defines a coarsening function `coarsen` mapping a request node set and a cache state
to a set of entry points, and a corresponding coverage relation `kappa`.
-/
structure Coarsening (V : Type*) [DecidableEq V] [Fintype V] (E : V → V → Prop) where
  coarsen : Finset V → Finset V → Finset V
  kappa : Finset V → Finset V → V → V → Prop

  -- The coarsen function produces a valid EosModel on V for any request U and cache C
  eos_model : Finset V → Finset V → EosModel V
  h_E : ∀ (U : Finset V) (C : Finset V), (eos_model U C).E = E
  h_S : ∀ (U : Finset V) (C : Finset V), (eos_model U C).S = coarsen U C
  h_κ : ∀ (U : Finset V) (C : Finset V), (eos_model U C).κ = kappa U C

  -- Monotonicity under cache growth
  monotone_cache : ∀ (U : Finset V) {C1 C2 : Finset V}, C1 ⊆ C2 → coarsen U C2 ⊆ coarsen U C1

  -- Subadditivity/deduplication under request union
  union_subadditive : ∀ {R : Type*} [Fintype R] (V_prime : R → Finset V) (C : Finset V),
    coarsen (univ.biUnion V_prime) C ⊆ univ.biUnion (fun i => coarsen (V_prime i) C)

/--
Corollary: entry point count is monotonically non-increasing under cache growth,
derived from a `Coarsening` instance.
-/
theorem coarsening_ep_monotone {V : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} (γ : Coarsening V E) (U : Finset V)
    {C₁ C₂ : Finset V} (h_sub : C₁ ⊆ C₂) :
    (γ.coarsen U C₂).card ≤ (γ.coarsen U C₁).card :=
  card_le_card (γ.monotone_cache U h_sub)

/--
Corollary: cache convergence from a `Coarsening` instance. If the cache grows
strictly at each step when not yet complete, it reaches completeness in at
most |V| steps.
-/
theorem coarsening_cache_convergence {V : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} (_γ : Coarsening V E)
    (C : ℕ → Finset V)
    (h_mono : ∀ k, C k ⊆ C (k + 1))
    (h_strict : ∀ k, C k ≠ univ → C k ⊂ C (k + 1)) :
    C (Fintype.card V) = univ := by
  have h_ge : ∀ k, (C k).card ≥ min (Fintype.card V) k := by
    intro k
    induction k with
    | zero => omega
    | succ k ih =>
      by_cases h_eq : C k = univ
      · have h_card : (C k).card = Fintype.card V := by rw [h_eq, card_univ]
        have h_card_le : (C k).card ≤ (C (k + 1)).card := card_le_card (h_mono k)
        rw [h_card] at h_card_le
        have h_min : min (Fintype.card V) (k + 1) ≤ Fintype.card V := min_le_left _ _
        omega
      · have h_card_lt : (C k).card < (C (k + 1)).card := card_lt_card (h_strict k h_eq)
        by_cases h_cases : k < Fintype.card V
        · have h_min1 : min (Fintype.card V) (k + 1) = k + 1 := min_eq_right (by omega)
          have h_min2 : min (Fintype.card V) k = k := min_eq_right (by omega)
          omega
        · have h_min1 : min (Fintype.card V) (k + 1) = Fintype.card V :=
            min_eq_left (by omega)
          omega
  have h_at_n := h_ge (Fintype.card V)
  simp only [min_self] at h_at_n
  have h_le : (C (Fintype.card V)).card ≤ Fintype.card V := card_le_univ _
  exact Finset.eq_univ_of_card _ (by omega)
