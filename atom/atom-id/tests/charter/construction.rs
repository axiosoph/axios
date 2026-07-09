//! Construction-correctness: every transaction in the committed corpus
//! verifies individually (signature + `typ`), and each attack's defining
//! shape holds on the loaded data — NOT on freshly-built in-memory
//! fixtures. This is what makes `corpus/charter/*.json` a real, standalone
//! artifact rather than a byproduct of the generator that made it.
//!
//! No chain/succession/ancestry validator runs here — see `main.rs` for
//! the red-test inventory those checks belong to (Phase 1).

use crate::fixtures;

#[test]
fn divergent_succession_transactions_verify() {
    let file = fixtures::load("divergent_succession.json");
    assert_eq!(
        file.charters.len(),
        3,
        "founding + two divergent successors"
    );
    for c in &file.charters {
        assert!(c.verify().is_ok(), "{}: {:?}", file.attack, c.verify());
    }

    let founding_czd = file.charters[0].czd();
    let successor_a = &file.charters[1];
    let successor_b = &file.charters[2];
    assert_eq!(
        successor_a.payload.prior,
        Some(founding_czd.clone()),
        "successor A must chain to the founding charter"
    );
    assert_eq!(
        successor_b.payload.prior,
        Some(founding_czd),
        "successor B must name the SAME prior as successor A — the fork"
    );
    assert_ne!(
        successor_a.payload, successor_b.payload,
        "the two successors must actually diverge in content"
    );
}

#[test]
fn prefix_rollback_transactions_verify() {
    let file = fixtures::load("prefix_rollback.json");
    assert_eq!(file.charters.len(), 3, "c0, c1, c2 of the full chain");
    for c in &file.charters {
        assert!(c.verify().is_ok(), "{}: {:?}", file.attack, c.verify());
    }

    let c0 = &file.charters[0];
    let c1 = &file.charters[1];
    let c2 = &file.charters[2];
    assert_eq!(c0.payload.prior, None, "c0 is the founding charter");
    assert_eq!(c1.payload.prior, Some(c0.czd()), "c1 chains to c0");
    assert_eq!(c2.payload.prior, Some(c1.czd()), "c2 chains to c1");
    assert!(
        c2.payload.now > c1.payload.now && c1.payload.now > c0.payload.now,
        "chain is temporally ordered"
    );
}

#[test]
fn claim_replacement_transactions_verify() {
    let file = fixtures::load("claim_replacement.json");
    assert_eq!(
        file.claims.len(),
        3,
        "ordinary claim + owner + governance replacement"
    );
    for c in &file.claims {
        assert!(c.verify().is_ok(), "{}: {:?}", file.attack, c.verify());
    }

    let ordinary = &file.claims[0];
    let owner_replacement = &file.claims[1];
    let governance_replacement = &file.claims[2];

    assert_eq!(ordinary.payload.prior, None);
    assert!(!ordinary.payload.governance);

    let replaced_czd = ordinary.czd();
    assert_eq!(
        owner_replacement.payload.prior,
        Some(replaced_czd.clone()),
        "owner replacement names the ordinary claim as `prior`"
    );
    assert!(
        !owner_replacement.payload.governance,
        "owner replacement is unmarked"
    );

    assert_eq!(
        governance_replacement.payload.prior,
        Some(replaced_czd),
        "governance replacement names the SAME replaced claim"
    );
    assert!(
        governance_replacement.payload.governance,
        "governance replacement MUST be marked"
    );
    assert_eq!(
        owner_replacement.pub_key, ordinary.pub_key,
        "owner replacement is signed by the replaced claim's OWN owner key"
    );
    assert_ne!(
        governance_replacement.pub_key, ordinary.pub_key,
        "governance replacement is signed by a DIFFERENT key (the effective charter's owner)"
    );
}

#[test]
fn bootstrap_seizure_transactions_verify() {
    let file = fixtures::load("bootstrap_seizure.json");
    assert_eq!(file.charters.len(), 1, "the attacker's founding charter");
    assert_eq!(file.claims.len(), 1, "the incumbent's pre-existing claim");

    assert!(
        file.charters[0].verify().is_ok(),
        "{:?}",
        file.charters[0].verify()
    );
    assert!(
        file.claims[0].verify().is_ok(),
        "{:?}",
        file.claims[0].verify()
    );

    let founding = &file.charters[0].payload;
    let pre_existing = &file.claims[0].payload;
    assert_eq!(
        founding.prior, None,
        "attack targets the founding-charter path"
    );
    assert!(
        pre_existing.now < founding.now,
        "the claim predates the founding charter — it is the pre-existing context"
    );
    assert_ne!(
        founding.owner, pre_existing.owner,
        "the founding signer is NOT the incumbent claim's owner — the seizure attempt"
    );
}
