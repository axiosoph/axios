/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs

/-!
# Theorem 4: Singleflight Deduplication Savings

This module contains the proof of Theorem 4, showing that the singleflight map
achieves makespan savings bounded by the union card, with equality iff the tasks
are pairwise disjoint.
-/

open Finset

-- Theorem 4: Singleflight Deduplication Savings Inequality
theorem theorem4_inequality {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V) :
    (univ.biUnion V_prime).card ≤ univ.sum (fun i => (V_prime i).card) := by
  exact card_biUnion_le

-- Theorem 4: Disjointness implies equality (perfect savings when disjoint)
theorem theorem4_disjoint_implies_equality {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V)
    (h : ∀ i j, i ≠ j → Disjoint (V_prime i) (V_prime j)) :
    (univ.biUnion V_prime).card = univ.sum (fun i => (V_prime i).card) := by
  classical
  apply card_biUnion
  intro x _ y _ hne
  exact h x y hne

lemma nat_add_le_self {x y z : Nat} (h1 : x + y = z) (h2 : z ≤ x) : y = 0 ∧ z = x := by
  omega

lemma disjoint_of_subset_right {α : Type*} {A B C : Finset α}
    (h : Disjoint A B) (hC : C ⊆ B) : Disjoint A C := by
  classical
  rw [disjoint_iff_ne] at *
  intro x hx y hy
  exact h x hx y (hC hy)

lemma biUnion_card_eq_sum_card_imp_disjoint {V : Type*} [DecidableEq V]
    {R : Type*} (s : Finset R) (f : R → Finset V)
    (h_eq : (s.biUnion f).card = s.sum (fun i => (f i).card)) :
    ∀ i ∈ s, ∀ j ∈ s, i ≠ j → Disjoint (f i) (f j) := by
  classical
  induction s using Finset.induction_on with
  | empty =>
    intro i hi
    simp at hi
  | insert a s ha ih =>
    rw [biUnion_insert, sum_insert ha] at h_eq
    have h_le : (s.biUnion f).card ≤ s.sum (fun i => (f i).card) := card_biUnion_le
    have h_union := card_union_add_card_inter (f a) (s.biUnion f)
    have h_arith : (f a).card + (s.biUnion f).card = (f a).card + s.sum (fun i => (f i).card) + (f a ∩ s.biUnion f).card := by
      omega
    have h_arith2 : (s.biUnion f).card = s.sum (fun i => (f i).card) + (f a ∩ s.biUnion f).card := by
      omega
    have h_res := nat_add_le_self h_arith2.symm h_le
    have h_empty_card := h_res.1
    have h_eq_ih := h_res.2
    have h_disj : Disjoint (f a) (s.biUnion f) := by
      rw [disjoint_iff_inter_eq_empty]
      exact card_eq_zero.mp h_empty_card
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

-- Theorem 4: Equality implies disjointness
theorem theorem4_equality_implies_disjoint {V : Type*} [DecidableEq V]
    {R : Type*} [Fintype R] (V_prime : R → Finset V)
    (h_eq : (univ.biUnion V_prime).card = univ.sum (fun i => (V_prime i).card)) :
    ∀ i j, i ≠ j → Disjoint (V_prime i) (V_prime j) := by
  intro i j hne
  exact biUnion_card_eq_sum_card_imp_disjoint univ V_prime h_eq i (mem_univ i) j (mem_univ j) hne
