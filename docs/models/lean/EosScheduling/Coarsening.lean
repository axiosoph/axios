/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs

/-!
# Coarsening Function Model

This module formalizes the coarsening function as a structure, defining its
relationship to the EosModel, monotonicity under cache growth, and union subadditivity.
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
