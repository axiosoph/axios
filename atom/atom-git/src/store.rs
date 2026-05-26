//! Implementation of [`AtomStore`] for the Git backend.
//!
//! Provides accumulation of packages from remote sources and filesystem
//! directories into a local Git store repository.

use std::fs;
use std::path::Path;

use atom_core::{AtomEntry, AtomId, AtomSource, AtomStore, AtomVersion, Label, RawVersion};
use coz_rs;
use gix::hash::ObjectId;
use gix::objs::Exists;
use gix::objs::tree::{Entry, EntryKind, EntryMode};
use gix::prelude::{Find, Write};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

use crate::error::GitError;
use crate::registry::GitRegistry;
use crate::source::{CozMessageEnvelope, GitEntry, GitSource};

/// Opaque sentinel bytes indicating a filesystem-sourced anchor.
pub const FS_SENTINEL_ANCHOR: &[u8] = b"fs-sentinel-anchor";

/// Write-enabled Git store.
///
/// Implements [`AtomStore`] to accumulate package versions, verify coz
/// signatures locally, and import filesystem paths as local dev packages.
pub struct GitStore {
    /// Read-only source interface for resolving and discovering references.
    pub source: GitSource,
}

impl GitStore {
    /// Create a new `GitStore` instance wrapping a Git repository.
    pub fn new(repo: gix::Repository) -> Self {
        Self {
            source: GitSource::new(repo),
        }
    }

    /// Import a filesystem directory into the store as an unsigned dev version.
    ///
    /// The imported files are written to the Git database as blobs, a tree is
    /// constructed, and an unsigned atom commit is written. A reference is updated at
    /// `refs/atom/dev/{atom_digest}/{dev_version}` pointing to the commit.
    pub fn import_path(
        &self,
        label: &Label,
        path: &Path,
        dev_version: &RawVersion,
    ) -> Result<(), GitError> {
        let repo = &self.source.repo;

        // 1. Construct AtomId using the filesystem sentinel anchor
        let anchor = atom_id::Anchor::new(FS_SENTINEL_ANCHOR.to_vec());
        let id = AtomId::new(anchor, label.clone());

        // 2. Compute the ES256 atom digest
        let digest = atom_core::AtomDigest::compute(&id, coz_rs::Alg::ES256)
            .ok_or_else(|| GitError::Validation("Failed to compute atom digest".into()))?;
        let digest_str = digest.to_string();

        // 3. Recursively write tree from filesystem path
        let tree_oid = write_tree_recursive(repo, path)?;

        // 4. Create an unsigned, parentless, deterministic commit with no src header
        let blank = crate::gix_util::blank_signature();
        let commit = gix::objs::Commit {
            tree: tree_oid,
            parents: Vec::new().into(),
            author: blank.clone(),
            committer: blank,
            encoding: None,
            message: gix::objs::bstr::BString::from(""),
            extra_headers: Vec::new(),
        };
        let commit_oid = repo.write_object(commit)?.detach();

        // 5. Write dev reference refs/atom/dev/{atom_digest}/{dev_version}
        let ref_name = format!("refs/atom/dev/{}/{}", digest_str, dev_version.as_str());
        let ref_fullname = FullName::try_from(ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;

        let edit = RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Import local dev version".into(),
                },
                expected: PreviousValue::Any,
                new: Target::Object(commit_oid),
            },
            name: ref_fullname,
            deref: false,
        };

        repo.edit_reference(edit)?;

        Ok(())
    }
}

impl AtomSource for GitStore {
    type Entry = GitEntry;
    type Error = GitError;

    fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error> {
        self.source.resolve(id)
    }

    fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error> {
        self.source.discover(query)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl AtomStore for GitStore {
    fn ingest<S: AtomSource>(&self, source: &S) -> Result<(), Self::Error> {
        let dest_repo = &self.source.repo;

        // 1. Downcast source to obtain its source repository
        let source_repo = if let Some(git_source) = source.as_any().downcast_ref::<GitSource>() {
            &git_source.repo
        } else if let Some(git_registry) = source.as_any().downcast_ref::<GitRegistry>() {
            &git_registry.source.repo
        } else if let Some(git_store) = source.as_any().downcast_ref::<GitStore>() {
            &git_store.source.repo
        } else {
            return Err(GitError::Validation(
                "Ingestion is only supported from Git-backed sources".into(),
            ));
        };

        // 2. Discover all atom identities in the source
        let discovered_ids = source
            .discover("")
            .map_err(|e| GitError::Validation(e.to_string()))?;

        for id in discovered_ids {
            if let Some(entry) = source
                .resolve(&id)
                .map_err(|e| GitError::Validation(e.to_string()))?
            {
                for v in entry.versions() {
                    let version = v.version();
                    let dig = v.dig();
                    let czd_opt = v.czd();
                    let claim_msg_opt = v.claim_msg();
                    let publish_msg_opt = v.publish_msg();

                    if let Some(czd_val) = czd_opt {
                        // Ingestion of a published version
                        let claim_msg = claim_msg_opt.ok_or_else(|| {
                            GitError::Validation(format!(
                                "Missing claim payload for version {}",
                                version.as_str()
                            ))
                        })?;
                        let publish_msg = publish_msg_opt.ok_or_else(|| {
                            GitError::Validation(format!(
                                "Missing publish payload for version {}",
                                version.as_str()
                            ))
                        })?;

                        // Parse and verify claim coz message
                        let claim_envelope: CozMessageEnvelope = serde_json::from_str(claim_msg)?;
                        let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
                        let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
                            GitError::Validation("Claim CozMessage is missing the key field".into())
                        })?;
                        let claim_alg_str = claim_envelope
                            .pay
                            .get("alg")
                            .and_then(|val| val.as_str())
                            .ok_or_else(|| {
                                GitError::Validation("Claim alg is missing or invalid".into())
                            })?;

                        let claim_payload = atom_id::verify_claim(
                            &claim_pay_bytes,
                            &claim_envelope.sig,
                            claim_alg_str,
                            claim_pub_key,
                        )?;

                        // Verify claim-pubkey thumbprint matches payload tmb (Step 4)
                        let computed_tmb =
                            coz_rs::compute_thumbprint_for_alg(claim_alg_str, claim_pub_key)
                                .ok_or_else(|| {
                                    GitError::Validation(
                                        "Failed to compute claim key thumbprint".into(),
                                    )
                                })?;
                        if computed_tmb != claim_payload.tmb {
                            return Err(GitError::Validation("Claim thumbprint mismatch".into()));
                        }

                        // Parse and verify publish coz message
                        let publish_envelope: CozMessageEnvelope =
                            serde_json::from_str(publish_msg)?;
                        let publish_pay_bytes = serde_json::to_vec(&publish_envelope.pay)?;
                        let pub_key = publish_envelope.key.as_ref().unwrap_or(claim_pub_key);
                        let publish_alg_str = publish_envelope
                            .pay
                            .get("alg")
                            .and_then(|val| val.as_str())
                            .ok_or_else(|| {
                                GitError::Validation("Publish alg is missing or invalid".into())
                            })?;

                        let publish_payload = atom_id::verify_publish(
                            &publish_pay_bytes,
                            &publish_envelope.sig,
                            publish_alg_str,
                            pub_key,
                        )?;

                        // Verify publish chains to claim (Step 5)
                        if publish_payload.claim != *czd_val {
                            return Err(GitError::Validation(
                                "Publish payload claim czd does not match version czd".into(),
                            ));
                        }

                        // Verify temporal ordering (Step 6)
                        if publish_payload.now <= claim_payload.now {
                            return Err(GitError::Validation(
                                "Temporal ordering violation: publish timestamp not after claim"
                                    .into(),
                            ));
                        }

                        let claim_oid =
                            ObjectId::from_bytes_or_panic(publish_payload.claim.as_bytes());
                        let claim_czd_hex = claim_oid.to_hex().to_string();

                        // Look up the version reference in the source repository to find the publish tag OID
                        let tag_ref_name = if source_repo
                            .try_find_reference(&format!(
                                "refs/atom/pub/{}/{}",
                                id.label(),
                                version.as_str()
                            ))?
                            .is_some()
                        {
                            format!("refs/atom/pub/{}/{}", id.label(), version.as_str())
                        } else {
                            format!("refs/atom/d/{}/{}", claim_czd_hex, version.as_str())
                        };
                        let tag_ref =
                            source_repo
                                .try_find_reference(&tag_ref_name)?
                                .ok_or_else(|| {
                                    GitError::Validation(format!(
                                        "Could not find version reference {} in source repository",
                                        tag_ref_name
                                    ))
                                })?;
                        let tag_oid = tag_ref.id().detach();

                        // 3. Transfer Git objects from source ODB to store ODB
                        copy_tag_chain(source_repo, dest_repo, tag_oid)?;
                        copy_claim_chain(source_repo, dest_repo, claim_oid)?;

                        // Verify atom commit tree hash matches payload dig (Step 1)
                        let atom_commit_oid = ObjectId::from_bytes_or_panic(dig);
                        let commit_obj = dest_repo.find_object(atom_commit_oid)?;
                        let commit = commit_obj.try_into_commit()?;
                        let tree_oid = commit.tree_id()?.detach();
                        if tree_oid.as_bytes() != publish_payload.dig {
                            return Err(GitError::Validation(
                                "Atom commit tree hash does not match publish payload dig".into(),
                            ));
                        }

                        // 4. Update the references in store layout
                        let store_claim_ref = format!("refs/atom/claims/d/{}", claim_czd_hex);
                        let store_version_ref =
                            format!("refs/atom/d/{}/{}", claim_czd_hex, version.as_str());

                        let mut edits = Vec::new();

                        let claim_ref_fullname = FullName::try_from(store_claim_ref.as_str())
                            .map_err(|e| GitError::Validation(e.to_string()))?;
                        edits.push(RefEdit {
                            change: Change::Update {
                                log: LogChange {
                                    mode: RefLog::AndReference,
                                    force_create_reflog: false,
                                    message: "Ingest claim commit".into(),
                                },
                                expected: PreviousValue::Any,
                                new: Target::Object(claim_oid),
                            },
                            name: claim_ref_fullname,
                            deref: false,
                        });

                        let version_ref_fullname =
                            FullName::try_from(store_version_ref.as_str())
                                .map_err(|e| GitError::Validation(e.to_string()))?;
                        edits.push(RefEdit {
                            change: Change::Update {
                                log: LogChange {
                                    mode: RefLog::AndReference,
                                    force_create_reflog: false,
                                    message: format!("Ingest version tag {}", version.as_str())
                                        .into(),
                                },
                                expected: PreviousValue::Any,
                                new: Target::Object(tag_oid),
                            },
                            name: version_ref_fullname,
                            deref: false,
                        });

                        dest_repo.edit_references(edits)?;
                    } else {
                        // Ingestion of an unsigned dev version
                        let commit_oid = ObjectId::from_bytes_or_panic(dig);
                        copy_object(source_repo, dest_repo, commit_oid)?;

                        let commit_obj = dest_repo.find_object(commit_oid)?;
                        let commit = commit_obj.try_into_commit()?;
                        let tree_oid = commit.tree_id()?.detach();
                        copy_tree_recursive(source_repo, dest_repo, tree_oid)?;

                        // Compute dev digest using ES256
                        let digest = atom_core::AtomDigest::compute(&id, coz_rs::Alg::ES256)
                            .ok_or_else(|| {
                                GitError::Validation("Failed to compute atom digest".into())
                            })?;
                        let digest_str = digest.to_string();

                        let dev_ref_name =
                            format!("refs/atom/dev/{}/{}", digest_str, version.as_str());
                        let dev_ref_fullname = FullName::try_from(dev_ref_name.as_str())
                            .map_err(|e| GitError::Validation(e.to_string()))?;

                        let edit = RefEdit {
                            change: Change::Update {
                                log: LogChange {
                                    mode: RefLog::AndReference,
                                    force_create_reflog: false,
                                    message: "Ingest dev version commit".into(),
                                },
                                expected: PreviousValue::Any,
                                new: Target::Object(commit_oid),
                            },
                            name: dev_ref_fullname,
                            deref: false,
                        };

                        dest_repo.edit_reference(edit)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn contains(&self, id: &AtomId) -> bool {
        // Resolve the identity to see if any versions exist
        match self.resolve(id) {
            Ok(Some(entry)) => !entry.versions.is_empty(),
            _ => false,
        }
    }
}

/// Recursively write directory tree entries from a path to a Git ODB.
fn write_tree_recursive(repo: &gix::Repository, path: &Path) -> Result<ObjectId, GitError> {
    let mut entries = Vec::new();

    for entry_res in fs::read_dir(path)? {
        let entry = entry_res?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str == ".git" {
            continue;
        }

        let entry_path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            let sub_tree_oid = write_tree_recursive(repo, &entry_path)?;
            entries.push(Entry {
                mode: EntryMode::from(EntryKind::Tree),
                filename: gix::objs::bstr::BString::from(file_name_str.to_string()),
                oid: sub_tree_oid,
            });
        } else if metadata.is_symlink() {
            let target = fs::read_link(&entry_path)?;
            let target_str = target.to_string_lossy();
            let blob_oid = repo
                .write_object(gix::objs::Blob {
                    data: target_str.as_bytes().to_vec(),
                })?
                .detach();
            entries.push(Entry {
                mode: EntryMode::from(EntryKind::Link),
                filename: gix::objs::bstr::BString::from(file_name_str.to_string()),
                oid: blob_oid,
            });
        } else if metadata.is_file() {
            let content = fs::read(&entry_path)?;
            let blob_oid = repo
                .write_object(gix::objs::Blob { data: content })?
                .detach();

            #[cfg(unix)]
            let is_exec = {
                use std::os::unix::fs::MetadataExt;
                metadata.mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let is_exec = false;

            let mode = if is_exec {
                EntryMode::from(EntryKind::BlobExecutable)
            } else {
                EntryMode::from(EntryKind::Blob)
            };

            entries.push(Entry {
                mode,
                filename: gix::objs::bstr::BString::from(file_name_str.to_string()),
                oid: blob_oid,
            });
        }
    }

    entries.sort();

    let tree = gix::objs::Tree { entries };
    let tree_oid = repo.write_object(tree)?.detach();
    Ok(tree_oid)
}

/// Copy a specific Git object from one repository ODB to another.
fn copy_object(
    src_repo: &gix::Repository,
    dest_repo: &gix::Repository,
    oid: ObjectId,
) -> Result<(), GitError> {
    if dest_repo.objects.exists(&oid) {
        return Ok(());
    }

    let mut buf = Vec::new();
    if let Some(obj) = src_repo
        .objects
        .try_find(&oid, &mut buf)
        .map_err(|e| GitError::Validation(e.to_string()))?
    {
        dest_repo
            .objects
            .write_buf(obj.kind, &buf)
            .map_err(|e| GitError::Validation(e.to_string()))?;
    }
    Ok(())
}

/// Recursively copy a Git tree and all referenced sub-trees and blobs.
fn copy_tree_recursive(
    src_repo: &gix::Repository,
    dest_repo: &gix::Repository,
    tree_oid: ObjectId,
) -> Result<(), GitError> {
    copy_object(src_repo, dest_repo, tree_oid)?;

    let obj = src_repo.find_object(tree_oid)?;
    let tree = obj.try_into_tree()?;
    for entry_ref in tree.iter() {
        let entry = entry_ref?;
        let mode = entry.mode();
        let oid = entry.oid();

        if mode.is_tree() {
            copy_tree_recursive(src_repo, dest_repo, oid.to_owned())?;
        } else {
            copy_object(src_repo, dest_repo, oid.to_owned())?;
        }
    }
    Ok(())
}

/// Recursively copy a tag chain back to the atom commit and transfer its tree.
fn copy_tag_chain(
    src_repo: &gix::Repository,
    dest_repo: &gix::Repository,
    tag_oid: ObjectId,
) -> Result<ObjectId, GitError> {
    let mut current_oid = tag_oid;
    loop {
        copy_object(src_repo, dest_repo, current_oid)?;

        let obj = src_repo.find_object(current_oid)?;
        match obj.kind {
            gix::object::Kind::Tag => {
                let tag = obj.try_into_tag()?;
                current_oid = tag.target_id()?.detach();
            },
            gix::object::Kind::Commit => {
                let commit = obj.try_into_commit()?;
                copy_tree_recursive(src_repo, dest_repo, commit.tree_id()?.detach())?;
                return Ok(current_oid);
            },
            _ => {
                return Err(GitError::Validation(format!(
                    "Unexpected object kind {} in tag chain",
                    obj.kind
                )));
            },
        }
    }
}

/// Recursively copy a claim commit chain back to its parentless root.
fn copy_claim_chain(
    src_repo: &gix::Repository,
    dest_repo: &gix::Repository,
    claim_oid: ObjectId,
) -> Result<(), GitError> {
    let mut current_oid = claim_oid;
    loop {
        copy_object(src_repo, dest_repo, current_oid)?;

        let obj = src_repo.find_object(current_oid)?;
        let commit = obj.try_into_commit()?;
        let parent_ids: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
        if !parent_ids.is_empty() {
            current_oid = parent_ids[0];
        } else {
            break;
        }
    }
    Ok(())
}
