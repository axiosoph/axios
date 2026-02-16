//! Tests for atom naming types.

use crate::name::VerifiedName;
use crate::{Error, Identifier, Label, Tag};

// ============================================================================
// Label
// ============================================================================

#[test]
fn valid_labels() {
    let valid = [
        "αβγ",
        "ΑΒΓ",
        "кириллица",
        "汉字",
        "ひらがな",
        "한글",
        "Ññ",
        "Öö",
        "Ææ",
        "Łł",
        "ئ",
        "א",
        "ก",
        "Ա",
        "ᚠ",
        "ᓀ",
        "あア",
        "한글漢字",
        "Café_au_lait-123",
    ];
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
    for s in ["9atom", "'atom", "_atom", "-atom", "%atom"] {
        assert_eq!(
            Label::try_from(s),
            Err(Error::InvalidStart(s.chars().next().unwrap())),
        );
    }
}

#[test]
fn label_rejects_invalid_chars() {
    assert_eq!(
        Label::try_from("a-!@#$%^&*()_-asdf"),
        Err(Error::InvalidCharacters("!@#$%^&*()".into())),
    );
}

#[test]
fn label_unicode_edge_cases() {
    // Single valid char
    assert_eq!(Label::try_from("α"), Ok(Label::validate("α").unwrap()));

    // Mix of Unicode, underscore, number
    assert_eq!(Label::try_from("ñ_1"), Ok(Label::validate("ñ_1").unwrap()));

    // Zero-width space: invalid start
    assert_eq!(
        Label::try_from("\u{200B}"),
        Err(Error::InvalidStart('\u{200B}')),
    );

    // Zero-width space: invalid in middle
    assert_eq!(
        Label::try_from("α\u{200B}"),
        Err(Error::InvalidCharacters("\u{200B}".into())),
    );
}

#[test]
fn label_invalid_unicode_comprehensive() {
    let invalid = [
        "123αβγ",
        "_ΑΒΓ",
        "-кириллица",
        "汉字!",
        "ひらがな。",
        "한글 ",
        "Ññ\u{200B}",
        "Öö\t",
        "Ææ\n",
        "Łł\r",
        "ئ،",
        "א״",
        "ก๏",
        "Ա։",
        "ᚠ᛫",
        "한글漢字♥",
        "Café_au_lait-123☕",
    ];
    for s in invalid {
        assert!(Label::try_from(s).is_err(), "expected '{s}' to be invalid");
    }
}

#[test]
fn label_specific_errors() {
    assert_eq!(Label::try_from("123αβγ"), Err(Error::InvalidStart('1')),);
    assert_eq!(
        Label::try_from("αβγ!@#"),
        Err(Error::InvalidCharacters("!@#".into())),
    );
    assert_eq!(
        Label::try_from("한글 漢字"),
        Err(Error::InvalidCharacters(" ".into())),
    );
    assert_eq!(
        Label::try_from("Café♥"),
        Err(Error::InvalidCharacters("♥".into())),
    );
}

// ============================================================================
// Identifier (strict UAX #31)
// ============================================================================

#[test]
fn identifier_rejects_hyphen() {
    assert!(
        Identifier::try_from("my-ident").is_err(),
        "hyphens are invalid in Identifier"
    );
}

#[test]
fn identifier_accepts_underscore_continue() {
    assert!(Identifier::try_from("my_ident").is_ok());
}

#[test]
fn identifier_rejects_empty() {
    assert_eq!(Identifier::try_from(""), Err(Error::Empty));
}

#[test]
fn identifier_rejects_number_start() {
    assert_eq!(Identifier::try_from("1foo"), Err(Error::InvalidStart('1')),);
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
    assert_eq!(Tag::try_from(".tag"), Err(Error::InvalidStart('.')),);
}
