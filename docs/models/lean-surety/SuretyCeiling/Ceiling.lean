import SuretyCeiling.Basic

/-!
# Surety Ceiling — the trust surface, the assumption basis, and the ceiling
theorems

Mirrors `surety_classification.als`'s asserted checks
(docs/specs/alloy/surety_classification.als) as proved Lean theorems over
`Basic.lean`'s inductive `classify`, rather than bounded-scope model-checked
assertions. Ground truth throughout: `docs/models/surety-of-source.md`
("`v0.4.md`" below).

## The disclosed Alloy divergence — F1 / `CircularJustification`

`surety_no_f1.als` demonstrates, *by removing* the `F1Acyclic` fact, that the
bare condition-(v) recursion admits a circular-justification fixed point:
two atoms each citing the other as an input, both classified `RCAS`, with
no genesis seed anywhere grounding the closure. `Basic.lean`'s `Artifact`
cannot express that instance at all — `inputs : List (Artifact ClassName)`
inside `.atom` makes every input a strictly smaller subterm, so `m2 ∈
m1.input ∧ m1 ∈ m2.input` has no term-level witness. This file therefore
has no `no_circular_justification` theorem to state: there is nothing to
prove, because the unsound instance is inexpressible, not excluded by an
argument. That is a strictly stronger property than the Alloy check
(`CircularJustification` returns UNSAT *within a bounded scope*, given the
axiom; here it is unsatisfiable at every scope, by the type), but it is a
DIFFERENT claim, and this note is exactly the disclosure the dispatch asked
for rather than a claimed re-derivation of Alloy's differential.
-/

namespace Surety

variable {Signer ClassName : Type} [DecidableEq ClassName] [DecidableEq Signer]

-- ===========================================================================
-- §1. The trust surface T(a), the assumption basis B(a), Total (v0.4.md §3-4)
-- ===========================================================================

/-- **The trust surface `T(a)`** (`v0.4.md` §3): the closure minus its
    `RCAS` members — every artifact that classifies into one of the three
    residue buckets. -/
def trustSurface (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (a : Artifact ClassName) :
    List (Artifact ClassName) :=
  (depclosure a).filter fun m => !decide (classify GateExec P σ m = .RCAS)

/-- **The assumption basis `B(a)`** (`v0.4.md` §3): the policy-admitted
    evidence the walk's *non-residue* classifications rest on — a counted
    corroboration for each `RCAS` member of `depclosure(a)`, and a counted
    vouch for each established member of `depclosure(a)`. (The third named
    component of `B(a)`, the genesis-seed identities, is a set of
    *artifacts*, not evidence — it is read directly off `depclosure a`
    filtered by `GenesisSeed`, not folded into this `Evidence` list.) -/
def basis (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (a : Artifact ClassName) :
    Snapshot Signer ClassName :=
  σ.filter fun e => match e with
    | .corroboration s t =>
        P.admittedBuilder s &&
          (depclosure a).any fun m =>
            decide (m.id = t) && decide (classify GateExec P σ m = .RCAS)
    | .vouch s t c =>
        P.admittedVoucher s &&
          (depclosure a).any fun m =>
            decide (m.id = t) &&
              match m with
              | .atom _ declClass _ _ pf pp _ =>
                  decide (c = declClass) && srcEstablished GateExec P σ declClass pf pp m
              | .leaf .. => false

/-- **`Total(a)`** (`v0.4.md` §4): the trust surface contains nothing but
    genesis seeds — every non-seed closure member is `RCAS`. -/
def Total (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (a : Artifact ClassName) : Prop :=
  ∀ m ∈ trustSurface GateExec P σ a, GenesisSeed m = true

/-- **Laundering's structural signature** (`surety_core.als`'s
    `launderedShape`, `v0.4.md` §8.1): a non-seed artifact either outside
    build accounting (C1: a leaf) or with self-declared-only sourcehood
    (C3: an atom whose establishment fails). Ground-truth "laundered" is
    not machine-representable (`v0.4.md` §8.2); this is its structural
    correlate, exactly as in the Alloy model. -/
def launderedShape (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) : Artifact ClassName → Bool
  | .leaf _ isSeed => !isSeed
  | m@(.atom _ c _ _ pf pp _) => !srcEstablished GateExec P σ c pf pp m

-- ===========================================================================
-- §2. Safety (sense 1) — `surety_classification.als`'s asserted checks,
--     as proved theorems.
-- ===========================================================================

/-- **`no_silent_laundering`** — `NoSilentLaundering`
    (surety_classification.als:26-30). No laundered-shaped member ever
    classifies into the closed bucket. -/
theorem no_silent_laundering (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (m : Artifact ClassName)
    (hlaunder : launderedShape GateExec P σ m = true) :
    classify GateExec P σ m ≠ .RCAS := by
  cases m with
  | leaf i isSeed => simp [classify]
  | atom i c mode ca pf pp inputs =>
      simp only [launderedShape, Bool.not_eq_true'] at hlaunder
      simp [classify, hlaunder]

/-- **`laundered_never_total`** — `LaunderedNeverPresentsAsTotal`
    (surety_classification.als:33-37). A closure containing any
    laundered-shaped member can never present as `Total`: the member is not
    `RCAS` (`no_silent_laundering`), not a seed (laundering's own
    signature), hence sits in `trustSurface a` in violation of `Total`. -/
theorem laundered_never_total (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (a m : Artifact ClassName)
    (hmem : m ∈ depclosure a) (hlaunder : launderedShape GateExec P σ m = true) :
    ¬ Total GateExec P σ a := by
  intro hTotal
  have hnotRCAS := no_silent_laundering GateExec P σ m hlaunder
  have hmemSurface : m ∈ trustSurface GateExec P σ a := by
    simp only [trustSurface, List.mem_filter]
    exact ⟨hmem, by simp [hnotRCAS]⟩
  have hseed : GenesisSeed m = true := hTotal m hmemSurface
  cases m with
  | leaf i isSeed =>
      simp only [launderedShape, Bool.not_eq_true'] at hlaunder
      simp only [GenesisSeed] at hseed
      rw [hlaunder] at hseed
      exact absurd hseed (by decide)
  | atom i c mode ca pf pp inputs =>
      simp [GenesisSeed] at hseed

/-- **`C1FetchPinForcedToTrustImport`.** A build-recordless member (a leaf
    in this model) is forced into `TrustImport` — unconditionally, by
    `classify`'s definition. -/
theorem C1_fetchPin_forced_to_trustImport (GateExec : ClassName → Bool)
    (P : Policy Signer) (σ : Snapshot Signer ClassName) (i : Nat) (isSeed : Bool) :
    classify GateExec P σ (Artifact.leaf i isSeed) = .trustImport := by
  simp [classify]

/-- **`SeedsAreTrustImports`** (surety_classification.als:71-75). Genesis
    seeds classify `TrustImport` — the permanent, named trust-import
    (`v0.4.md` §4). -/
theorem seeds_are_trustImports (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) (m : Artifact ClassName)
    (hseed : GenesisSeed m = true) : classify GateExec P σ m = .trustImport := by
  cases m with
  | leaf i isSeed => simp [classify]
  | atom i c mode ca pf pp inputs => simp [GenesisSeed] at hseed

/-- **`DeclarationAloneNeverCloses`** (surety_classification.als:56-60): the
    closed bucket is never reached by self-declaration alone — every `RCAS`
    verdict carries both an actual corroboration (condition (ii)'s
    empirical half) and an actual established source-class-vouch
    (condition (iv)). -/
theorem declaration_alone_never_closes (GateExec : ClassName → Bool)
    (P : Policy Signer) (σ : Snapshot Signer ClassName) (m : Artifact ClassName)
    (hrcas : classify GateExec P σ m = .RCAS) :
    (∃ c pf pp, srcEstablished GateExec P σ c pf pp m = true) ∧
      corroborated P σ m = true := by
  cases m with
  | leaf i isSeed => simp [classify] at hrcas
  | atom i c mode ca pf pp inputs =>
      simp only [classify] at hrcas
      by_cases hest : srcEstablished GateExec P σ c pf pp (Artifact.atom i c mode ca pf pp inputs)
      · simp only [hest, Bool.not_true, if_neg (by simp : ¬ ((false : Bool) = true) )] at hrcas
        by_cases hcascade :
            (mode && corroborated P σ (Artifact.atom i c mode ca pf pp inputs) && ca &&
              inputs.attach.all fun p => GenesisSeed p.1 ||
                decide (classify GateExec P σ p.1 = .RCAS)) = true
        · simp only [hcascade, if_pos] at hrcas
          simp only [Bool.and_eq_true_iff] at hcascade
          exact ⟨⟨c, pf, pp, hest⟩, hcascade.1.1.2⟩
        · simp [hcascade] at hrcas
      · simp only [Bool.not_eq_true] at hest
        simp [hest] at hrcas

/-- Atom-specialized strengthening of `declaration_alone_never_closes`:
    pins the established witness to the atom's OWN `(declaredClass, pf, pp)`
    fields rather than an unrelated existential triple. `RCAS` is only ever
    reached via `classify`'s cascade calling `srcEstablished` with the
    atom's own fields (never a substitute), so this is provable — but it is
    NOT interchangeable with `declaration_alone_never_closes`'s conclusion:
    that theorem's `∃ c pf pp, ...` forgets which witness was used, and
    `basis`'s membership condition needs the vouch's class to match THIS
    atom's own `declaredClass` field, not an opaque existential one. -/
theorem atom_established_own_class (GateExec : ClassName → Bool)
    (P : Policy Signer) (σ : Snapshot Signer ClassName)
    (i : Nat) (c : ClassName) (mode ca pf pp : Bool)
    (inputs : List (Artifact ClassName))
    (hrcas : classify GateExec P σ (Artifact.atom i c mode ca pf pp inputs) = .RCAS) :
    srcEstablished GateExec P σ c pf pp (Artifact.atom i c mode ca pf pp inputs) = true := by
  simp only [classify] at hrcas
  by_cases hest : srcEstablished GateExec P σ c pf pp (Artifact.atom i c mode ca pf pp inputs) = true
  · exact hest
  · simp only [Bool.not_eq_true] at hest
    simp [hest] at hrcas

/-- **`RealTotalCarriesVouchInBasis`** (surety_classification.als:80-84):
    every real (non-seed) `Total` verdict carries at least one admitted
    vouch enumerated in the assumption basis. Because `a ∈ depclosure a`
    always (`self_mem_depclosure`), a non-seed `a` for which `Total a`
    holds cannot itself sit in the residue (`Total` would then force it to
    be a seed) — so `classify a = RCAS`, which by
    `declaration_alone_never_closes` forces an actual established vouch for
    `a`, and that same vouch is exactly what `basis a`'s filter keeps
    (`a ∈ depclosure a`, target = `a.id`, class matches, established
    holds). -/
theorem total_carries_vouch_in_basis (GateExec : ClassName → Bool)
    (P : Policy Signer) (σ : Snapshot Signer ClassName) (a : Artifact ClassName)
    (hTotal : Total GateExec P σ a) (hnotSeed : GenesisSeed a = false) :
    ∃ e ∈ basis GateExec P σ a, ∃ s t c, e = .vouch s t c := by
  -- Step 1: `a` itself must classify RCAS.
  have hRCAS : classify GateExec P σ a = .RCAS := by
    by_cases hne : classify GateExec P σ a = .RCAS
    · exact hne
    · exfalso
      have hmemSurface : a ∈ trustSurface GateExec P σ a := by
        simp only [trustSurface, List.mem_filter]
        exact ⟨self_mem_depclosure a, by simp [hne]⟩
      have := hTotal a hmemSurface
      rw [hnotSeed] at this
      exact absurd this (by decide)
  -- Step 2: extract the established vouch witnessing condition (iv).
  match a, hnotSeed, hRCAS with
  | .leaf i isSeed, hnotSeed, hRCAS => simp [classify] at hRCAS
  | .atom i c mode ca pf pp inputs, _, hRCAS =>
      have hest := atom_established_own_class GateExec P σ i c mode ca pf pp inputs hRCAS
      have hunfold := hest
      simp only [srcEstablished] at hunfold
      obtain ⟨_, hv⟩ := Bool.and_eq_true_iff.mp hunfold
      simp only [List.any_eq_true] at hv
      obtain ⟨e, heσ, hemat⟩ := hv
      cases e with
      | vouch s t cl =>
          refine ⟨.vouch s t cl, ?_, s, t, cl, rfl⟩
          simp only [basis, List.mem_filter]
          refine ⟨heσ, ?_⟩
          simp only [Bool.and_eq_true_iff] at hemat
          obtain ⟨⟨hsv, htv⟩, hcv⟩ := hemat
          simp only [hsv, Bool.true_and, List.any_eq_true]
          refine ⟨Artifact.atom i c mode ca pf pp inputs,
            self_mem_depclosure _, ?_⟩
          simp [hcv, hest]
          exact (of_decide_eq_true htv).symm
      | corroboration s t => simp at hemat

end Surety
