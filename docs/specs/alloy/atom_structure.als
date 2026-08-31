module atom_structure

// ============================================================================
// 1. SIGNATURES (Ontology from Model §1 Olog)
// ============================================================================

// [anchor-content-addressed] (amended 2026-07-08): an anchor IS a coz digest --
// specifically czd(charter) -- so at the type level an Anchor is a Czd. The
// charter facts below pin it to the charter's own czd directly: genesis-once
// (ADR-0007 SS3, 2026-08-05) means there is exactly one charter per anchor,
// ever, so "the charter" needs no founding/successor qualifier anymore.
sig Anchor in Czd {}
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

-- AtomId: an opaque digest -- the pre-signature preimage identity of an
-- UNCLAIMED atom (glossary, "Atom identity (and the unclaimed development
-- atom)"; the degenerate case, glossary-repudiated 2026-07-18 as anything
-- more). It carries no `anchor`/`label` fields: a digest has no legible
-- internal structure to recover, unlike the fused pair the old doctrine
-- asserted. Its determinism (same (anchor, label) -> same digest,
-- collision-free) is stated over Atom in `atomid_content_addressed` below,
-- since Atom -- not AtomId -- is what carries the two preimage inputs.
-- A CLAIMED atom's identity is its claim's own `czd` field directly (see
-- Claim below) -- there is no separate AtomId on Claim or Publish.
sig AtomId {}

-- AtomSet: Collection of atoms sharing a common anchor
sig AtomSet {
  setAnchor: one Anchor
}

-- Manifest: Ecosystem-agnostic package metadata
sig Manifest {
  label: one Label,
  version: one Version
}

-- Atom: Fundamental unit of publishing (detached subtree) -- the CONTENT
-- object, independent of claim status. Its atomId is the pre-claim
-- preimage digest: the degenerate identity of an unclaimed development
-- atom, never the atom's permanent identity once claimed.
sig Atom {
  source: one Source,
  atomSet: one AtomSet,
  manifest: one Manifest,
  label: one Label,
  atomId: one AtomId
}

-- Claim: Establishes ownership over a label. A claim's identity is its OWN
-- czd -- not the atomId of the content it claims (glossary-repudiated,
-- 2026-07-18: "the fused pair has no role outside the pre-signature
-- preimage of an unclaimed development atom"). claimAnchor/claimLabel are
-- the symmetric-payload raw fields ([symmetric-payloads]) -- a consumer
-- coherence check against the source, never an identity reconstruction.
sig Claim {
  owner: one Owner,
  source: one Source,
  czd: one Czd,
  claimAnchor: one Anchor,  -- symmetric payloads
  claimLabel: one Label     -- symmetric payloads
}

-- Publish: Extends the identity with an immutable version snapshot. A
-- publish's identity is `claimCzd` -- the czd of the claim that authorizes
-- it. No separate atomId field: the publish->claim czd reference IS the
-- binding ([publish-claim-coherence], [publish-chains-claim]).
-- pubAnchor/pubLabel are the base payload's own symmetric-payload raw
-- fields ([symmetric-payloads]) -- independently verifiable against the
-- claim's, not derived from any atomId.
sig Publish {
  version: one Version,
  claimCzd: one Czd,
  dig: one Dig,
  src: one Src,
  pubAnchor: one Anchor,  -- symmetric payloads
  pubLabel: one Label      -- symmetric payloads
}

-- Charter: roots the trust chain (2026-07-08 amendment; genesis-once
-- 2026-08-05, ADR-0007 SS3). One record per anchor, permanent, never
-- reissued -- one of the three genesis-once record kinds
-- ([atom-genesis-and-facts]). Its own `czd` IS the atom-set anchor
-- directly ([charter-anchor]: `Anchor := czd(charter)`); charter_properties
-- below makes Anchor exactly the set of Charter czds, so there is no
-- separate anchor-derivation field to carry and no successor to chain to.
-- Owner-set membership change and key rotation happen as a `fact`
-- extending the charter's own scope instead ([charter-owner-fact]) --
-- not modeled here (this file has no owner-fact mechanism at all; see
-- atom-transactions.md's Verification section).
sig Charter {
  czd: one Czd  -- content-addressed digest of the signed charter payload
}

-- AtomSource: Read-only trait interface (coalgebra observer). `atoms` is
-- content-addressed by the pre-claim AtomId digest -- storage/ingestion
-- operate on atom CONTENT, independent of claim/ownership status.
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

-- Local content-addressed derivation simulator: the digest an unclaimed
-- atom at (anchor, label) is identified by. Returns the atomId of any Atom
-- with a matching source-anchor and label; `atomid_content_addressed`
-- below guarantees this set has at most one distinct member.
fun computeId[a: Anchor, l: Label]: set AtomId {
  { atm: Atom | atm.source.anchor = a and atm.label = l }.atomId
}

-- Ingest condition definition
pred after_ingest[st: AtomStore, s: AtomSource] {
  s in st.ingested
}

-- Fork Scenario: Multiple sources share the same anchor, different owners
-- independently claim the SAME underlying atom slot (same claimAnchor,
-- claimLabel). Under the corrected doctrine the resulting claims have
-- DIFFERENT identities (different czds, since owner is a czd input) --
-- that is the doctrinally correct reading of a fork: competing claims,
-- never one shared identity ([identity-content-addressed]).
pred fork_scenario {
  some disj s1, s2: Source, disj c1, c2: Claim |
    s1.anchor = s2.anchor and
    c1.source = s1 and c2.source = s2 and
    c1.claimAnchor = c2.claimAnchor and c1.claimLabel = c2.claimLabel and
    c1.owner != c2.owner
}

// ============================================================================
// 3. FACTS (Structural Invariants & Constraints)
// ============================================================================

fact atomid_content_addressed {
  -- [identity-content-addressed], pre-claim (unclaimed-atom) regime: the
  -- digest is a deterministic, collision-free function of (anchor, label).
  -- Stated directly over Atom (which carries both preimage inputs) since
  -- AtomId itself carries no anchor/label fields to compare -- a digest,
  -- not the pair.
  all a1, a2: Atom |
    (a1.source.anchor = a2.source.anchor and a1.label = a2.label) iff a1.atomId = a2.atomId
}

fact claim_properties {
  -- Czd is a strictly unique digest for each claim -- a claim's identity.
  all c1, c2: Claim | c1.czd = c2.czd implies c1 = c2
  -- The claim's anchor must match the source's derivation
  all c: Claim | c.source.anchor = c.claimAnchor
}

fact publish_properties {
  -- Verification Chain Completeness ([publish-chains-claim] &
  -- [publish-claim-coherence]): a publish's identity is entirely carried
  -- by its claimCzd reference -- no separate atomId to also match.
  all p: Publish | some c: Claim | c.czd = p.claimCzd
  -- [symmetric-payloads]: the publish base payload's own raw anchor/label
  -- MUST agree with its authorizing claim's -- independently verifiable
  -- consistency, never an atomId reconstruction (there is no atomId here).
  all p: Publish | some c: Claim |
    c.czd = p.claimCzd and p.pubAnchor = c.claimAnchor and p.pubLabel = c.claimLabel
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
}

fact manifest_properties {
  -- Manifest Minimality ([manifest-minimal])
  -- Manifest exactly reflects the atom's human-readable identifier.
  all a: Atom | a.manifest.label = a.label
}

fact charter_properties {
  -- Content-addressed digest: czd is injective over the signed charter --
  -- one of the three genesis-once record kinds ([atom-genesis-and-facts],
  -- ADR-0007 SS3): permanent, never reissued. Combined with the
  -- anchor-derivation fact below this gives exactly one Charter per Anchor
  -- -- see anchor_content_addressed, which checks that combination.
  all disj c1, c2: Charter | c1.czd != c2.czd
  -- [charter-anchor]: every anchor traces to some charter's own czd --
  -- existence only; uniqueness comes from the injectivity fact above, not
  -- from this fact repeating it. Genesis-once removes the founding/
  -- successor split this used to need: there is no successor charter for
  -- a chain to distinguish "founding" from.
  all a: Anchor | some c: Charter | c.czd = a
  -- [claim-chains-charter]: a claim's anchor traces to some charter's czd
  -- -- exactly one, again by the injectivity fact above.
  all cl: Claim | some c: Charter | c.czd = cl.claimAnchor
}

// ============================================================================
// 4. ASSERTIONS & VERIFICATIONS
// ============================================================================

// [identity-content-addressed]: an unclaimed atom's AtomId is a deterministic
// digest of the (anchor, label) preimage, not the pair itself. "Content-
// addressed" means the digest is collision-free and depends on nothing but
// its declared inputs -- it does NOT mean the pair is observable in the
// AtomId value. This assertion checks the digest's determinism/injectivity.
// @docket: identity-content-addressed :: docs/specs/run_model_check.sh
assert identity_content_addressed {
  all a1, a2: Atom |
    (a1.source.anchor = a2.source.anchor and a1.label = a2.label) implies a1.atomId = a2.atomId
}

// [ingest-preserves-identity]
// @docket: ingest-preserves-identity :: docs/specs/run_model_check.sh
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

// [symmetric-payloads] / [publish-chains-claim]: every publish resolves to
// exactly the claim its claimCzd names, and the two payloads' raw
// anchor/label fields agree.
// @docket: publish-chains-claim :: docs/specs/run_model_check.sh
assert verification_chain {
  all p: Publish |
    some c: Claim |
      c.czd = p.claimCzd and
      p.pubAnchor = c.claimAnchor and p.pubLabel = c.claimLabel
}

// [anchor-content-addressed]: every anchor resolves to EXACTLY ONE charter --
// content addressing is unambiguous. This is a joint consequence of the
// anchor-derivation fact (existence) and czd injectivity (uniqueness); it is
// NOT a restatement of either -- dropping injectivity yields two charters
// sharing an anchor and refutes it. An owned, signed charter, never a free or
// source-derived value. Genesis-once removes the founding-charter qualifier:
// every charter fills the role "founding" used to single out.
// @docket: anchor-content-addressed :: docs/specs/run_model_check.sh
assert anchor_content_addressed {
  all a: Anchor | one c: Charter | c.czd = a
}

// [claim-chains-charter]: every claim's anchor chains to EXACTLY ONE charter
// -- an unforgeable, unambiguous link from claim authority back to the set's
// one and only charter (again existence + injectivity, not a fact restated).
// @docket: claim-chains-charter :: docs/specs/run_model_check.sh
assert claim_chains_charter {
  all cl: Claim | one c: Charter | c.czd = cl.claimAnchor
}

// [charter-fork-distinction] -- dedicated assertion RETIRED 2026-08-05, not
// its content. Originally: "distinct FOUNDING charters have distinct
// anchors" -- a real, non-trivial consequence of czd injectivity plus the
// recursive founding/successor anchor-derivation split retired above
// (ADR-0007 SS3, genesis-once). With that split gone, Anchor IS czd(charter)
// directly (charter_properties, [charter-anchor]) -- there is no separate
// chAnchor field left to derive. Restated in those terms the assertion
// becomes "distinct charters have distinct czds", which is
// charter_properties' own injectivity conjunct with the variables renamed:
// a direct syntactic restatement of an existing fact, not a claim this
// checker explores any new structure to confirm -- checking it would be
// checking a fact against itself, the same defect class as
// `ownership_independence`'s retired tautology (Verification, above), just
// arrived at by construction here rather than by pure logic.
// atom-transactions.md's own [charter-fork-distinction] text already
// anticipates this: "this now follows directly from [charter-anchor] and
// ordinary digest collision-resistance, with no separate chain-validity
// check needed."
//
// The content survives, machine-checked, as a genuine joint consequence
// inside anchor_content_addressed above: "every anchor resolves to AT MOST
// ONE charter" is the same fact read from the other direction. Re-graded to
// `evaluator: none` at [charter-fork-distinction]'s own claim block in
// atom-transactions.md -- nothing in this file checks it as its own claim
// any longer; its content is covered by anchor_content_addressed's
// machine-check, not by a dedicated assertion.

// ============================================================================
// 5. RUN / CHECK BLOCKS
// ============================================================================

check identity_content_addressed for 4
check ingest_preserves_identity for 4
check anchor_set_coherence for 4
check verification_chain for 4
check anchor_content_addressed for 4
check claim_chains_charter for 4
-- charter_fork_distinction: retired, not checked here -- see the retirement
-- comment in the ASSERTIONS section above (after claim_chains_charter).

run fork_scenario for 4

// Charter-rooted fork (the amended fork, re-modeled against charter): two
// claims by DIFFERENT owners chaining to the SAME charter -- the legitimate
// shared-anchor fork within one set. Must be satisfiable (SAT).
pred charter_rooted_fork {
  some c: Charter | some disj cl1, cl2: Claim |
    cl1.claimAnchor = c.czd and cl2.claimAnchor = c.czd and
    cl1.owner != cl2.owner
}
run charter_rooted_fork for 4
