import SuretyCeiling.Basic
import SuretyCeiling.Ceiling

/-!
# Surety Ceiling — non-vacuity witnesses for the ceiling theorems

`Ceiling.lean`'s `total_carries_vouch_in_basis` and `laundered_never_total`
are universally quantified over every `GateExecutable`, `Policy`, `Snapshot`,
and `Artifact` — true, but worthless, if their hypotheses are unsatisfiable.
This file exhibits one concrete, fully-computable instantiation for each
theorem's hypothesis set and proves the conclusion is actually reached, not
merely derivable in the abstract.

## The instantiation

`ClassName := Unit`, `Signer := Unit` — the smallest types that still let
every field `classify`/`Total`/`basis` inspect take a real value. A single
gate-executable class, a single admitted signer for both builder and voucher
roles: enough structure to populate every conjunct in `srcEstablished` and
`corroborated`, nothing more.
-/

namespace Surety
namespace Nonvacuity

/-- The class alphabet: one class, always gate-executable. -/
abbrev C := Unit

/-- The signer alphabet: one signer, admitted for every role. -/
abbrev Sg := Unit

def gateExec : C → Bool := fun _ => true

def pol : Policy Sg := { admittedBuilder := fun _ => true, admittedVoucher := fun _ => true }

-- ===========================================================================
-- §1. Positive witness — a non-seed atom that is `RCAS`, `Total`, and whose
--     `basis` carries a real vouch (`total_carries_vouch_in_basis`).
-- ===========================================================================

/-- The witness atom: id 1, declared class `()`, every gate/mode/CA flag
    true, no build inputs — so its `depclosure` is just itself and every
    closure-recursion condition is vacuously satisfied by the empty list. -/
def posAtom : Artifact C := .atom 1 () true true true true []

/-- The snapshot: one admitted corroboration and one admitted vouch, both
    targeting `posAtom`'s id (and, for the vouch, its declared class) —
    exactly what condition (ii) and condition (iv) each require. -/
def posSnap : Snapshot Sg C := [.corroboration () 1, .vouch () 1 ()]

theorem posAtom_not_seed : GenesisSeed posAtom = false := rfl

theorem posAtom_established :
    srcEstablished gateExec pol posSnap () true true posAtom = true := by
  simp [srcEstablished, posSnap, gateExec, pol, posAtom, Artifact.id]

theorem posAtom_corroborated : corroborated pol posSnap posAtom = true := by
  simp [corroborated, posSnap, pol, posAtom, Artifact.id]

/-- `posAtom` actually classifies `RCAS` — condition (i)-(v) all fire. -/
theorem posAtom_rcas : classify gateExec pol posSnap posAtom = .RCAS := by
  simp [classify, srcEstablished, corroborated, posSnap, posAtom, gateExec, pol,
    Artifact.id]

/-- `posAtom`'s `depclosure` is just itself: no inputs, nothing to recurse
    into. -/
theorem posAtom_depclosure : depclosure posAtom = [posAtom] := by
  simp [depclosure, posAtom]

/-- `posAtom`'s trust surface is empty: its lone closure member (itself)
    classifies `RCAS`, so the residue filter keeps nothing. -/
theorem posAtom_trustSurface :
    trustSurface gateExec pol posSnap posAtom = [] := by
  simp [trustSurface, posAtom_depclosure, posAtom_rcas]

/-- `Total posAtom` holds — vacuously over an empty trust surface, but that
    emptiness is itself earned by `posAtom_rcas`, not assumed. -/
theorem posAtom_total : Total gateExec pol posSnap posAtom := by
  intro m hm
  simp [posAtom_trustSurface] at hm

/-- The concrete vouch that condition (iv) rests on is really present in
    `basis posAtom` — not merely asserted to exist. -/
theorem posAtom_vouch_in_basis :
    Evidence.vouch () 1 () ∈ basis gateExec pol posSnap posAtom := by
  simp [basis, posSnap, depclosure, posAtom, srcEstablished, gateExec, pol, Artifact.id]

/-- The conclusion shape of `total_carries_vouch_in_basis`, met by the
    concrete vouch above. -/
theorem posAtom_vouch_witness :
    ∃ e ∈ basis gateExec pol posSnap posAtom, ∃ s t c, e = .vouch s t c :=
  ⟨.vouch () 1 (), posAtom_vouch_in_basis, (), 1, (), rfl⟩

/-- Cross-check: applying the general `total_carries_vouch_in_basis` theorem
    to this concrete instantiation reproduces the same witness shape — the
    theorem is not just satisfiable in principle, it fires on `posAtom`. -/
example :
    ∃ e ∈ basis gateExec pol posSnap posAtom, ∃ s t c, e = .vouch s t c :=
  total_carries_vouch_in_basis gateExec pol posSnap posAtom posAtom_total posAtom_not_seed

-- ===========================================================================
-- §2. Negative witness — a non-seed laundered ATOM (establishment fails,
--     C3) that is never `Total` (`laundered_never_total`).
-- ===========================================================================

/-- Same shape as `posAtom`, different id — but paired with an EMPTY
    snapshot below, so its establishment fails for want of any vouch. -/
def negAtom : Artifact C := .atom 2 () true true true true []

/-- No evidence at all: no vouch exists for `negAtom`, so condition (iv)
    fails and `launderedShape`'s C3 clause fires. -/
def negSnap : Snapshot Sg C := []

theorem negAtom_not_seed : GenesisSeed negAtom = false := rfl

theorem negAtom_establishment_fails :
    srcEstablished gateExec pol negSnap () true true negAtom = false := by
  simp [srcEstablished, negSnap, negAtom]

/-- `negAtom` is laundered-shaped: an atom whose establishment fails
    (`launderedShape`'s C3 clause), reachable with a genuinely empty
    evidence snapshot — not merely a hypothetical. -/
theorem negAtom_laundered :
    launderedShape gateExec pol negSnap negAtom = true := by
  simp [launderedShape, negAtom, srcEstablished, negSnap]

/-- `negAtom` is never `Total` — via the actual `laundered_never_total`
    theorem, applied to this concrete reachable laundered atom. -/
theorem negAtom_not_total : ¬ Total gateExec pol negSnap negAtom :=
  laundered_never_total gateExec pol negSnap negAtom negAtom
    (self_mem_depclosure negAtom) negAtom_laundered

end Nonvacuity
end Surety
