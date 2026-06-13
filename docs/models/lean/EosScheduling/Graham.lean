/-
Copyright (c) 2026 nrd. All rights reserved.
Released under Apache 2.0 license as described in the file LICENSE.
Authors: nrd, Antigravity
-/
import EosScheduling.HEFT
import Mathlib.Tactic.FieldSimp

/-!
# Graham's List-Scheduling Approximation Bound (Corollary)

This module instantiates the abstract approximation ratio `α` of Theorem 2 with
Graham's classical list-scheduling bound `α = 2 - 1/|W|` (Graham, 1966), for the
**identical-machines** case (see ADR-0004 §"Concrete Instantiation of α (Graham's
Bound)").

Two results are mechanized:

* `graham_list_scheduling_bound`: any work-conserving schedule has makespan at
  most `(2 - 1/|W|)` times the makespan of any other (e.g. optimal) schedule,
  combining the structural work-conserving bound with the two classical makespan
  lower bounds — the critical path and total-work-over-workers.
* `graham_consistency_bound`: the end-to-end consistency bound of Theorem 2 with
  `α := 2 - 1/|W|` substituted, yielding the machine-checked constant
  `(2 - 1/|W|) · (1 + ε)/(1 - ε)`.

## Scope and hypotheses

* **Identical machines.** Graham's `2 - 1/|W|` bound assumes identical machines.
  The heterogeneous case (ADR-0004) loosens by the heterogeneity ratio and is out
  of scope here.

* **The `(2 - 1/|W|)` factor needs the *tight* work-conserving bound.** The naive
  argument `CP + Σd/|W| ≤ (2 - 1/|W|)·max(CP, Σd/|W|)` is false (take `CP =
  Σd/|W| = c > 0`: LHS `= 2c`, RHS `= (2 - 1/|W|)·c < 2c`). The weakened
  conclusion of `work_conserving_makespan_bound` (`M ≤ CP + Σd/|W|`) therefore
  only yields a factor of `2`. The `1/|W|` saving comes from the *un-weakened*
  work-conserving inequality `|W|·M - Σd ≤ (|W|-1)·CP`, i.e.
  `M ≤ Σd/|W| + (1 - 1/|W|)·CP`, which enters here as the hypothesis `h_wc` (the
  same premise `work_conserving_makespan_bound` consumes, before it discards the
  `1/|W|`).

* **Total-work lower bound as a hypothesis.** The lower bound `Σd/|W| ≤ M(σ*)` is
  taken as an explicit hypothesis `h_work_lb`. Under the `Schedule` structure's
  capacity semantics (`h_cap` bounds *concurrent load* per worker, not
  tasks-per-worker) this lower bound does NOT hold in general: with large
  capacity many tasks may run concurrently on one worker, so the total duration
  `Σd` can exceed `|W|·M`. The bound is precisely the identical-machines
  (one-task-per-machine) assumption, surfaced here as a documented premise rather
  than silently assumed. The critical-path lower bound, by contrast, is *derived*
  from `schedule_makespan_ge_critical_path`.
-/

set_option linter.unusedSectionVars false

open Finset

variable {V W : Type*} [DecidableEq V] [Fintype V]
variable {E : V → V → Prop} {d : V → Real} {load : V → Real}
variable {τ : W → W → Real} {pool : WorkerPool W}

/--
Graham's list-scheduling bound (identical-machines case): a work-conserving
schedule `σ` has makespan at most `(2 - 1/|W|)` times that of any comparison
schedule `σstar` (in particular, an optimal one).

The proof composes three facts:
* the tight work-conserving structural bound `h_wc`
  (`|W|·M(σ) - Σd ≤ (|W|-1)·CP`),
* the critical-path lower bound `CP ≤ M(σstar)` (derived from
  `schedule_makespan_ge_critical_path`),
* the total-work lower bound `Σd/|W| ≤ M(σstar)` (`h_work_lb`, the
  identical-machines premise — see module docstring).
-/
theorem graham_list_scheduling_bound [Fintype W] [wf : WellFoundedRelation V]
    (σ σstar : Schedule E d load τ pool)
    (hd : ∀ v, 0 ≤ d v)
    (h_W : 0 < (Fintype.card W : Real))
    (h_rel : ∀ u v, E u v ↔ wf.rel u v)
    (hτ : ∀ w1 w2, 0 ≤ τ w1 w2)
    (h_wc : (Fintype.card W : Real) * schedule_makespan σ - (univ.sum d)
      ≤ (Fintype.card W - 1 : Real) * critical_path_makespan d E)
    (h_work_lb : (univ.sum d) / (Fintype.card W : Real) ≤ schedule_makespan σstar) :
    schedule_makespan σ ≤ (2 - 1 / (Fintype.card W : Real)) * schedule_makespan σstar := by
  classical
  -- |W| ≥ 1 (a nonempty worker pool), established before abbreviating.
  have hm1 : (1 : Real) ≤ (Fintype.card W : Real) := by
    have hcard : 0 < Fintype.card W := by exact_mod_cast h_W
    exact_mod_cast hcard
  set m : Real := (Fintype.card W : Real) with hm
  have hm_pos : 0 < m := by linarith
  -- Critical-path lower bound on the comparison schedule: CP ≤ M(σstar).
  have h_cp : critical_path_makespan d E ≤ schedule_makespan σstar :=
    schedule_makespan_ge_critical_path σstar h_rel hd hτ
  -- Total-work lower bound (hypothesis), cleared of division: Σd ≤ |W|·M(σstar).
  have h_S_le : univ.sum d ≤ m * schedule_makespan σstar := by
    have h := (div_le_iff₀ hm_pos).mp h_work_lb
    linarith [mul_comm (schedule_makespan σstar) m, h]
  -- Scale the critical-path bound by (|W| - 1) ≥ 0.
  have h_cp_scaled : (m - 1) * critical_path_makespan d E
      ≤ (m - 1) * schedule_makespan σstar :=
    mul_le_mul_of_nonneg_left h_cp (by linarith)
  -- Key scaled inequality: |W|·M(σ) ≤ (2|W| - 1)·M(σstar).
  have h_key : m * schedule_makespan σ ≤ (2 * m - 1) * schedule_makespan σstar := by
    have e2 : (m - 1) * schedule_makespan σstar + m * schedule_makespan σstar
        = (2 * m - 1) * schedule_makespan σstar := by ring
    linarith [h_wc, h_cp_scaled, h_S_le, e2]
  -- Divide the scaled inequality back through by |W| > 0.
  have h_eq : (2 - 1 / m) * schedule_makespan σstar
      = ((2 * m - 1) * schedule_makespan σstar) / m := by
    field_simp
  rw [h_eq, le_div_iff₀ hm_pos]
  linarith [h_key, mul_comm (schedule_makespan σ) m]

/--
End-to-end consistency bound with Graham's ratio substituted: Theorem 2's
`α`-parametric bound instantiated at `α := 2 - 1/|W|`, giving the machine-checked
constant `(2 - 1/|W|) · (1 + ε)/(1 - ε)`.
-/
theorem graham_consistency_bound {S : Type*} [Fintype S] [WellFoundedRelation S]
    {W : Type*} [Fintype W]
    {d d_hat d_star d_hat_star : S → Real} {τ : S → S → Real} {ε : Real}
    (h_eps_pos : 0 ≤ ε) (h_eps_lt : ε < 1)
    (h_W : 0 < (Fintype.card W : Real))
    (h_tau_nonneg : ∀ s' s, 0 ≤ τ s' s)
    (h_d_hat_nonneg : ∀ s, 0 ≤ d_hat s)
    (h_d_hat_star_nonneg : ∀ s, 0 ≤ d_hat_star s)
    (h_nonempty : Nonempty S)
    (h_err_h : ∀ s, |d s - d_hat s| ≤ ε * d_hat s)
    (h_err_star : ∀ s, |d_star s - d_hat_star s| ≤ ε * d_hat_star s)
    (h_approx : makespan d_hat τ
      ≤ (2 - 1 / (Fintype.card W : Real)) * makespan d_hat_star τ) :
    makespan d τ
      ≤ (2 - 1 / (Fintype.card W : Real)) * ((1 + ε) / (1 - ε)) * makespan d_star τ := by
  -- Graham's ratio is nonnegative: 1/|W| ≤ 1 ≤ 2 since |W| ≥ 1.
  have h_alpha : 0 ≤ (2 - 1 / (Fintype.card W : Real)) := by
    have hcard : 0 < Fintype.card W := by exact_mod_cast h_W
    have hm1 : (1 : Real) ≤ (Fintype.card W : Real) := by exact_mod_cast hcard
    have h_inv : 1 / (Fintype.card W : Real) ≤ 1 := by rw [div_le_one h_W]; exact hm1
    linarith
  exact theorem2_consistency_bound h_eps_pos h_eps_lt h_alpha h_tau_nonneg
    h_d_hat_nonneg h_d_hat_star_nonneg h_nonempty h_err_h h_err_star h_approx
