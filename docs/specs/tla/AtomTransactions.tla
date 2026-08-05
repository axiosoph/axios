------------------------ MODULE AtomTransactions ------------------------
EXTENDS Naturals, FiniteSets

CONSTANTS
    SOURCES,      \* Set of all remote sources
    OWNERS,       \* Set of abstract identity digests (e.g., Coz thumbprints)
    LABELS,       \* Set of human-readable labels
    VERSIONS,     \* Set of version strings
    KEYS,         \* Set of cryptographic public keys
    DIGESTS,      \* Set of atom snapshot hashes
    SRCS,         \* Set of source revision hashes
    MAX_CLOCK,    \* Maximum logical time to bound model checking
    SourceAnchor  \* Function mapping each source to its cryptographic anchor

VARIABLES
    claims,       \* Set of claim records
    publishes,    \* Set of publish records
    clock         \* Monotonic logical clock to model `now` timestamps

vars == <<claims, publishes, clock>>

-----------------------------------------------------------------------------
\* DEFINITIONS

\* [identity-content-addressed]: a claim's identity IS its czd -- the digest of
\* its full signed payload (anchor, label, and every other claim-payload field;
\* owner/key/now stand in for the rest here). (anchor, label) are two INPUTS to
\* that digest, never the identity value itself -- there is no separate AtomId
\* type distinct from Czd.
Czd(anchor, label, owner, key, now) == <<anchor, label, owner, key, now>>

-----------------------------------------------------------------------------
\* INITIALIZATION

Init ==
    /\ claims = {}
    /\ publishes = {}
    /\ clock = 1

-----------------------------------------------------------------------------
\* ACTIONS

\* Claim: any owner claims any label at any source.
\* Fork claims are NOT a special action — they emerge naturally from
\* two independent Claim actions on sources that share an anchor.
Claim(source, label, owner, key) ==
    LET
        anchor == SourceAnchor[source]
        czd == Czd(anchor, label, owner, key, clock)
        newClaim == [
            anchor |-> anchor,
            source |-> source,
            label  |-> label,
            owner  |-> owner,
            czd    |-> czd,
            now    |-> clock,
            key    |-> key
        ]
    IN
        /\ clock < MAX_CLOCK
        \* Guard: no duplicate czd (same payload+sig can't produce two claims)
        /\ ~\E c \in claims : c.czd = czd
        /\ claims' = claims \cup {newClaim}
        /\ publishes' = publishes
        /\ clock' = clock + 1

\* Publish: publish a version against an existing claim.
\* [session-ordering]: requires an existing claim, chained by czd alone --
\* the publish->claim czd reference IS the identity binding
\* ([identity-content-addressed]; the publish carries no separate atomId).
\* [no-backdated-publish]: publish.now > claim.now.
\* [no-duplicate-version]: unique (claimCzd, version).
Publish(version, claimCzd, dig, src) ==
    /\ clock < MAX_CLOCK
    \* Data flow & temporal ordering constraints
    /\ \E c \in claims :
        /\ c.czd = claimCzd
        /\ clock > c.now
    \* [no-duplicate-version]
    /\ ~\E p \in publishes :
        /\ p.claimCzd = claimCzd
        /\ p.version = version
    /\ publishes' = publishes \cup {[
           version  |-> version,
           claimCzd |-> claimCzd,
           dig      |-> dig,
           src      |-> src,
           now      |-> clock
       ]}
    /\ claims' = claims
    /\ clock' = clock + 1

\* Terminating: avoids TLC flagging deadlock when MAX_CLOCK bounds the trace
Terminating ==
    /\ clock >= MAX_CLOCK
    /\ UNCHANGED vars

-----------------------------------------------------------------------------
\* STATE TRANSITIONS

\* The Next relation deliberately passes every claim's czd to Publish,
\* not only the czd of the claim that produced it, to exhaustively test
\* cross-pollination — the Publish action's internal guard rejects any
\* claimCzd that does not resolve to an existing claim.
Next ==
    \/ \E source \in SOURCES, label \in LABELS, owner \in OWNERS, key \in KEYS :
           Claim(source, label, owner, key)
    \/ \E c \in claims, version \in VERSIONS, dig \in DIGESTS, src \in SRCS :
           Publish(version, c.czd, dig, src)
    \/ Terminating

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* SAFETY PROPERTIES (INVARIANTS)

\* [atomid-per-source-unique] -- RETIRED from the checked INVARIANTS list
\* (both .cfg files), 2026-08-05. Under the old (anchor, label)-pair
\* identity, this held vacuously: AtomId was a pure function of the two
\* fields being compared, so no reachable-state exploration was needed --
\* it proved nothing about actual claim traffic. Substituting the correct
\* identity (a claim's own czd, which also depends on owner/key/now) makes
\* the literal substitution FALSE: this model's Claim action has no guard
\* against two independent claims for the same (source, label) -- e.g. the
\* same owner re-claiming at a later clock tick produces a second, distinct
\* czd. TLC confirms this in 2 states (see report). The underlying
\* protocol constraint this invariant names is source-content-structural
\* (a source cannot hold two atoms under one label) -- this model has no
\* "atom content" entity at all, only claims/publishes, so it never tested
\* that property either. Verifying it for real requires either a
\* single-active-claim-per-(source,label) guard on Claim, or an atom-slot
\* entity distinct from claims -- both are new modeling scope, not a
\* substitution, and are left to the architect's decision (see report).
\* AtomIdPerSourceUnique ==
\*     \A c1, c2 \in claims :
\*         (c1.source = c2.source /\ c1.label = c2.label) => (c1.czd = c2.czd)

\* [publish-claim-coherence]
\* @docket: publish-claim-coherence :: docs/specs/run_model_check.sh
\* Every publish references a valid claim czd -- the czd reference IS the
\* binding (there is no separate atomId to match alongside it).
PublishClaimCoherence ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd

\* [no-unclaimed-publish]
\* @docket: no-unclaimed-publish :: docs/specs/run_model_check.sh
\* No publish exists without a corresponding claim
NoUnclaimedPublish ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd

\* [no-duplicate-version]
\* @docket: no-duplicate-version :: docs/specs/run_model_check.sh
\* Per claimCzd, each version appears at most once
NoDuplicateVersion ==
    \A p1, p2 \in publishes :
        (p1.claimCzd = p2.claimCzd /\ p1.version = p2.version) => (p1 = p2)

\* [session-ordering]
\* @docket: session-ordering :: docs/specs/run_model_check.sh
\* Claim precedes publish: data flow + temporal (model §3.1)
SessionOrdering ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd /\ p.now > c.now

\* [no-backdated-publish]
\* @docket: no-backdated-publish :: docs/specs/run_model_check.sh
\* publish.now > claim.now for matching claim
NoBackdatedPublish ==
    \A p \in publishes :
        \A c \in claims :
            (c.czd = p.claimCzd) => (p.now > c.now)

\* [identity-stability] -- RETIRED from the checked INVARIANTS list (both
\* .cfg files), 2026-08-05, same reasoning as AtomIdPerSourceUnique above:
\* vacuous under the pair doctrine (same anchor+label trivially gave the
\* same pure-function AtomId), and its literal substitution is FALSE under
\* the corrected doctrine -- two independent claims sharing an anchor and
\* label (this model's fork scenario, MC_ForkAnchor) legitimately have
\* DIFFERENT identities (different czds) now, which is the doctrinally
\* correct outcome: forks are competing claims, not one shared identity.
\* The claim this invariant names -- "[identity-stability]" -- covers TWO
\* separable properties: identity stability ACROSS VERSIONS (a claim's own
\* czd never changes as publishes accrue against it -- already proven,
\* vacuously-but-truly, by PublishClaimCoherence/NoUnclaimedPublish/
\* PublishChainsClaim in this section, since a claim record is immutable
\* once created) and identity stability ACROSS OWNERSHIP TRANSFERS / KEY
\* ROTATIONS (per atom-model.md: the label-scope GENESIS claim's czd stays
\* stable through claim replacement). THIS model's claims have no `prior`
\* field at all -- no claim-replacement chain here, unlike AtomCharter.tla,
\* whose OWN (separate) `claims` variable DOES carry `prior` and models
\* replacement via ClaimReplace/ReplacementAuthority. Even there, though,
\* genesis-czd persistence is not machine-checked non-vacuously: its
\* anchor/label conjuncts hold only because ClaimReplace hardcodes them
\* (see AtomCharter.tla's ReplacementAuthority comment). So the
\* ownership/key-rotation half of [identity-stability] is unmodeled here
\* and, where a replacement chain does exist, still only definitionally
\* (not state-space) established -- new modeling scope either way, left
\* to the architect (see report).
\* IdentityStability ==
\*     \A c1, c2 \in claims :
\*         (SourceAnchor[c1.source] = SourceAnchor[c2.source] /\ c1.label = c2.label)
\*             => (c1.czd = c2.czd)

\* [publish-chains-claim]
\* @docket: publish-chains-claim :: docs/specs/run_model_check.sh
\* publish.claimCzd matches an existing claim's czd
PublishChainsClaim ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd

-----------------------------------------------------------------------------
\* MODEL-CHECKING HELPERS

\* Fork scenario: both sources share the same anchor
MC_ForkAnchor == [s \in SOURCES |-> "anchorX"]

\* Non-fork scenario: sources have distinct anchors
MC_DistinctAnchors == [s \in SOURCES |-> IF s = "srcA" THEN "anchorX" ELSE "anchorY"]

=============================================================================
