# Lean surety-ceiling mechanization — reviewer guide

This corpus is a second, independent evaluator over the same classification
law as `../surety-of-source.md`'s Alloy models (`docs/specs/alloy/`): it
mechanizes the surety-ceiling classification (`classify`, the trust surface,
the assumption basis, `Total`) as Lean 4 definitions and proves the ceiling
theorems — the determination gate, the structural ceiling classification,
forced-generator grounding, and concrete non-vacuity — as **universally
quantified proofs**, not bounded-scope search. Where the Alloy Analyzer finds
a counterexample or its absence within a finite instance scope, these
theorems hold for every instance, with no scope bound at all, once a
definition is accepted as a faithful rendering of the prose. Every theorem
below is a checked Lean proof term with no `sorry`; see
[§11.2 of the proof text](#the-proven-vs-cited-boundary) for the one
deliberate exception (Theorem 1 / Rice's theorem, cited, not mechanized).

For the full prose account — the model this corpus mechanizes, the ceiling
theorems in natural language, and the assumption base — see
[`../surety-of-source.md` §11, "The Lean mechanization"](../surety-of-source.md#11-the-lean-mechanization).
This README is a build/navigation entry point; it does not restate that
material.

## File guide

Six files, in build order (the order the ground-truth prose also presents
them in):

- **[`SuretyEonEalm.lean`](SuretyEonEalm.lean)** — the determination gate,
  instantiated over EON/EALM's own upstream machinery (a distinct formal
  system from the five files below; see the cross-repo dependency, next
  section). `genuineness_admits_no_snapshot_scheme`: if genuineness is not
  record-determined (`hfiber : ¬ Determined genuineness`, carried as an
  explicit hypothesis, never proved or smuggled in as an axiom), genuineness
  admits no snapshot-sound evidence scheme over any commitment — no verifier
  certifying from committed bytes alone can characterize it.
- **[`SuretyCeiling/Basic.lean`](SuretyCeiling/Basic.lean)** — `classify` as
  a total, computable function over an inductive `Artifact` (`.leaf` /
  `.atom`). Acyclicity is true by the shape of the type (a build input is a
  strictly smaller subterm), not an asserted axiom.
- **[`SuretyCeiling/Ceiling.lean`](SuretyCeiling/Ceiling.lean)** —
  `trustSurface` ($T(a)$), `basis` ($B(a)$), and `Total`, plus the ceiling
  theorems: `laundered_never_total` and `total_carries_vouch_in_basis`
  (Theorem 3a), `no_silent_laundering`, `declaration_alone_never_closes`,
  `seeds_are_trustImports`, and `atom_established_own_class`.
- **[`SuretyCeiling/Generator.lean`](SuretyCeiling/Generator.lean)** — the
  forced generator: `degenerate` / `genuine` over an abstract denotation
  map, `degenerate_extensional` (the semantic property Rice's theorem
  requires, proved rather than assumed), and `bounded_domain_trivializes`
  (why the unbounded-domain clause is load-bearing).
- **[`SuretyCeiling/Grounding.lean`](SuretyCeiling/Grounding.lean)** — binds
  the generator model to the ceiling: `rcas_forces_established` (every
  `RCAS` verdict forces `srcEstablished`'s vouch clause true, with no
  premise about the generator at all) and `genuineness_underdetermined` (a
  concrete genuine generator and a concrete degenerate one realizing the
  identical output on some input — the tangible form of Theorem 1(ii)).
- **[`SuretyCeiling/Nonvacuity.lean`](SuretyCeiling/Nonvacuity.lean)** —
  concrete witnesses proving the ceiling theorems bite on reachable
  configurations, not merely hold vacuously: a non-seed atom that is
  `RCAS`, `Total`, and carries a real vouch in its basis, in both the
  empty-trust-surface case (`posAtom`) and the characteristic
  base-bounded case (`baseBoundAtom`, `Total` via a non-empty surface whose
  sole member is a genesis seed); and a non-seed atom whose establishment
  fails (`negAtom`), shown never `Total`.

`SuretyCeiling.lean` (package root) is the aggregate import
(`Basic` + `Ceiling` + `Grounding` + `Nonvacuity`); `Grounding` itself
imports `Generator`, so all five `SuretyCeiling/*` files are reachable from
this one root.

## Build and verify

**Toolchain:** `lean-toolchain` pins `leanprover/lean4:v4.29.1`.

**Cross-repo prerequisite — read this before running `lake build`.** This
package is not self-contained: `lakefile.toml` declares a **path** (not git,
not vendored) require on this project's own upstream EON/EALM Lean corpus:

```toml
[[require]]
name = "EonEalm"
path = "../../../../../Cyphrme/Cyphr/docs/models/lean"
```

That relative path resolves correctly only if this repository
(`axiosoph/axios`) and `Cyphrme/Cyphr` are checked out as **sibling
directories under the same `github.com/` parent** — e.g.
`.../github.com/axiosoph/axios` and `.../github.com/Cyphrme/Cyphr` side by
side. A reviewer with only this repository cloned cannot build the package;
clone `Cyphrme/Cyphr` alongside it first. (This mirrors Cyphr's own
`docs/models/lean-eml-bridge` package, which cites eml's Lean corpus the
same way, and both sides pin the identical toolchain, `v4.29.1`.)

With both repos present in that layout:

```sh
cd docs/models/lean-surety
lake build
```

This builds both library targets (`SuretyEonEalm` and `SuretyCeiling`,
`lakefile.toml`'s `defaultTargets`). Neither depends on Mathlib.

## The proven-vs-cited boundary

Every theorem named above is a machine-checked Lean proof (`#print axioms`
reports no `sorryAx` anywhere in the corpus — `SuretyEonEalm.lean`'s one
theorem additionally depends on `[Context, Entry]`, EON/EALM's own opaque
model-type parameters, not a classical or quotient axiom). One thing is
deliberately **not** mechanized: **Theorem 1** (`../surety-of-source.md`
§9.1 — Rice's theorem applied to the index set of finite-range partial
computable functions) is cited in prose, never proved as an internal Lean
fact. `Generator.lean` carries no computation model, no program-size notion,
and no `Decidable` instance for `degenerate` — there is nothing for an
undecidability theorem to be stated *about*. No theorem in this corpus takes
an undecidability hypothesis, because none of the mechanized results need
one: `rcas_forces_established` holds unconditionally, and
`genuineness_underdetermined` is a concrete exhibited pair, not a claim
resting on Rice's theorem being assumed.

A second, related finding: a correspondence between `launderedShape` and
`degenerate` ("a laundered atom's forced generator is degenerate") was
sought while building `Grounding.lean` and found **false in both
directions** — see `Grounding.lean`'s own header and
[`../surety-of-source.md` §11.1](../surety-of-source.md#111-the-decoupling-classification-is-vouch-decided-not-degeneracy-decided)
for the full argument. This is reported as a negative result, not papered
over: no correspondence theorem is stated anywhere in this corpus.

For the complete assumption base (`hfiber` and Theorem 1, the only two
premises the result rests on) and the contrast with the bounded-scope Alloy
models under `../../specs/alloy/`, see
[`../surety-of-source.md` §11.2](../surety-of-source.md#112-proven-here-vs-cited-theorem-1-is-not-mechanized)
and [§11.3](../surety-of-source.md#113-the-assumption-base).

## Honest bound

Unlike the Alloy models, re-verified in CI on every push and pull request
(`.github/workflows/model-check.yml`), this Lean corpus is **not yet wired
into CI**: the results above are re-verifiable by `lake build` against this
repository state, not continuously re-checked on every change. See
[`../surety-of-source.md` §11.4](../surety-of-source.md#114-honest-bounds).
