//! Content digest abstraction.
//!
//! Provides the generic [`Digest`] trait and the concrete [`Blake3Digest`]
//! implementation.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Error returned when parsing a [`Blake3Digest`] from a string.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseBlake3DigestError {
    /// The string length was not 64 characters (32 hex bytes).
    #[error("invalid Blake3 digest length: expected 64 hex characters, got {0}")]
    InvalidLength(usize),
    /// The string contained invalid hex characters.
    #[error("invalid hex character in Blake3 digest")]
    InvalidHex,
}

/// A generic content-addressed digest trait.
///
/// Abstracts the stored value comparisons and serialization from the hashing computation.
pub trait Digest:
    AsRef<[u8]> + Eq + std::hash::Hash + Clone + Send + Sync + fmt::Debug + fmt::Display + 'static
{
    /// Returns the algorithm identifier (e.g., "blake3").
    fn algorithm(&self) -> &str;

    /// Returns the raw digest bytes without framing.
    fn as_bytes(&self) -> &[u8];

    /// Returns the byte length of the digest.
    fn len(&self) -> usize;

    /// Returns true if the digest has a length of 0.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A 32-byte BLAKE3 content digest.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Blake3Digest(pub [u8; 32]);

impl Digest for Blake3Digest {
    #[inline]
    fn algorithm(&self) -> &str {
        "blake3"
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    fn len(&self) -> usize {
        32
    }
}

impl AsRef<[u8]> for Blake3Digest {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Blake3Digest {
    #[inline]
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for Blake3Digest {
    type Error = ParseBlake3DigestError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != 32 {
            return Err(ParseBlake3DigestError::InvalidLength(bytes.len() * 2));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }
}

impl fmt::Display for Blake3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Blake3Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blake3Digest({})", self)
    }
}

impl FromStr for Blake3Digest {
    type Err = ParseBlake3DigestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(ParseBlake3DigestError::InvalidLength(s.len()));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|_| ParseBlake3DigestError::InvalidHex)?;
        }
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_blake3_digest_properties() {
        fn assert_copy<T: Copy>() {}
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_copy::<Blake3Digest>();
        assert_send::<Blake3Digest>();
        assert_sync::<Blake3Digest>();
    }

    #[test]
    fn digest_round_trip_preserves_bytes() {
        let bytes = [42u8; 32];
        let digest = Blake3Digest::from(bytes);
        assert_eq!(digest.as_bytes(), &bytes);
        assert_eq!(digest.as_ref(), &bytes);

        let parsed = Blake3Digest::try_from(&bytes[..]).unwrap();
        assert_eq!(parsed, digest);
    }

    #[test]
    fn digest_string_parsing() {
        let s = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";
        let digest: Blake3Digest = s.parse().unwrap();
        assert_eq!(digest.0, [42u8; 32]);
        assert_eq!(digest.to_string(), s);

        let err_len = "2a";
        assert!(err_len.parse::<Blake3Digest>().is_err());

        let err_hex = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2axz";
        assert!(err_hex.parse::<Blake3Digest>().is_err());
    }
}
