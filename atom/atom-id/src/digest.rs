//! Tagged, per-algorithm-encoded digest for the lock/store layer.
//!
//! [`AtomDigest`] is the single digest representation shared by the
//! store-level index (a hash of an [`AtomId`]) and every digest-shaped lock
//! field — coz transaction digests, git object ids, and content hashes alike.
//! It is keyed on the *hash* algorithm (not the signing scheme): the human
//! form is `<token>:<encoding>`, where the encoding is a fixed property of the
//! hash algorithm — `base64url`-unpadded for the SHA-2 family (matching coz's
//! own convention), lowercase hex for `sha1` (git object ids) and `blake3`
//! (content digests). One canonical encoding per token keeps a lock
//! byte-deterministic (`[lock-canonical-form]`).

use std::fmt;
use std::str::FromStr;

use coz_rs::base64ct::{Base64UrlUnpadded, Encoding as _};
use coz_rs::{Cad, Czd};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::AtomId;

/// A hash algorithm that can label an [`AtomDigest`].
///
/// A superset of coz's signing-hash algorithms (`sha256`/`sha384`/`sha512`):
/// it adds `sha1` for git object ids and `blake3` for content digests, which
/// have no coz signing structure but appear in the lock. Convert the coz
/// subset with [`From<coz_rs::HashAlg>`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlg {
    /// SHA-256 — 32-byte digest, base64url-unpadded encoding.
    Sha256,
    /// SHA-384 — 48-byte digest, base64url-unpadded encoding.
    Sha384,
    /// SHA-512 — 64-byte digest, base64url-unpadded encoding.
    Sha512,
    /// SHA-1 — 20-byte digest, lowercase-hex encoding (git object ids).
    Sha1,
    /// BLAKE3 — 32-byte digest, lowercase-hex encoding (content digests).
    Blake3,
}

/// The conventional encoding a [`HashAlg`] renders its bytes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    /// Base64url, unpadded — the coz digest convention.
    B64Ut,
    /// Lowercase hexadecimal — the git / b3sum convention.
    Hex,
}

impl HashAlg {
    /// The lowercase token that names this algorithm in the `<token>:<enc>` form.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
            Self::Sha1 => "sha1",
            Self::Blake3 => "blake3",
        }
    }

    /// Parse a lowercase token into its [`HashAlg`], or `None` if unknown.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "sha256" => Some(Self::Sha256),
            "sha384" => Some(Self::Sha384),
            "sha512" => Some(Self::Sha512),
            "sha1" => Some(Self::Sha1),
            "blake3" => Some(Self::Blake3),
            _ => None,
        }
    }

    /// The digest size in bytes — the exact length a payload must decode to.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
            Self::Sha1 => 20,
            Self::Blake3 => 32,
        }
    }

    /// The fixed encoding this algorithm's bytes are rendered with.
    const fn encoding(self) -> Encoding {
        match self {
            Self::Sha256 | Self::Sha384 | Self::Sha512 => Encoding::B64Ut,
            Self::Sha1 | Self::Blake3 => Encoding::Hex,
        }
    }
}

/// The coz signing-hash algorithms map onto their [`HashAlg`] counterparts.
impl From<coz_rs::HashAlg> for HashAlg {
    fn from(alg: coz_rs::HashAlg) -> Self {
        match alg {
            coz_rs::HashAlg::Sha256 => Self::Sha256,
            coz_rs::HashAlg::Sha384 => Self::Sha384,
            coz_rs::HashAlg::Sha512 => Self::Sha512,
        }
    }
}

impl Encoding {
    fn encode(self, bytes: &[u8]) -> String {
        match self {
            Self::B64Ut => Base64UrlUnpadded::encode_string(bytes),
            Self::Hex => hex::encode(bytes),
        }
    }

    fn decode(self, s: &str) -> Result<Vec<u8>, DigestParseError> {
        match self {
            Self::B64Ut => Base64UrlUnpadded::decode_vec(s).map_err(|_| DigestParseError::Encoding),
            Self::Hex => {
                // Enforce lowercase-hex canonicity: `hex::decode` accepts mixed
                // case, but only lowercase is a valid canonical form here.
                if !s
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(DigestParseError::Encoding);
                }
                hex::decode(s).map_err(|_| DigestParseError::Encoding)
            },
        }
    }
}

/// A tagged, per-algorithm-encoded digest of an atom.
///
/// The one digest representation for the store index and every lock digest
/// field. Its human form is `<token>:<encoding>` — e.g. `sha256:<b64url>`,
/// `sha1:<hex>`, `blake3:<hex>` — keyed on the hash algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "digests should not be discarded"]
pub struct AtomDigest {
    alg: HashAlg,
    cad: Cad,
}

impl AtomDigest {
    /// Compute the store-index digest of an [`AtomId`] under a coz hash algorithm.
    ///
    /// Canonicalizes the identity as `{"anchor":"<b64ut>","label":"<str>"}`
    /// (field order `["anchor", "label"]`) and hashes it. The parameter is
    /// `coz_rs::HashAlg` because the store index is a coz-family digest;
    /// obtain it from a signing algorithm via [`coz_rs::Alg::hash_alg`].
    pub fn compute(id: &AtomId, alg: coz_rs::HashAlg) -> Self {
        // Anchor.to_b64() (base64url) and Label (UAX #31) are JSON-safe, so
        // this format! produces valid JSON with no escaping concerns.
        let json = format!(
            r#"{{"anchor":"{}","label":"{}"}}"#,
            id.anchor().to_b64(),
            id.label(),
        );
        // The constructed JSON is always well-formed, so canonicalization of a
        // valid AtomId cannot fail.
        let canonical = coz_rs::canonical(json.as_bytes(), Some(&["anchor", "label"]))
            .expect("AtomId canonical JSON is always well-formed");
        Self {
            alg: alg.into(),
            cad: Cad::from_bytes(alg.hash_bytes(&canonical)),
        }
    }

    /// The hash algorithm labelling this digest.
    #[must_use]
    pub fn alg(&self) -> HashAlg {
        self.alg
    }

    /// The raw digest bytes.
    pub fn cad(&self) -> &Cad {
        &self.cad
    }
}

/// A [`Czd`] is a sha256 coz digest, so it is a `sha256` [`AtomDigest`]; the
/// bytes coincide.
impl From<Czd> for AtomDigest {
    fn from(czd: Czd) -> Self {
        Self {
            alg: HashAlg::Sha256,
            cad: Cad::from_bytes(czd.as_bytes().to_vec()),
        }
    }
}

impl fmt::Display for AtomDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            self.alg.token(),
            self.alg.encoding().encode(self.cad.as_bytes()),
        )
    }
}

impl FromStr for AtomDigest {
    type Err = DigestParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (token, payload) = s
            .split_once(':')
            .ok_or(DigestParseError::MissingSeparator)?;
        let alg = HashAlg::from_token(token)
            .ok_or_else(|| DigestParseError::UnknownToken(token.into()))?;
        let bytes = alg.encoding().decode(payload)?;
        if bytes.len() != alg.digest_len() {
            return Err(DigestParseError::Length {
                alg,
                expected: alg.digest_len(),
                got: bytes.len(),
            });
        }
        Ok(Self {
            alg,
            cad: Cad::from_bytes(bytes),
        })
    }
}

/// Errors produced parsing an [`AtomDigest`] from its `<token>:<enc>` form.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DigestParseError {
    /// No `:` separating token from encoded payload.
    #[error("missing ':' separator in digest")]
    MissingSeparator,
    /// The token before `:` names no known hash algorithm.
    #[error("unknown hash token: '{0}'")]
    UnknownToken(String),
    /// The payload is not valid for its token's encoding (charset/canonicity).
    #[error("malformed digest encoding")]
    Encoding,
    /// The decoded payload is the wrong length for its algorithm.
    #[error("wrong digest length for {alg:?}: expected {expected} bytes, got {got}")]
    Length {
        /// The algorithm whose length was violated.
        alg: HashAlg,
        /// The digest length the algorithm requires.
        expected: usize,
        /// The length actually decoded.
        got: usize,
    },
}

#[cfg(feature = "serde")]
impl Serialize for AtomDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for AtomDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [HashAlg; 5] = [
        HashAlg::Sha256,
        HashAlg::Sha384,
        HashAlg::Sha512,
        HashAlg::Sha1,
        HashAlg::Blake3,
    ];

    fn digest(alg: HashAlg, seed: u8) -> AtomDigest {
        AtomDigest {
            alg,
            cad: Cad::from_bytes(vec![seed; alg.digest_len()]),
        }
    }

    // c-digest-roundtrip: encode -> parse -> equal, every HashAlg.
    #[test]
    fn round_trips_every_alg() {
        for alg in ALL {
            let d = digest(alg, 0xAB);
            let parsed: AtomDigest = d.to_string().parse().expect("round-trip parse");
            assert_eq!(d, parsed, "{alg:?} must round-trip");
        }
    }

    // Display shape + encoding is per-token (b64ut vs lowercase hex).
    #[test]
    fn display_encoding_is_per_token() {
        // sha1 -> lowercase hex, 40 chars for 20 bytes.
        let s = digest(HashAlg::Sha1, 0xde).to_string();
        assert_eq!(s, format!("sha1:{}", "de".repeat(20)));
        // blake3 -> lowercase hex, 64 chars for 32 bytes.
        let b = digest(HashAlg::Blake3, 0x0f).to_string();
        assert_eq!(b, format!("blake3:{}", "0f".repeat(32)));
        // sha256 -> base64url-unpadded (no '+' '/' '=' and no ':').
        let h = digest(HashAlg::Sha256, 0xff).to_string();
        assert!(h.starts_with("sha256:"));
        let enc = h.strip_prefix("sha256:").unwrap();
        assert!(
            !enc.contains(['+', '/', '=', ':']),
            "b64url alphabet only: {enc}"
        );
    }

    #[test]
    fn parse_rejects_missing_separator() {
        assert_eq!(
            "sha256deadbeef".parse::<AtomDigest>(),
            Err(DigestParseError::MissingSeparator),
        );
    }

    #[test]
    fn parse_rejects_unknown_token() {
        assert_eq!(
            "md5:abcd".parse::<AtomDigest>(),
            Err(DigestParseError::UnknownToken("md5".into())),
        );
    }

    #[test]
    fn parse_rejects_wrong_length() {
        // Valid b64url of 3 bytes, but sha256 wants 32.
        match "sha256:YWJj".parse::<AtomDigest>() {
            Err(DigestParseError::Length {
                expected: 32,
                got: 3,
                ..
            }) => {},
            other => panic!("expected Length error, got {other:?}"),
        }
        // 4 hex chars = 2 bytes, but sha1 wants 20.
        match "sha1:dead".parse::<AtomDigest>() {
            Err(DigestParseError::Length {
                expected: 20,
                got: 2,
                ..
            }) => {},
            other => panic!("expected Length error, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_wrong_charset() {
        // Uppercase hex is non-canonical for a hex token.
        assert_eq!(
            format!("sha1:{}", "DE".repeat(20)).parse::<AtomDigest>(),
            Err(DigestParseError::Encoding),
        );
        // Standard-base64 chars ('+') are not in the url-safe alphabet.
        assert_eq!(
            "sha256:++++".parse::<AtomDigest>(),
            Err(DigestParseError::Encoding),
        );
        // Non-hex letter for a hex token.
        assert_eq!(
            format!("blake3:{}", "zz".repeat(32)).parse::<AtomDigest>(),
            Err(DigestParseError::Encoding),
        );
    }

    // compute: determinism + algorithm agility (different hash => different digest).
    #[test]
    fn compute_is_deterministic_and_agile() {
        use coz_rs::Alg;

        use crate::{Anchor, Label};
        let id = AtomId::new(
            Anchor::new(b"anchor-bytes".to_vec()),
            Label::try_from("test-pkg").unwrap(),
        );

        let a = AtomDigest::compute(&id, Alg::ES256.hash_alg());
        let b = AtomDigest::compute(&id, Alg::ES256.hash_alg());
        assert_eq!(a, b, "same id + alg is deterministic");
        assert_eq!(a.alg(), HashAlg::Sha256);
        assert!(a.to_string().starts_with("sha256:"), "honest hash token");

        let c = AtomDigest::compute(&id, Alg::ES384.hash_alg());
        assert_ne!(a, c, "different hash algorithm => different digest");
        assert_eq!(c.alg(), HashAlg::Sha384);

        // Ed25519 hashes with SHA-512 (grounded in coz-rs).
        let e = AtomDigest::compute(&id, Alg::Ed25519.hash_alg());
        assert_eq!(e.alg(), HashAlg::Sha512);
    }

    // A Czd converts to a byte-identical sha256 AtomDigest.
    #[test]
    fn from_czd_is_sha256_bytes() {
        let czd = Czd::from_bytes(vec![7u8; 32]);
        let d = AtomDigest::from(czd.clone());
        assert_eq!(d.alg(), HashAlg::Sha256);
        assert_eq!(d.cad().as_bytes(), czd.as_bytes());
        let parsed: AtomDigest = d.to_string().parse().expect("round-trip");
        assert_eq!(d, parsed);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_is_the_display_string() {
        let d = digest(HashAlg::Sha256, 0x42);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, format!("\"{d}\""));
        let back: AtomDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed() {
        assert!(serde_json::from_str::<AtomDigest>("\"sha256:++++\"").is_err());
        assert!(serde_json::from_str::<AtomDigest>("\"nope:abcd\"").is_err());
    }
}
