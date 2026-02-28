//! # Atom Identity
//!
//! Identity primitives for the Atom protocol: validated naming types and
//! the protocol-level atom identity pair.
//!
//! ## Naming Hierarchy
//!
//! Three validated string types forming a strict hierarchy:
//!
//! > [`Identifier`] ⊂ [`Label`] ⊂ [`Tag`]
//!
//! - **Identifier**: UAX #31 compliant Unicode identifiers.
//! - **Label**: Identifiers plus hyphen (`-`).
//! - **Tag**: Labels plus separators (`:` and `.`).
//!
//! All input is NFKC-normalized and capped at 128 bytes.
//!
//! ## Atom Identity
//!
//! An [`AtomId`] is the unique identity of an atom — the pair of an
//! [`Anchor`] and a [`Label`]. Identity is determined solely by these
//! two values: two atoms with the same anchor and label ARE the same
//! atom, regardless of version, owner, or hash algorithm.
//!
//! The `AtomId` is algorithm-free and permanent. Compact hash
//! representations for store indexing (`AtomDigest`) are a separate
//! concern, computed downstream by stores and ingestors.
//!
//! ## Anchor
//!
//! An [`Anchor`] is an opaque byte vector establishing atom-set identity.
//! Its derivation is backend-specific (e.g., genesis commit hash for git).
//!
//! [Coz]: https://github.com/Cyphrme/Coz

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

mod name;

use std::fmt;
use std::str::FromStr;

pub use coz_rs::{Alg, Cad, Czd};
pub use name::{Identifier, Label, Name, Tag};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum byte length for validated name types.
pub const NAME_MAX: usize = 128;

// ============================================================================
// Anchor
// ============================================================================

/// An opaque anchor establishing atom-set identity.
///
/// The anchor pins an atom-set to an immutable reference point in the
/// source's history. Its derivation is backend-specific (e.g., the hash
/// of the genesis commit for git backends).
///
/// Displayed and serialized as a base64url-unpadded string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Anchor(Vec<u8>);

impl Anchor {
    /// Create an anchor from raw bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The raw anchor bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encode as a base64url-unpadded string.
    pub fn to_b64(&self) -> String {
        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        Base64UrlUnpadded::encode_string(&self.0)
    }
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_b64())
    }
}

impl AsRef<[u8]> for Anchor {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for Anchor {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl Serialize for Anchor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_b64())
    }
}

impl<'de> Deserialize<'de> for Anchor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        let bytes = Base64UrlUnpadded::decode_vec(&s).map_err(serde::de::Error::custom)?;
        Ok(Self(bytes))
    }
}

// ============================================================================
// AtomId
// ============================================================================

/// The unique protocol-level identity of an atom.
///
/// An `AtomId` is the pair of an [`Anchor`] and a [`Label`]. Identity is
/// determined solely by these two values — two atoms with the same anchor
/// and label ARE the same atom, regardless of version, owner, or hash
/// algorithm.
///
/// The `AtomId` is algorithm-free and permanent. Compact hash
/// representations for store indexing are a downstream concern.
///
/// Serialized as `<anchor_b64ut>::<label>` (double-colon delimited).
///
/// ```text
/// dGVzdA::my-package
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtomId {
    anchor: Anchor,
    label: Label,
}

impl AtomId {
    /// Construct an `AtomId` from an anchor and label.
    pub fn new(anchor: Anchor, label: Label) -> Self {
        Self { anchor, label }
    }

    /// The anchor establishing atom-set identity.
    pub fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    /// The atom's label within its atom-set.
    pub fn label(&self) -> &Label {
        &self.label
    }
}

/// Display as `<anchor_b64ut>::<label>` (double-colon delimited).
impl fmt::Display for AtomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.anchor.to_b64(), self.label)
    }
}

/// Parse from `<anchor_b64ut>::<label>` format.
impl FromStr for AtomId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (anchor_str, label_str) = s.split_once("::").ok_or(Error::InvalidFormat)?;

        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        let anchor_bytes =
            Base64UrlUnpadded::decode_vec(anchor_str).map_err(|_| Error::InvalidAnchor)?;

        let label = Label::try_from(label_str).map_err(|_| Error::InvalidLabel)?;

        Ok(Self {
            anchor: Anchor::new(anchor_bytes),
            label,
        })
    }
}

impl Serialize for AtomId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AtomId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ============================================================================
// Errors
// ============================================================================

/// Errors produced by atom identity operations.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    /// The name is empty.
    #[error("cannot be empty")]
    Empty,
    /// The name contains invalid characters.
    #[error("contains invalid characters: '{0}'")]
    InvalidCharacters(String),
    /// The name starts with an invalid character.
    #[error("cannot start with: '{0}'")]
    InvalidStart(char),
    /// The name contains invalid Unicode.
    #[error("must be valid unicode")]
    InvalidUnicode,
    /// The name exceeds the maximum allowed length.
    #[error("cannot be more than {} bytes", NAME_MAX)]
    TooLong,
    /// Atom ID string does not match `anchor_b64ut::label` format.
    #[error("invalid atom ID format, expected `anchor_b64ut::label`")]
    InvalidFormat,
    /// Invalid base64url anchor encoding.
    #[error("invalid anchor encoding")]
    InvalidAnchor,
    /// Invalid label in atom ID.
    #[error("invalid label in atom ID")]
    InvalidLabel,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
