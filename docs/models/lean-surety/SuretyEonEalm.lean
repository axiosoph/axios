import EonEalm

/-!
# SuretyEonEalm — the determination-gate instantiation for atom genuineness

Instantiates EON/EALM's proven determination gate
(`EonEalm.snapshot_characterization_determined`, `lean:Result1.lean:46-55`) for
axios's own **genuineness** claim, cashing out `.ledger/surety/statement-v0.3.md`
§4: *genuineness is not record-determined ⟹ it admits no evidence scheme.*
Ground truth: `statement-v0.3.md` §2 (the FLAG-8 fiber witness) and §4 (the
instantiation, hand-traced against this actual source by round-2 refuter B
with no mismatch found, `statement-v0.3.md:387-389`).

## The identification (bridge (a), genuineness-only scope)

An atom's committed bytes (`w : Record`, tree + build plan) do not, by
themselves, fix *whose* artifact they honestly are — the same bytes can
result from the committer's own build or from a third party's binary
laundered through identical output (the FLAG-8 witness,
`statement-v0.3.md:177-190`: `(stock inflate, z = deflate(B))`, honest-B vs
laundered-B, same committed bytes, two histories). That "whose" is exactly
the **unrecorded authorship history** (D3, `statement-v0.3.md` §1) — and
EON/EALM already charters a single global type of such ambient facts:
`EonEalm.Context`, "authorship facts, intentions, key custody, other
artifacts" (`lean:Model.lean:54-55`). No new type is introduced; the
identification is stated, not smuggled — `genuineness` below is an
`EonEalm.Claim := Record → Context → Prop` like any other, rigid in the one
global `Context` axiom `Determined`/`Scheme`/`SnapshotSound` are all stated
over (`lean:Axes.lean:21`, `lean:Schemes.lean:45,55`).

## `hfiber` — a modeling hypothesis, not a theorem

`hfiber : ¬ Determined genuineness` is the FLAG-8 witness's content, taken as
an **explicit hypothesis** — the same register EON/EALM uses for its own
Lemma-1 witnesses ("modeling facts, not mechanized", the EON/EALM statement's
own `v0.3:57-58`, distinct from this repo's `statement-v0.3.md`). It is not
proved here, and must not be: proving it would mean picking a concrete
`GenuineIn` and a concrete pair of contexts, exactly the FLAG-8 ratification
this package is scoped *not* to prejudge. Every theorem below carries it as
an explicit argument, never a `sorry`, never a fresh axiom.

## What this instantiation does and does not establish

`genuineness_admits_no_snapshot_scheme` is
`EonEalm.snapshot_characterization_determined` applied contrapositively: any
snapshot-sound scheme forces `Determined genuineness`, contradicting
`hfiber`. This is bridge (a)'s genuineness-only scope
(`statement-v0.3.md:241`) — it says nothing about T1's execute-and-diff gate
(c), which never appears in `EonEalm.Result1`.
-/

namespace SuretyEonEalm

open EonEalm

-- The genuineness verdict, left abstract: `w` is the committed bytes (tree +
-- build plan), `ξ` is the unrecorded authorship/provenance context
-- (`EonEalm.Context`) it was actually produced under. `variable` cannot carry
-- a doc-comment (Lean parses `/-- -/` only before a declaration).
variable (GenuineIn : Record → Context → Prop)

/-- Genuineness as an `EonEalm.Claim` — no new structure, the direct
    identification `statement-v0.3.md` §4 states. -/
def genuineness : Claim := fun w ξ => GenuineIn w ξ

/-- **The surety determination gate, instantiated.** If genuineness is not
    record-determined (`hfiber` — the FLAG-8 witness, an explicit modeling
    hypothesis), it admits no snapshot-sound evidence scheme over any
    commitment `Γ`: no committed-bytes-only verifier can certify genuineness
    without also depending on the (unrecorded) authorship context. -/
theorem genuineness_admits_no_snapshot_scheme
    (hfiber : ¬ Determined (genuineness GenuineIn))
    {Comm : Type} (Γ : Commitment Comm) :
    ¬ ∃ S : Scheme Γ (genuineness GenuineIn), SnapshotSound S :=
  fun ⟨S, hs⟩ => hfiber (snapshot_characterization_determined S hs)

end SuretyEonEalm
