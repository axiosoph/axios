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
//! [Coz]: https://github.com/Cyphrme/Coz

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

mod name;

pub use name::{Identifier, Label, Name, Tag};
use thiserror::Error;

/// Maximum byte length for validated name types.
pub const NAME_MAX: usize = 128;

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
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
