//! Tests for atom identity types.

use std::ffi::OsStr;
use std::str::FromStr;

use crate::{AtomId, Error, Identifier, Label, NAME_MAX, Tag};

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
// AtomId
// ============================================================================

#[test]
fn atom_id_all_algs_roundtrip() {
    let digest = [0xABu8; 32];
    for (alg, name) in [
        (crate::Alg::ES256, "ES256"),
        (crate::Alg::ES384, "ES384"),
        (crate::Alg::ES512, "ES512"),
        (crate::Alg::Ed25519, "Ed25519"),
    ] {
        let id = AtomId::new(alg, crate::Czd::from_bytes(digest.to_vec()));
        let s = id.to_string();
        assert!(
            s.starts_with(&format!("{name}.")),
            "expected {name}. prefix"
        );
        let parsed: AtomId = s.parse().unwrap();
        assert_eq!(parsed, id, "roundtrip failed for {name}");
    }
}

#[test]
fn atom_id_accessors() {
    let id = AtomId::new(crate::Alg::ES256, crate::Czd::from_bytes(vec![1, 2, 3]));
    assert_eq!(id.alg(), crate::Alg::ES256);
    assert_eq!(id.czd(), &crate::Czd::from_bytes(vec![1, 2, 3]));
}

#[test]
fn atom_id_empty_string() {
    assert_eq!(AtomId::from_str(""), Err(Error::InvalidFormat));
}

#[test]
fn atom_id_missing_delimiter() {
    assert_eq!(
        AtomId::from_str("ES256_U5XUZots"),
        Err(Error::InvalidFormat),
    );
}

#[test]
fn atom_id_unknown_alg() {
    assert_eq!(
        AtomId::from_str("FAKE.U5XUZots"),
        Err(Error::UnknownAlgorithm),
    );
}

#[test]
fn atom_id_bad_base64() {
    assert_eq!(
        AtomId::from_str("ES256.!!!invalid!!!"),
        Err(Error::InvalidDigest),
    );
}

#[test]
fn atom_id_serde_roundtrip() {
    let input = "ES384.U5XUZots-WmQYcQWmsO751Xk0yeVi9XUKWQ2mGz6Aqg";
    let id: AtomId = input.parse().unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{input}\""));
    let back: AtomId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}
