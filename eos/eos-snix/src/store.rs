//! Artifact store implementation for the Snix backend.
//!
//! Provides the [`ArtifactStore`] implementation delegating to the underlying
//! Snix store services (`BlobService`, `DirectoryService`, `PathInfoService`).

use std::sync::Arc;

use bytes::Bytes;
use eos_core::digest::Blake3Digest;
use eos_core::job::ArtifactInfo;
use eos_core::store::{ArtifactStore, BoxStream};
use futures::stream::StreamExt;
use nix_compat::nixhash::CAHash;
use nix_compat::store_path::build_ca_path;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_store::nar::ingest_nar_and_hash;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};
use tokio_util::io::StreamReader;

use crate::convert::{b3_to_blake3, nix_to_store_path};
use crate::error::SnixError;

/// Snix-backed implementation of the [`ArtifactStore`] trait.
pub struct SnixStore {
    blob_service: Arc<dyn BlobService>,
    directory_service: Arc<dyn DirectoryService>,
    path_info_service: Arc<dyn PathInfoService>,
}

impl SnixStore {
    /// Creates a new `SnixStore`.
    #[must_use]
    pub fn new(
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
    ) -> Self {
        Self {
            blob_service,
            directory_service,
            path_info_service,
        }
    }
}

impl ArtifactStore for SnixStore {
    type Digest = Blake3Digest;
    type Error = SnixError;

    async fn has(&self, digest: &Self::Digest) -> Result<bool, Self::Error> {
        let mut key = [0u8; 20];
        key.copy_from_slice(&digest.0[..20]);
        self.path_info_service
            .has(key)
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "has",
                source: e,
            })
    }

    async fn get_info(
        &self,
        digest: &Self::Digest,
    ) -> Result<Option<ArtifactInfo<Self::Digest>>, Self::Error> {
        let mut key = [0u8; 20];
        key.copy_from_slice(&digest.0[..20]);
        let maybe_info = self
            .path_info_service
            .get(key)
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "get_info",
                source: e,
            })?;

        match maybe_info {
            None => Ok(None),
            Some(info) => {
                let node_digest = match &info.node {
                    snix_castore::Node::File { digest, .. } => Some(b3_to_blake3(*digest)),
                    snix_castore::Node::Directory { digest, .. } => {
                        Some(b3_to_blake3(*digest))
                    }
                    snix_castore::Node::Symlink { .. } => None,
                };

                let info_digest = node_digest.unwrap_or(*digest);

                let deriver = info.deriver.as_ref().map(|d| {
                    let mut deriver_bytes = [0u8; 32];
                    deriver_bytes[..20].copy_from_slice(d.digest());
                    Blake3Digest(deriver_bytes)
                });

                Ok(Some(ArtifactInfo {
                    digest: info_digest,
                    store_path: nix_to_store_path(info.store_path),
                    size: info.nar_size,
                    references: info.references.into_iter().map(nix_to_store_path).collect(),
                    deriver,
                }))
            }
        }
    }

    async fn import(
        &self,
        content: BoxStream<'static, std::io::Result<Bytes>>,
        expected: Option<&Self::Digest>,
    ) -> Result<ArtifactInfo<Self::Digest>, Self::Error> {
        let name = match expected {
            Some(dig) => dig.to_string(),
            None => "imported-artifact".to_string(),
        };

        // Convert BoxStream to AsyncRead via tokio_util::io::StreamReader
        let mut reader = StreamReader::new(content);

        let (root_node, nar_sha256, nar_size) = ingest_nar_and_hash(
            self.blob_service.clone(),
            self.directory_service.clone(),
            &mut reader,
            &None,
        )
        .await
        .map_err(|e| SnixError::StoreError {
            operation: "import_ingest",
            source: Box::new(e),
        })?;

        let ca_hash = CAHash::Nar(nix_compat::nixhash::NixHash::Sha256(nar_sha256));
        let store_path = build_ca_path(&name, &ca_hash, Vec::<String>::new(), false).map_err(
            |e| SnixError::ConversionError {
                from: "ca_hash",
                to: "StorePath",
                detail: e.to_string(),
            },
        )?;

        let path_info = PathInfo {
            store_path: store_path.clone(),
            node: root_node.clone(),
            references: vec![],
            nar_size,
            nar_sha256,
            signatures: vec![],
            deriver: None,
            ca: Some(ca_hash),
        };

        self.path_info_service
            .put(path_info)
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "import_persist",
                source: e,
            })?;

        let node_digest = match &root_node {
            snix_castore::Node::File { digest, .. } => Some(b3_to_blake3(*digest)),
            snix_castore::Node::Directory { digest, .. } => Some(b3_to_blake3(*digest)),
            snix_castore::Node::Symlink { .. } => None,
        };

        let info_digest = node_digest.or_else(|| expected.copied()).unwrap_or_else(|| {
            let mut h = [0u8; 32];
            h[..20].copy_from_slice(store_path.digest());
            Blake3Digest(h)
        });

        if expected.is_some_and(|&expected_dig| expected_dig != info_digest) {
            return Err(SnixError::ConversionError {
                from: "imported_digest",
                to: "expected_digest",
                detail: format!(
                    "digest mismatch: expected {}, got {}",
                    expected.unwrap(), info_digest
                ),
            });
        }

        Ok(ArtifactInfo {
            digest: info_digest,
            store_path: nix_to_store_path(store_path),
            size: nar_size,
            references: vec![],
            deriver: None,
        })
    }

    fn list(&self) -> BoxStream<'static, Result<ArtifactInfo<Self::Digest>, Self::Error>> {
        let stream = self.path_info_service.list();
        let mapped = stream.map(|res| match res {
            Err(e) => Err(SnixError::StoreError {
                operation: "list",
                source: e,
            }),
            Ok(info) => {
                let node_digest = match &info.node {
                    snix_castore::Node::File { digest, .. } => Some(b3_to_blake3(*digest)),
                    snix_castore::Node::Directory { digest, .. } => {
                        Some(b3_to_blake3(*digest))
                    }
                    snix_castore::Node::Symlink { .. } => None,
                };

                let info_digest = node_digest.unwrap_or_else(|| {
                    let mut h = [0u8; 32];
                    h[..20].copy_from_slice(info.store_path.digest());
                    Blake3Digest(h)
                });

                let deriver = info.deriver.as_ref().map(|d| {
                    let mut deriver_bytes = [0u8; 32];
                    deriver_bytes[..20].copy_from_slice(d.digest());
                    Blake3Digest(deriver_bytes)
                });

                Ok(ArtifactInfo {
                    digest: info_digest,
                    store_path: nix_to_store_path(info.store_path),
                    size: info.nar_size,
                    references: info.references.into_iter().map(nix_to_store_path).collect(),
                    deriver,
                })
            }
        });
        Box::pin(mapped)
    }
}
