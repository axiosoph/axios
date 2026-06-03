use std::path::Path;
use std::sync::Arc;

use eos_core::digest::Blake3Digest;
use eos_core::eval::ResolvedInput;
use eos_core::ingest::ContentIngestService;
use eos_core::store::StorePath;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::PathInfoService;

/// Ingestion service implementation for the Snix build engine.
pub struct SnixIngestService {
    pub blob_service: Arc<dyn BlobService>,
    pub directory_service: Arc<dyn DirectoryService>,
    pub path_info_service: Arc<dyn PathInfoService>,
    pub nar_calculation_service: Arc<dyn NarCalculationService>,
}

#[derive(Debug, thiserror::Error)]
pub enum SnixIngestError {
    #[error("Failed to import path: {0}")]
    Import(String),
    #[error("Ingested root node cannot be a symlink")]
    SymlinkRoot,
}

impl ContentIngestService for SnixIngestService {
    type Digest = Blake3Digest;
    type Error = SnixIngestError;

    async fn ingest_path(
        &self,
        path: &Path,
        name: &str,
    ) -> Result<ResolvedInput<Blake3Digest>, Self::Error> {
        let path_info = snix_store::import::import_path_as_nar_ca(
            path,
            name,
            self.blob_service.clone(),
            self.directory_service.clone(),
            &self.path_info_service,
            &*self.nar_calculation_service,
        )
        .await
        .map_err(|e| SnixIngestError::Import(e.to_string()))?;

        let digest = match &path_info.node {
            snix_castore::Node::File { digest, .. } => *digest,
            snix_castore::Node::Directory { digest, .. } => *digest,
            snix_castore::Node::Symlink { .. } => {
                return Err(SnixIngestError::SymlinkRoot);
            },
        };

        Ok(ResolvedInput {
            digest: Blake3Digest(digest.into()),
            store_path: StorePath(path_info.store_path.to_string()),
        })
    }
}
