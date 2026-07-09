//! Red-test inventory — attack #2, prefix rollback (`[chain-monotonicity]`).
//!
//! Unlike attack #1's divergent-successors red test (`atom/atom-id/src/charter.rs`),
//! which calls `verify_succession_chain` directly — `[charter-succession-linear]`
//! is checkable from the chain's own content alone, so that stateless
//! function is the right thing to call — `[chain-monotonicity]`
//! (`docs/specs/atom-transactions.md:490`) is inherently STATEFUL: a
//! consumer must compare a served chain against a previously recorded
//! head. `verify_succession_chain`'s current signature
//! (`fn verify_succession_chain(_chain: &[CharterPayload])`,
//! `atom/atom-id/src/charter.rs:143`) has no recorded-head parameter, so
//! calling it on `[c0, c1]` would NOT exercise this attack: `[c0, c1]`
//! is, on its own, a perfectly valid, correctly-linked 2-charter chain —
//! a stateless chain-walk has no basis to reject it, and a real
//! implementation would return `Ok`, not `Err`. So this test does NOT
//! call `verify_succession_chain`; it documents the missing obligation
//! directly (like `bootstrap_gate.rs`'s attack #4) and WILL need
//! editing — not just un-ignoring — once Phase 1 decides how a recorded
//! head is threaded through (an extended signature, a new function, or a
//! stateful validator type).

use crate::fixtures;

#[test]
#[ignore = "[chain-monotonicity] Phase 1: no stateful chain-monotonicity check exists yet — \
            verify_succession_chain's signature has no recorded-head parameter to exercise against \
            (see corpus/charter/prefix_rollback.json)"]
fn chain_monotonicity_requires_recorded_head_check() {
    // The fixture: a valid chain c0->c1->c2 (`prefix_rollback.json`) and
    // the [c0, c1] prefix of it. A consumer who already recorded head
    // czd(c2) must refuse being served this shorter chain — but that
    // refusal requires comparing against the RECORDED head, state no
    // stateless chain-walk sees.
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

    unimplemented!(
        "Phase 1: [chain-monotonicity] (docs/specs/atom-transactions.md [chain-monotonicity]) \
         requires comparing a served chain against a consumer's previously recorded head (here \
         czd(c2), corpus/charter/prefix_rollback.json's third charter) — state \
         verify_succession_chain's current stateless signature cannot express. No such check \
         ships in this test corpus."
    );
}
