//! Tests for atom identity types.

use std::ffi::OsStr;
use std::str::FromStr;

use crate::{Anchor, AtomId, Error, Identifier, Label, NAME_MAX, RawVersion, Tag};

// ============================================================================
// Label
// ============================================================================

#[test]
fn label_valid_representative() {
    // Latin + extensions, CJK, Mixed — 3 representative cases
    let valid = ["Café_au_lait-123", "漢字ひらがな", "αβγ_кириллица"];
    for s in valid {
        assert!(Label::try_from(s).is_ok(), "expected '{s}' to be valid");
    }
}

#[test]
fn label_rejects_empty() {
    assert_eq!(Label::try_from(""), Err(Error::Empty));
}

#[test]
fn label_rejects_invalid_start() {
    for (input, expected_char) in [
        ("9atom", '9'),
        ("_atom", '_'),
        ("-atom", '-'),
        ("%atom", '%'),
    ] {
        assert_eq!(
            Label::try_from(input),
            Err(Error::InvalidStart(expected_char)),
        );
    }
}

#[test]
fn label_rejects_invalid_chars() {
    // Multiple invalid chars collected
    assert_eq!(
        Label::try_from("a-!@#$%^&*()_-asdf"),
        Err(Error::InvalidCharacters("!@#$%^&*()".into())),
    );
    // Space
    assert_eq!(
        Label::try_from("hello world"),
        Err(Error::InvalidCharacters(" ".into())),
    );
    // Emoji
    assert_eq!(
        Label::try_from("Café♥"),
        Err(Error::InvalidCharacters("♥".into())),
    );
}

#[test]
fn label_rejects_zero_width_space() {
    // ZWS as start
    assert_eq!(
        Label::try_from("\u{200B}"),
        Err(Error::InvalidStart('\u{200B}')),
    );
    // ZWS in middle
    assert_eq!(
        Label::try_from("α\u{200B}"),
        Err(Error::InvalidCharacters("\u{200B}".into())),
    );
}

#[test]
fn label_rejects_control_chars() {
    for (input, bad) in [("Öö\t", "\t"), ("Ææ\n", "\n"), ("Łł\r", "\r")] {
        assert_eq!(
            Label::try_from(input),
            Err(Error::InvalidCharacters(bad.into())),
        );
    }
}

#[test]
fn label_rejects_too_long() {
    // Exactly NAME_MAX bytes: should succeed
    let at_limit = "a".repeat(NAME_MAX);
    assert!(Label::try_from(at_limit.as_str()).is_ok());

    // One byte over: should fail
    let over_limit = "a".repeat(NAME_MAX + 1);
    assert_eq!(Label::try_from(over_limit.as_str()), Err(Error::TooLong));
}

#[test]
fn label_nfkc_normalization() {
    // NFKC: the ligature ﬁ normalizes to "fi"
    let label_fi = Label::try_from("ﬁlter").unwrap();
    assert_eq!(&*label_fi, "filter");

    // Composed vs decomposed ñ should produce the same label
    let composed = Label::try_from("año").unwrap();
    let decomposed = Label::try_from("an\u{0303}o").unwrap();
    assert_eq!(composed, decomposed);
}

#[test]
fn label_display_deref() {
    let label = Label::try_from("myLabel").unwrap();
    assert_eq!(label.to_string(), "myLabel");
    let s: &str = &label;
    assert_eq!(s, "myLabel");
}

#[test]
fn label_from_str() {
    let parsed: Label = "hello".parse().unwrap();
    assert_eq!(&*parsed, "hello");
}

#[test]
fn label_try_from_string() {
    let owned = String::from("hello");
    let label = Label::try_from(owned).unwrap();
    assert_eq!(&*label, "hello");
}

#[test]
fn label_try_from_os_str() {
    let os = OsStr::new("hello");
    let label = Label::try_from(os).unwrap();
    assert_eq!(&*label, "hello");
}

#[test]
fn label_serde_roundtrip() {
    let label = Label::try_from("myLabel").unwrap();
    let json = serde_json::to_string(&label).unwrap();
    assert_eq!(json, "\"myLabel\"");
    let back: Label = serde_json::from_str(&json).unwrap();
    assert_eq!(back, label);
}

#[test]
fn label_serde_rejects_invalid() {
    let result: Result<Label, _> = serde_json::from_str("\"9invalid\"");
    assert!(result.is_err());
}

// ============================================================================
// Identifier (strict UAX #31)
// ============================================================================

#[test]
fn identifier_accepts_underscore_continue() {
    assert!(Identifier::try_from("my_ident").is_ok());
}

#[test]
fn identifier_rejects_hyphen() {
    assert!(
        Identifier::try_from("my-ident").is_err(),
        "hyphens are invalid in Identifier"
    );
}

#[test]
fn identifier_rejects_empty() {
    assert_eq!(Identifier::try_from(""), Err(Error::Empty));
}

#[test]
fn identifier_rejects_number_start() {
    assert_eq!(Identifier::try_from("1foo"), Err(Error::InvalidStart('1')));
}

#[test]
fn identifier_serde_roundtrip() {
    let id = Identifier::try_from("myIdent").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"myIdent\"");
    let back: Identifier = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

// ============================================================================
// Tag
// ============================================================================

#[test]
fn tag_allows_colon_and_dot() {
    assert!(Tag::try_from("version:1.0").is_ok());
}

#[test]
fn tag_allows_hyphen() {
    assert!(Tag::try_from("my-tag:1.0").is_ok());
}

#[test]
fn tag_rejects_double_dot() {
    assert_eq!(
        Tag::try_from("bad..tag"),
        Err(Error::InvalidCharacters("..".into())),
    );
}

#[test]
fn tag_rejects_invalid_start() {
    assert_eq!(Tag::try_from(".tag"), Err(Error::InvalidStart('.')));
    assert_eq!(Tag::try_from(":tag"), Err(Error::InvalidStart(':')));
}

#[test]
fn tag_serde_roundtrip() {
    let tag = Tag::try_from("v1.0:stable").unwrap();
    let json = serde_json::to_string(&tag).unwrap();
    assert_eq!(json, "\"v1.0:stable\"");
    let back: Tag = serde_json::from_str(&json).unwrap();
    assert_eq!(back, tag);
}

// ============================================================================
// Anchor
// ============================================================================

#[test]
fn anchor_display_b64ut() {
    let anchor = Anchor::new(vec![0xAB, 0xCD, 0xEF]);
    let s = anchor.to_string();
    assert!(!s.contains('='), "b64ut should not contain padding");
    assert!(!s.is_empty());
}

#[test]
fn anchor_roundtrip() {
    let bytes = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let anchor = Anchor::new(bytes.clone());
    assert_eq!(anchor.as_bytes(), &bytes);
}

#[test]
fn anchor_serde_roundtrip() {
    let anchor = Anchor::new(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let json = serde_json::to_string(&anchor).unwrap();
    let back: Anchor = serde_json::from_str(&json).unwrap();
    assert_eq!(back, anchor);
}

#[test]
fn anchor_from_vec() {
    let bytes = vec![1, 2, 3];
    let anchor: Anchor = bytes.clone().into();
    assert_eq!(anchor.as_bytes(), &bytes);
}

// ============================================================================
// AtomId
// ============================================================================

#[test]
fn atom_id_same_inputs_same_id() {
    let a1 = AtomId::new(Anchor::new(vec![1, 2, 3]), Label::try_from("pkg").unwrap());
    let a2 = AtomId::new(Anchor::new(vec![1, 2, 3]), Label::try_from("pkg").unwrap());
    assert_eq!(a1, a2, "same (anchor, label) must produce equal AtomId");
}

#[test]
fn atom_id_different_labels() {
    let a1 = AtomId::new(
        Anchor::new(vec![1, 2, 3]),
        Label::try_from("alpha").unwrap(),
    );
    let a2 = AtomId::new(Anchor::new(vec![1, 2, 3]), Label::try_from("beta").unwrap());
    assert_ne!(a1, a2, "different labels must produce different AtomId");
}

#[test]
fn atom_id_different_anchors() {
    let a1 = AtomId::new(Anchor::new(vec![1, 2, 3]), Label::try_from("pkg").unwrap());
    let a2 = AtomId::new(Anchor::new(vec![4, 5, 6]), Label::try_from("pkg").unwrap());
    assert_ne!(a1, a2, "different anchors must produce different AtomId");
}

#[test]
fn atom_id_accessors() {
    let anchor = Anchor::new(vec![10, 20]);
    let label = Label::try_from("myPkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());
    assert_eq!(id.anchor(), &anchor);
    assert_eq!(id.label(), &label);
}

#[test]
fn atom_id_display_format() {
    let id = AtomId::new(
        Anchor::new(vec![0xDE, 0xAD]),
        Label::try_from("test").unwrap(),
    );
    let s = id.to_string();
    assert!(s.contains("::"), "display must use :: delimiter");
    assert!(s.ends_with("::test"), "display must end with ::label");
}

#[test]
fn atom_id_roundtrip() {
    let id = AtomId::new(
        Anchor::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
        Label::try_from("my-package").unwrap(),
    );
    let s = id.to_string();
    let parsed: AtomId = s.parse().unwrap();
    assert_eq!(parsed, id, "roundtrip must preserve identity");
}

#[test]
fn atom_id_empty_string() {
    assert_eq!(AtomId::from_str(""), Err(Error::InvalidFormat));
}

#[test]
fn atom_id_missing_delimiter() {
    assert_eq!(AtomId::from_str("AQID.test"), Err(Error::InvalidFormat),);
}

#[test]
fn atom_id_bad_anchor_encoding() {
    assert_eq!(
        AtomId::from_str("!!!invalid!!!::test"),
        Err(Error::InvalidAnchor),
    );
}

#[test]
fn atom_id_bad_label() {
    assert_eq!(AtomId::from_str("AQID::9invalid"), Err(Error::InvalidLabel),);
}

#[test]
fn atom_id_serde_roundtrip() {
    let id = AtomId::new(
        Anchor::new(vec![0xAB, 0xCD, 0xEF]),
        Label::try_from("serde-test").unwrap(),
    );
    let json = serde_json::to_string(&id).unwrap();
    let back: AtomId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

// ============================================================================
// RawVersion
// ============================================================================

#[test]
fn rawversion_any_string() {
    // Empty, unicode, spaces — all valid
    for s in ["", "1.0.0", "v2.3-rc1", "日本語", "with spaces", "🎉"] {
        let v = RawVersion::new(s.to_owned());
        assert_eq!(v.as_str(), s);
    }
}

#[test]
fn rawversion_display() {
    let v = RawVersion::new("1.2.3".into());
    assert_eq!(v.to_string(), "1.2.3");
}

#[test]
fn rawversion_from_str() {
    let v: RawVersion = "1.0.0".parse().unwrap();
    assert_eq!(v.as_str(), "1.0.0");
}

#[test]
fn rawversion_equality() {
    let a = RawVersion::new("1.0".into());
    let b = RawVersion::new("1.0".into());
    let c = RawVersion::new("2.0".into());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn rawversion_ordering() {
    let a = RawVersion::new("1.0".into());
    let b = RawVersion::new("2.0".into());
    assert!(a < b, "lexicographic ordering via derived Ord");
}

#[test]
fn rawversion_serde_roundtrip() {
    let v = RawVersion::new("3.1.4-beta".into());
    let json = serde_json::to_string(&v).unwrap();
    assert_eq!(json, "\"3.1.4-beta\"");
    let back: RawVersion = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v);
}

// ============================================================================
// ClaimPayload
// ============================================================================

fn test_anchor() -> Anchor {
    Anchor::new(vec![1, 2, 3, 4])
}

fn test_label() -> Label {
    Label::try_from("my-pkg").unwrap()
}

fn test_id() -> AtomId {
    AtomId::new(test_anchor(), test_label())
}

fn test_tmb() -> crate::Thumbprint {
    crate::Thumbprint::from_bytes(vec![10, 20, 30])
}

#[test]
fn claim_payload_typ_constant() {
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    assert_eq!(claim.typ, crate::TYP_CLAIM);
    assert_eq!(claim.typ, "atom/claim");
}

#[test]
fn publish_payload_typ_constant() {
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    assert_eq!(publish.typ, crate::TYP_PUBLISH);
    assert_eq!(publish.typ, "atom/publish");
}

#[test]
fn claim_payload_has_anchor_label() {
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    assert_eq!(claim.anchor, test_anchor());
    assert_eq!(claim.label, test_label());
}

#[test]
fn publish_payload_has_anchor_label() {
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    assert_eq!(publish.anchor, test_anchor());
    assert_eq!(publish.label, test_label());
}

#[test]
fn claim_payload_serde_roundtrip() {
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let json = serde_json::to_string(&claim).unwrap();
    let back: crate::ClaimPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, claim);
}

// ============================================================================
// ClaimPayload replacement (`[claim-replacement-authority]`)
// ============================================================================

#[test]
fn claim_payload_ordinary_has_no_replacement() {
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    assert_eq!(claim.prior, None);
    assert!(!claim.governance);

    let json = serde_json::to_string(&claim).unwrap();
    let back: crate::ClaimPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, claim);
}

#[test]
fn claim_payload_owner_replacement_construct_and_roundtrip() {
    let claim = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![99],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![1, 2, 3]),
        false,
        vec![1; 32],
        test_tmb(),
    );
    assert_eq!(claim.prior, Some(crate::Czd::from_bytes(vec![1, 2, 3])));
    assert!(!claim.governance);

    let json = serde_json::to_string(&claim).unwrap();
    let back: crate::ClaimPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, claim);
}

#[test]
fn claim_payload_governance_replacement_construct_and_roundtrip() {
    let claim = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![99],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![4, 5, 6]),
        true,
        vec![1; 32],
        test_tmb(),
    );
    assert_eq!(claim.prior, Some(crate::Czd::from_bytes(vec![4, 5, 6])));
    assert!(claim.governance);

    let json = serde_json::to_string(&claim).unwrap();
    let back: crate::ClaimPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, claim);
}

#[test]
fn verify_claim_replacement_accepts_owner_replacement() {
    // Ordinary path: replacement signed by the replaced claim's own
    // owner, unmarked.
    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![1], // prior claim's owner
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let charter_owner = vec![2]; // unrelated to prior.owner

    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000, // strictly after prior.now
        vec![9],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![9, 9, 9]),
        false, // unmarked — the ordinary owner-replacement path
        vec![1; 32],
        crate::Thumbprint::from_bytes(vec![1]), // == prior.owner
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(
        result.is_ok(),
        "owner replacement must be accepted: {result:?}"
    );
}

#[test]
fn verify_claim_replacement_accepts_governance_replacement() {
    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![1], // prior claim's owner
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let charter_owner = vec![2];

    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![9],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![9, 9, 9]),
        true, // MUST carry governance: true
        vec![1; 32],
        crate::Thumbprint::from_bytes(vec![2]), // == charter_owner, NOT prior.owner
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(
        result.is_ok(),
        "marked governance replacement must be accepted: {result:?}"
    );
}

#[test]
fn verify_claim_replacement_rejects_governance_authorized_but_unmarked() {
    // Signed by the charter owner but NOT marked `governance: true` —
    // [claim-replacement-authority]'s "MUST carry governance: true" makes
    // this its own failure mode, distinct from a genuine third party.
    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![1],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let charter_owner = vec![2];

    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![9],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![9, 9, 9]),
        false, // unmarked, despite being signed by the charter owner
        vec![1; 32],
        crate::Thumbprint::from_bytes(vec![2]), // == charter_owner
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(
        matches!(result, Err(crate::VerifyError::Unauthorized)),
        "charter-owner-signed but unmarked replacement must be rejected: {result:?}"
    );
}

#[test]
fn verify_claim_replacement_rejects_stale_now() {
    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![1],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let charter_owner = vec![2];

    // Authorized (owner path) but `now` does not exceed prior.now.
    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000, // == prior.now, not strictly after
        vec![9],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![9, 9, 9]),
        false,
        vec![1; 32],
        crate::Thumbprint::from_bytes(vec![1]), // == prior.owner
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(
        matches!(result, Err(crate::VerifyError::ReplacementNotAfterPrior)),
        "replacement.now not strictly after prior.now must be rejected: {result:?}"
    );
}

#[test]
fn verify_claim_replacement_rejects_changed_identity() {
    let prior_id = test_id();
    let other_id = AtomId::new(
        Anchor::new(vec![0xAA, 0xBB]),
        Label::try_from("other-pkg").unwrap(),
    );

    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        prior_id,
        1000,
        vec![1],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let charter_owner = vec![2];

    // Authorized and temporally fine, but names a different (anchor, label).
    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        other_id,
        2000,
        vec![9],
        "cargo".to_string(),
        crate::Czd::from_bytes(vec![9, 9, 9]),
        false,
        vec![1; 32],
        crate::Thumbprint::from_bytes(vec![1]), // == prior.owner
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(
        matches!(result, Err(crate::VerifyError::ReplacementIdentityChanged)),
        "a replacement changing (anchor, label) must be rejected: {result:?}"
    );
}

#[test]
fn verify_claim_replacement_rejects_third_authority() {
    // Once implemented: `[claim-replacement-authority]` names exactly two
    // authorities — owner replacement (signed by the replaced claim's
    // owner) and governance replacement (signed by the effective
    // charter's owner, MUST carry `governance: true`). A replacement
    // signed by neither is a third, unauthorized path and MUST fail
    // closed rather than be admitted.
    let prior = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![1], // prior claim's owner
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let prior_czd = crate::Czd::from_bytes(vec![9, 9, 9]); // stand-in for czd(prior)
    let charter_owner = vec![2]; // effective charter's owner (unrelated to prior.owner)

    // Unmarked replacement signed by neither prior.owner nor charter_owner.
    let replacement = crate::ClaimPayload::new_replacement(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![3], // third-party owner — authorized by neither authority
        "cargo".to_string(),
        prior_czd,
        false,
        vec![1; 32],
        test_tmb(),
    );

    let result = crate::verify_claim_replacement(&replacement, &prior, &charter_owner);
    assert!(result.is_err(), "third-party replacement must fail closed");
}

#[test]
fn publish_payload_serde_roundtrip() {
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    let json = serde_json::to_string(&publish).unwrap();
    let back: crate::PublishPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, publish);
}

#[test]
fn atom_id_from_claim() {
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let id = AtomId::new(claim.anchor.clone(), claim.label.clone());
    assert_eq!(id, test_id());
}

#[test]
fn atom_id_from_publish() {
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    let id = AtomId::new(publish.anchor.clone(), publish.label.clone());
    assert_eq!(id, test_id());
}

#[test]
fn both_payloads_same_identity() {
    let id = test_id();
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        id.clone(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        id.clone(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    let claim_id = AtomId::new(claim.anchor, claim.label);
    let publish_id = AtomId::new(publish.anchor, publish.label);
    assert_eq!(
        claim_id, publish_id,
        "same AtomId → same identity in both payloads"
    );
}

// ============================================================================
// Verification
// ============================================================================

/// Helper: generate an Ed25519 key pair and return (prv_bytes, pub_bytes, thumbprint).
fn gen_ed25519_key() -> (Vec<u8>, Vec<u8>, crate::Thumbprint) {
    use coz_rs::Ed25519;

    let sk = coz_rs::SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes();
    let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
    let tmb = sk.thumbprint().clone();
    (prv, pub_bytes, tmb)
}

#[test]
fn verify_claim_roundtrip() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let pay_json = serde_json::to_vec(&claim).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

    let result = crate::verify_claim(&pay_json, &sig, "Ed25519", &pub_bytes);
    assert!(result.is_ok(), "valid claim should verify: {result:?}");
    let verified = result.unwrap();
    assert_eq!(verified.anchor, test_anchor());
    assert_eq!(verified.label, test_label());
    assert_eq!(verified.typ, crate::TYP_CLAIM);
}

#[test]
fn verify_publish_roundtrip() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let publish = crate::PublishPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        2000,
        "src/lib".into(),
        vec![9, 10],
        tmb,
        crate::RawVersion::new("1.0.0".into()),
    );
    let pay_json = serde_json::to_vec(&publish).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

    let result = crate::verify_publish(&pay_json, &sig, "Ed25519", &pub_bytes);
    assert!(result.is_ok(), "valid publish should verify: {result:?}");
    let verified = result.unwrap();
    assert_eq!(verified.anchor, test_anchor());
    assert_eq!(verified.label, test_label());
    assert_eq!(verified.typ, crate::TYP_PUBLISH);
}

#[test]
fn verify_claim_wrong_sig() {
    let (_prv, pub_bytes, tmb) = gen_ed25519_key();
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let pay_json = serde_json::to_vec(&claim).unwrap();
    let bad_sig = vec![0u8; 64]; // garbage signature

    let result = crate::verify_claim(&pay_json, &bad_sig, "Ed25519", &pub_bytes);
    assert!(
        matches!(result, Err(crate::VerifyError::InvalidSignature)),
        "wrong sig should be InvalidSignature: {result:?}"
    );
}

#[test]
fn verify_claim_wrong_typ() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    // Build a claim but tamper with the typ field in the JSON
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let mut json_val: serde_json::Value = serde_json::to_value(&claim).unwrap();
    json_val["typ"] = serde_json::Value::String("atom/publish".into());
    let pay_json = serde_json::to_vec(&json_val).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

    let result = crate::verify_claim(&pay_json, &sig, "Ed25519", &pub_bytes);
    assert!(
        matches!(result, Err(crate::VerifyError::WrongTyp { .. })),
        "tampered typ should fail with WrongTyp: {result:?}"
    );
}

#[test]
fn verify_claim_unknown_alg() {
    let (_prv, pub_bytes, tmb) = gen_ed25519_key();
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let pay_json = serde_json::to_vec(&claim).unwrap();

    let result = crate::verify_claim(&pay_json, &[], "UNSUPPORTED", &pub_bytes);
    assert!(
        matches!(result, Err(crate::VerifyError::UnsupportedAlgorithm(_))),
        "unknown alg should be UnsupportedAlgorithm: {result:?}"
    );
}

// ============================================================================
// czd_for_alg
// ============================================================================

#[test]
fn czd_for_alg_matches_independent_computation() {
    use coz_rs::Ed25519;

    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let pay_json = serde_json::to_vec(&claim).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

    let czd = crate::czd_for_alg(&pay_json, &sig, "Ed25519").expect("valid alg computes a czd");

    // Independent recomputation via a separate code path: the compile-time
    // generic `Czd::compute`, not the runtime dispatcher under test.
    let cad = coz_rs::canonical_hash::<Ed25519>(&pay_json, None).unwrap();
    let expected = crate::Czd::compute::<Ed25519>(&cad, &sig);
    assert_eq!(
        czd, expected,
        "czd_for_alg must match independent Czd::compute"
    );
}

#[test]
fn czd_for_alg_is_deterministic_and_binds_to_sig() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let claim = crate::ClaimPayload::new(
        crate::Alg::Ed25519,
        test_id(),
        1000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        tmb,
    );
    let pay_json = serde_json::to_vec(&claim).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

    let czd1 = crate::czd_for_alg(&pay_json, &sig, "Ed25519").unwrap();
    let czd2 = crate::czd_for_alg(&pay_json, &sig, "Ed25519").unwrap();
    assert_eq!(czd1, czd2, "czd computation must be deterministic");

    let other_sig = vec![0u8; sig.len()];
    let czd3 = crate::czd_for_alg(&pay_json, &other_sig, "Ed25519").unwrap();
    assert_ne!(czd1, czd3, "czd must bind to the signature, not just pay");
}

#[test]
fn czd_for_alg_unknown_alg() {
    let result = crate::czd_for_alg(b"{}", &[], "UNSUPPORTED");
    assert!(
        matches!(result, Err(crate::VerifyError::UnsupportedAlgorithm(_))),
        "unknown alg should be UnsupportedAlgorithm: {result:?}"
    );
}

// ============================================================================
// Charter-dependent pipeline steps (7, 9, 10)
// ============================================================================
//
// Steps 2 and 3 (charter chain signatures / succession) are tested in
// `charter.rs`. Step 12 (`verify_claim_replacement`) is tested above.

#[test]
fn verify_claim_chains_charter_accepts_matching_anchor() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let founding =
        crate::CharterPayload::new(crate::Alg::Ed25519, 1000, vec![1], None, vec![0; 32], tmb);
    let founding_json = serde_json::to_vec(&founding).unwrap();
    let (founding_sig, _cad) =
        coz_rs::sign_json(&founding_json, "Ed25519", &prv, &pub_bytes).unwrap();
    let founding_czd = crate::czd_for_alg(&founding_json, &founding_sig, "Ed25519").unwrap();

    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        AtomId::new(Anchor::new(founding_czd.as_bytes().to_vec()), test_label()),
        2000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );

    let result =
        crate::verify_claim_chains_charter(&claim, &founding_json, &founding_sig, "Ed25519");
    assert!(
        result.is_ok(),
        "claim anchored to the founding charter's czd must chain: {result:?}"
    );
}

#[test]
fn verify_claim_chains_charter_rejects_mismatched_anchor() {
    let (prv, pub_bytes, tmb) = gen_ed25519_key();
    let founding =
        crate::CharterPayload::new(crate::Alg::Ed25519, 1000, vec![1], None, vec![0; 32], tmb);
    let founding_json = serde_json::to_vec(&founding).unwrap();
    let (founding_sig, _cad) =
        coz_rs::sign_json(&founding_json, "Ed25519", &prv, &pub_bytes).unwrap();

    // Anchor that does not correspond to the founding charter's czd.
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        AtomId::new(Anchor::new(vec![0xFF; 32]), test_label()),
        2000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );

    let result =
        crate::verify_claim_chains_charter(&claim, &founding_json, &founding_sig, "Ed25519");
    assert!(
        matches!(result, Err(crate::VerifyError::ClaimChartersMismatch)),
        "claim anchored to an unrelated czd must be rejected: {result:?}"
    );
}

fn temporal_triple(
    charter_now: u64,
    claim_now: u64,
    publish_now: u64,
) -> (
    crate::CharterPayload,
    crate::ClaimPayload,
    crate::PublishPayload,
) {
    let charter = crate::CharterPayload::new(
        crate::Alg::ES256,
        charter_now,
        vec![1],
        None,
        vec![0; 32],
        test_tmb(),
    );
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        claim_now,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        test_tmb(),
    );
    let publish = crate::PublishPayload::new(
        crate::Alg::ES256,
        test_id(),
        crate::Czd::from_bytes(vec![5, 6]),
        vec![7, 8],
        publish_now,
        "src/lib".into(),
        vec![9, 10],
        test_tmb(),
        crate::RawVersion::new("1.0.0".into()),
    );
    (charter, claim, publish)
}

#[test]
fn verify_temporal_ordering_accepts_strictly_increasing() {
    let (charter, claim, publish) = temporal_triple(1000, 2000, 3000);
    let result = crate::verify_temporal_ordering(&charter, &claim, &publish);
    assert!(
        result.is_ok(),
        "a genuinely ordered triple must pass: {result:?}"
    );
}

#[test]
fn verify_temporal_ordering_rejects_charter_not_before_claim() {
    let (charter, claim, publish) = temporal_triple(2000, 2000, 3000);
    let result = crate::verify_temporal_ordering(&charter, &claim, &publish);
    assert!(
        matches!(result, Err(crate::VerifyError::TemporalOrderViolation)),
        "charter.now >= claim.now must be rejected: {result:?}"
    );
}

#[test]
fn verify_temporal_ordering_rejects_claim_not_before_publish() {
    let (charter, claim, publish) = temporal_triple(1000, 3000, 3000);
    let result = crate::verify_temporal_ordering(&charter, &claim, &publish);
    assert!(
        matches!(result, Err(crate::VerifyError::TemporalOrderViolation)),
        "claim.now >= publish.now must be rejected: {result:?}"
    );
}

#[test]
fn verify_temporal_ordering_rejects_both_violations() {
    let (charter, claim, publish) = temporal_triple(3000, 2000, 1000);
    let result = crate::verify_temporal_ordering(&charter, &claim, &publish);
    assert!(
        matches!(result, Err(crate::VerifyError::TemporalOrderViolation)),
        "a fully reversed triple must be rejected: {result:?}"
    );
}

#[test]
fn verify_claim_authorized_by_charter_accepts_matching_owner() {
    let charter_owner = vec![7];
    let charter = crate::CharterPayload::new(
        crate::Alg::ES256,
        1000,
        charter_owner.clone(),
        None,
        vec![0; 32],
        test_tmb(),
    );
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        crate::Thumbprint::from_bytes(charter_owner),
    );

    let result = crate::verify_claim_authorized_by_charter(&claim, &charter);
    assert!(
        result.is_ok(),
        "claim signed by the effective charter's owner must be authorized: {result:?}"
    );
}

#[test]
fn verify_claim_authorized_by_charter_rejects_stranger() {
    let charter = crate::CharterPayload::new(
        crate::Alg::ES256,
        1000,
        vec![7],
        None,
        vec![0; 32],
        test_tmb(),
    );
    let claim = crate::ClaimPayload::new(
        crate::Alg::ES256,
        test_id(),
        2000,
        vec![99],
        "cargo".to_string(),
        vec![0; 32],
        crate::Thumbprint::from_bytes(vec![0xAA]), // does not match charter.owner
    );

    let result = crate::verify_claim_authorized_by_charter(&claim, &charter);
    assert!(
        matches!(result, Err(crate::VerifyError::Unauthorized)),
        "claim signed by a non-owner key must be rejected: {result:?}"
    );
}

#[cfg(test)]
mod proptests {
    use coz_rs::SigningKey;
    use proptest::prelude::*;

    use super::*;
    use crate::{Alg, Anchor, AtomId, ClaimPayload, Czd, PublishPayload, RawVersion};

    fn arb_label() -> impl Strategy<Value = String> {
        let start = "[a-zA-Z]";
        let cont = "[a-zA-Z0-9_-]*";
        (start, cont).prop_map(|(s, c)| format!("{}{}", s, c))
    }

    fn arb_bytes() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(any::<u8>(), 1..64)
    }

    fn arb_hash() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(any::<u8>(), 32)
    }

    proptest! {
        #[test]
        fn test_claim_payload_serde(
            lbl in arb_label(),
            anchor_bytes in arb_hash(),
            now in any::<u64>(),
            owner in arb_bytes(),
            pkg in "[a-z]{3,10}",
            src in arb_hash(),
        ) {
            let label = Label::try_from(lbl.as_str()).unwrap();
            let anchor = Anchor::new(anchor_bytes);
            let id = AtomId::new(anchor, label);
            let tmb = coz_rs::Thumbprint::from_bytes(vec![0; 32]);
            let original = ClaimPayload::new(Alg::Ed25519, id, now, owner, pkg, src, tmb);

            let serialized = serde_json::to_string(&original).unwrap();
            let deserialized: ClaimPayload = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(deserialized, original);
        }

        #[test]
        fn test_publish_payload_serde(
            lbl in arb_label(),
            anchor_bytes in arb_hash(),
            claim_bytes in arb_hash(),
            dig_bytes in arb_hash(),
            now in any::<u64>(),
            path in "[a-zA-Z0-9/_-]{1,50}",
            src in arb_hash(),
            version_str in "[0-9]\\.[0-9]\\.[0-9]",
        ) {
            let label = Label::try_from(lbl.as_str()).unwrap();
            let anchor = Anchor::new(anchor_bytes);
            let id = AtomId::new(anchor, label);
            let claim = Czd::from_bytes(claim_bytes);
            let tmb = coz_rs::Thumbprint::from_bytes(vec![0; 32]);
            let version = RawVersion::new(version_str);
            let original = PublishPayload::new(
                Alg::Ed25519, id, claim, dig_bytes, now, path, src, tmb, version);

            let serialized = serde_json::to_string(&original).unwrap();
            let deserialized: PublishPayload = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(deserialized, original);
        }

        #[test]
        fn test_verification_robustness(
            lbl in arb_label(),
            anchor_bytes in arb_hash(),
            now in any::<u64>(),
            owner in arb_bytes(),
            pkg in "[a-z]{3,10}",
            src in arb_hash(),
            mutate_index in 0..64usize,
            mutation in 1..255u8,
        ) {
            // Generate Ed25519 keypair
            let sk = SigningKey::<coz_rs::Ed25519>::generate();
            let prv = sk.private_key_bytes().to_vec();
            let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
            let tmb = coz_rs::compute_thumbprint_for_alg("Ed25519", &pub_bytes).unwrap();

            let label = Label::try_from(lbl.as_str()).unwrap();
            let anchor = Anchor::new(anchor_bytes);
            let id = AtomId::new(anchor, label);
            let claim = ClaimPayload::new(Alg::Ed25519, id, now, owner, pkg, src, tmb);
            let pay_json = serde_json::to_vec(&claim).unwrap();

            let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

            // Assert normal verification succeeds
            let result = crate::verify_claim(&pay_json, &sig, "Ed25519", &pub_bytes);
            prop_assert!(result.is_ok(), "Verification failed for valid claim: {:?}", result);

            // Mutate a byte in the signature
            let mut corrupted_sig = sig.clone();
            if mutate_index < corrupted_sig.len() {
                corrupted_sig[mutate_index] ^= mutation;
                let result = crate::verify_claim(&pay_json, &corrupted_sig, "Ed25519", &pub_bytes);
                prop_assert!(result.is_err(), "Verification should fail for corrupted signature");
            }

            // Mutate a byte in the verifying key
            let mut corrupted_pub = pub_bytes.clone();
            if mutate_index < corrupted_pub.len() {
                corrupted_pub[mutate_index] ^= mutation;
                let result = crate::verify_claim(&pay_json, &sig, "Ed25519", &corrupted_pub);
                prop_assert!(result.is_err(), "Verification should fail for corrupted public key");
            }
        }
    }
}

#[test]
fn atom_id_parse_roundtrip() {
    bolero::check!()
        .with_type::<(Vec<u8>, String)>()
        .for_each(|(anchor_bytes, label_str)| {
            if let Ok(label) = Label::try_from(label_str.as_str()) {
                let anchor = Anchor::new(anchor_bytes.clone());
                let original = AtomId::new(anchor, label);
                let serialized = original.to_string();
                let parsed = serialized.parse::<AtomId>().unwrap();
                assert_eq!(parsed, original);
            }
        });
}

#[test]
fn atom_id_parse_no_panic() {
    bolero::check!().with_type::<String>().for_each(|s| {
        let _ = s.parse::<AtomId>();
    });
}

#[test]
fn name_validation_hierarchy() {
    bolero::check!().with_type::<String>().for_each(|s| {
        let is_ident = Identifier::try_from(s.as_str()).is_ok();
        let is_label = Label::try_from(s.as_str()).is_ok();
        let is_tag = Tag::try_from(s.as_str()).is_ok();

        // Identifier ⊂ Label ⊂ Tag
        if is_ident {
            assert!(is_label, "Identifier parsed but Label did not: '{}'", s);
        }
        if is_label {
            assert!(is_tag, "Label parsed but Tag did not: '{}'", s);
        }
    });
}

#[test]
fn label_nfkc_idempotency() {
    bolero::check!().with_type::<String>().for_each(|s| {
        if let Ok(label) = Label::try_from(s.as_str()) {
            // NFKC normalization idempotency
            use unicode_normalization::UnicodeNormalization;
            let normalized: String = s.nfkc().collect();
            let label_norm = Label::try_from(normalized.as_str()).unwrap();
            assert_eq!(label, label_norm);
        }
    });
}

#[test]
fn label_parse_properties() {
    bolero::check!().with_type::<String>().for_each(|s| {
        if let Ok(label) = Label::try_from(s.as_str()) {
            assert!(label.to_string().parse::<Label>().is_ok());
            assert!(label.len() <= 128);
        }
    });
}

#[test]
fn tag_consecutive_dots_rejection() {
    bolero::check!().with_type::<String>().for_each(|s| {
        if s.contains("..") {
            assert!(Tag::try_from(s.as_str()).is_err());
        }
    });
}

#[derive(bolero::TypeGenerator, Debug)]
struct FuzzVerifyInput {
    is_claim: bool,
    alg: u8,
    payload: Vec<u8>,
    signature: Vec<u8>,
    public_key: Vec<u8>,
}

#[test]
fn test_verify_robustness_bolero() {
    bolero::check!()
        .with_type::<FuzzVerifyInput>()
        .for_each(|input| {
            let alg = match input.alg % 3 {
                0 => "Ed25519",
                1 => "ES256",
                _ => "UNSUPPORTED",
            };
            if input.is_claim {
                let _ =
                    crate::verify_claim(&input.payload, &input.signature, alg, &input.public_key);
            } else {
                let _ =
                    crate::verify_publish(&input.payload, &input.signature, alg, &input.public_key);
            }
        });
}
