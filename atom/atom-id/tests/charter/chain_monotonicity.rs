//! Attack #2, prefix rollback (`[chain-monotonicity]`).
//!
//! `verify_succession_chain` now takes an optional `recorded_head: &Czd`:
//! a served chain must demonstrably extend past it (some successor's
//! `prior` names it), or it's rejected as a regression. `[c0, c1]`, taken
//! alone, is a perfectly valid, correctly-linked chain — the violation is
//! only visible against the recorded head `czd(c2)`, which this chain
//! never mentions.

use crate::fixtures;

#[test]
fn chain_monotonicity_requires_recorded_head_check() {
    // The fixture: a valid chain c0->c1->c2 (`prefix_rollback.json`) and
    // the [c0, c1] prefix of it. A consumer who already recorded head
    // czd(c2) must refuse being served this shorter chain.
    let file = fixtures::load("prefix_rollback.json");
    assert_eq!(file.charters.len(), 3, "c0, c1, c2 of the full chain");

    let rolled_back_chain: Vec<_> = file.charters[..2]
        .iter()
        .map(|c| c.payload.clone())
        .collect();
    assert_eq!(
        rolled_back_chain.len(),
        2,
        "the served chain is a strict prefix of the full one"
    );
    assert_eq!(
        rolled_back_chain[1].prior,
        Some(file.charters[0].czd()),
        "sanity: [c0, c1] is, taken alone, a valid and correctly-linked chain — nothing in its \
         own content marks it as a rollback"
    );

    let recorded_head = file.charters[2].czd();
    let result = atom_id::verify_succession_chain(&rolled_back_chain, Some(&recorded_head));
    assert!(
        result.is_err(),
        "a chain that never reaches the recorded head must fail closed"
    );
}
