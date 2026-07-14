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

pub use atom_id::{
    Alg, Anchor, AtomDigest, AtomId, Cad, Czd, HashAlg, Label, RawVersion, Thumbprint,
    VersionScheme,
};

// ============================================================================
// Traits
// ============================================================================

/// Minimal package metadata.
///
/// Every package format defines its own manifest (e.g., `Cargo.toml`,
/// `package.json`, `atom.toml`). The atom protocol requires exactly
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
/// Trait representing an observed entry in an atom source.
pub trait AtomEntry {
    /// Concrete version observation type.
    type Version: AtomVersion;

    /// Iterator over the versions of this entry.
    type VersionIter<'a>: Iterator<Item = &'a Self::Version> + 'a
    where
        Self: 'a;

    /// The unique identity of the atom.
    fn id(&self) -> &AtomId;

    /// Iterate over all resolved versions of the atom.
    fn versions(&self) -> Self::VersionIter<'_>;
}

/// Trait representing an observed version of an atom.
pub trait AtomVersion {
    /// The unparsed version string.
    fn version(&self) -> &RawVersion;

    /// Content snapshot digest.
    fn dig(&self) -> &[u8];

    /// Opaque Coz digest of the authorizing claim.
    fn czd(&self) -> Option<&Czd>;

    /// Raw claim Coz message envelope JSON string, if signed.
    fn claim_msg(&self) -> Option<&str>;

    /// Raw publish Coz message envelope JSON string, if signed.
    fn publish_msg(&self) -> Option<&str>;
}

/// Read-only observation of an atom store or source.
///
/// The common interface shared by sources and stores (model §2.1).
/// Two implementations are interchangeable if `resolve` and `discover`
/// agree pointwise (bisimulation equivalence).
///
/// Observations are wrapped in `Result` to distinguish "not found"
/// (`Ok(None)`) from backend failure (`Err`).
pub trait AtomSource: Send + Sync + 'static {
    /// Backend-defined observation type returned by [`resolve`](Self::resolve).
    type Entry: AtomEntry;

    /// Backend-specific error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Look up an atom by its identity.
    ///
    /// Returns `Ok(None)` if the atom is not present in this source.
    /// Returns `Err` on backend failure (network, disk, permission, etc.).
    fn resolve(
        &self,
        id: &AtomId,
    ) -> impl std::future::Future<Output = Result<Option<Self::Entry>, Self::Error>> + Send;

    /// Search for atoms matching a query string.
    ///
    /// Returns atom identities, not full entries — use
    /// [`resolve`](Self::resolve) for observation data.
    fn discover(
        &self,
        query: &str,
    ) -> impl std::future::Future<Output = Result<Vec<AtomId>, Self::Error>> + Send;
}

/// A single entry in an atom's content tree.
///
/// Represents one node in the abstract tree yielded by
/// [`AtomContent::content`]. Entries are ordered
/// children-before-parents (leaves-to-root) to satisfy
/// castore ingestion ordering requirements.
#[derive(Clone, Debug)]
pub enum ContentEntry {
    /// A regular file with content bytes.
    Regular {
        /// Relative path within the atom tree (e.g., "src/lib.rs").
        path: String,
        /// Raw file content.
        data: Vec<u8>,
        /// Whether the file is executable.
        executable: bool,
    },
    /// A symbolic link.
    Symlink {
        /// Relative path of the symlink.
        path: String,
        /// Target of the symlink.
        target: Vec<u8>,
    },
    /// A directory marker.
    Directory {
        /// Relative path of the directory.
        path: String,
    },
}

/// Content observation interface (model §2.1a).
///
/// Extends [`AtomSource`] with the ability to yield the content
/// tree for a specific atom version. This is the _content recovery_
/// functor — it recovers the tree data that `AtomSource` (the
/// forgetful functor) deliberately omits.
///
/// Implementations provide backend-specific tree extraction:
/// - Git backend: walks `gix` tree objects
/// - Future backends: extract from their native representation
///
/// Consumers (e.g., [`AtomStore::ingest`], eos bridge) use this
/// trait to transfer content across backend boundaries without
/// runtime downcasting.
pub trait AtomContent: AtomSource {
    /// Yield the content tree for a specific atom version.
    ///
    /// Returns the full tree as a `Vec<ContentEntry>` ordered
    /// children-before-parents (leaves-to-root).
    ///
    /// # Arguments
    ///
    /// * `id` — the atom's identity
    /// * `dig` — backend-specific content snapshot digest (e.g., 20-byte git tree OID)
    ///
    /// Returns `None` if the content is not found.
    fn content(
        &self,
        id: &AtomId,
        dig: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<Vec<ContentEntry>>, Self::Error>> + Send;
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

    /// Charter (found or succeed) an atom-set.
    ///
    /// `prior: None` founds a new atom-set: the returned [`Czd`] becomes
    /// the atom-set's [`Anchor`]. `prior: Some(czd)` signs a successor to
    /// the charter named by `czd`, transferring ownership without
    /// changing the anchor.
    ///
    /// # Arguments
    ///
    /// * `owner` — opaque identity digest of the new/incoming owner
    /// * `src` — source revision demarking the chartering point
    /// * `prior` — czd of the charter this one succeeds, or `None` to found a new atom-set
    fn charter(&self, owner: &[u8], src: &[u8], prior: Option<&Czd>) -> Result<Czd, Self::Error>;
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
pub trait AtomStore: AtomContent {
    /// Import atoms from a source into this store.
    ///
    /// After completion, this store contains at least every atom
    /// that was in `source` (⊇ condition).
    fn ingest<S: AtomContent>(
        &self,
        source: &S,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;

    /// Check whether an atom is present in this store.
    fn contains(
        &self,
        id: &AtomId,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
}
