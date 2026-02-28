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
