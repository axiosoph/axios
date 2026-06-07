/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Theorem4
import Mathlib.Tactic.Linarith
import Mathlib.Algebra.Order.BigOperators.Group.Finset

/-!
# Theorem 4': Weighted Structural Deduplication Savings

This module generalizes Theorem 4 to duration-weighted sums, proving the total
computation reduction from content-addressed storage deduplication.
-/

open Finset

theorem sum_biUnion_le {α β : Type*} [DecidableEq α]
    (s : Finset β) (f : β → Finset α) (d : α → ℝ) (hd : ∀ x, 0 ≤ d x) :
    (s.biUnion f).sum d ≤ s.sum (fun i => (f i).sum d) := by
  classical
  induction s using Finset.induction_on with
  | empty =>
    simp
  | insert a s ha ih =>
    rw [biUnion_insert, sum_insert ha]
    have h_union := sum_union_inter (s₁ := f a) (s₂ := s.biUnion f) (f := d)
    have h_inter_nonneg : 0 ≤ (f a ∩ s.biUnion f).sum d := Finset.sum_nonneg (fun x _ => hd x)
    linarith [ih]

-- Theorem 4': Weighted Structural Deduplication Inequality
theorem theorem4_prime_weighted_inequality {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V) (d : V → ℝ) (hd : ∀ v, 0 ≤ d v) :
    (univ.biUnion V_prime).sum d ≤ univ.sum (fun i => (V_prime i).sum d) := by
  classical
  exact sum_biUnion_le univ V_prime d hd

-- Theorem 4': Disjointness implies equality
theorem theorem4_prime_disjoint_implies_equality {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V) (d : V → ℝ)
    (h : ∀ i j, i ≠ j → Disjoint (V_prime i) (V_prime j)) :
    (univ.biUnion V_prime).sum d = univ.sum (fun i => (V_prime i).sum d) := by
  classical
  apply sum_biUnion
  intro x _ y _ hne
  exact h x y hne

lemma biUnion_sum_eq_sum_sum_imp_disjoint {V : Type*} [DecidableEq V]
    {R : Type*} (s : Finset R) (f : R → Finset V) (d : V → ℝ) (hd : ∀ x, 0 < d x)
    (h_eq : (s.biUnion f).sum d = s.sum (fun i => (f i).sum d)) :
    ∀ i ∈ s, ∀ j ∈ s, i ≠ j → Disjoint (f i) (f j) := by
  classical
  induction s using Finset.induction_on with
  | empty =>
    intro i hi
    simp at hi
  | insert a s ha ih =>
    rw [biUnion_insert, sum_insert ha] at h_eq
    have hd_nonneg : ∀ x, 0 ≤ d x := fun x => le_of_lt (hd x)
    have h_le : (s.biUnion f).sum d ≤ s.sum (fun i => (f i).sum d) :=
      sum_biUnion_le s f d hd_nonneg
    have h_union := sum_union_inter (s₁ := f a) (s₂ := s.biUnion f) (f := d)
    have h_arith : (s.biUnion f).sum d =
        s.sum (fun i => (f i).sum d) + (f a ∩ s.biUnion f).sum d := by
      linarith [h_eq, h_union]
    have h_inter_nonneg : 0 ≤ (f a ∩ s.biUnion f).sum d :=
      Finset.sum_nonneg (fun x _ => hd_nonneg x)
    have h_eq_ih : (s.biUnion f).sum d = s.sum (fun i => (f i).sum d) := by
      linarith [h_le, h_inter_nonneg, h_arith]
    have h_inter_zero : (f a ∩ s.biUnion f).sum d = 0 := by
      linarith [h_le, h_inter_nonneg, h_arith]
    have h_inter_empty : f a ∩ s.biUnion f = ∅ := by
      by_contra hc
      obtain ⟨x, hx⟩ := Finset.nonempty_of_ne_empty hc
      have h_mem : x ∈ f a ∩ s.biUnion f := hx
      have h_zero : d x = 0 := by
        have h_all_zero : ∀ y ∈ f a ∩ s.biUnion f, d y = 0 := by
          apply (Finset.sum_eq_zero_iff_of_nonneg (fun y _ => hd_nonneg y)).mp h_inter_zero
        exact h_all_zero x h_mem
      have h_pos : 0 < d x := hd x
      linarith
    have h_disj : Disjoint (f a) (s.biUnion f) := disjoint_iff_inter_eq_empty.mpr h_inter_empty
    intro i hi j hj hne
    rw [mem_insert] at hi hj
    rcases hi with rfl | hi
    · rcases hj with rfl | hj
      · contradiction
      · have h_sub : f j ⊆ s.biUnion f := subset_biUnion_of_mem f hj
        exact disjoint_of_subset_right h_disj h_sub
    · rcases hj with rfl | hj
      · have h_sub : f i ⊆ s.biUnion f := subset_biUnion_of_mem f hi
        exact (disjoint_of_subset_right h_disj h_sub).symm
      · exact ih h_eq_ih i hi j hj hne

-- Theorem 4': Equality implies disjointness
theorem theorem4_prime_equality_implies_disjoint {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V) (d : V → ℝ) (hd : ∀ v, 0 < d v)
    (h_eq : (univ.biUnion V_prime).sum d = univ.sum (fun i => (V_prime i).sum d)) :
    ∀ i j, i ≠ j → Disjoint (V_prime i) (V_prime j) := by
  intro i j hne
  exact biUnion_sum_eq_sum_sum_imp_disjoint univ V_prime d hd h_eq i (mem_univ i) j (mem_univ j) hne
