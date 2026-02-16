//! # Atom Identity
//!
//! Identity primitives for the Atom protocol: validated naming types and
//! cryptographic atom identification based on the [Coz] specification.
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
//! An [`AtomId`] is the unique, cryptographic identity of an atom — the Coz
//! digest ([`Czd`]) of a signed `atom/claim` transaction, paired with its
//! algorithm ([`Alg`]).
//!
//! Serialized as `alg.b64ut` (dot-delimited for git ref safety).
//!
//! ```text
//! Ed25519.xrYMu87EXes58PnEACcDW1t0jF2ez4FCN-njTF0MHNo
//! ```
//!
//! [Coz]: https://github.com/Cyphrme/Coz

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

mod name;

use std::fmt;
use std::str::FromStr;

pub use coz_rs::{Alg, Czd};
pub use name::{Identifier, Label, Name, Tag};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum byte length for validated name types.
pub const NAME_MAX: usize = 128;

// ============================================================================
// AtomId
// ============================================================================

/// The unique identity of an atom.
///
/// Pairs an algorithm ([`Alg`]) with the Coz digest ([`Czd`]) of the atom's
/// signed claim transaction. Together they form a self-describing,
/// content-addressed identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomId {
    alg: Alg,
    czd: Czd,
}

impl AtomId {
    /// Construct an `AtomId` from an algorithm and Coz digest.
    pub fn new(alg: Alg, czd: Czd) -> Self {
        Self { alg, czd }
    }

    /// The algorithm that produced this digest.
    pub fn alg(&self) -> Alg {
        self.alg
    }

    /// The raw Coz digest.
    pub fn czd(&self) -> &Czd {
        &self.czd
    }
}

/// Display as `alg.b64ut` (dot-delimited).
impl fmt::Display for AtomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.alg.name(), self.czd)
    }
}

/// Parse from `alg.b64ut` format (dot-delimited).
impl FromStr for AtomId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (alg_str, digest_str) = s.split_once('.').ok_or(Error::InvalidFormat)?;

        let alg = Alg::from_str(alg_str).ok_or(Error::UnknownAlgorithm)?;

        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        let bytes = Base64UrlUnpadded::decode_vec(digest_str).map_err(|_| Error::InvalidDigest)?;

        Ok(Self {
            alg,
            czd: Czd::from_bytes(bytes),
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
    /// Atom ID string does not match `alg.b64ut` format.
    #[error("invalid atom ID format, expected `alg.b64ut`")]
    InvalidFormat,
    /// Unrecognized algorithm in atom ID.
    #[error("unknown algorithm")]
    UnknownAlgorithm,
    /// Invalid base64url digest value.
    #[error("invalid digest encoding")]
    InvalidDigest,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
