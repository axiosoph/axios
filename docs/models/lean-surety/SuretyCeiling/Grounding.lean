import SuretyCeiling.Basic
import SuretyCeiling.Ceiling
import SuretyCeiling.Generator

/-!
# Surety Ceiling — Grounding: binding the forced generator to the ceiling

Phase 2 of the two-phase build `Generator.lean` names in its own header: binds
the structural ceiling (`Ceiling.lean`'s `launderedShape`, `Total`) to the
forced-generator model (`Generator.lean`'s `degenerate`/`genuine`/
`GeneratorLaundering`) so the ceiling's genuineness axis is grounded in a real
computational object rather than an opaque predicate. Ground truth:
`docs/models/surety-of-source.md` §8, §8.3, §10 ("`v0.4.md`" below).

## §10's fidelity constraint, restated as this file's own law

> "The forced generator appears in the model only through its structural
> consequence ...; its degeneracy is never computed — the model does not
> decide the undecidable." (`v0.4.md` §10)

Nothing below defines a `Decidable degenerate` instance, a function computing
`degenerate`/`genuine` from a `Gen` value, or an internal decision procedure.
Theorem 1 itself (`v0.4.md` §9.1) is **cited in prose, not mechanized as a
hypothesis in this file** — no theorem below takes an `hRice`-shaped
undecidability premise, because none needs one: `rcas_forces_established`
holds unconditionally (it is `Gen`-blind by construction, needing no premise
about degeneracy's decidability), and `genuineness_underdetermined` below is
a concrete existential exhibited outright, not a claim resting on Rice's
theorem being assumed. `#print axioms` on the latter confirms the only
axioms in play are Lean's core three, never a `sorryAx`.

## STOP — obligation 1 (correspondence) does not hold as stated

The dispatch for this file asked for a theorem binding `launderedShape`
(`Ceiling.lean`) to `GeneratorLaundering`/`degenerate` (`Generator.lean`) under
an explicit instantiation of `Gen`/`Input`/`Output`, reading "a laundered
atom's forced generator is degenerate." **This does not hold, in either
direction, and is not merely unproved — it is refuted by the model's own
§8.3/§10 discussion, independent of any instantiation choice:**

- **`degenerate ⇏ launderedShape`.** The §8.3 witness (the rigged,
  finite-range `inflate`-literal emitter) is exercised in §10 under *both*
  outcomes of the very same degenerate generator: unvouched, its atom sits in
  `SourceClassResidue` (`launderedShape = true`); vouched, an admitted,
  unretracted, anchored source-class-vouch makes it `established`, hence
  `RCAS`, hence `launderedShape = false` (`v0.4.md` §10, "Satisfiability …
  vouch-dependent" and the witness's dual fate in §8.3's closing paragraph).
  `launderedShape` is governed entirely by vouch-admission; the generator's
  degeneracy never enters `srcEstablished`'s definition (`Basic.lean`) at
  all, so it cannot be what flips `launderedShape` between these two
  outcomes.
- **`launderedShape ⇏ degenerate`.** A `.leaf` (a raw fetched payload, C1)
  is `launderedShape = true` whenever it is not a seed, and carries no
  generator, degenerate or otherwise, to be a witness for — the C1 clause of
  laundering (`v0.4.md` §2's precedence cascade) has nothing to do with any
  forced generator.
- **No live wiring exists to state it anyway.** `Artifact` (`Basic.lean`) has
  exactly two constructors, `.leaf` and `.atom`, neither carrying a `Gen`,
  `Input`, `Output`, or `Denote` field — there is no term of `Artifact
  ClassName` from which "this atom's forced generator" can be extracted, and
  `Generator.lean`'s own header confirms this is deliberate ("No wiring into
  `SuretyCeiling.Basic`/`SuretyCeiling.Ceiling`… how the two compose is a
  Phase 2 question for the composer to settle first"). Stating the
  correspondence would require inventing a new relation binding artifacts to
  generators — a structural addition to the GREEN `Basic.lean`, not merely
  exposing a definition, and one the bidirectional counterexample above shows
  would have to be *false* regardless of how it were defined.

Per the dispatch's own instruction ("If this correspondence is FALSE as the
two are defined, STOP and report the mismatch — do not redefine either side
to force it"), no correspondence theorem is stated. This is a valid terminal
finding for obligation 1, not a gap papered over; obligations 2 and 3 below
do not depend on it.

## Obligation 2 — the §10 grounding

The correct relationship — the one `Generator.lean`'s own header already
anticipates ("a build realized through a forced generator … can only reach
`RCAS` via the vouch path, never by any predicate over `genuine`/
`degenerate`") — is the path actually taken (`rcas_forces_established`):
every `RCAS` verdict in `Basic.lean`'s `classify` forces `srcEstablished` —
the vouch clause — true. This is `Ceiling.lean`'s already-proved
`atom_established_own_class`, restated here in the generator-aware
vocabulary this file is scoped to connect: `classify`/`srcEstablished` are
`Gen`-free total `Bool` functions, so this fact holds *regardless* of
whatever generator (degenerate or genuine) sits behind the atom's build —
which is exactly the grounding claim's "never via degeneracy" half made
visible in the type signature (no `Gen`, `Denote`, or `degenerate` premise
is needed to derive it).

The other half — *why* no generator-aware alternative could have existed —
is not separately mechanized here. An earlier draft of this file stated it
as a dedicated theorem (`hRice ⟨f, hf⟩` applied to a purported total
decision function), but that term is the definitional unfolding of `hRice`
itself (`¬∃ isDeg, …` applied to a witness of the existential it negates,
i.e. `False` by the very shape of negation) — it proves nothing beyond
restating its own hypothesis in uncurried form, and is not new mathematics.
The substantive undecidability claim is Theorem 1 (`v0.4.md` §9.1(i)),
**cited, not proved here**: it is Rice's theorem applied to the index set of
finite-range partial computable functions, and this file's only obligation
toward it is to *use* it as a named hypothesis (`hRice`, as it would appear
in any theorem admitting one) rather than smuggle it in as an axiom or a
`Decidable` instance — a discipline `rcas_forces_established` already
satisfies by needing no such hypothesis at all.

## Obligation 2a — the determination axis: genuineness is underdetermined

The two theorems above ground *why* establishment cannot lean on degeneracy;
this section grounds the fact `v0.4.md` §8 and the blog's "Forced Generator"
section spend a whole argument on: observing the atom's output bytes alone
cannot distinguish a genuine generator from a degenerate (laundering) one.
`genuineness_underdetermined` exhibits an actual pair — not an assumption,
not a citation — of a genuine generator and a degenerate one that realize
the *same* output on some input: `Bool` as the two-generator universe,
`Nat` as the shared input/output type, the identity map (`true`, unbounded
range, genuine) against the constant-42 map (`false`, range `{42}`,
degenerate), both realizing `42`. This is the tangible core of Theorem
1(ii)'s "knowing the one input/output pair the build exhibited buys the
verifier nothing": here, the pair (`z = 42`, `B = 42`) is common to both a
genuine and a degenerate generator, so no fact about that pair alone can
settle which side of the split the committed `G` is actually on.

## Obligation 3 — the hard non-vacuity obligation

`Generator.degenerate`/`genuine` only bite when `Input` satisfies
`UnboundedDomain` (`Generator.lean`'s own `bounded_domain_trivializes`: over
any finite domain every function is a lookup table, and the split collapses).
The abstract witness proves this is *achievable*; `unboundedDomain_byteInput`
below proves it *transfers* to a faithful concrete choice for what an atom's
generator actually consumes: arbitrary-length byte/bit strings (`List Bool`)
— the §8.3 witness's own `z`, the committed compressed literal, is exactly
such a string, of no fixed bound. No finite list of `List Bool` values covers
every `List Bool`: for any covering candidate, a string longer than every
member of the list cannot equal any of them (distinct lengths), so the
candidate never actually covers. This is a real transfer, not a convenient
evasion — build inputs are open-ended byte payloads in the ratified model
(`Basic.lean`'s own docstring: "a raw fetched payload"), never fixed-width.
-/

namespace Surety

variable {Signer ClassName Gen Input Output : Type} [DecidableEq ClassName]
  [DecidableEq Signer]

-- ===========================================================================
-- §1. Obligation 2 — the grounding: the path actually taken.
-- ===========================================================================

/-- **The path actually taken.** Every `RCAS` verdict forces the atom's own
    `srcEstablished` (the vouch clause) true — restated here, in
    `Grounding.lean`'s generator-aware vocabulary, from `Ceiling.lean`'s
    `atom_established_own_class`. Non-triviality: the statement carries no
    `Gen`/`Denote`/`degenerate` premise at all, and needs none — the fact
    that reaching `RCAS` is `Gen`-blind is not an assumption here, it is what
    the proof (an unmodified citation of the existing theorem) demonstrates
    by having nothing to case-split on. -/
theorem rcas_forces_established
    (GateExec : ClassName → Bool) (P : Policy Signer) (σ : Snapshot Signer ClassName)
    (i : Nat) (c : ClassName) (mode ca pf pp : Bool) (inputs : List (Artifact ClassName))
    (hrcas : classify GateExec P σ (Artifact.atom i c mode ca pf pp inputs) = .RCAS) :
    srcEstablished GateExec P σ c pf pp (Artifact.atom i c mode ca pf pp inputs) = true :=
  atom_established_own_class GateExec P σ i c mode ca pf pp inputs hrcas

-- ===========================================================================
-- §2. Shared helper: bounding a `Nat` list, used by both the
--     underdetermination witness below and `unboundedDomain_byteInput`.
-- ===========================================================================

/-- An element of a `Nat` list is bounded by the list's `foldr max`. Core-Lean
    helper, no Mathlib (matching this package's own precedent). -/
private theorem le_foldr_max : ∀ (xs : List Nat) (n : Nat), n ∈ xs → n ≤ xs.foldr max 0
  | [], _, hn => by cases hn
  | a :: as, n, hn => by
      rcases List.mem_cons.mp hn with h | h
      · subst h
        show n ≤ max n (as.foldr max 0)
        exact Nat.le_max_left _ _
      · show n ≤ max a (as.foldr max 0)
        exact Nat.le_trans (le_foldr_max as n h) (Nat.le_max_right _ _)

-- ===========================================================================
-- §3. Obligation 2a — the determination axis: a genuine/degenerate pair
--     realizing the same output, so the output alone cannot decide which.
-- ===========================================================================

/-- **The underdetermination witness.** A genuine generator (`true`, the
    identity on `Nat` — unbounded range, so `genuine`) and a degenerate one
    (`false`, constantly `42` — range `{42}`, so `degenerate`) both realize
    the *same* output `42`, the first on input `42` (identity(42) = 42), the
    second on input `0` (constant(0) = 42). Non-triviality: this is an actual
    exhibited pair over a concrete `Denote`, not a hypothesis about one — the
    tangible cash-out of `v0.4.md` §8's "the bytes underdetermine it" and
    Theorem 1(ii)'s "knowing the one input/output pair … buys the verifier
    nothing": the pair `(z, B) = (42, 42)` (or `(z', B) = (0, 42)`) is
    consistent with both a genuine and a degenerate generator, so observing
    it settles nothing about which side of the genuine/degenerate split the
    committed `G` is actually on. -/
theorem genuineness_underdetermined :
    ∃ (Gen Input Output : Type) (denote : Generator.Denote Gen Input Output)
      (G G' : Gen) (z z' : Input) (B : Output),
      Generator.genuine denote G ∧ Generator.degenerate denote G'
        ∧ Generator.Realizes denote G z B ∧ Generator.Realizes denote G' z' B := by
  refine ⟨Bool, Nat, Nat,
    fun g n => match g with | true => some n | false => some 42,
    true, false, 42, 0, 42, ?_, ?_, rfl, rfl⟩
  · -- genuine true: ¬ RangeFinite (fun n => some n) — no finite list bounds all of ℕ.
    rintro ⟨ys, hys⟩
    have hmem : (ys.foldr max 0 + 1) ∈ ys := hys _ _ rfl
    have hbound := le_foldr_max ys _ hmem
    omega
  · -- degenerate false: RangeFinite (fun _ => some 42), witnessed by [42].
    refine ⟨[42], fun x y hxy => ?_⟩
    have : (42 : Nat) = y := by injection hxy
    simp [this]

-- ===========================================================================
-- §4. Obligation 3 — the hard non-vacuity obligation: `UnboundedDomain`
--     transfers to a faithful concrete `Input` type.
-- ===========================================================================

/-- **The transfer, proved rather than merely asserted.** `List Bool` —
    arbitrary-length bit strings, the faithful shape of a committed byte
    payload (`Basic.lean`'s own "raw fetched payload" framing; the §8.3
    witness's `z` is exactly such a string) — satisfies `UnboundedDomain`:
    given any finite covering candidate `l`, the all-`false` string one
    longer than every member of `l` cannot equal any member of `l` (distinct
    lengths), so `l` fails to cover it. -/
theorem unboundedDomain_byteInput : Generator.UnboundedDomain (List Bool) := by
  intro l
  refine ⟨List.replicate ((l.map List.length).foldr max 0 + 1) false, ?_⟩
  generalize hn : (l.map List.length).foldr max 0 + 1 = n
  intro hmem
  have hlen : (List.replicate n false).length = n := by simp
  have hmem' : (List.replicate n false).length ∈ l.map List.length :=
    List.mem_map_of_mem hmem
  rw [hlen] at hmem'
  have hbound : n ≤ (l.map List.length).foldr max 0 := le_foldr_max (l.map List.length) n hmem'
  omega

end Surety
