/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs

/-!
# Theorem 1: Coverage Existence

This module constructs a valid entry point selection (EosModel) for any finite DAG
using the identity coverage witness.
-/

open Finset

def construct_identity_model {V : Type*} [DecidableEq V] [Fintype V]
    (E : V → V → Prop) [DecidableRel E] (wf : WellFounded E) : EosModel V where
  E := E
  E_decidable := inferInstance
  E_wf := wf
  S := univ
  κ := fun u v => u = v
  κ_decidable := fun u v => by infer_instance
  total_coverage := by
    intro v
    use v
    simp
  self_coverage_1 := by
    intro s _
    rfl
  self_coverage_2 := by
    intro s _ s' h
    exact h.symm
  transitive_containment := by
    intro v s h hne
    exfalso
    exact hne h
  downward_closure := by
    intro v s u _ _ hu
    simp at hu
