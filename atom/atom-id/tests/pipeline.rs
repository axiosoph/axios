//! Integration tests for the no-charter Verification Pipeline subset.
//!
//! Exercises Verification Pipeline steps 6 (claim side only), 8, 11, and
//! 13 (`docs/specs/atom-transactions.md`, "Verification Pipeline" →
//! "Local Verification") against real signed-and-verified transactions,
//! built from freshly generated Ed25519 keys rather than fixture-faked
//! values. Steps 2, 3, 7, 9, 10, 12 require charter data and are out of
//! scope for this node (`n2-verify-core`) — see `n3-verify-charter-steps`.
//!
//! Spec: `docs/specs/atom-transactions.md` `[claim-key-required]`,
//! `[publish-chains-claim]`, `[owner-authorization-delegated]`,
//! `[symmetric-payloads]`.

use atom_id::{
    Alg, Anchor, AtomId, ClaimPayload, Czd, Label, OwnerKind, OwnerRef, PublishPayload, RawVersion,
    Thumbprint, VerifyError, czd_for_alg, verify_atom_id, verify_claim_key_thumbprint,
    verify_publish_authorized, verify_publish_chains_claim, verify_publish_key_thumbprint,
};
use coz_rs::Ed25519;
use serde::Serialize;

// ============================================================================
// Deterministic key + signing helpers
// ============================================================================

/// Derive a deterministic Ed25519 keypair from a repeated seed byte.
///
/// A fixture-only shortcut — real keys are never constructed this way.
fn key(seed: u8) -> (Vec<u8>, Vec<u8>, Thumbprint) {
    let sk = coz_rs::signing_key_from_bytes::<Ed25519>(&[seed; 32])
        .expect("a fixed 32-byte seed is always a valid Ed25519 signing key");
    let prv = sk.private_key_bytes();
    let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
    let tmb = sk.thumbprint().clone();
    (prv, pub_bytes, tmb)
}

/// Sign a payload's canonical JSON with a fixture key.
///
/// Returns the exact `pay_json` bytes that were signed alongside the
/// signature — the pair a consumer needs to recompute `czd`.
fn sign<T: Serialize>(payload: &T, prv: &[u8], pub_bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let pay_json = serde_json::to_vec(payload).expect("fixture payload always serializes");
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", prv, pub_bytes)
        .expect("fixed Ed25519 fixture keys always sign");
    (pay_json, sig)
}

fn test_anchor() -> Anchor {
    Anchor::new(vec![0xA0; 4])
}

fn test_label() -> Label {
    Label::try_from("pipeline-atom").expect("valid label")
}

fn test_atom_id() -> AtomId {
    AtomId::new(test_anchor(), test_label())
}

/// Build a claim payload signed by the key at `seed`, with its own
/// declared `tmb` set from `tmb_field` (independent of the signing key,
/// so callers can construct a genuine tmb/key mismatch for step 6).
#[allow(clippy::too_many_arguments)]
fn build_claim(
    signer_seed: u8,
    owner: Vec<u8>,
    tmb_field: Thumbprint,
) -> (ClaimPayload, Vec<u8>, Vec<u8>, Vec<u8>) {
    let (prv, pub_bytes, _signer_tmb) = key(signer_seed);
    let claim = ClaimPayload::new(
        Alg::Ed25519,
        test_atom_id(),
        1_000,
        OwnerRef::new(OwnerKind::SingleKey, owner),
        "cargo".to_string(),
        vec![0; 32],
        tmb_field,
    );
    let (pay_json, sig) = sign(&claim, &prv, &pub_bytes);
    (claim, pay_json, sig, pub_bytes)
}

/// Build a publish payload signed by the key at `seed`, chaining to
/// `claim_czd` and declaring `tmb_field` as its own signing-key
/// thumbprint.
fn build_publish(
    signer_seed: u8,
    claim_czd: Czd,
    tmb_field: Thumbprint,
) -> (PublishPayload, Vec<u8>, Vec<u8>) {
    let (prv, pub_bytes, _signer_tmb) = key(signer_seed);
    let publish = PublishPayload::new(
        Alg::Ed25519,
        test_atom_id(),
        claim_czd,
        vec![0xAB; 32],
        2_000,
        "".to_string(),
        vec![1; 32],
        tmb_field,
        RawVersion::new("1.0.0".to_string()),
    );
    let (pay_json, sig) = sign(&publish, &prv, &pub_bytes);
    (publish, pay_json, sig)
}

// ============================================================================
// Step 6 — key thumbprints match (claim side)
// ============================================================================

#[test]
fn step6_accepts_claim_whose_declared_tmb_matches_its_signing_key() {
    let (_prv, pub_a, tmb_a) = key(20);
    let (claim, _pay_json, _sig, _pub_bytes) = build_claim(20, vec![0; 4], tmb_a);

    let result = verify_claim_key_thumbprint(&claim, "Ed25519", &pub_a);
    assert!(
        result.is_ok(),
        "declared tmb matching the real signing key must pass: {result:?}"
    );
}

#[test]
fn step6_rejects_claim_whose_declared_tmb_does_not_match_its_signing_key() {
    // Signed with key 21's real key, but the payload LIES about which
    // key signed it by declaring key 22's thumbprint instead — the
    // signature itself still verifies fine (any key can sign its own
    // payload); only the thumbprint-binding check catches this.
    let (_prv_a, pub_a, _tmb_a) = key(21);
    let (_prv_b, _pub_b, tmb_b) = key(22);
    let (claim, _pay_json, _sig, _pub_bytes) = build_claim(21, vec![0; 4], tmb_b);

    let result = verify_claim_key_thumbprint(&claim, "Ed25519", &pub_a);
    assert!(
        matches!(result, Err(VerifyError::ThumbprintMismatch)),
        "mismatched declared tmb must be rejected: {result:?}"
    );
}

// ============================================================================
// Step 6 (publish side) — closes the documented tmb-binding soundness gap
// ============================================================================
//
// `verify_publish_authorized`'s own doc comment names this precondition
// precisely: nothing bound `tmb(publish.key) == publish.tmb` before this
// function existed, which is exploitable exactly the way `build_publish`'s
// `tmb_field` parameter (independent of the actual signing key) already
// lets these tests construct: a signature that verifies fine while the
// payload's declared `tmb` names an unrelated key.

#[test]
fn step6_accepts_publish_whose_declared_tmb_matches_its_signing_key() {
    let (_prv, pub_a, tmb_a) = key(60);
    let claim_czd = Czd::from_bytes(vec![1; 32]);
    let (publish, _pay_json, _sig) = build_publish(60, claim_czd, tmb_a);

    let result = verify_publish_key_thumbprint(&publish, "Ed25519", &pub_a);
    assert!(
        result.is_ok(),
        "declared tmb matching the real signing key must pass: {result:?}"
    );
}

#[test]
fn step6_rejects_publish_whose_declared_tmb_does_not_match_its_signing_key() {
    // Signed with key 61's real key, but the payload LIES about which key
    // signed it by declaring key 62's thumbprint instead -- the signature
    // itself still verifies fine; only the thumbprint-binding check
    // catches this, exactly the exploit `verify_publish_authorized`'s doc
    // comment names.
    let (_prv_a, pub_a, _tmb_a) = key(61);
    let (_prv_b, _pub_b, tmb_b) = key(62);
    let claim_czd = Czd::from_bytes(vec![1; 32]);
    let (publish, _pay_json, _sig) = build_publish(61, claim_czd, tmb_b);

    let result = verify_publish_key_thumbprint(&publish, "Ed25519", &pub_a);
    assert!(
        matches!(result, Err(VerifyError::ThumbprintMismatch)),
        "mismatched declared tmb must be rejected: {result:?}"
    );
}

// ============================================================================
// Step 8 — publish chains to claim
// ============================================================================

#[test]
fn step8_accepts_publish_naming_the_correct_claims_czd() {
    let (_prv_owner, _pub_owner, tmb_owner) = key(30);
    let (_claim, claim_pay_json, claim_sig, _pub_bytes) =
        build_claim(30, vec![9; 4], tmb_owner.clone());
    let claim_czd = czd_for_alg(&claim_pay_json, &claim_sig, "Ed25519")
        .expect("Ed25519 is a supported algorithm");

    let (publish, _pub_pay_json, _pub_sig) = build_publish(31, claim_czd, tmb_owner);

    let result = verify_publish_chains_claim(&publish, &claim_pay_json, &claim_sig, "Ed25519");
    assert!(
        result.is_ok(),
        "publish naming the claim's real czd must chain: {result:?}"
    );
}

#[test]
fn step8_rejects_publish_naming_the_wrong_claims_czd() {
    let (_prv_owner, _pub_owner, tmb_owner) = key(32);
    let (_claim, claim_pay_json, claim_sig, _pub_bytes) =
        build_claim(32, vec![9; 4], tmb_owner.clone());

    // A czd that does not correspond to this claim at all.
    let wrong_czd = Czd::from_bytes(vec![0xFF; 32]);
    let (publish, _pub_pay_json, _pub_sig) = build_publish(33, wrong_czd, tmb_owner);

    let result = verify_publish_chains_claim(&publish, &claim_pay_json, &claim_sig, "Ed25519");
    assert!(
        matches!(result, Err(VerifyError::ClaimChainMismatch)),
        "publish naming an unrelated czd must be rejected: {result:?}"
    );
}

// ============================================================================
// Step 11 — publish signer authorized by claim.owner
// ============================================================================

#[test]
fn step11_accepts_publish_signed_by_claims_owner() {
    let (_prv_owner, _pub_owner, tmb_owner) = key(40);
    let (claim, claim_pay_json, claim_sig, _pub_bytes) =
        build_claim(40, tmb_owner.as_bytes().to_vec(), tmb_owner.clone());
    let claim_czd = czd_for_alg(&claim_pay_json, &claim_sig, "Ed25519")
        .expect("Ed25519 is a supported algorithm");

    // Signed by the SAME key the claim names as owner.
    let (publish, _pub_pay_json, _pub_sig) = build_publish(40, claim_czd, tmb_owner);

    let result = verify_publish_authorized(&publish, &claim);
    assert!(
        result.is_ok(),
        "publish signed by claim.owner's own key must be authorized: {result:?}"
    );
}

#[test]
fn step11_rejects_publish_signed_by_a_stranger() {
    let (_prv_owner, _pub_owner, tmb_owner) = key(41);
    let (claim, claim_pay_json, claim_sig, _pub_bytes) =
        build_claim(41, tmb_owner.as_bytes().to_vec(), tmb_owner);
    let claim_czd = czd_for_alg(&claim_pay_json, &claim_sig, "Ed25519")
        .expect("Ed25519 is a supported algorithm");

    // Signed by an unrelated key — its thumbprint doesn't match claim.owner.
    let (_prv_stranger, _pub_stranger, tmb_stranger) = key(42);
    let (publish, _pub_pay_json, _pub_sig) = build_publish(42, claim_czd, tmb_stranger);

    let result = verify_publish_authorized(&publish, &claim);
    assert!(
        matches!(result, Err(VerifyError::Unauthorized)),
        "publish signed by a non-owner key must be rejected: {result:?}"
    );
}

// ============================================================================
// Step 13 — AtomId matches payload fields
// ============================================================================

#[test]
fn step13_accepts_claim_payload_matching_the_expected_atom_id() {
    let (_prv, _pub_bytes, tmb) = key(50);
    let (claim, _pay_json, _sig, _pub_bytes2) = build_claim(50, vec![0; 4], tmb);

    let result = verify_atom_id(&claim.anchor, &claim.label, &test_atom_id());
    assert!(
        result.is_ok(),
        "claim payload's own (anchor, label) must match the expected AtomId: {result:?}"
    );
}

#[test]
fn step13_accepts_publish_payload_matching_the_expected_atom_id() {
    let (_prv, _pub_bytes, tmb) = key(51);
    let claim_czd = Czd::from_bytes(vec![7; 32]);
    let (publish, _pay_json, _sig) = build_publish(51, claim_czd, tmb);

    let result = verify_atom_id(&publish.anchor, &publish.label, &test_atom_id());
    assert!(
        result.is_ok(),
        "publish payload's own (anchor, label) must match the expected AtomId: {result:?}"
    );
}

#[test]
fn step13_rejects_payload_with_mismatched_anchor() {
    let (_prv, _pub_bytes, tmb) = key(52);
    let (claim, _pay_json, _sig, _pub_bytes2) = build_claim(52, vec![0; 4], tmb);

    let wrong_expected = AtomId::new(Anchor::new(vec![0xDE; 4]), test_label());
    let result = verify_atom_id(&claim.anchor, &claim.label, &wrong_expected);
    assert!(
        matches!(result, Err(VerifyError::AtomIdMismatch)),
        "a mismatched anchor must be rejected: {result:?}"
    );
}

#[test]
fn step13_rejects_payload_with_mismatched_label() {
    let (_prv, _pub_bytes, tmb) = key(53);
    let (claim, _pay_json, _sig, _pub_bytes2) = build_claim(53, vec![0; 4], tmb);

    let wrong_expected = AtomId::new(
        test_anchor(),
        Label::try_from("some-other-label").expect("valid label"),
    );
    let result = verify_atom_id(&claim.anchor, &claim.label, &wrong_expected);
    assert!(
        matches!(result, Err(VerifyError::AtomIdMismatch)),
        "a mismatched label must be rejected: {result:?}"
    );
}
