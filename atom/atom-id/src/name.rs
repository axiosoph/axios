//! Validated naming types: [`Identifier`], [`Label`], and [`Tag`].
//!
//! These types enforce Unicode identifier rules (UAX #31) with atom-specific
//! extensions, forming a strict hierarchy: Identifier ⊂ Label ⊂ Tag.

use std::ffi::OsStr;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::{Error, NAME_MAX};

// ============================================================================
// Types
// ============================================================================

/// A strict UAX #31 Unicode identifier (XID_Start + XID_Continue).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Identifier(String);

/// A validated atom label: UAX #31 plus hyphen (`-`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Label(String);

/// A convenience alias for [`Label`].
pub type Name = Label;

/// A metadata tag: labels plus separators (`:` and `.`).
///
/// Consecutive dots (`..`) are disallowed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Tag(String);

// ============================================================================
// VerifiedName trait (sealed)
// ============================================================================

/// Rules for validating and constructing a name type.
///
/// Default implementations encode UAX #31; subtypes extend per their rules.
pub(crate) trait VerifiedName: Sized + sealed::Construct {
    /// Check whether `c` is a valid starting character.
    fn is_valid_start(c: char) -> bool {
        unicode_ident::is_xid_start(c)
    }

    /// Check whether `c` is valid in a continuation position.
    fn is_valid_char(c: char) -> bool {
        unicode_ident::is_xid_continue(c)
    }

    /// Hook for subtype-specific rules (default: no-op).
    fn extra_validation(_s: &str) -> Result<(), Error> {
        Ok(())
    }

    /// NFKC-normalize and validate a string, returning the constructed type.
    fn validate(s: &str) -> Result<Self, Error> {
        let normalized: String = s.nfkc().collect();

        if normalized.len() > NAME_MAX {
            return Err(Error::TooLong);
        }

        match normalized.chars().next() {
            Some(c) if Self::is_valid_start(c) => {},
            Some(c) => return Err(Error::InvalidStart(c)),
            None => return Err(Error::Empty),
        }

        let invalid: String = normalized
            .chars()
            .filter(|&c| !Self::is_valid_char(c))
            .collect();

        if !invalid.is_empty() {
            return Err(Error::InvalidCharacters(invalid));
        }

        Self::extra_validation(&normalized)?;

        Ok(sealed::Construct::new(normalized))
    }
}

mod sealed {
    /// Private constructor — prevents external implementations of [`VerifiedName`].
    pub trait Construct {
        fn new(s: String) -> Self;
    }
}

// ============================================================================
// Shared impls (macro)
// ============================================================================

/// Generate the common conversion impls for a [`VerifiedName`] newtype.
macro_rules! verified_name_impls {
    ($Type:ident) => {
        impl sealed::Construct for $Type {
            fn new(s: String) -> Self {
                Self(s)
            }
        }

        impl Deref for $Type {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $Type {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $Type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $Type {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::validate(s)
            }
        }

        impl TryFrom<&str> for $Type {
            type Error = Error;

            fn try_from(s: &str) -> Result<Self, Self::Error> {
                Self::validate(s)
            }
        }

        impl TryFrom<String> for $Type {
            type Error = Error;

            fn try_from(s: String) -> Result<Self, Self::Error> {
                Self::validate(&s)
            }
        }

        impl TryFrom<&OsStr> for $Type {
            type Error = Error;

            fn try_from(s: &OsStr) -> Result<Self, Self::Error> {
                let s = s.to_str().ok_or(Error::InvalidUnicode)?;
                Self::validate(s)
            }
        }
    };
}

// ============================================================================
// Per-type validation rules + shared impls
// ============================================================================

impl VerifiedName for Identifier {}
verified_name_impls!(Identifier);

impl VerifiedName for Label {
    fn is_valid_char(c: char) -> bool {
        unicode_ident::is_xid_continue(c) || c == '-'
    }
}
verified_name_impls!(Label);

impl VerifiedName for Tag {
    fn is_valid_char(c: char) -> bool {
        unicode_ident::is_xid_continue(c) || c == '-' || c == '.' || c == ':'
    }

    fn extra_validation(s: &str) -> Result<(), Error> {
        if s.contains("..") {
            return Err(Error::InvalidCharacters("..".into()));
        }
        Ok(())
    }
}
verified_name_impls!(Tag);
