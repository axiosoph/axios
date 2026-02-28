module atom_structure

// ============================================================================
// 1. SIGNATURES (Ontology from Model §1 Olog)
// ============================================================================

sig Anchor {}
sig Label {}
sig Version {}
sig Owner {}
sig Czd {}
sig Dig {}
sig Src {}

-- Source: The canonical upstream location of publishing
sig Source {
  anchor: one Anchor
}

-- AtomId: Deterministic pair of (Anchor, Label)
sig AtomId {
  anchor: one Anchor,
  label: one Label
}

-- AtomSet: Collection of atoms sharing a common anchor
sig AtomSet {
  setAnchor: one Anchor
}

-- Manifest: Ecosystem-agnostic package metadata
sig Manifest {
  label: one Label,
  version: one Version
}

-- Atom: Fundamental unit of publishing (detached subtree)
sig Atom {
  source: one Source,
  atomSet: one AtomSet,
  manifest: one Manifest,
  atomId: one AtomId,
  label: one Label
}

-- Claim: Establishes ownership over an AtomId
sig Claim {
  atomId: one AtomId,
  owner: one Owner,
  source: one Source,
  czd: one Czd,
  claimAnchor: one Anchor,  -- symmetric payloads
  claimLabel: one Label     -- symmetric payloads
}

-- Publish: Extends the identity with an immutable version snapshot
sig Publish {
  atomId: one AtomId,
  version: one Version,
  claimCzd: one Czd,
  dig: one Dig,
  src: one Src
}

-- AtomSource: Read-only trait interface (coalgebra observer)
sig AtomSource {
  atoms: set AtomId
}

-- AtomStore: Extends AtomSource (forgetful functor), aggregates from remotes
sig AtomStore extends AtomSource {
  ingested: set AtomSource
}

// ============================================================================
// 2. FUNCTIONS & PREDICATES
// ============================================================================

-- Local content-addressed derivation simulator
fun computeId[a: Anchor, l: Label]: set AtomId {
  { id: AtomId | id.anchor = a and id.label = l }
}

-- Ingest condition definition
pred after_ingest[st: AtomStore, s: AtomSource] {
  s in st.ingested
}

-- Fork Scenario: Multiple sources share the same anchor,
-- different owners claim the same AtomId.
pred fork_scenario {
  some disj s1, s2: Source, disj c1, c2: Claim |
    s1.anchor = s2.anchor and
    c1.source = s1 and c2.source = s2 and
    c1.atomId = c2.atomId and
    c1.owner != c2.owner
}

// ============================================================================
// 3. FACTS (Structural Invariants & Constraints)
// ============================================================================

fact identity_bijection {
  -- Identity Computation (olog §1): `computed_from: AtomId → (Anchor × Label)` is a bijection.
  -- Two atoms with the same anchor and label MUST have the same AtomId.
  all a1, a2: AtomId | (a1.anchor = a2.anchor and a1.label = a2.label) implies a1 = a2
}

fact claim_properties {
  -- Czd is a strictly unique digest for each claim
  all c1, c2: Claim | c1.czd = c2.czd implies c1 = c2
  -- Symmetric payloads: Claim carries raw anchor/label matching its atomId
  all c: Claim | c.claimAnchor = c.atomId.anchor and c.claimLabel = c.atomId.label
  -- The claim's anchor must match the source's derivation
  all c: Claim | c.source.anchor = c.claimAnchor
}

fact publish_properties {
  -- Verification Chain Completeness ([publish-chains-claim] & [publish-claim-coherence])
  -- The atomId in the publish MUST strictly match the atomId of the referenced claim.
  all p: Publish | some c: Claim |
    c.czd = p.claimCzd and p.atomId = c.atomId
}

fact store_topology {
  -- Source/Store Topology (model §2.3, §2.6)
  -- ⊇ condition: after ingest, all atoms natively in the source exist in the store.
  all st: AtomStore, s: AtomSource |
    after_ingest[st, s] implies s.atoms in st.atoms
}

fact anchor_properties {
  -- Anchor Properties (§Anchor)
  -- All atoms sharing an anchor structurally belong to the same atom-set.
  all a: Atom | a.atomSet.setAnchor = a.source.anchor
  -- AtomSets are uniquely identified purely by their anchor.
  all as1, as2: AtomSet | as1.setAnchor = as2.setAnchor implies as1 = as2
  -- Identity derivation: AtomId traces to source's anchor and atom's label.
  all a: Atom | a.atomId.anchor = a.source.anchor and a.atomId.label = a.label
}

fact manifest_properties {
  -- Manifest Minimality ([manifest-minimal])
  -- Manifest exactly reflects the atom's human-readable identifier.
  all a: Atom | a.manifest.label = a.label
}

// ============================================================================
// 4. ASSERTIONS & VERIFICATIONS
// ============================================================================

// [identity-content-addressed]
assert identity_content_addressed {
  all a1, a2: AtomId |
    (a1.anchor = a2.anchor and a1.label = a2.label) implies a1 = a2
}

// [ownership-independence]
assert ownership_independence {
  all a: AtomId, c1, c2: Claim |
    (c1.atomId = a and c2.atomId = a and c1.owner != c2.owner)
    implies c1.atomId = c2.atomId
  -- ownership changes don't alter identity
}

// [ingest-preserves-identity]
assert ingest_preserves_identity {
  all s: AtomSource, st: AtomStore, a: AtomId |
    a in s.atoms implies (after_ingest[st, s] implies a in st.atoms)
}

// [anchor-set-coherence]
assert anchor_set_coherence {
  all a1, a2: Atom |
    (a1.source.anchor = a2.source.anchor)
    implies a1.atomSet = a2.atomSet
}

// [verification-chain]
assert verification_chain {
  all p: Publish |
    some c: Claim |
      c.czd = p.claimCzd and
      computeId[c.claimAnchor, c.claimLabel] = p.atomId
}

// ============================================================================
// 5. RUN / CHECK BLOCKS
// ============================================================================

check identity_content_addressed for 4
check ownership_independence for 4
check ingest_preserves_identity for 4
check anchor_set_coherence for 4
check verification_chain for 4

run fork_scenario for 4
