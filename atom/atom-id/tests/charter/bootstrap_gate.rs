//! Red-test inventory — attack #4, bootstrap seizure (`[charter-transition]`
//! PRE, founding).
//!
//! Unlike attacks #1-#3, no seam function exists yet to call here:
//! `[charter-transition]` PRE's bootstrap gate ("if the source already
//! carries claims predating any charter, the founding charter MUST be
//! authorized by the owner of the earliest such claim") has no declared
//! stub anywhere in `atom/atom-id/src/{charter,lib}.rs` — and this
//! corpus's Non-Goals bar adding one (no seam edits, no
//! authorization/ancestry implementation). So this test cannot mirror
//! the "call the declared stub" convention `chain_monotonicity.rs` and
//! `atom/atom-id/src/charter.rs` use: it documents the missing
//! obligation directly and will need EDITING (not just un-ignoring) once
//! Phase 1 introduces the real check and its call signature.

use crate::fixtures;

#[test]
#[ignore = "[charter-transition] PRE Phase 1: no bootstrap-gate authorization check exists yet — \
            see corpus/charter/bootstrap_seizure.json for the constructed attack"]
fn bootstrap_seizure_requires_incumbent_authorization() {
    // The fixture: an incumbent's pre-existing claim, and a founding
    // charter over the same source signed by an unrelated stranger's
    // key. Both verify individually today (construction-correctness,
    // exercised in `construction.rs`) — that is all `verify_charter`/
    // `verify_claim` check. What is MISSING is the bootstrap gate
    // itself: Phase 1 must reject the founding charter unless its
    // signer is authorized by `pre_existing_claim.owner`.
    let file = fixtures::load("bootstrap_seizure.json");
    let founding = &file.charters[0].payload;
    let pre_existing_claim = &file.claims[0].payload;
    assert_ne!(
        founding.owner, pre_existing_claim.owner,
        "sanity: the fixture models an unauthorized signer, not a coincidental match"
    );

    unimplemented!(
        "Phase 1: charter bootstrap-seizure authorization check — see \
         docs/specs/atom-transactions.md [charter-transition] PRE (bootstrap gate). No \
         CharterStore/ClaimStore ancestry lookup or authorization check ships in this corpus; see \
         N-charter-corpus Non-Goals."
    );
}
