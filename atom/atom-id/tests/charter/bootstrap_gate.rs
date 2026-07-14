//! Attack #4, bootstrap seizure (`[charter-transition]` PRE, founding).
//!
//! Calls `atom_id::verify_bootstrap_gate` directly — a pure authorization
//! check over an already-resolved `(founding, earliest_preexisting_claim)`
//! pair, mirroring `verify_succession_chain`'s division of labor: the
//! caller resolves the earliest pre-existing claim from storage; this
//! function checks only whether the founding charter's signer is
//! authorized by that claim's owner.

use crate::fixtures;

#[test]
fn bootstrap_seizure_requires_incumbent_authorization() {
    // The fixture: an incumbent's pre-existing claim, and a founding
    // charter over the same source signed by an unrelated stranger's
    // key. Both verify individually (construction-correctness, exercised
    // in `construction.rs`) — that is all `verify_charter`/`verify_claim`
    // check. `verify_bootstrap_gate` rejects the founding charter unless
    // its signer (`tmb`) is authorized by `pre_existing_claim.owner`.
    let file = fixtures::load("bootstrap_seizure.json");
    let founding = &file.charters[0].payload;
    let pre_existing_claim = &file.claims[0].payload;
    assert!(
        !pre_existing_claim.owner.authorizes(&founding.tmb),
        "sanity: the fixture models an unauthorized signer, not a coincidental match"
    );

    let result = atom_id::verify_bootstrap_gate(founding, Some(pre_existing_claim));
    assert!(
        result.is_err(),
        "a founding charter signed by a stranger must fail the bootstrap gate"
    );
}
