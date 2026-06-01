//! Artifact store abstraction.
//!
//! Provides the [`ArtifactStore`] trait for content-addressed artifact registration
//! and retrieval.

use std::fmt;
use std::pin::Pin;

use futures_core::Stream;

use crate::digest::Digest;
use crate::job::ArtifactInfo;

/// Pin-boxed stream alias.
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

/// Opaque representation of a path in the [`ArtifactStore`].
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorePath(pub String);

impl fmt::Display for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorePath({})", self.0)
    }
}

impl AsRef<str> for StorePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Trait representing a content-addressed storage backend for build artifacts.
///
/// Implementations delegate the actual blob, directory, and metadata management
/// to concrete storage services.
pub trait ArtifactStore: Send + Sync + 'static {
    /// The digest algorithm used by this store.
    type Digest: Digest;

    /// The structured error type returned by store operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Checks if an artifact with the given digest exists in the store.
    async fn has(&self, digest: &Self::Digest) -> Result<bool, Self::Error>;

    /// Retrieves metadata for an artifact in the store.
    async fn get_info(
        &self,
        digest: &Self::Digest,
    ) -> Result<Option<ArtifactInfo<Self::Digest>>, Self::Error>;

    /// Imports an artifact from a stream of bytes into the store.
    ///
    /// If `expected` is provided, verifies that the imported content's digest
    /// matches the expected digest.
    async fn import(
        &self,
        content: BoxStream<'static, std::io::Result<bytes::Bytes>>,
        expected: Option<&Self::Digest>,
    ) -> Result<ArtifactInfo<Self::Digest>, Self::Error>;

    /// Returns a stream of all artifacts registered in the store.
    fn list(&self) -> BoxStream<'static, Result<ArtifactInfo<Self::Digest>, Self::Error>>;
}
