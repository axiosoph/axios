//! Content ingestion service trait.
//!
//! Defines [`ContentIngestService`], the interface for ingesting
//! filesystem content into the build engine's content-addressed
//! store. Used for non-atom dependencies (Nix, NixGit, NixTar,
//! NixSrc) that are fetched from URLs and verified by eos.

use std::path::Path;

use crate::digest::Digest;
use crate::eval::ResolvedInput;

/// Service for ingesting filesystem paths into the content store.
///
/// Constructed at the wiring site (scheduler) alongside the engine,
/// then threaded through the orchestrator as a generic parameter.
#[trait_variant::make(Send)]
pub trait ContentIngestService: Send + Sync {
    /// The digest algorithm used by this service's store.
    type Digest: Digest;

    /// Backend-specific error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Ingest a filesystem path into the content store.
    ///
    /// Imports the file or directory at `path`, registers it under
    /// `name` in the store, and returns the resolved input metadata.
    async fn ingest_path(
        &self,
        path: &Path,
        name: &str,
    ) -> Result<ResolvedInput<Self::Digest>, Self::Error>;
}
