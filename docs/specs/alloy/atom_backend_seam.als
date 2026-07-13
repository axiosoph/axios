module atom_backend_seam

// ============================================================================
// BACKEND SEAM MODEL --- the typed seam law made checkable
// ============================================================================
//
// Charge (docs/specs/atom-backend-contract.md §Verification): this model
// verifies the seam law that `atom_structure.als` structurally CANNOT reach.
// That sibling model already declares `Czd`, `Dig`, `Src` as disjoint
// top-level sorts (a direct czd-vs-dig comparison is analyzer-rejected
// today), but those sorts are ANCESTRY-FREE FIELD ATOMS: there is no
// unified `OID` sort carrying hash-committed `parents` (so P15's "asserting
// false ancestry requires a collision" is inexpressible), and NO carrier
// objects at all (so the czd/carrier seam --- czd computed over payload
// bytes, independent of `oid(carrier(m))` --- has no surface to be checked
// on). This model supplies exactly those two surfaces. It does NOT restate
// or re-verify any of atom_structure.als's eight assertions.
//
// Normative sources formalized here:
//   [backend-seam-typed]   --- `Czd` and `OID` MUST be disjoint sorts; a
//                              message's protocol content-address is never
//                              its carrier's backend identity.
//   [backend-ancestry-sound] / P15 --- a revision OID commits to its
//                              hash-committed parents; `⊑` is derived from
//                              `parents`, and false ancestry requires a
//                              hash collision.
//
// SCOPE: 4 (c5). Chosen to mirror the sibling `atom_structure.als` (scope 4)
// so the two models are checked at a common bound. Scope 4 is SUFFICIENT for
// this model's witnesses: a hash-collision scenario needs two objects sharing
// one OID plus two distinct parent objects (4 objects, 3 OIDs), and the
// seam-independence scenario needs two messages over two carriers --- both
// fit strictly inside scope 4. No property here is scope-sensitive beyond
// exhibiting these small witnesses, so a larger scope buys no coverage.
//
// DELEGATED DECISIONS (IBC S3):
//   * `parents` is `set Object` (committing `parents.oid`), not `Seq<OID>`.
//     Order is a preimage detail; the soundness law quantifies over the
//     committed parent SET, so set-vs-sequence does not change the
//     collision-gated injectivity being checked.
//   * a hash collision is an EXPLICIT `Collision` signature (not the mere
//     negation of an injectivity fact), so the SAT scenario can exhibit the
//     collision as the sole escape hatch from ancestry soundness.

// ============================================================================
// 1. SORTS
// ============================================================================

// Protocol content-address: coz digest over signed payload bytes.
sig Czd {}
// Transaction-payload field sorts named by [backend-seam-typed] (the `dig`
// and `src` OID-rendering surfaces). Declared so OID's disjointness from the
// full protocol sort inventory is stated, not merely from `Czd`.
sig Dig {}
sig Src {}

// Backend object identifier: the backend-chosen hash of a stored object.
// A distinct top-level sig, hence disjoint from Czd/Dig/Src by construction
// (the [backend-seam-typed] `Czd ∩ OID = ∅` law, and its Dig/Src analogues).
sig OID {}

// Content bytes, abstracted to an identity. Both stored objects and coz
// messages are addressed over their payload bytes.
sig Payload {}

// ============================================================================
// 2. CARRIER SURFACE (the czd/carrier seam --- absent in atom_structure.als)
// ============================================================================

// A stored backend object: content bytes plus the hash-committed parent
// links of the revision Merkle DAG. Its `oid` is the backend identity that
// commits to (payload, parents).
sig Object {
  oid:     one OID,
  payload: one Payload,
  parents: set Object
}

// A coz message: payload bytes, the protocol content-address `czd` computed
// over those bytes, and the backend `carrier` object that happens to store
// it. `czd` and `oid(carrier)` are the two identities the seam keeps apart.
sig CozMessage {
  msgPayload: one Payload,
  czd:        one Czd,
  carrier:    one Object
}

// An exhibited hash collision: two distinct objects whose (payload, parent-
// oid) preimages differ yet share one OID. Its presence is the ONLY way
// ancestry soundness can be escaped.
sig Collision {
  lo: one Object,
  hi: one Object
}

// ============================================================================
// 3. DERIVED RELATIONS & PREDICATES
// ============================================================================

// Two objects share a hash preimage iff they commit the same payload bytes
// and the same set of parent OIDs. (Parent OIDs, not parent objects: the
// hash embeds the parents' identifiers.)
pred samePreimage[o1, o2: Object] {
  o1.payload = o2.payload and o1.parents.oid = o2.parents.oid
}

// Whether a modeled collision relates o1 and o2 (either orientation).
pred collides[o1, o2: Object] {
  some k: Collision |
    (k.lo = o1 and k.hi = o2) or (k.lo = o2 and k.hi = o1)
}

// Ancestry `⊑` (docs/specs/atom-backend-contract.md §Sorts): the
// reflexive-transitive closure of the hash-committed `parents` links ---
// DERIVED from `parents`, never an independent input.
fun ancestors[o: Object]: set Object { o.^parents }

// ============================================================================
// 4. FACTS
// ============================================================================

fact hash_commitment {
  // Determinism: the backend hash is a function of the preimage. Objects
  // committing the same (payload, parent-oids) carry the same OID.
  all o1, o2: Object |
    samePreimage[o1, o2] implies o1.oid = o2.oid
  // Collision-gated injectivity: an OID commits to its preimage. Two objects
  // may share one OID ONLY by exhibiting a hash collision --- rebinding an
  // OID to a different preimage is otherwise impossible.
  all disj o1, o2: Object |
    (o1.oid = o2.oid and not samePreimage[o1, o2]) implies collides[o1, o2]
}

fact collision_wellformed {
  // A Collision is a genuine witness: distinct objects, one shared OID,
  // differing preimages. Rules out spurious collision atoms.
  all k: Collision |
    k.lo != k.hi and k.lo.oid = k.hi.oid and not samePreimage[k.lo, k.hi]
}

fact dag_acyclic {
  // The revision DAG is acyclic: no object is its own ancestor.
  all o: Object | o not in ancestors[o]
}

fact czd_over_payload {
  // [backend-seam-typed]: `czd(m)` is computed over payload bytes ALONE ---
  // independent of which backend object carries `m`. Same payload => same
  // czd, whatever the carrier's OID.
  all m1, m2: CozMessage |
    m1.msgPayload = m2.msgPayload implies m1.czd = m2.czd
}

// ============================================================================
// 5. ASSERTIONS
// ============================================================================

// [c1] OID is disjoint from every protocol field sort. Structural (distinct
// top-level sigs), stated explicitly as this model's typed-seam foundation.
assert oid_disjoint_from_protocol_sorts {
  no (OID & Czd)
  no (OID & Dig)
  no (OID & Src)
}

// [c2 / P15] Ancestry soundness. A revision OID commits to its hash-committed
// parent set: two objects that share an OID yet commit to DIFFERENT parents
// cannot coexist without a modeled hash collision --- "asserting false
// ancestry requires a hash collision." Transitivity lifts by induction ---
// each parent OID likewise commits to ITS parents --- so the depth-1 law
// stated here, holding over all objects, closes the full committed ancestry.
assert ancestry_soundness {
  all disj o1, o2: Object |
    (o1.oid = o2.oid and o1.parents.oid != o2.parents.oid)
      implies collides[o1, o2]
}

// [c3 / backend-seam-typed] Carrier/czd seam. A protocol content-address is
// never a backend object identity: `Czd ∩ OID = ∅` holds on the carrier
// surface, so no message's czd can be conflated with the OID of any carrier.
// (A direct `czd = oid` equality is additionally rejected by the analyzer's
// type checker; this set-level statement is what a `check` can evaluate.)
assert carrier_czd_seam {
  no (Czd & OID)
  no (CozMessage.czd & Object.oid)
}

// ============================================================================
// 6. CHECK / RUN BLOCKS
// ============================================================================

check oid_disjoint_from_protocol_sorts for 4
check ancestry_soundness for 4
check carrier_czd_seam for 4

// Non-vacuity (c4): a hash collision is what lets two objects with DIFFERENT
// committed ancestry share one OID --- forging false ancestry. Exhibiting it
// proves the collision is a real, reachable escape hatch, not dead syntax.
pred collision_forges_ancestry {
  some disj o1, o2: Object |
    o1.oid = o2.oid and
    o1.parents.oid != o2.parents.oid and
    collides[o1, o2]
}
run collision_forges_ancestry for 4

// Non-vacuity (c4): two messages over the SAME payload carry the SAME czd yet
// sit on carriers with DISTINCT OIDs --- czd is fixed by content while the
// carrier identity varies freely. The seam is a genuine separation, not a
// bijection: neither identity determines the other.
pred seam_independence {
  some disj m1, m2: CozMessage |
    m1.msgPayload = m2.msgPayload and
    m1.carrier.oid != m2.carrier.oid
}
run seam_independence for 4
