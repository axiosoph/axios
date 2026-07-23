/-!
# Surety Ceiling — Basic: the artifact/evidence structural core

Mirrors `surety_core.als`'s sorts and classification law
(docs/specs/alloy/surety_core.als, docs/specs/alloy/surety_classification.als)
as an INDUCTIVE model instead of a relational one — `classify`'s five
conditions and precedence cascade from `docs/models/surety-of-source.md`
§2–§4, made a total, computable Lean function.

## Modeling identifications (disclosed, never silent)

- **Build-record presence is folded into the constructor choice.** Alloy's
  `BuildRecordNamesInputs` fact (`no m.input` for a build-recordless member)
  means a build-recordless member behaves, for every purpose this model
  cares about, exactly like a leaf with no inputs. So `Artifact` has exactly
  two shapes: `.leaf` (no build record — a raw fetched payload, an
  unrecorded genesis seed, or any other build-recordless member; precedence
  clause 1 applies uniformly) and `.atom` (build record present, by
  construction — clause 1 never fires on an `.atom`).
- **Acyclicity is by construction, not by axiom.** `inputs : List (Artifact
  ClassName)` inside `.atom` means a build input is a strictly smaller
  subterm, so no artifact can (transitively) name itself as its own input —
  `surety_classification.als`'s separate `F1Acyclic` fact
  (`docs/specs/alloy/surety_classification.als:17-19`) is true here by the
  shape of the type, not asserted. The direct consequence: this file
  structurally CANNOT state `surety_no_f1.als`'s differential
  (`CircularJustificationAdmitted`, `CircularSelfJustifyingTotal` — both
  require two atoms each in the other's `input`, which no term of this
  `Artifact` type can express). See `Ceiling.lean`'s closing note for the
  disclosure this earns instead of faking the differential.
- **Artifact identity is a bare `Nat` tag, not structural equality.** Every
  node carries an `id`, and `Evidence` targets an `id`, not a full
  `Artifact` value — mirroring Alloy, where `Atom`/`Evidence` sigs are
  distinct *objects* compared by identity, never by unfolding their field
  structure. This sidesteps a spurious proof obligation (deriving
  `DecidableEq` through a self-referential `List (Artifact ClassName)`
  field) that has no counterpart in the relational model either.
- **`GateExecutable` is verifier capability, not policy.** Kept as a bare
  `ClassName → Bool` parameter to every definition that needs it, distinct
  from `Policy` (`v0.4.md` §7's relativity statement: `P` decides which
  *signers* count, never which classes a verifier can parse).
-/

namespace Surety

/-- The four classification buckets (`v0.4.md` §2). -/
inductive Bucket where
  | RCAS               -- ReproducibleCASource
  | attestationResidue
  | trustImport
  | sourceClassResidue
  deriving DecidableEq, Repr

/-- A closure member: either a **leaf** (no build record — raw fetched
    payload, or a genesis seed when `isSeed = true`; `v0.4.md` §4,
    `SeedsHaveNoBuildRecord`) or an **atom node** carrying a build record,
    its declared source class, its declared reproducibility mode, whether
    its output is content-addressed, whether it passes the two hard gates
    (format, parse — `v0.4.md` §5.1(a)/(b)), and its list of build inputs.
    `passesFormatGate`/`passesParseGate`/`caOutput`/`declaredReproducible`
    are free structural facts about *this* committed tree, mirroring
    `surety_core.als`'s free unary relations over `Atom`. -/
inductive Artifact (ClassName : Type) where
  | leaf (id : Nat) (isSeed : Bool) : Artifact ClassName
  | atom (id : Nat) (declaredClass : ClassName) (declaredReproducible : Bool)
         (caOutput : Bool) (passesFormatGate : Bool) (passesParseGate : Bool)
         (inputs : List (Artifact ClassName)) : Artifact ClassName

/-- The bare identity tag every `Artifact` carries (see the module
    docstring on why identity, not structural equality, is the comparison
    Evidence targets use). -/
def Artifact.id {ClassName : Type} : Artifact ClassName → Nat
  | .leaf i _ => i
  | .atom i .. => i

/-- A genesis seed is a leaf flagged as such (`v0.4.md` §4); every other
    leaf is an ordinary build-recordless artifact (a raw fetched payload). -/
def GenesisSeed {ClassName : Type} : Artifact ClassName → Bool
  | .leaf _ isSeed => isSeed
  | .atom .. => false

/-- The **dependency closure** of `m` (`v0.4.md` §1): reflexive — `m` is
    always its own head — and the transitive image of `.atom`'s `inputs`.
    Well-founded by construction: `p.val` is a strictly smaller subterm of
    `m` for every `p` drawn from `inputs.attach`, which `decreasing_by`
    below discharges via `List.sizeOf_lt_of_mem`. No fuel, no axiom. -/
def depclosure {ClassName : Type} (m : Artifact ClassName) :
    List (Artifact ClassName) :=
  match m with
  | .leaf i s => [.leaf i s]
  | .atom i c mode ca pf pp inputs =>
      .atom i c mode ca pf pp inputs ::
        inputs.attach.flatMap (fun p => depclosure p.1)
termination_by sizeOf m
decreasing_by
  all_goals
    simp_wf
    have := List.sizeOf_lt_of_mem p.2
    omega

theorem self_mem_depclosure {ClassName : Type} (m : Artifact ClassName) :
    m ∈ depclosure m := by
  cases m <;> simp [depclosure]

/-- Evidence (`surety_core.als`'s `Evidence` sort, `v0.4.md` §3's
    derived/asserted split): a **corroboration** is a re-runnable execution
    record (a rebuild); a **vouch** is pure keyed judgment naming a
    `target` (an artifact id) and the `class` vouched for. -/
inductive Evidence (Signer ClassName : Type) where
  | corroboration (signer : Signer) (target : Nat) : Evidence Signer ClassName
  | vouch (signer : Signer) (target : Nat) (cls : ClassName) :
      Evidence Signer ClassName
  deriving DecidableEq

/-- The evidence snapshot `σ` (`v0.4.md` §2, §7): the finite set of signed
    records that exist at evaluation time. Retraction is modeled as
    omission, exactly as in `surety_core.als`'s header comment. -/
abbrev Snapshot (Signer ClassName : Type) := List (Evidence Signer ClassName)

/-- The consumer's admission policy `P` (`v0.4.md` §7): which signers count
    as corroborating builders, and which as source-class vouchers. Bool
    rather than `Prop`-valued so `classify` stays computable and total by
    construction; this is a finite-model simplification of Alloy's free
    subset sigs, faithful at any concrete bounded instance. -/
structure Policy (Signer : Type) where
  admittedBuilder : Signer → Bool
  admittedVoucher : Signer → Bool

variable {Signer ClassName : Type} [DecidableEq ClassName]

/-- Condition (ii)'s empirical half (`v0.4.md` §6): at least one
    policy-admitted corroborating rebuild targets `m`. -/
def corroborated [DecidableEq Signer]
    (P : Policy Signer) (σ : Snapshot Signer ClassName)
    (m : Artifact ClassName) : Bool :=
  σ.any fun e => match e with
    | .corroboration s t => P.admittedBuilder s && decide (t = m.id)
    | .vouch .. => false

/-- Condition (iv), `established(m)` (`v0.4.md` §5.3): the declared class is
    gate-executable, the tree passes both hard gates, and an admitted,
    unretracted, anchored vouch for exactly this (member, class) pair exists
    in `σ`. Anchoring/retraction are folded into "exists in `σ`" (the
    snapshot already omits retracted records, per the header note above);
    this model does not carry a separate anchoring predicate because
    nothing downstream distinguishes an anchored-in-`σ` record from an
    unanchored one. -/
def srcEstablished [DecidableEq Signer]
    (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName)
    (declaredClass : ClassName) (passesFormat passesParse : Bool)
    (m : Artifact ClassName) : Bool :=
  GateExec declaredClass && passesFormat && passesParse &&
  σ.any fun e => match e with
    | .vouch s t c => P.admittedVoucher s && decide (t = m.id) && decide (c = declaredClass)
    | .corroboration .. => false

/-- **`classify`** (`v0.4.md` §2): total by construction — every case of
    `Artifact` is matched, and the `if`/`else` cascade always yields exactly
    one `Bucket`, with no third status. `.leaf` is precedence clause 1
    (`TrustImport`, uniformly — build-recordless, seed or not: `v0.4.md`
    §4's "genesis seed correctly is a `TrustImport`"). `.atom` always has a
    build record (by the modeling identification above), so clause 1 never
    fires there; the cascade is exactly clauses 2/3 plus the biconditional:
    not established ⟹ `SourceClassResidue` (clause 2); established but any
    of (ii)/(iii)/(v) fails ⟹ `AttestationResidue` (clause 3); all five
    conditions ⟹ `RCAS`. -/
def classify [DecidableEq Signer]
    (GateExec : ClassName → Bool) (P : Policy Signer)
    (σ : Snapshot Signer ClassName) :
    Artifact ClassName → Bucket
  | .leaf _ _ => .trustImport
  | m@(.atom _ c mode ca pf pp inputs) =>
      let est := srcEstablished GateExec P σ c pf pp m
      let corr := corroborated P σ m
      let recClosed := inputs.attach.all fun p =>
        GenesisSeed p.1 || decide (classify GateExec P σ p.1 = .RCAS)
      if !est then .sourceClassResidue
      else if mode && corr && ca && recClosed then .RCAS
      else .attestationResidue
termination_by m => sizeOf m
decreasing_by
  all_goals
    simp_wf
    have := List.sizeOf_lt_of_mem p.2
    omega

end Surety
