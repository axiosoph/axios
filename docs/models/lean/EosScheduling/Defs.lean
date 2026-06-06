/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Fintype.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Order.Bounds.Basic

/-!
# Shared Definitions for Eos Scheduling Formal Proofs

This module defines the DAG structure, path reachability, and entry point coverage relations.
-/

-- Inductive definition of a path in E with no intermediate nodes in S
inductive PathNoS {V : Type*} (E : V → V → Prop) (S : Finset V) : V → V → Prop where
  | step (u v : V) : E u v → PathNoS E S u v
  | trans (u z v : V) : E u z → z ∉ S → PathNoS E S z v → PathNoS E S u v

-- Lemma: PathNoS implies TransGen of E
lemma pathNoS_impl_transGen {V : Type*} {E : V → V → Prop} {S : Finset V} {u v : V}
    (h : PathNoS E S u v) : Relation.TransGen E u v := by
  induction h with
  | step u v he => exact Relation.TransGen.single he
  | trans u z v he hnz _ ih => exact Relation.TransGen.trans (Relation.TransGen.single he) ih

structure EosModel (V : Type*) [DecidableEq V] [Fintype V] where
  E : V → V → Prop
  E_decidable : DecidableRel E
  E_wf : WellFounded E
  S : Finset V
  κ : V → V → Prop
  κ_decidable : DecidableRel κ

  -- Property 1: Total coverage
  total_coverage : ∀ v : V, ∃ s ∈ S, κ v s

  -- Property 2: Self-coverage
  self_coverage_1 : ∀ s ∈ S, κ s s
  self_coverage_2 : ∀ s ∈ S, ∀ s', κ s s' → s' = s

  -- Property 3: Transitive containment
  transitive_containment : ∀ v s, κ v s → v ≠ s → Relation.TransGen E v s

  -- Property 4: Downward closure within coverage
  downward_closure : ∀ v s u, κ v s → E u v → u ∉ S → κ u s
