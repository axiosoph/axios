--------------------------- MODULE AtomCharter ---------------------------
(***************************************************************************)
(* Charter layer of the atom transaction protocol (2026-07-14 owner-set     *)
(* amendment).                                                              *)
(*                                                                         *)
(* Re-models the fork scenario against charter succession, as mandated by  *)
(* the amendment note in docs/specs/atom-transactions.md.  The existing    *)
(* AtomTransactions.tla continues to verify the claim/publish subchain;    *)
(* this module verifies the charter/authorization/anchor layer that now    *)
(* roots the trust chain.                                                   *)
(*                                                                         *)
(* Anchor semantics (amended): an anchor is czd(charter_0) -- the coz       *)
(* digest of the signed FOUNDING charter -- never source metadata.  A       *)
(* founding charter is the unique charter in a succession chain carrying    *)
(* no `prior`.  Succession preserves the anchor for the life of the set.    *)
(*                                                                         *)
(* Authorization (amended [charter-owner-set]): a charter's `owner` is a    *)
(* NON-EMPTY SET of owner-references, not one value -- "who's on the team  *)
(* under this anchor", every member holding full, equal, undifferentiated  *)
(* authority.  A signing key is authorized by a charter iff it is          *)
(* authorized by ANY member of that set ([owner-authorization-delegated]'s  *)
(* set composition rule -- disjunction over single-valued authorization,    *)
(* never a distinct mechanism).  Each member is itself modeled at the       *)
(* single-key tier of [owner-abstract] (Authorized(k,o) == KeyOwner[k]=o);  *)
(* richer per-member identity frameworks only ENLARGE that per-value        *)
(* relation, so every safety invariant proved here is preserved a fortiori  *)
(* under hierarchical / rooted identity -- orthogonal to set cardinality,   *)
(* which this module now models directly rather than a-fortiori-ing past.   *)
(*                                                                         *)
(* [charter-succession-linear]'s add/remove asymmetry: adding a principal   *)
(* requires that principal's own possession-proof, chained exactly as       *)
(* full ownership transfer worked under the single-owner model              *)
(* (CharterAddMember below).  Removing a principal requires only a          *)
(* signature from an existing REMAINING member -- no possession-proof, no   *)
(* consent from the removed principal (CharterRemoveMember below).  Safety  *)
(* against a malicious/compromised member abusing either path comes from    *)
(* this module's existing fork-fail-closed rule (ForkFailClosed), not from  *)
(* either action being intrinsically safe on its own.                      *)
(*                                                                         *)
(* Discharges (TLC-routed): [charter-anchor], [claim-charter-authorization],*)
(* [charter-ancestry], [charter-succession], [charter-succession-linear],   *)
(* [chain-monotonicity], [claim-replacement-authority], [anchor-immutable]. *)
(* Also now machine-checks [charter-owner-set]'s non-empty constraint       *)
(* (OwnerSetNonEmpty) and exercises [owner-authorization-delegated]'s set   *)
(* composition rule (AuthorizedBySet) -- the corresponding spec-text        *)
(* VERIFIED tags are unmodified here (out of this rework's file surface).  *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS
    OWNERS,     \* abstract identity digests (charter members / claim owners)
    KEYS,       \* signing keys
    SRCS,       \* source revisions as naturals; ancestry = numeric >=
    LABELS,     \* human-readable labels
    KeyOwner,   \* [KEYS -> OWNERS] : single-key authorization relation
    MAX_CLOCK   \* logical-time bound for model checking

VARIABLES
    charters,     \* set of charter records; owner is a non-empty SUBSET OWNERS
    claims,       \* set of claim records
    recordedHead, \* consumer's recorded charter-chain head czd (or NoHead)
    clock         \* monotonic logical clock

vars == <<charters, claims, recordedHead, clock>>

\* Sentinels. The prior/head sentinels are 1-tuples so that TLC compares them
\* against czd tuples structurally (different length => not equal) instead of
\* raising on a string-vs-tuple equality.
NoPrior      == <<"NONE">>    \* a founding charter's `prior`
NoClaimPrior == <<"FRESH">>   \* a fresh (non-replacement) claim's `prior`
NoHead       == <<"NOHEAD">>  \* consumer has recorded no head yet

-----------------------------------------------------------------------------
\* DEFINITIONS

\* Single-key authorization: key k speaks for owner o.
Authorized(k, o) == /\ k \in KEYS
                    /\ KeyOwner[k] = o

\* Set-membership authorization ([owner-authorization-delegated]'s set
\* composition rule): key k speaks for owner-set S iff authorized by ANY
\* member -- a disjunction over single-valued authorization, never a
\* distinct mechanism of its own.
AuthorizedBySet(k, S) == \E o \in S : Authorized(k, o)

\* Ancestry floor over source revisions: a descends from b (linear history).
Descends(a, b) == a >= b

\* Content-addressed digests (injective: each carries the unique clock `t`).
\* `owner` may be a single value (claims) or a set (charters) -- either way
\* the tuple stays injective on `t` alone, so digest uniqueness never
\* depends on what `owner` contains.
CharterCzd(owner, key, src, t) == <<"CH", owner, key, src, t>>
ClaimCzd(owner, key, src, t)   == <<"CL", owner, key, src, t>>

\* All charters of the set identified by anchor `a`.
Chain(a) == {c \in charters : c.anchor = a}

\* Set-authority fork: two DISTINCT charters name the same real prior.
Divergent(a) ==
    \E c1, c2 \in Chain(a) :
        /\ c1 # c2
        /\ c1.prior = c2.prior
        /\ c1.prior # NoPrior

\* Chain heads: charters in the set with no successor.
Heads(a) == {c \in Chain(a) : ~\E c2 \in Chain(a) : c2.prior = c.czd}

\* The effective charter is well-defined only on a non-divergent chain with a
\* single head -- a consumer FAILS CLOSED for any authority decision otherwise.
EffectiveDefined(a) == /\ ~Divergent(a)
                       /\ Cardinality(Heads(a)) = 1
EffectiveCharter(a) == CHOOSE c \in Heads(a) : TRUE

\* Ancestor czds of a charter (self + all charters reachable via `prior`).
\* `prior` is czd-injective, so the resolving set is empty or a singleton.
RECURSIVE AncestorCzds(_)
AncestorCzds(c) ==
    IF c.prior = NoPrior
        THEN {c.czd}
        ELSE LET ps == {p \in charters : p.czd = c.prior}
             IN {c.czd} \cup (IF ps = {} THEN {}
                              ELSE AncestorCzds(CHOOSE p \in ps : TRUE))

\* Ancestors of the charter carrying czd `x` (empty if x is absent/sentinel).
AncestorCzdsOf(x) ==
    LET cs == {c \in charters : c.czd = x}
    IN IF cs = {} THEN {} ELSE AncestorCzds(CHOOSE c \in cs : TRUE)

-----------------------------------------------------------------------------
\* INITIALIZATION

Init ==
    /\ charters = {}
    /\ claims = {}
    /\ recordedHead = NoHead
    /\ clock = 1

-----------------------------------------------------------------------------
\* ACTIONS

\* [charter-transition] founding: establishes a set and its anchor.
\* PRE: signing key authorized by SOME member of the declared founding set
\* ([owner-authorization-delegated]'s set composition rule); anchor == own
\* czd. Founding has no `prior`, so [charter-succession-linear]'s
\* possession-proof-on-addition rule -- which triggers on principals added
\* RELATIVE TO a prior -- has no prior to compare against and does not
\* apply: a founding charter unilaterally DEFINES the set from nothing, the
\* same bootstrap act the single-owner model already permitted, generalized
\* to a set. [charter-owner-set-non-empty]: `ownerSet` is drawn from
\* non-empty subsets only.
CharterFound(ownerSet, key, src) ==
    LET czd == CharterCzd(ownerSet, key, src, clock) IN
    /\ clock < MAX_CLOCK
    /\ ownerSet # {}
    /\ AuthorizedBySet(key, ownerSet)
    /\ ~\E c \in charters : c.czd = czd
    /\ charters' = charters \cup {[
           czd |-> czd, prior |-> NoPrior, anchor |-> czd,
           owner |-> ownerSet, key |-> key,
           src |-> src, now |-> clock ]}
    /\ UNCHANGED <<claims, recordedHead>>
    /\ clock' = clock + 1

\* [charter-succession] non-transfer succession (rotation): owner-SET
\* membership unchanged (owner' = prior.owner, set equality). PRE: signing
\* key authorized by SOME member of the PRIOR charter's owner set; now >
\* prior.now. `owner` is dropped as a free parameter (unlike the scalar
\* model) because it is fully determined by `prior` -- carrying it as an
\* independent existential over SUBSET OWNERS would only multiply the state
\* space with choices the guard immediately discards.
\* The action does NOT forbid a second successor of the same prior -- that is
\* how a set-authority fork becomes constructible; fail-closed is enforced at
\* the consumer/claim decision points, not by preventing the signing.
\* Membership-changing succession is CharterAddMember/CharterRemoveMember
\* below -- see their comments for why those cannot be this same step.
CharterSucceed(prior, key, src) ==
    LET czd == CharterCzd(prior.owner, key, src, clock) IN
    /\ clock < MAX_CLOCK
    /\ prior \in charters
    /\ AuthorizedBySet(key, prior.owner)
    /\ clock > prior.now
    /\ Descends(src, prior.src)
    /\ ~\E c \in charters : c.czd = czd
    /\ charters' = charters \cup {[
           czd |-> czd, prior |-> prior.czd, anchor |-> prior.anchor,
           owner |-> prior.owner, key |-> key,
           src |-> src, now |-> clock ]}
    /\ UNCHANGED <<claims, recordedHead>>
    /\ clock' = clock + 1

\* [charter-succession-linear] (addition, chained possession-proof,
\* generalized from the scalar model's "transfer"). A coz message carries
\* exactly one signature (`czd` digests a single {cad,sig} pair -- Coz
\* README.md "Canon"), so proof of possession for an ADDED principal cannot
\* be a second signature embedded in one charter. It is instead a SECOND,
\* independently-signed charter `d` chained onto the successor `c`
\* (d.prior = c.czd), signed by a key authorized by the INCOMING principal
\* -- the same succession-chain mechanism [charter-succession] already
\* uses, one link further. The two charters are submitted together as one
\* atomic step, mirroring how they are pushed as one logical transaction in
\* practice. One-at-a-time addition is modeled (rather than multi-add-in-
\* one-step): it mirrors the spec's per-principal possession-proof framing
\* exactly, and a multi-member enlargement is representable as repeated
\* single additions with no loss of reachable end states, only step count.
\* An enlarged set with no chained possession-proof for the new entry is
\* never observable at any reachable state, so TransferDualSigned (below)
\* holds at every reachable state rather than only eventually.
CharterAddMember(prior, addedOwner, key, possessionKey, src, possessionSrc) ==
    LET newOwnerSet == prior.owner \cup {addedOwner}
        czd  == CharterCzd(newOwnerSet, key, src, clock)
        dczd == CharterCzd(newOwnerSet, possessionKey, possessionSrc, clock + 1)
    IN
    /\ clock + 1 < MAX_CLOCK
    /\ prior \in charters
    /\ addedOwner \notin prior.owner
    /\ AuthorizedBySet(key, prior.owner)
    /\ Authorized(possessionKey, addedOwner)
    /\ clock > prior.now
    /\ Descends(src, prior.src)
    /\ Descends(possessionSrc, src)
    /\ ~\E c \in charters : c.czd = czd
    /\ ~\E c \in charters : c.czd = dczd
    /\ charters' = charters \cup {
           [ czd |-> czd, prior |-> prior.czd, anchor |-> prior.anchor,
             owner |-> newOwnerSet, key |-> key, src |-> src, now |-> clock ],
           [ czd |-> dczd, prior |-> czd, anchor |-> prior.anchor,
             owner |-> newOwnerSet, key |-> possessionKey,
             src |-> possessionSrc, now |-> clock + 1 ] }
    /\ UNCHANGED <<claims, recordedHead>>
    /\ clock' = clock + 2

\* [charter-succession-linear] (removal). Removing a principal requires
\* only a signature from an EXISTING REMAINING member -- no possession-
\* proof, no consent from the removed principal (a compromised or
\* malicious member MUST NOT be able to block its own removal by
\* withholding cooperation). This rule adds no NEW weakness: flat set
\* authority already lets any single existing member sign an arbitrary
\* successor, including one that removes every other member. The actual
\* protection against a malicious/compromised member racing a legitimate
\* removal is NOT this rule itself -- it is ForkFailClosed: two conflicting
\* successors naming the same `prior` (a mutual-ejection race is one
\* instance) fork set authority and freeze downstream decisions.
\* [charter-owner-set-non-empty]: a removal that would empty the set is
\* refused outright (precondition), and OwnerSetNonEmpty (below) checks
\* the consequence holds at every reachable state.
CharterRemoveMember(prior, removedOwner, key, src) ==
    LET newOwnerSet == prior.owner \ {removedOwner}
        czd == CharterCzd(newOwnerSet, key, src, clock)
    IN
    /\ clock < MAX_CLOCK
    /\ prior \in charters
    /\ removedOwner \in prior.owner
    /\ newOwnerSet # {}
    /\ AuthorizedBySet(key, newOwnerSet)
    /\ clock > prior.now
    /\ Descends(src, prior.src)
    /\ ~\E c \in charters : c.czd = czd
    /\ charters' = charters \cup {[
           czd |-> czd, prior |-> prior.czd, anchor |-> prior.anchor,
           owner |-> newOwnerSet, key |-> key,
           src |-> src, now |-> clock ]}
    /\ UNCHANGED <<claims, recordedHead>>
    /\ clock' = clock + 1

\* [claim-transition] fresh claim.
\* PRE: the set has a well-defined effective charter; signing key authorized
\*      by SOME member of that charter's owner set
\*      ([claim-charter-authorization], via AuthorizedBySet); claim src
\*      descends from the charter src ([charter-ancestry]). The claim's own
\*      `owner` stays single-valued ([claim-owner-single]) -- unaffected by
\*      the charter-level set generalization; only the AUTHORIZING check
\*      against the charter's owner-set changes.
Claim(anchor, label, owner, key, src) ==
    LET czd == ClaimCzd(owner, key, src, clock) IN
    /\ clock < MAX_CLOCK
    /\ EffectiveDefined(anchor)
    /\ LET ec == EffectiveCharter(anchor) IN
        /\ AuthorizedBySet(key, ec.owner)
        /\ Descends(src, ec.src)
        /\ owner = KeyOwner[key]
        /\ ~\E cl \in claims : cl.czd = czd
        /\ claims' = claims \cup {[
               czd |-> czd, anchor |-> anchor, label |-> label,
               owner |-> owner, key |-> key, src |-> src,
               prior |-> NoClaimPrior, governance |-> FALSE,
               authCharter |-> ec.czd, now |-> clock ]}
    /\ UNCHANGED <<charters, recordedHead>>
    /\ clock' = clock + 1

\* [claim-replacement-authority] / [claim-replacement-transition].
\* Two authorities, distinguishable by every consumer:
\*   owner replacement  -- key authorized by the replaced claim's (single)
\*                         owner; governance = FALSE (ordinary, unmarked).
\*   governance seizure -- key authorized by SOME member of the effective
\*                         charter's owner set but NOT by the replaced
\*                         claim's owner; governance = TRUE (marked, first-
\*                         class seizure).
\* Anchor/label are never altered by replacement (identity is stable).
ClaimReplace(old, key, src) ==
    LET czd     == ClaimCzd(KeyOwner[key], key, src, clock)
        ownerOK == Authorized(key, old.owner)
        ec      == EffectiveCharter(old.anchor)
        govOK   == /\ ~Authorized(key, old.owner)
                   /\ AuthorizedBySet(key, ec.owner)
    IN
    /\ clock < MAX_CLOCK
    /\ old \in claims
    /\ EffectiveDefined(old.anchor)
    /\ Descends(src, ec.src)
    /\ (ownerOK \/ govOK)
    /\ ~\E cl \in claims : cl.czd = czd
    /\ claims' = claims \cup {[
           czd |-> czd, anchor |-> old.anchor, label |-> old.label,
           owner |-> KeyOwner[key], key |-> key, src |-> src,
           prior |-> old.czd, governance |-> (~ownerOK),
           authCharter |-> ec.czd, now |-> clock ]}
    /\ UNCHANGED <<charters, recordedHead>>
    /\ clock' = clock + 1

\* [chain-monotonicity] consumer observes a SERVED chain head. The served
\* charter is adversarial: it may be any charter, including a stale one offered
\* by a rollback attempt. Two guards accept it:
\*   fail-closed  -- the set is non-divergent (EffectiveDefined); a forked set
\*                   yields no authority decision.
\*   monotonic    -- the recorded head is an ancestor-or-self of the served
\*                   head, so a chain that regresses BELOW the recorded head
\*                   (a prefix of observed state) is a detected rollback and is
\*                   refused. First contact (NoHead) is a TOFU acceptance.
ConsumerObserve(c) ==
    /\ clock < MAX_CLOCK
    /\ c \in charters
    /\ EffectiveDefined(c.anchor)
    /\ (recordedHead = NoHead \/ recordedHead \in AncestorCzds(c))
    /\ recordedHead' = c.czd
    /\ UNCHANGED <<charters, claims>>
    /\ clock' = clock + 1

\* Avoids TLC flagging deadlock once the clock bound is reached.
Terminating ==
    /\ clock >= MAX_CLOCK
    /\ UNCHANGED vars

-----------------------------------------------------------------------------
\* STATE TRANSITIONS

Next ==
    \/ \E ownerSet \in (SUBSET OWNERS) \ {{}}, key \in KEYS, src \in SRCS :
           CharterFound(ownerSet, key, src)
    \/ \E prior \in charters, key \in KEYS, src \in SRCS :
           CharterSucceed(prior, key, src)
    \/ \E prior \in charters, addedOwner \in OWNERS, key \in KEYS,
          possessionKey \in KEYS, src \in SRCS, possessionSrc \in SRCS :
           CharterAddMember(prior, addedOwner, key, possessionKey, src, possessionSrc)
    \/ \E prior \in charters, removedOwner \in OWNERS, key \in KEYS, src \in SRCS :
           CharterRemoveMember(prior, removedOwner, key, src)
    \/ \E anchor \in {c.anchor : c \in charters}, label \in LABELS,
          owner \in OWNERS, key \in KEYS, src \in SRCS :
           Claim(anchor, label, owner, key, src)
    \/ \E old \in claims, key \in KEYS, src \in SRCS :
           ClaimReplace(old, key, src)
    \/ \E c \in charters :
           ConsumerObserve(c)
    \/ Terminating

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* SAFETY PROPERTIES (INVARIANTS)

\* [charter-anchor] + [anchor-content-addressed] (dynamic facet) +
\* [anchor-immutable] (charter facet): a founding charter's anchor is its own
\* czd; a successor inherits its prior's anchor, never minting a fresh one.
AnchorIsFoundingCzd ==
    \A c \in charters :
        /\ (c.prior = NoPrior => c.anchor = c.czd)
        /\ (c.prior # NoPrior =>
              \E p \in charters : p.czd = c.prior /\ c.anchor = p.anchor)

\* [charter-anchor]: at most one founding charter per anchor.
FoundingUnique ==
    \A c1, c2 \in charters :
        (c1.anchor = c2.anchor /\ c1.prior = NoPrior /\ c2.prior = NoPrior)
            => c1 = c2

\* [charter-owner-set-non-empty]: no charter's owner set is ever empty.
OwnerSetNonEmpty ==
    \A c \in charters : c.owner # {}

\* [claim-charter-authorization]: a FRESH claim's key is authorized by SOME
\* member of the owner set of the charter it names (its set's effective
\* charter at claim time). Replacement claims chain their authority to the
\* replaced claim, not the charter, and are governed by ReplacementAuthority
\* instead.
ClaimAuthorized ==
    \A cl \in claims :
        cl.prior = NoClaimPrior =>
            \E c \in charters :
                /\ c.czd = cl.authCharter
                /\ c.anchor = cl.anchor
                /\ AuthorizedBySet(cl.key, c.owner)

\* [charter-ancestry]: a claim's src descends from its authorizing charter's src.
ClaimAncestry ==
    \A cl \in claims :
        \E c \in charters :
            c.czd = cl.authCharter /\ Descends(cl.src, c.src)

\* [charter-succession]: a successor is signed by a key authorized by SOME
\* member of the prior's owner set, and preserves the anchor.
SuccessionAuthorized ==
    \A c \in charters :
        c.prior # NoPrior =>
            \E p \in charters :
                /\ p.czd = c.prior
                /\ AuthorizedBySet(c.key, p.owner)
                /\ c.anchor = p.anchor

\* [charter-succession-linear] (chained proof of possession, generalized to
\* set addition): a successor c that ADDS at least one principal relative to
\* its prior p (some member of c.owner absent from p.owner) is followed by
\* a SEPARATE charter d chained onto c (d.prior = c.czd) signed by a key
\* authorized by (at least) one of the newly-added principals -- proof of
\* possession expressed as the next chain link, never as a second signature
\* embedded in c's own message. Pure removal (c.owner \subseteq p.owner) is
\* explicitly exempt: no possession-proof is owed by, or from, a departing
\* principal.
TransferDualSigned ==
    \A c \in charters :
        (\E p \in charters : p.czd = c.prior /\ ~(c.owner \subseteq p.owner))
        => \E p \in charters :
             /\ p.czd = c.prior
             /\ \E d \in charters :
                  /\ d.prior = c.czd
                  /\ \E o \in (c.owner \ p.owner) : Authorized(d.key, o)

\* [claim-replacement-authority]: every replacement is validly authorized and
\* its governance flag is set iff (and only iff) it is a governance seizure.
ReplacementAuthority ==
    \A cl \in claims :
        cl.prior # NoClaimPrior =>
            \E old \in claims :
                /\ old.czd = cl.prior
                /\ cl.anchor = old.anchor        \* identity never altered
                /\ cl.label = old.label
                /\ ( \/ (Authorized(cl.key, old.owner) /\ cl.governance = FALSE)
                     \/ (~Authorized(cl.key, old.owner)
                         /\ (\E c \in charters :
                               c.czd = cl.authCharter
                               /\ AuthorizedBySet(cl.key, c.owner))
                         /\ cl.governance = TRUE) )

\* [anchor-immutable]: a claim's anchor equals its authorizing charter's anchor,
\* so identity is fixed to the founding czd across claims and replacements.
AnchorImmutable ==
    \A cl \in claims :
        \E c \in charters :
            c.czd = cl.authCharter /\ cl.anchor = c.anchor

TypeOK ==
    /\ recordedHead = NoHead \/ (\E c \in charters : c.czd = recordedHead)
    /\ clock \in 1..MAX_CLOCK
    /\ \A c \in charters : c.anchor = c.czd \/ c.prior # NoPrior
    /\ \A c \in charters : c.owner \subseteq OWNERS

-----------------------------------------------------------------------------
\* TEMPORAL PROPERTIES

\* [chain-monotonicity]: recordedHead never regresses -- each step keeps it,
\* starts it from NoHead, or advances it to a descendant (old head an ancestor).
MonotonicHead ==
    [][ \/ recordedHead' = recordedHead
        \/ recordedHead = NoHead
        \/ recordedHead \in AncestorCzdsOf(recordedHead') ]_vars

\* [charter-succession-linear] (fail-closed): the consumer commits a head only
\* when that head's set is non-divergent. Under a set-authority fork the head
\* freezes -- no authority decision is made downstream of the divergence.
\* Generalizes for free across the new add/remove actions: this property is
\* stated over `charters`/`Divergent` generically, never over a specific
\* action, so it ranges over CharterAddMember/CharterRemoveMember exactly as
\* it already did over CharterSucceed/CharterTransfer.
ForkFailClosed ==
    [][ (recordedHead' # recordedHead) =>
          \E c \in charters : c.czd = recordedHead' /\ ~Divergent(c.anchor) ]_vars

-----------------------------------------------------------------------------
\* MODEL-CHECKING HELPERS

\* Succession scenario: two owners, one key each (single-key identity tier).
MC_KeyOwner == [k \in KEYS |-> IF k = "k1" THEN "o1" ELSE "o2"]

\* Rotation scenario: owner o1 holds TWO keys (k1, k3) -- key rotation within an
\* owner, a step toward the hierarchical identity tier of [owner-abstract]. An
\* owner replacement may re-sign with the rotated key; authorization must track
\* the owner, not the individual key.
MC_KeyOwnerRot == [k \in KEYS |-> IF k \in {"k1", "k3"} THEN "o1" ELSE "o2"]

=============================================================================
