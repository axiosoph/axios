//! Red-test inventory — attack #2, prefix rollback (`[chain-monotonicity]`).
//!
//! Mirrors the convention `atom/atom-id/src/charter.rs` already
//! establishes for attack #1 (`verify_succession_chain_rejects_divergent_successors`):
//! call the declared (but deliberately `unimplemented!()`) chain-walk
//! function on an attack-shaped chain, marked `#[ignore]` with a
//! phase-tag, so Phase 1's real implementation turns it green without
//! this test needing to change.

use atom_id::verify_succession_chain;

use crate::fixtures;

#[test]
#[ignore = "[chain-monotonicity] Phase 1: charter succession chain-walk is unimplemented — a \
            consumer that already recorded head czd(c2) (see corpus/charter/prefix_rollback.json) \
            must refuse being served this shorter, regressed prefix"]
fn verify_succession_chain_rejects_prefix_rollback() {
    // Once implemented: `[chain-monotonicity]` requires consumers to
    // refuse any served chain that regresses below a previously recorded
    // head. `prefix_rollback.json`'s full chain is c0->c1->c2; a
    // consumer who recorded head czd(c2) is here served just [c0, c1] —
    // a detectable rollback, not an alternative history.
    let file = fixtures::load("prefix_rollback.json");
    let rolled_back_chain: Vec<_> = file.charters[..2]
        .iter()
        .map(|c| c.payload.clone())
        .collect();

    let result = verify_succession_chain(&rolled_back_chain);
    assert!(
        result.is_err(),
        "a chain regressing below a previously recorded head must fail closed"
    );
}
