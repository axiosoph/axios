//! # Atom Core
//!
//! Protocol trait surface for the Atom ecosystem.
//!
//! This crate defines the behavioral contracts that all Atom backends must
//! implement. The traits are derived from the formal layer model's L1
//! coalgebras (see `models/publishing-stack-layers.md`):
//!
//! | Trait             | Model § | Role                              |
//! |:------------------|:--------|:----------------------------------|
//! | [`AtomSource`]    | §2.1    | Read-only observation             |
//! | [`AtomRegistry`]  | §2.2    | Claiming and publishing (source)  |
//! | [`AtomStore`]     | §2.3    | Local accumulation (consumer)     |
//! | [`Manifest`]      | §1      | Minimal package metadata          |
//!
//! Two implementations of the same trait are interchangeable if their
//! observations agree pointwise (bisimulation equivalence from the model).
//!
//! ## `AtomDigest`
//!
//! [`AtomDigest`] is a compact, self-describing multihash of an [`AtomId`],
//! used for store-level indexing and git ref paths. Multiple valid digests
//! exist per identity — one per algorithm.
//!
//! ## Design principles
//!
//! - **Backend-agnostic**: trait signatures contain no git types, no concrete version types, no
//!   serialization framework types. Backend specifics are expressed exclusively through associated
//!   types.
//! - **Crypto-free**: all identity and verification logic lives in `atom-id`. This crate consumes
//!   `atom-id`'s types and re-exported coz-rs primitives.
//! - **Minimal**: no gix, no semver, no tokio. One dependency: `atom-id`.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use std::fmt;

use atom_id::canonical_hash_for_alg;
pub use atom_id::{Alg, Anchor, AtomId, Cad, Czd, Label, RawVersion, Thumbprint, VersionScheme};

// ============================================================================
// Types
// ============================================================================

/// Compact, self-describing multihash of an [`AtomId`].
///
/// Used for store-level indexing, git ref paths, and wire format.
/// The algorithm is chosen by the store or ingestor, **not** the protocol.
/// Multiple valid digests exist for the same `AtomId` — one per algorithm.
///
/// Display format: `alg_name.b64ut_cad` (e.g., `ES256.abc123`).
///
/// # Computation
///
/// The digest is the canonical hash of the JSON representation
/// `{"anchor":"<b64ut>","label":"<str>"}` with field ordering
/// `["anchor", "label"]`, computed via [`canonical_hash_for_alg`].
///
/// [`canonical_hash_for_alg`]: atom_id::canonical_hash_for_alg
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "digests should not be discarded"]
pub struct AtomDigest {
    alg: Alg,
    cad: Cad,
}

// ============================================================================
// Traits
// ============================================================================

/// Minimal package metadata.
///
/// Every package format defines its own manifest (e.g., `Cargo.toml`,
/// `package.json`, `ion.toml`). The atom protocol requires exactly
/// two properties — everything else is ecosystem-specific.
///
/// Spec constraint: `[manifest-minimal]`.
pub trait Manifest {
    /// The human-readable package name.
    fn label(&self) -> &Label;

    /// The unparsed version string.
    ///
    /// Implementors resolve this via [`VersionScheme`] at consumption time.
    fn version(&self) -> &RawVersion;
}

/// Read-only observation of an atom store or source.
///
/// The common interface shared by sources and stores (model §2.1).
/// Two implementations are interchangeable if `resolve` and `discover`
/// agree pointwise (bisimulation equivalence).
///
/// Observations are wrapped in `Result` to distinguish "not found"
/// (`Ok(None)`) from backend failure (`Err`).
pub trait AtomSource {
    /// Backend-defined observation type returned by [`resolve`](Self::resolve).
    type Entry;

    /// Backend-specific error type.
    type Error;

    /// Look up an atom by its identity.
    ///
    /// Returns `Ok(None)` if the atom is not present in this source.
    /// Returns `Err` on backend failure (network, disk, permission, etc.).
    fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error>;

    /// Search for atoms matching a query string.
    ///
    /// Returns atom identities, not full entries — use
    /// [`resolve`](Self::resolve) for observation data.
    fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error>;
}

/// Claiming and publishing interface (source-side).
///
/// Extends [`AtomSource`] with write operations. Lives at the canonical
/// source (model §2.2, spec §Source/Store).
///
/// Session ordering is enforced by data flow: [`claim`](Self::claim)
/// returns a [`Czd`] that [`publish`](Self::publish) requires as input.
pub trait AtomRegistry: AtomSource {
    /// Establish ownership of an atom identity.
    ///
    /// Returns the claim's [`Czd`] (coz digest), which must be passed
    /// to [`publish`](Self::publish) to authorize version publication.
    fn claim(&self, id: &AtomId, owner: &[u8]) -> Result<Czd, Self::Error>;

    /// Publish a version against an existing claim.
    ///
    /// # Arguments
    ///
    /// * `id` — the atom being published
    /// * `claim` — czd of the authorizing claim (from [`claim`](Self::claim))
    /// * `version` — unparsed version string
    /// * `dig` — content snapshot digest
    /// * `src` — source revision identifier
    /// * `path` — subtree path within the source tree
    #[allow(clippy::too_many_arguments)]
    fn publish(
        &self,
        id: &AtomId,
        claim: &Czd,
        version: &RawVersion,
        dig: &[u8],
        src: &[u8],
        path: &str,
    ) -> Result<(), Self::Error>;
}

/// Local accumulation interface (consumer-side).
///
/// Extends [`AtomSource`] with ingestion from remote sources (model §2.3,
/// spec §Source/Store).
///
/// **Accumulation guarantee** (spec `[ingest-preserves-identity]`):
/// after [`ingest`](Self::ingest), for every atom in the source,
/// [`resolve`](AtomSource::resolve) on this store MUST return at least
/// what the source's `resolve` returns. The store accumulates — it never
/// loses atoms through ingestion.
pub trait AtomStore: AtomSource {
    /// Import atoms from a source into this store.
    ///
    /// After completion, this store contains at least every atom
    /// that was in `source` (⊇ condition).
    fn ingest<S: AtomSource>(&self, source: &S) -> Result<(), Self::Error>;

    /// Check whether an atom is present in this store.
    fn contains(&self, id: &AtomId) -> bool;
}

// ============================================================================
// Implementations
// ============================================================================

impl AtomDigest {
    /// Compute the digest of an [`AtomId`] using the given algorithm.
    ///
    /// Constructs the canonical JSON `{"anchor":"<b64ut>","label":"<str>"}`
    /// with field ordering `["anchor", "label"]`, then hashes it.
    ///
    /// Returns `None` if the algorithm is unrecognized by the underlying
    /// hashing library (should not occur for valid [`Alg`] variants).
    #[must_use]
    pub fn compute(id: &AtomId, alg: Alg) -> Option<Self> {
        // Anchor.to_b64() and Label characters are JSON-safe (b64ut
        // and UAX #31 respectively), so format! produces valid JSON
        // without escaping concerns.
        let json = format!(
            r#"{{"anchor":"{}","label":"{}"}}"#,
            id.anchor().to_b64(),
            id.label(),
        );
        let cad = canonical_hash_for_alg(json.as_bytes(), alg.name(), Some(&["anchor", "label"]))?;
        Some(Self { alg, cad })
    }

    /// The algorithm used for this digest.
    #[must_use]
    pub fn alg(&self) -> Alg {
        self.alg
    }

    /// The canonical digest bytes.
    pub fn cad(&self) -> &Cad {
        &self.cad
    }
}

impl fmt::Display for AtomDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.alg.name(), self.cad.to_b64())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that different algorithms produce different digests for
    /// the same canonical input — the `digest-algorithm-agile` property.
    #[test]
    fn digest_algorithm_agile() {
        let json = r#"{"anchor":"dGVzdA","label":"test-pkg"}"#;

        let cad_256 = canonical_hash_for_alg(json.as_bytes(), "ES256", Some(&["anchor", "label"]))
            .expect("ES256 is a known algorithm");

        let cad_384 = canonical_hash_for_alg(json.as_bytes(), "ES384", Some(&["anchor", "label"]))
            .expect("ES384 is a known algorithm");

        assert_ne!(
            cad_256, cad_384,
            "different algorithms must produce different digests"
        );
    }

    /// Verify Display format: `alg_name.b64ut_cad`.
    #[test]
    fn digest_display_format() {
        let json = r#"{"anchor":"dGVzdA","label":"test-pkg"}"#;
        let cad = canonical_hash_for_alg(json.as_bytes(), "ES256", Some(&["anchor", "label"]))
            .expect("known algorithm");

        let digest = AtomDigest {
            alg: Alg::ES256,
            cad: cad.clone(),
        };

        let display = digest.to_string();
        assert!(
            display.starts_with("ES256."),
            "display must start with algorithm name"
        );
        assert_eq!(display, format!("ES256.{}", cad.to_b64()));
    }

    /// Same input + same algorithm = same digest (determinism).
    #[test]
    fn digest_deterministic() {
        let json = r#"{"anchor":"dGVzdA","label":"test-pkg"}"#;

        let cad_a = canonical_hash_for_alg(json.as_bytes(), "Ed25519", Some(&["anchor", "label"]))
            .expect("known algorithm");

        let cad_b = canonical_hash_for_alg(json.as_bytes(), "Ed25519", Some(&["anchor", "label"]))
            .expect("known algorithm");

        assert_eq!(cad_a, cad_b, "same input must produce same digest");
    }

    /// Accessors return the correct values.
    #[test]
    fn digest_accessors() {
        let json = r#"{"anchor":"dGVzdA","label":"test-pkg"}"#;
        let cad = canonical_hash_for_alg(json.as_bytes(), "ES256", Some(&["anchor", "label"]))
            .expect("known algorithm");

        let digest = AtomDigest {
            alg: Alg::ES256,
            cad: cad.clone(),
        };

        assert_eq!(digest.alg(), Alg::ES256);
        assert_eq!(digest.cad(), &cad);
    }
}
