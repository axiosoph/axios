//! Git-to-castore content bridge.
//!
//! Implements [`AtomContentBridge`] for git-backed atoms, transferring content
//! from a git repository's tree objects directly into the snix castore without
//! intermediate filesystem materialization.
//!
//! The bridge walks the git tree recursively, uploads each blob to the
//! [`BlobService`], then feeds the collected [`IngestionEntry`] stream to
//! [`ingest_entries`] in children-before-parents order. This avoids the
//! `git archive → tar → extract → import_path` round-trip that the prior
//! implementation used.

use std::sync::Arc;

use atom_id::AtomId;
use eos_core::bridge::AtomContentBridge;
use eos_core::digest::Blake3Digest;
use eos_core::eval::ResolvedInput;
use eos_core::store::StorePath;
use gix::object::tree::EntryKind;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_castore::import::{IngestionEntry, ingest_entries};
use snix_castore::{B3Digest, PathBuf as CastorePathBuf};
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};
use tokio::io::AsyncWriteExt;
use tracing::warn;

/// Errors arising from the git-to-castore bridge.
#[derive(Debug, thiserror::Error)]
pub enum GitBridgeError {
    /// The content digest bytes could not be parsed as a git object ID.
    #[error("invalid git object ID ({len} bytes): expected 20 (SHA-1) or 32 (SHA-256)")]
    InvalidObjectId { len: usize },

    /// The git object referenced by the digest was not found.
    #[error("git object {oid} not found")]
    ObjectNotFound {
        oid: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The git object was not a tree or commit (cannot extract a source tree).
    #[error("git object {oid} is {kind}, expected tree or commit")]
    UnexpectedObjectKind { oid: String, kind: String },

    /// A tree entry's filename is not valid UTF-8.
    #[error("non-UTF-8 filename in git tree at '{context}'")]
    InvalidFilename { context: String },

    /// A path could not be parsed as a castore path.
    #[error("invalid castore path '{path}'")]
    InvalidCastorePath {
        path: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Failed to read a git object's data.
    #[error("failed to read git object")]
    ObjectRead(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Failed to iterate tree entries.
    #[error("failed to iterate git tree entries")]
    TreeIteration(#[source] Box<dyn std::error::Error + Send + Sync>),

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
}

// ---------------------------------------------------------------------------
// Intermediate types for the two-phase (sync walk → async ingest) pipeline
// ---------------------------------------------------------------------------

/// A collected entry from the synchronous git tree walk.
///
/// Holds all data needed to produce an [`IngestionEntry`] during the
/// async ingestion phase. Blob data is eagerly read into memory during
/// the walk so that the async phase can upload without holding gix borrows
/// across `.await` points.
enum CollectedEntry {
    Regular {
        path: String,
        data: Vec<u8>,
        executable: bool,
    },
    Symlink {
        path: String,
        target: Vec<u8>,
    },
    Dir {
        path: String,
    },
}

/// Git-backed implementation of [`AtomContentBridge`].
///
/// Constructed at the scheduler (wiring site) where the concrete `gix::Repository`
/// is known. Holds cloned service handles so it can be moved into async tasks.
pub struct GitCastoreBridge {
    /// Git repository for tree traversal.
    repo: gix::ThreadSafeRepository,
    /// Castore blob storage.
    blob_service: Arc<dyn BlobService>,
    /// Castore directory storage.
    directory_service: Arc<dyn DirectoryService>,
    /// Nix path info metadata service.
    path_info_service: Arc<dyn PathInfoService>,
    /// NAR hash calculation service.
    nar_calculation_service: Arc<dyn NarCalculationService>,
}

impl GitCastoreBridge {
    /// Creates a new bridge from a git repository and snix service handles.
    pub fn new(
        repo: gix::Repository,
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
        nar_calculation_service: Arc<dyn NarCalculationService>,
    ) -> Self {
        Self {
            repo: repo.into_sync(),
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
        }
    }
}

impl AtomContentBridge for GitCastoreBridge {
    type Digest = Blake3Digest;
    type Error = GitBridgeError;

    async fn ingest_atom(
        &self,
        _id: &AtomId,
        label: &str,
        dig: &[u8],
    ) -> Result<ResolvedInput<Blake3Digest>, GitBridgeError> {
        let repo = self.repo.to_thread_local();

        // ---------------------------------------------------------------
        // Phase 1: Resolve the git tree from the content digest
        // ---------------------------------------------------------------
        let oid = parse_object_id(dig)?;
        let tree_oid = resolve_tree_oid(&repo, oid)?;

        // ---------------------------------------------------------------
        // Phase 2: Walk the tree synchronously, collecting all entries
        // ---------------------------------------------------------------
        // The recursive walk naturally produces children before parents,
        // satisfying the `ingest_entries` stream invariant by construction.
        let mut collected = Vec::new();
        walk_tree_recursive(&repo, tree_oid, label, &mut collected)?;
        // Add the root directory entry last (single-component path).
        collected.push(CollectedEntry::Dir {
            path: label.to_owned(),
        });

        // ---------------------------------------------------------------
        // Phase 3: Upload blobs and build the IngestionEntry stream
        // ---------------------------------------------------------------
        let mut ingestion_entries = Vec::with_capacity(collected.len());

        for entry in collected {
            match entry {
                CollectedEntry::Regular {
                    path,
                    data,
                    executable,
                } => {
                    let size = data.len() as u64;
                    let digest = self.upload_blob(&path, &data).await?;
                    let castore_path = parse_castore_path(&path)?;
                    ingestion_entries.push(IngestionEntry::Regular {
                        path: castore_path,
                        size,
                        executable,
                        digest,
                    });
                },
                CollectedEntry::Symlink { path, target } => {
                    let castore_path = parse_castore_path(&path)?;
                    ingestion_entries.push(IngestionEntry::Symlink {
                        path: castore_path,
                        target,
                    });
                },
                CollectedEntry::Dir { path } => {
                    let castore_path = parse_castore_path(&path)?;
                    ingestion_entries.push(IngestionEntry::Dir { path: castore_path });
                },
            }
        }

        // ---------------------------------------------------------------
        // Phase 4: Feed entries into the castore directory service
        // ---------------------------------------------------------------
        let root_node = ingest_entries(
            self.directory_service.clone(),
            futures::stream::iter(ingestion_entries.into_iter().map(Ok::<_, std::io::Error>)),
        )
        .await
        .map_err(|e| GitBridgeError::DirectoryIngestion(Box::new(e)))?;

        // ---------------------------------------------------------------
        // Phase 5: Calculate NAR hash and register PathInfo
        // ---------------------------------------------------------------
        let (nar_size, nar_sha256) = self
            .nar_calculation_service
            .calculate_nar(&root_node)
            .await
            .map_err(|e| GitBridgeError::NarCalculation(e))?;

        let ca = nix_compat::nixhash::CAHash::Nar(nix_compat::nixhash::NixHash::Sha256(nar_sha256));

        let output_path: nix_compat::store_path::StorePath<String> =
            nix_compat::store_path::build_ca_path(label, &ca, std::iter::empty::<&str>(), false)
                .map_err(|_| GitBridgeError::InvalidStoreName {
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
            .map_err(|e| GitBridgeError::PathInfoRegistration {
                label: label.to_owned(),
                source: e,
            })?;

        // ---------------------------------------------------------------
        // Phase 6: Extract the root digest and return ResolvedInput
        // ---------------------------------------------------------------
        let root_digest = match &root_node {
            snix_castore::Node::Directory { digest, .. }
            | snix_castore::Node::File { digest, .. } => *digest,
            snix_castore::Node::Symlink { .. } => {
                return Err(GitBridgeError::UnexpectedObjectKind {
                    oid: hex::encode(dig),
                    kind: "symlink root node".to_owned(),
                });
            },
        };

        Ok(ResolvedInput {
            digest: Blake3Digest(root_digest.into()),
            store_path: StorePath(path_info.store_path.to_string()),
        })
    }
}

impl GitCastoreBridge {
    /// Uploads blob data to the castore [`BlobService`] and returns its BLAKE3 digest.
    async fn upload_blob(&self, path: &str, data: &[u8]) -> Result<B3Digest, GitBridgeError> {
        let mut writer = self.blob_service.open_write().await;
        writer
            .write_all(data)
            .await
            .map_err(|e| GitBridgeError::BlobUpload {
                path: path.to_owned(),
                source: e,
            })?;
        writer
            .close()
            .await
            .map_err(|e| GitBridgeError::BlobUpload {
                path: path.to_owned(),
                source: e,
            })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses raw content-digest bytes into a `gix::ObjectId`.
fn parse_object_id(dig: &[u8]) -> Result<gix::ObjectId, GitBridgeError> {
    gix::ObjectId::try_from(dig).map_err(|_| GitBridgeError::InvalidObjectId { len: dig.len() })
}

/// Resolves a git OID to a tree OID, peeling commits if necessary.
fn resolve_tree_oid(
    repo: &gix::Repository,
    oid: gix::ObjectId,
) -> Result<gix::ObjectId, GitBridgeError> {
    let object = repo
        .find_object(oid)
        .map_err(|e| GitBridgeError::ObjectNotFound {
            oid: oid.to_string(),
            source: Box::new(e),
        })?;

    match object.kind {
        gix::object::Kind::Tree => Ok(oid),
        gix::object::Kind::Commit => {
            let commit = object
                .try_into_commit()
                .map_err(|e| GitBridgeError::ObjectRead(Box::new(e)))?;
            let tree_id = commit
                .tree_id()
                .map_err(|e| GitBridgeError::ObjectRead(Box::new(e)))?
                .detach();
            Ok(tree_id)
        },
        other => Err(GitBridgeError::UnexpectedObjectKind {
            oid: oid.to_string(),
            kind: other.to_string(),
        }),
    }
}

/// Recursively walks a git tree, collecting entries in children-before-parents order.
///
/// The recursive structure naturally ensures that all children of a directory
/// are appended to `out` before the directory's own `Dir` entry, satisfying
/// the [`ingest_entries`] stream invariant.
fn walk_tree_recursive(
    repo: &gix::Repository,
    tree_oid: gix::ObjectId,
    prefix: &str,
    out: &mut Vec<CollectedEntry>,
) -> Result<(), GitBridgeError> {
    let tree_obj = repo
        .find_object(tree_oid)
        .map_err(|e| GitBridgeError::ObjectNotFound {
            oid: tree_oid.to_string(),
            source: Box::new(e),
        })?;
    let tree = tree_obj
        .try_into_tree()
        .map_err(|e| GitBridgeError::ObjectRead(Box::new(e)))?;

    for entry_result in tree.iter() {
        let entry = entry_result.map_err(|e| GitBridgeError::TreeIteration(Box::new(e)))?;

        let filename =
            std::str::from_utf8(entry.filename()).map_err(|_| GitBridgeError::InvalidFilename {
                context: prefix.to_owned(),
            })?;

        let child_path = format!("{prefix}/{filename}");

        match entry.mode().kind() {
            EntryKind::Tree => {
                let child_oid = entry.object_id();
                // Recurse first — all children will be appended before this Dir entry.
                walk_tree_recursive(repo, child_oid, &child_path, out)?;
                out.push(CollectedEntry::Dir { path: child_path });
            },
            EntryKind::Blob => {
                let obj = repo.find_object(entry.object_id()).map_err(|e| {
                    GitBridgeError::ObjectNotFound {
                        oid: entry.object_id().to_string(),
                        source: Box::new(e),
                    }
                })?;
                out.push(CollectedEntry::Regular {
                    path: child_path,
                    data: obj.data.to_vec(),
                    executable: false,
                });
            },
            EntryKind::BlobExecutable => {
                let obj = repo.find_object(entry.object_id()).map_err(|e| {
                    GitBridgeError::ObjectNotFound {
                        oid: entry.object_id().to_string(),
                        source: Box::new(e),
                    }
                })?;
                out.push(CollectedEntry::Regular {
                    path: child_path,
                    data: obj.data.to_vec(),
                    executable: true,
                });
            },
            EntryKind::Link => {
                let obj = repo.find_object(entry.object_id()).map_err(|e| {
                    GitBridgeError::ObjectNotFound {
                        oid: entry.object_id().to_string(),
                        source: Box::new(e),
                    }
                })?;
                out.push(CollectedEntry::Symlink {
                    path: child_path,
                    target: obj.data.to_vec(),
                });
            },
            EntryKind::Commit => {
                warn!(
                    path = %child_path,
                    "skipping submodule entry in atom tree"
                );
            },
        }
    }

    Ok(())
}

/// Parses a string path into a castore [`PathBuf`](CastorePathBuf).
fn parse_castore_path(path: &str) -> Result<CastorePathBuf, GitBridgeError> {
    path.parse()
        .map_err(|e: std::io::Error| GitBridgeError::InvalidCastorePath {
            path: path.to_owned(),
            source: Box::new(e),
        })
}
