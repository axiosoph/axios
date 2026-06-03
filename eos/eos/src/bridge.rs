//! Generic-to-castore content bridge.
//!
//! Implements [`AtomContentBridge`] using the generic [`AtomContent`] trait,
//! transferring content directly into the snix castore without intermediate
//! filesystem materialization.

use std::sync::Arc;

use atom_core::{AtomContent, ContentEntry};
use atom_id::AtomId;
use eos_core::bridge::AtomContentBridge;
use eos_core::digest::Blake3Digest;
use eos_core::eval::ResolvedInput;
use eos_core::store::StorePath;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_castore::import::{IngestionEntry, ingest_entries};
use snix_castore::{B3Digest, PathBuf as CastorePathBuf};
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};
use tokio::io::AsyncWriteExt;

/// Errors arising from the castore bridge.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// A path could not be parsed as a castore path.
    #[error("invalid castore path '{path}'")]
    InvalidCastorePath {
        path: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Failed to upload a blob to the castore.
    #[error("failed to upload blob '{path}' to castore")]
    BlobUpload {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Failed to ingest the directory tree into the castore.
    #[error("failed to ingest directory tree")]
    DirectoryIngestion(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Failed to calculate the NAR representation.
    #[error("failed to calculate NAR hash")]
    NarCalculation(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// The store path name derived from the label was invalid.
    #[error("invalid store path name '{name}'")]
    InvalidStoreName { name: String },

    /// Failed to register the path info in the store.
    #[error("failed to register path info for '{label}'")]
    PathInfoRegistration {
        label: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Content was not found in the source.
    #[error("content not found for atom {id} with digest {dig}")]
    ContentNotFound { id: String, dig: String },

    /// Source error while fetching content.
    #[error("source error: {0}")]
    Source(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Generic implementation of [`AtomContentBridge`].
///
/// Constructed at the scheduler (wiring site) where the concrete `AtomContent` implementation
/// is known. Holds cloned service handles so it can be moved into async tasks.
pub struct CastoreBridge<C: AtomContent> {
    /// Generic atom content source.
    source: C,
    /// Castore blob storage.
    blob_service: Arc<dyn BlobService>,
    /// Castore directory storage.
    directory_service: Arc<dyn DirectoryService>,
    /// Nix path info metadata service.
    path_info_service: Arc<dyn PathInfoService>,
    /// NAR hash calculation service.
    nar_calculation_service: Arc<dyn NarCalculationService>,
}

impl<C: AtomContent> CastoreBridge<C> {
    /// Creates a new bridge from an atom content source and snix service handles.
    pub fn new(
        source: C,
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
        nar_calculation_service: Arc<dyn NarCalculationService>,
    ) -> Self {
        Self {
            source,
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
        }
    }

    /// Uploads blob data to the castore [`BlobService`] and returns its BLAKE3 digest.
    async fn upload_blob(&self, path: &str, data: &[u8]) -> Result<B3Digest, BridgeError> {
        let mut writer = self.blob_service.open_write().await;
        writer
            .write_all(data)
            .await
            .map_err(|e| BridgeError::BlobUpload {
                path: path.to_owned(),
                source: e,
            })?;
        writer.close().await.map_err(|e| BridgeError::BlobUpload {
            path: path.to_owned(),
            source: e,
        })
    }
}

impl<C: AtomContent> AtomContentBridge for CastoreBridge<C> {
    type Digest = Blake3Digest;
    type Error = BridgeError;

    async fn ingest_atom(
        &self,
        id: &AtomId,
        label: &str,
        dig: &[u8],
    ) -> Result<ResolvedInput<Blake3Digest>, BridgeError> {
        let content_entries = self
            .source
            .content(id, dig)
            .await
            .map_err(|e| BridgeError::Source(Box::new(e)))?
            .ok_or_else(|| BridgeError::ContentNotFound {
                id: id.to_string(),
                dig: hex::encode(dig),
            })?;

        let mut ingestion_entries = Vec::with_capacity(content_entries.len() + 1);

        for entry in content_entries {
            match entry {
                ContentEntry::Regular {
                    path,
                    data,
                    executable,
                } => {
                    let digest = self.upload_blob(&path, &data).await?;
                    ingestion_entries.push(IngestionEntry::Regular {
                        path: parse_castore_path(&format!("{label}/{path}"))?,
                        size: data.len() as u64,
                        executable,
                        digest,
                    });
                },
                ContentEntry::Symlink { path, target } => {
                    ingestion_entries.push(IngestionEntry::Symlink {
                        path: parse_castore_path(&format!("{label}/{path}"))?,
                        target,
                    });
                },
                ContentEntry::Directory { path } => {
                    ingestion_entries.push(IngestionEntry::Dir {
                        path: parse_castore_path(&format!("{label}/{path}"))?,
                    });
                },
            }
        }

        // The bridge prepends "label/" to all paths, so we need to add the root directory
        // entry (named just "label") at the very end of the stream.
        ingestion_entries.push(IngestionEntry::Dir {
            path: parse_castore_path(label)?,
        });

        // Ingest the entries into castore
        let root_node = ingest_entries(
            self.directory_service.clone(),
            futures::stream::iter(ingestion_entries.into_iter().map(Ok::<_, std::io::Error>)),
        )
        .await
        .map_err(|e| BridgeError::DirectoryIngestion(Box::new(e)))?;

        // Calculate NAR hash and register PathInfo
        let (nar_size, nar_sha256) = self
            .nar_calculation_service
            .calculate_nar(&root_node)
            .await
            .map_err(BridgeError::NarCalculation)?;

        let ca = nix_compat::nixhash::CAHash::Nar(nix_compat::nixhash::NixHash::Sha256(nar_sha256));

        let output_path: nix_compat::store_path::StorePath<String> =
            nix_compat::store_path::build_ca_path(label, &ca, std::iter::empty::<&str>(), false)
                .map_err(|_| BridgeError::InvalidStoreName {
                    name: label.to_owned(),
                })?;

        let path_info = self
            .path_info_service
            .as_ref()
            .put(PathInfo {
                store_path: output_path.to_owned(),
                node: root_node.clone(),
                references: vec![],
                nar_size,
                nar_sha256,
                signatures: vec![],
                deriver: None,
                ca: Some(ca),
            })
            .await
            .map_err(|e| BridgeError::PathInfoRegistration {
                label: label.to_owned(),
                source: e,
            })?;

        // Extract the root digest and return ResolvedInput
        let root_digest = match &root_node {
            snix_castore::Node::Directory { digest, .. }
            | snix_castore::Node::File { digest, .. } => *digest,
            snix_castore::Node::Symlink { .. } => {
                return Err(BridgeError::PathInfoRegistration {
                    label: label.to_owned(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "root node cannot be a symlink",
                    )),
                });
            },
        };

        Ok(ResolvedInput {
            digest: Blake3Digest(root_digest.into()),
            store_path: StorePath(path_info.store_path.to_string()),
        })
    }
}

/// Parses a string path into a castore [`PathBuf`](CastorePathBuf).
fn parse_castore_path(path: &str) -> Result<CastorePathBuf, BridgeError> {
    path.parse()
        .map_err(|e: std::io::Error| BridgeError::InvalidCastorePath {
            path: path.to_owned(),
            source: Box::new(e),
        })
}
