/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.Defs
import EosScheduling.Theorem2
import Mathlib.Tactic.Linarith

/-!
# Worker Assignment and Scheduling Model

This module formalizes worker capacity, schedule validity, makespan, and the
fundamental critical path makespan lower bound under worker assignment.
-/

open Finset

-- Worker pool with capacity
structure WorkerPool (W : Type*) where
  capacity : W → Real
  h_cap_pos : ∀ w, 0 < capacity w

-- Valid schedule with dependency constraints and worker capacity bounds
open Classical in
structure Schedule {V W : Type*} [DecidableEq V] [Fintype V]
    (E : V → V → Prop) (d : V → Real) (load : V → Real) (τ : W → W → Real)
    (pool : WorkerPool W) where
  worker : V → W
  start : V → Real
  h_start_nonneg : ∀ v, 0 ≤ start v
  h_dep : ∀ u v, E u v → start v ≥ start u + d u + τ (worker u) (worker v)
  h_cap : ∀ w t,
    (univ.filter (fun v => worker v = w ∧ start v ≤ t ∧ t < start v + d v)).sum load
      ≤ pool.capacity w

-- Schedule makespan definition
open Classical in
noncomputable def schedule_makespan {V W : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} {d : V → Real} {load : V → Real} {τ : W → W → Real} {pool : WorkerPool W}
    (σ : Schedule E d load τ pool) : Real :=
  WithBot.unbotD 0 (univ.image (fun v => σ.start v + d v)).max

-- Makespan is non-negative
theorem schedule_makespan_nonneg {V W : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} {d : V → Real} {load : V → Real} {τ : W → W → Real} {pool : WorkerPool W}
    (σ : Schedule E d load τ pool) (hd : ∀ v, 0 ≤ d v) :
    0 ≤ schedule_makespan σ := by
  dsimp [schedule_makespan]
  apply unbotD_max_nonneg
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨v, rfl⟩
  have h1 : 0 ≤ σ.start v := σ.h_start_nonneg v
  have h2 : 0 ≤ d v := hd v
  linarith

-- Critical path duration definition (without communication cost)
open Classical in
noncomputable def critical_path {V : Type*} [DecidableEq V] [Fintype V]
    (d : V → Real) (E : V → V → Prop) [wf : WellFoundedRelation V] (v : V) : Real :=
  let preds := filter (fun u => wf.rel u v) univ
  let vals := preds.attach.image (fun ⟨u, h⟩ =>
    have : wf.rel u v := by
      rcases (mem_filter.mp h) with ⟨_, h1⟩
      exact h1
    critical_path d E u)
  WithBot.unbotD 0 vals.max + d v
termination_by v

-- Helper lemma: start time plus duration bounds the critical path
open Classical in
theorem schedule_start_ge_critical_path {V W : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} [wf : WellFoundedRelation V] {d : V → Real} {load : V → Real}
    {τ : W → W → Real} {pool : WorkerPool W} (σ : Schedule E d load τ pool)
    (h_rel : ∀ u v, E u v ↔ wf.rel u v) (hτ : ∀ w1 w2, 0 ≤ τ w1 w2) (v : V) :
    σ.start v + d v ≥ critical_path d E v := by
  induction v using wf.wf.induction with
  | h v ih =>
    rw [critical_path]
    dsimp
    have h_le : WithBot.unbotD 0 (image (fun x => critical_path d E x.val)
      (attach (filter (fun u => wf.rel u v) univ))).max ≤ σ.start v := by
      apply (unbotD_max_le_iff (σ.h_start_nonneg v)).mpr
      intro x hx
      simp only [mem_image, mem_attach, true_and, Subtype.exists] at hx
      rcases hx with ⟨u, hu, rfl⟩
      simp only [mem_filter, mem_univ, true_and] at hu
      have h_edge : E u v := (h_rel u v).mpr hu
      have h_dep_inseq := σ.h_dep u v h_edge
      have h_tau_nonneg := hτ (σ.worker u) (σ.worker v)
      have h_ind := ih u hu
      linarith
    linarith

-- Critical path makespan definition
open Classical in
noncomputable def critical_path_makespan {V : Type*} [DecidableEq V] [Fintype V]
    (d : V → Real) (E : V → V → Prop) [wf : WellFoundedRelation V] : Real :=
  WithBot.unbotD 0 (univ.image (critical_path d E)).max

-- Schedule makespan is bounded below by the critical path makespan
open Classical in
theorem schedule_makespan_ge_critical_path {V W : Type*} [DecidableEq V] [Fintype V]
    {E : V → V → Prop} [wf : WellFoundedRelation V] {d : V → Real} {load : V → Real}
    {τ : W → W → Real} {pool : WorkerPool W} (σ : Schedule E d load τ pool)
    (h_rel : ∀ u v, E u v ↔ wf.rel u v) (hd : ∀ v, 0 ≤ d v) (hτ : ∀ w1 w2, 0 ≤ τ w1 w2) :
    schedule_makespan σ ≥ critical_path_makespan d E := by
  dsimp [schedule_makespan, critical_path_makespan]
  have h_nonneg : 0 ≤ WithBot.unbotD 0 (image (fun v ↦ σ.start v + d v) univ).max := by
    have h_ms_nonneg := schedule_makespan_nonneg σ hd
    exact h_ms_nonneg
  apply (unbotD_max_le_iff h_nonneg).mpr
  intro x hx
  simp only [mem_image, mem_univ, true_and] at hx
  rcases hx with ⟨v, rfl⟩
  have h1 := schedule_start_ge_critical_path σ h_rel hτ v
  have h_mem : σ.start v + d v ∈ univ.image (fun v => σ.start v + d v) := by
    simp
  have h2 := unbotD_max_ge_self h_mem
  linarith
