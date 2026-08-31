/-!
# Surety Ceiling — Generator: the `⟦·⟧` denotation and degeneracy

Models the **forced generator** object from `docs/models/surety-of-source.md`
§8 ("`v0.4.md`" below) and the blog's
`.scratch/blog/everything-is-a-trust-decision-final.md` ("The Forced
Generator" section): a program `G`, committed in a laundering atom's tree,
whose execution under the atom's build plan reconstructs the atom's output
bytes. `⟦G⟧` is `G`'s **denotation** — the (partial) computable function it
computes, in the Scott–Strachey sense the corpus borrows the bracket
notation from. Ground truth throughout: `v0.4.md` §8 (`degenerate(G) ⟺
finite(range(⟦G⟧))`, considered "with its nominal, unbounded input
signature"), the blog's matching definition and underdetermination display,
and the §8.3 compressed-blob witness (`stock inflate` vs. the rigged
one-shot emitter).

This is **PHASE 1** of a two-phase build: the generator model only, for a
faithfulness check before any ceiling theorem is stated over it. Nothing
below asserts a ceiling claim.

## What this module deliberately does NOT do

- **No computation model, hence no undecidability theorem.** `Gen` carries
  no evaluator, no notion of program size, no `Decidable` instance for
  `degenerate` — there is nothing here for Rice's theorem (`v0.4.md`
  Theorem 1) to be stated *about* as an internal fact. Theorem 1 (and its
  promise-refinement, (ii)) is cited evidence in the surrounding prose, not
  mechanized in this repo's Lean corpus, and this module does not attempt
  a first mechanization of it either — only the **predicate** `degenerate`
  it ranges over.
- **No claim that `RCAS`/establishment is "the generator genuinely
  computing."** See the note below; that framing does not match the
  ratified model and is corrected here rather than encoded.
- **No wiring into `SuretyCeiling.Basic`/`SuretyCeiling.Ceiling`.** Those
  files mechanize `classify`/`T(a)`/`B(a)`/`Total` directly from
  `v0.4.md` §2–§4 and already exist in this working tree (uncommitted,
  apparently from a concurrent effort — see the report accompanying this
  file). This module does not import or modify them; how the two compose
  is a Phase 2 question for the composer to settle first.

## Why "RCAS iff the generator is genuine" is not modeled

The dispatch for this file asked for "`ReproducibleCASource` in terms of the
generator genuinely computing." That equivalence is not in the ratified
model, and stating it would be a defeater, not a simplification. `v0.4.md`
§10 is explicit: *"The forced generator appears in the model only through
its structural consequence ...; its degeneracy is never computed — the
model does not decide the undecidable."* Establishment (`v0.4.md` §5.3)
never inspects a generator's degeneracy — it substitutes a
**source-class-vouch** for exactly the check that Theorem 1 forecloses.
The correct relationship, left for Phase 2 to state as a theorem: a build
realized through a forced generator (`Realizes` below) can only reach
`RCAS` via the vouch path, never by any predicate over `genuine`/
`degenerate` — because no such predicate is decidable to appeal to. That
is the ceiling-shaped content the two-place absence (no genuineness check
in `classify`, a vouch in its place) is evidence of.

## No Mathlib

Following this package's own precedent (`SuretyEonEalm.lean`'s header;
`SuretyCeiling.Basic`'s docstring on avoiding a spurious `DecidableEq`
obligation), finiteness of a range is stated without `Set.Finite`: a
function's range is finite iff some `List Output` bounds every value it
ever produces. This is classically equivalent to `Set.Finite` over the
induced set of realized outputs, without a new Mathlib dependency.
-/

namespace Surety
namespace Generator

/-- **A generator's denotation map** `⟦·⟧ : Gen → (Input ⇀ Output)` — the
    same operator the corpus uses for `⟦c⟧` compositions
    (`docs/models/composition-model.md` §2) and `⟦req⟧` execution requests
    (`docs/models/execution-model.md` §2.2), reused here for `Gen`
    (`v0.4.md` §8.1: "a generator is one more thing this corpus assigns a
    meaning to, not a new formalism"). Partiality (`Option Output`) models
    non-halting or no output on a given input. Left as a bare parameter
    rather than an evaluator defined by recursion on `Gen`'s syntax,
    precisely because every definition below depends only on this map, never
    on how `G` is written — `degenerate_extensional` cashes that out as a
    proved fact, not an assumption. -/
abbrev Denote (Gen Input Output : Type) : Type := Gen → Input → Option Output

variable {Gen Input Output : Type}

/-- **Range-finiteness**, without `Set.Finite`: `f`'s range is finite iff
    some list of outputs bounds every value `f` ever produces. -/
def RangeFinite (f : Input → Option Output) : Prop :=
  ∃ ys : List Output, ∀ x y, f x = some y → y ∈ ys

/-- **Degeneracy** (`v0.4.md` §8.1, the blog's matching definition):
    `G` is degenerate iff `⟦G⟧`'s range is finite — emission from a finite
    table, uniformly covering the constant emitter and the lookup-table
    emitter, as opposed to a genuine parameterized transformation whose
    range over an unbounded domain is infinite. `G` is considered here with
    its *nominal* input type `Input` — the domain finiteness is measured
    against is the generator's declared signature, never the single
    committed argument any one build actually supplied (`Realizes` below is
    the latter, and is a different, non-quantified notion). A purely
    *semantic* (extensional) property of `⟦G⟧` alone, never of `G`'s syntax,
    size, or program text — exactly Rice's theorem's precondition
    (`v0.4.md` §8.2, §9.1(i); the theorem itself is cited, not proved,
    here). -/
def degenerate (denote : Denote Gen Input Output) (G : Gen) : Prop :=
  RangeFinite (denote G)

/-- **Genuineness**: the complementary case, an infinite-range, genuinely
    parameterized transformation. The prose poses the question as a strict
    binary ("Is this a genuine parameterized transformation, or a
    hand-rigged emitter ... whose range is finite?"), which is sound
    because finite/infinite range is exhaustive by construction (classical
    logic) — there is no third case being elided. -/
def genuine (denote : Denote Gen Input Output) (G : Gen) : Prop :=
  ¬ degenerate denote G

theorem genuine_iff_infinite_range (denote : Denote Gen Input Output) (G : Gen) :
    genuine denote G ↔ ¬ RangeFinite (denote G) := Iff.rfl

/-- **Degeneracy depends only on `⟦G⟧`** — the extensionality Rice's
    theorem requires (`v0.4.md` §8.2), stated as a proved consequence of the
    definition rather than merely asserted: two generators, however
    differently written, with the same denotation are degenerate (or not)
    together. -/
theorem degenerate_extensional (denote : Denote Gen Input Output) {G G' : Gen}
    (h : denote G = denote G') : degenerate denote G ↔ degenerate denote G' := by
  simp only [degenerate, h]

/-- **The unbounded-domain clause, made precise.** `Input` has no finite
    covering list. This is what `v0.4.md` §8.1 calls load-bearing ("over
    any finite domain every function is a lookup table and the property
    trivializes") — stated as a hypothesis a later theorem may carry, not
    baked into `degenerate`'s own definition (which stays meaningful, if
    trivial, without it: see `bounded_domain_trivializes`). -/
def UnboundedDomain (Input : Type) : Prop := ∀ l : List Input, ∃ x : Input, x ∉ l

/-- **The trivialization, proved rather than merely asserted.** Over a
    domain covered by a finite list, *every* function is degenerate — the
    distinction collapses exactly as the prose warns, which is why
    `degenerate` is only a meaningful (non-vacuous) split when `Input`
    satisfies `UnboundedDomain`. -/
theorem bounded_domain_trivializes {l : List Input} (hcover : ∀ x : Input, x ∈ l)
    (f : Input → Option Output) : RangeFinite f := by
  refine ⟨l.filterMap f, fun x y hxy => ?_⟩
  exact List.mem_filterMap.mpr ⟨x, hcover x, hxy⟩

/-- **A build, as a generator applied to committed source** (`v0.4.md`
    §8.1's forced-generator instantiation, and the blog's "Evade [the
    gates], and hermeticity forces your hand"): the atom's build record
    realizes `B = ⟦G⟧(z)` for the atom's own committed input `z` and
    committed output `B` — the *one* input/output pair the build actually
    exhibited. Deliberately **not** `degenerate`/`genuine`: those quantify
    over the whole of `Input`, this over a single witnessed pair. Keeping
    the two apart is exactly Theorem 1(ii)'s content: the witnessed pair
    buys the verifier nothing about `G`'s behavior on the rest of its
    domain — a fact this module states as a shape (the two predicates take
    different arguments) rather than proves (the proof is the cited
    halting-problem reduction, out of Phase 1's scope). -/
def Realizes (denote : Denote Gen Input Output) (G : Gen) (z : Input) (B : Output) : Prop :=
  denote G z = some B

/-- **Laundering via a degenerate forced generator** — the generator-
    specific slice of `v0.4.md` §8.1's laundering definition ("a smuggled
    container, a non-parsing payload, a literal emission, or a computed
    emission through a degenerate generator"): a build that realizes its
    output through a generator that is, in fact, degenerate. The §8.3
    witness is the canonical instance: `G` hardcodes one compressed literal
    `z` and always inflates it, so `⟦G⟧`'s range is finite (a single point,
    in the sharpest case) regardless of what `G`'s nominal signature could
    accept — as opposed to stock `inflate`, whose range over varying
    compressed inputs is unbounded. -/
def GeneratorLaundering (denote : Denote Gen Input Output) (G : Gen) (z : Input)
    (B : Output) : Prop :=
  Realizes denote G z B ∧ degenerate denote G

/-- A `GeneratorLaundering` build is, in particular, a `Realizes` witness —
    recorded as a one-line sanity theorem, not left merely implicit in the
    `∧`. -/
theorem generatorLaundering_realizes (denote : Denote Gen Input Output) {G : Gen}
    {z : Input} {B : Output} (h : GeneratorLaundering denote G z B) :
    Realizes denote G z B := h.1

end Generator
end Surety
