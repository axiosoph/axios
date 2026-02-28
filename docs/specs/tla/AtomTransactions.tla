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

\* [identity-content-addressed]: AtomId is a deterministic function of (anchor, label)
AtomId(anchor, label) == <<anchor, label>>

\* czd is a deterministic tuple of the claim payload (models cryptographic effect)
\* Different owner/key/timestamp => different czd (models signature uniqueness)
Czd(atomId, owner, key, now) == <<atomId, owner, key, now>>

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
        atomId == AtomId(anchor, label)
        czd == Czd(atomId, owner, key, clock)
        newClaim == [
            atomId |-> atomId,
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
\* [session-ordering]: requires an existing claim with matching atomId.
\* [no-backdated-publish]: publish.now > claim.now.
\* [no-duplicate-version]: unique (atomId, claimCzd, version).
Publish(atomId, version, claimCzd, dig, src) ==
    /\ clock < MAX_CLOCK
    \* Data flow & temporal ordering constraints
    /\ \E c \in claims :
        /\ c.czd = claimCzd
        /\ c.atomId = atomId
        /\ clock > c.now
    \* [no-duplicate-version]
    /\ ~\E p \in publishes :
        /\ p.atomId = atomId
        /\ p.claimCzd = claimCzd
        /\ p.version = version
    /\ publishes' = publishes \cup {[
           atomId   |-> atomId,
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

\* The Next relation deliberately passes all combinations of c1.atomId
\* and c2.czd to exhaustively test cross-pollination — the Publish
\* action's internal guard rejects invalid combinations.
Next ==
    \/ \E source \in SOURCES, label \in LABELS, owner \in OWNERS, key \in KEYS :
           Claim(source, label, owner, key)
    \/ \E c1 \in claims, c2 \in claims, version \in VERSIONS, dig \in DIGESTS, src \in SRCS :
           Publish(c1.atomId, version, c2.czd, dig, src)
    \/ Terminating

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* SAFETY PROPERTIES (INVARIANTS)

\* [atomid-per-source-unique]
\* Within a single source, same label => same atomId (by construction)
AtomIdPerSourceUnique ==
    \A c1, c2 \in claims :
        (c1.source = c2.source /\ c1.label = c2.label) => (c1.atomId = c2.atomId)

\* [publish-claim-coherence]
\* Every publish references a valid claim czd with matching atomId
PublishClaimCoherence ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd /\ c.atomId = p.atomId

\* [no-unclaimed-publish]
\* No publish exists without a corresponding claim
NoUnclaimedPublish ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd

\* [no-duplicate-version]
\* Per (AtomId, claimCzd) pair, each version appears at most once
NoDuplicateVersion ==
    \A p1, p2 \in publishes :
        (p1.atomId = p2.atomId /\ p1.claimCzd = p2.claimCzd /\ p1.version = p2.version) => (p1 = p2)

\* [session-ordering]
\* Claim precedes publish: data flow + temporal (model §3.1)
SessionOrdering ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd /\ p.now > c.now

\* [no-backdated-publish]
\* publish.now > claim.now for matching claim
NoBackdatedPublish ==
    \A p \in publishes :
        \A c \in claims :
            (c.czd = p.claimCzd) => (p.now > c.now)

\* [identity-stability]
\* Same anchor + same label => same atomId, regardless of source identity
IdentityStability ==
    \A c1, c2 \in claims :
        (SourceAnchor[c1.source] = SourceAnchor[c2.source] /\ c1.label = c2.label)
            => (c1.atomId = c2.atomId)

\* [publish-chains-claim]
\* publish.claimCzd matches a claim with the same atomId
PublishChainsClaim ==
    \A p \in publishes :
        \E c \in claims :
            c.czd = p.claimCzd /\ c.atomId = p.atomId

-----------------------------------------------------------------------------
\* MODEL-CHECKING HELPERS

\* Fork scenario: both sources share the same anchor
MC_ForkAnchor == [s \in SOURCES |-> "anchorX"]

\* Non-fork scenario: sources have distinct anchors
MC_DistinctAnchors == [s \in SOURCES |-> IF s = "srcA" THEN "anchorX" ELSE "anchorY"]

=============================================================================
