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
    let claim = crate::ClaimPayload::new(crate::Alg::ES256, test_id(), 1000, vec![99], test_tmb());
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
    let claim = crate::ClaimPayload::new(crate::Alg::ES256, test_id(), 1000, vec![99], test_tmb());
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
    let claim = crate::ClaimPayload::new(crate::Alg::ES256, test_id(), 1000, vec![99], test_tmb());
    let json = serde_json::to_string(&claim).unwrap();
    let back: crate::ClaimPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back, claim);
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
    let claim = crate::ClaimPayload::new(crate::Alg::ES256, test_id(), 1000, vec![99], test_tmb());
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
    let claim = crate::ClaimPayload::new(crate::Alg::ES256, id.clone(), 1000, vec![99], test_tmb());
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
    let claim = crate::ClaimPayload::new(crate::Alg::Ed25519, test_id(), 1000, vec![99], tmb);
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
    let claim = crate::ClaimPayload::new(crate::Alg::Ed25519, test_id(), 1000, vec![99], tmb);
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
    let claim = crate::ClaimPayload::new(crate::Alg::Ed25519, test_id(), 1000, vec![99], tmb);
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
    let claim = crate::ClaimPayload::new(crate::Alg::Ed25519, test_id(), 1000, vec![99], tmb);
    let pay_json = serde_json::to_vec(&claim).unwrap();

    let result = crate::verify_claim(&pay_json, &[], "UNSUPPORTED", &pub_bytes);
    assert!(
        matches!(result, Err(crate::VerifyError::UnsupportedAlgorithm(_))),
        "unknown alg should be UnsupportedAlgorithm: {result:?}"
    );
}
