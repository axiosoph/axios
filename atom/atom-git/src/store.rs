//! Implementation of [`AtomStore`] for the Git backend.
//!
//! Provides accumulation of packages from remote sources and filesystem
//! directories into a local Git store repository.

use std::fs;
use std::path::Path;

use atom_core::{
    AtomContent, AtomEntry, AtomId, AtomSource, AtomStore, AtomVersion, ContentEntry, Label,
    RawVersion,
};
use coz_rs;
use gix::hash::ObjectId;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

use crate::error::GitError;
use crate::source::{CozMessageEnvelope, GitEntry, GitSource};

/// Opaque sentinel bytes indicating a filesystem-sourced anchor.
pub const FS_SENTINEL_ANCHOR: &[u8] = b"fs-sentinel-anchor";

/// Render an [`AtomDigest`](atom_core::AtomDigest) into a git-ref-safe segment.
///
/// The canonical digest form is `<token>:<encoding>`, but git forbids `:` in
/// reference names. Dev refs live at `refs/atom/dev/{segment}/{version}`, so the
/// `:` separator is rendered as `.` — a single dot-joined segment, as the former
/// signing-alg form was. The token and both encodings (base64url, lowercase hex)
/// contain no `.`, so the substitution is unambiguous.
pub fn dev_ref_digest(digest: &atom_core::AtomDigest) -> String {
    digest.to_string().replace(':', ".")
}

/// Lowercase-hex encode arbitrary bytes for a ref-path segment.
///
/// Backs the `czd-hex` ref family (`docs/specs/git-storage-format.md`
/// `[czd-oid-disjoint]`, `refs/atom/claims/d/{claim_czd}`) and the
/// `blake3(publish_czd)` store version-ref family
/// (`[store-ref-by-publish-czd]`). Public so `GitSource::resolve` (which
/// must derive the same claim-ref key from a publish payload's `claim`
/// field to find its owning claim under the flat ref layout) and test
/// fixtures can compute identical keys without duplicating the encoding.
pub fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{:02x}", b);
            s
        })
}

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
        let repo = self.source.repo();

        // 1. Construct AtomId using the filesystem sentinel anchor
        let anchor = atom_id::Anchor::new(FS_SENTINEL_ANCHOR.to_vec());
        let id = AtomId::new(anchor, label.clone());

        // 2. Compute the sha256 store-index digest of the atom id
        let digest = atom_core::AtomDigest::compute(&id, coz_rs::Alg::ES256.hash_alg());
        let digest_str = dev_ref_digest(&digest);

        // 3. Recursively write tree from filesystem path
        let tree_oid = write_tree_recursive(&repo, path)?;

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

    /// Evict (delete) a version ref from the store.
    ///
    /// `store_key_hex` is the flat store ref's own key,
    /// `hex(blake3(publish_czd))` (`[store-ref-by-publish-czd]`) -- the
    /// same segment `GitStore::ingest` writes under. Implements
    /// `[store-claim-cleanup]`: after removing the version ref, if no
    /// other surviving `refs/atom/d/*` ref's publish tag still chains to
    /// the same claim czd, the corresponding
    /// `refs/atom/claims/d/{claim_czd}` ref is also deleted. The flat
    /// scheme carries no claim segment in its own path, so the owning
    /// claim can only be discovered by inspecting the ref's own publish
    /// tag payload before deletion.
    ///
    /// # Concurrency
    ///
    /// Steps 1 (delete version) and 2 (scan siblings) are not atomic.
    /// Concurrent eviction of the last two versions under the same claim
    /// could leave an orphan claim ref. Callers must serialize evictions
    /// per claim, or a periodic GC pass should sweep orphaned claims.
    pub fn evict_version(&self, store_key_hex: &str) -> Result<(), GitError> {
        let repo = self.source.repo();
        let version_ref_name = format!("refs/atom/d/{}", store_key_hex);
        let version_fullname = FullName::try_from(version_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;

        // Discover the owning claim czd from this ref's own publish tag
        // payload before deleting it -- there is no path segment to read
        // it from under the flat scheme.
        let claim_czd = match repo.try_find_reference(&version_ref_name)? {
            Some(existing) => claim_czd_of_store_ref(&repo, &existing)?,
            None => None,
        };

        // 1. Delete the version reference
        let edit = RefEdit {
            change: Change::Delete {
                expected: PreviousValue::Any,
                log: RefLog::AndReference,
            },
            name: version_fullname,
            deref: false,
        };
        repo.edit_reference(edit)?;

        let Some(claim_czd) = claim_czd else {
            return Ok(());
        };

        // 2. Check whether any surviving refs/atom/d/* ref's publish tag
        // still chains to the same claim -- a full scan, since flat refs
        // share no claim-prefix to scan under.
        let mut any_left = false;
        for r in repo.references()?.prefixed("refs/atom/d/")? {
            let Ok(r) = r else { continue };
            if claim_czd_of_store_ref(&repo, &r)?.as_ref() == Some(&claim_czd) {
                any_left = true;
                break;
            }
        }

        // 3. If no other versions remain, delete refs/atom/claims/d/{claim_czd}
        if !any_left {
            let claim_czd_hex = hex_encode(claim_czd.as_bytes());
            let claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
            if let Ok(claim_fullname) = FullName::try_from(claim_ref_name.as_str()) {
                let claim_edit = RefEdit {
                    change: Change::Delete {
                        expected: PreviousValue::Any,
                        log: RefLog::AndReference,
                    },
                    name: claim_fullname,
                    deref: false,
                };
                let _ = repo.edit_reference(claim_edit);
            }
        }

        Ok(())
    }
}

/// Read the claim czd a store version ref's publish tag chains to,
/// without full signature verification -- eviction cleanup
/// (`[store-claim-cleanup]`) is a best-effort GC pass keyed by claim
/// identity, not a trust-establishing operation. Returns `None` if the
/// ref's target isn't a tag (should not occur for a real
/// `refs/atom/d/*` entry, but cleanup degrades gracefully rather than
/// erroring on unexpected store contents).
fn claim_czd_of_store_ref(
    repo: &gix::Repository,
    reference: &gix::Reference,
) -> Result<Option<coz_rs::Czd>, GitError> {
    let oid = reference.id().detach();
    let obj = repo.find_object(oid)?;
    if obj.kind != gix::object::Kind::Tag {
        return Ok(None);
    }
    let tag = obj.try_into_tag()?;
    let tag_decoded = tag.decode()?;
    let msg_str = tag_decoded.message.to_string();
    let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str)?;
    let pay_value = serde_json::to_value(&envelope.pay)?;
    let publish_payload: atom_id::PublishPayload = serde_json::from_value(pay_value)?;
    Ok(Some(publish_payload.claim))
}

impl AtomSource for GitStore {
    type Entry = GitEntry;
    type Error = GitError;

    async fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error> {
        self.source.resolve(id).await
    }

    async fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error> {
        self.source.discover(query).await
    }
}

impl AtomContent for GitStore {
    async fn content(
        &self,
        id: &AtomId,
        dig: &[u8],
    ) -> Result<Option<Vec<ContentEntry>>, Self::Error> {
        self.source.content(id, dig).await
    }
}

impl AtomStore for GitStore {
    async fn ingest<S: AtomContent>(&self, source: &S) -> Result<(), Self::Error> {
        let dest_repo = self.source.repo();

        // 2. Discover all atom identities in the source
        let discovered_ids = source
            .discover("")
            .await
            .map_err(|e| GitError::Validation(e.to_string()))?;

        for id in discovered_ids {
            let versions_to_ingest = {
                let entry_opt = source
                    .resolve(&id)
                    .await
                    .map_err(|e| GitError::Validation(e.to_string()))?;
                let mut list = Vec::new();
                if let Some(entry) = entry_opt {
                    for v in entry.versions() {
                        list.push((
                            v.version().clone(),
                            v.dig().to_vec(),
                            v.czd().cloned(),
                            v.claim_msg().map(String::from),
                            v.publish_msg().map(String::from),
                        ));
                    }
                }
                list
            };

            for (version, dig, czd_opt, claim_msg_opt, publish_msg_opt) in versions_to_ingest {
                if let Some(czd_val) = &czd_opt {
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
                    let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg)?;
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

                    // Verify claim-pubkey thumbprint matches payload tmb
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
                    let publish_envelope: CozMessageEnvelope = serde_json::from_str(&publish_msg)?;
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

                    // The store's version ref is keyed by the PUBLISH
                    // transaction's own czd (`[store-ref-by-publish-czd]`),
                    // not the claim's -- distinct from `czd_val`/
                    // `publish_payload.claim` above.
                    let publish_czd = atom_id::czd_for_alg(
                        &publish_pay_bytes,
                        &publish_envelope.sig,
                        publish_alg_str,
                    )?;

                    // Verify publish chains to claim
                    if publish_payload.claim != *czd_val {
                        return Err(GitError::Validation(
                            "Publish payload claim czd does not match version czd".into(),
                        ));
                    }

                    // Verify temporal ordering
                    if publish_payload.now <= claim_payload.now {
                        return Err(GitError::Validation(
                            "Temporal ordering violation: publish timestamp not after claim".into(),
                        ));
                    }

                    // Resolve the claim commit through its own ref family
                    // (`refs/atom/claims/d/{claim_czd}`,
                    // docs/specs/git-storage-format.md `[store-claim-ref]`)
                    // instead of assuming the claim's czd is a git object id
                    // (issue #64). `Czd` and `ObjectId` are disjoint sorts
                    // (`[czd-oid-disjoint]`): the destination OID for a claim
                    // commit can only come from a real ref lookup or from
                    // actually writing the commit — never from casting or
                    // guessing one.
                    let claim_czd_hex = hex_encode(publish_payload.claim.as_bytes());
                    let store_claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);

                    let claim_oid = match dest_repo.try_find_reference(&store_claim_ref_name)? {
                        Some(existing) => existing.id().detach(),
                        None => {
                            // A claim replacement chains to its prior claim via
                            // the payload's own signed `prior` field — never
                            // guessed. The prior claim MUST already be ingested
                            // for a rotation to resolve; an absent prior claim
                            // is a genuine sequencing problem, not a defect in
                            // this lookup.
                            let parent_oid = match &claim_payload.prior {
                                Some(prior_czd) => {
                                    let prior_hex = hex_encode(prior_czd.as_bytes());
                                    let prior_ref_name =
                                        format!("refs/atom/claims/d/{}", prior_hex);
                                    let prior_ref = dest_repo
                                        .try_find_reference(&prior_ref_name)?
                                        .ok_or_else(|| {
                                            GitError::Validation(format!(
                                                "claim {} replaces {} but no claim commit for it \
                                                 has been ingested yet",
                                                claim_czd_hex, prior_hex
                                            ))
                                        })?;
                                    Some(prior_ref.id().detach())
                                },
                                None => None,
                            };
                            crate::gix_util::write_claim_commit(
                                &dest_repo,
                                claim_msg.to_string(),
                                parent_oid,
                            )?
                        },
                    };

                    // Write each ContentEntry as a git object and reconstruct the tree
                    let content_entries = source
                        .content(&id, &dig)
                        .await
                        .map_err(|e| GitError::Validation(e.to_string()))?
                        .ok_or_else(|| {
                            GitError::Validation(format!("Content not found for atom {}", id))
                        })?;
                    let tree_oid = self.write_content_tree(&dest_repo, &content_entries)?;

                    // Verify atom commit tree hash matches payload dig
                    if tree_oid.as_bytes() != publish_payload.dig {
                        return Err(GitError::Validation(
                            "Atom commit tree hash does not match publish payload dig".into(),
                        ));
                    }

                    // Reconstruct/write deterministic atom commit
                    let publish_src_oid =
                        crate::gix_util::seam::oid_from_src_field(publish_payload.src.as_slice())
                            .map_err(|e| {
                            GitError::Validation(format!("Invalid publish src OID: {}", e))
                        })?;
                    let atom_commit_oid = crate::gix_util::write_deterministic_commit(
                        &dest_repo,
                        tree_oid,
                        publish_src_oid,
                    )?;
                    if atom_commit_oid.as_bytes() != dig {
                        return Err(GitError::Validation(
                            "Reconstructed atom commit OID does not match version dig".into(),
                        ));
                    }

                    // Write the publish tag
                    let tag_name = format!("{}-{}", id.label(), version.as_str());
                    let new_tag_oid = crate::gix_util::write_publish_tag(
                        &dest_repo,
                        &tag_name,
                        atom_commit_oid,
                        gix::object::Kind::Commit,
                        crate::gix_util::blank_signature(),
                        publish_msg.to_string(),
                    )?;

                    // 4. Update the references in store layout. The
                    // version ref is flat and keyed by
                    // blake3(publish_czd), not nested under the claim
                    // (`[store-ref-by-publish-czd]`); the version string
                    // lives only in the publish payload now, never in the
                    // ref path.
                    let store_claim_ref = format!("refs/atom/claims/d/{}", claim_czd_hex);
                    let store_version_ref = format!(
                        "refs/atom/d/{}",
                        hex_encode(blake3::hash(publish_czd.as_bytes()).as_bytes())
                    );

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

                    let version_ref_fullname = FullName::try_from(store_version_ref.as_str())
                        .map_err(|e| GitError::Validation(e.to_string()))?;
                    edits.push(RefEdit {
                        change: Change::Update {
                            log: LogChange {
                                mode: RefLog::AndReference,
                                force_create_reflog: false,
                                message: format!("Ingest version tag {}", version.as_str()).into(),
                            },
                            expected: PreviousValue::Any,
                            new: Target::Object(new_tag_oid),
                        },
                        name: version_ref_fullname,
                        deref: false,
                    });

                    dest_repo.edit_references(edits)?;
                } else {
                    // Ingestion of an unsigned dev version
                    let content_entries = source
                        .content(&id, &dig)
                        .await
                        .map_err(|e| GitError::Validation(e.to_string()))?
                        .ok_or_else(|| {
                            GitError::Validation(format!("Content not found for atom {}", id))
                        })?;
                    let tree_oid = self.write_content_tree(&dest_repo, &content_entries)?;

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
                    let commit_oid = dest_repo.write_object(commit)?.detach();
                    if commit_oid.as_bytes() != dig {
                        return Err(GitError::Validation(
                            "Reconstructed dev commit OID does not match version dig".into(),
                        ));
                    }

                    // Compute the sha256 store-index digest of the atom id
                    let digest = atom_core::AtomDigest::compute(&id, coz_rs::Alg::ES256.hash_alg());
                    let digest_str = dev_ref_digest(&digest);

                    let dev_ref_name = format!("refs/atom/dev/{}/{}", digest_str, version.as_str());
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

        Ok(())
    }

    async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error> {
        // Resolve the identity to see if any versions exist
        match self.resolve(id).await {
            Ok(Some(entry)) => Ok(!entry.versions.is_empty()),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl GitStore {
    /// Reconstruct the content tree in the given repository from a sequence of content entries.
    pub fn write_content_tree(
        &self,
        repo: &gix::Repository,
        entries: &[ContentEntry],
    ) -> Result<ObjectId, GitError> {
        use std::collections::HashMap;

        use gix::object::tree::EntryKind;
        use gix::objs::tree::{Entry, EntryMode};

        let mut tree_entries: HashMap<String, Vec<Entry>> = HashMap::new();

        for entry in entries {
            match entry {
                ContentEntry::Regular {
                    path,
                    data,
                    executable,
                } => {
                    let blob_oid = repo
                        .write_object(gix::objs::Blob { data: data.clone() })?
                        .detach();
                    let (parent, filename) = match path.rfind('/') {
                        Some(idx) => (&path[..idx], &path[idx + 1..]),
                        None => ("", path.as_str()),
                    };
                    let mode = if *executable {
                        EntryMode::from(EntryKind::BlobExecutable)
                    } else {
                        EntryMode::from(EntryKind::Blob)
                    };
                    tree_entries
                        .entry(parent.to_string())
                        .or_default()
                        .push(Entry {
                            mode,
                            filename: filename.into(),
                            oid: blob_oid,
                        });
                },
                ContentEntry::Symlink { path, target } => {
                    let blob_oid = repo
                        .write_object(gix::objs::Blob {
                            data: target.clone(),
                        })?
                        .detach();
                    let (parent, filename) = match path.rfind('/') {
                        Some(idx) => (&path[..idx], &path[idx + 1..]),
                        None => ("", path.as_str()),
                    };
                    tree_entries
                        .entry(parent.to_string())
                        .or_default()
                        .push(Entry {
                            mode: EntryMode::from(EntryKind::Link),
                            filename: filename.into(),
                            oid: blob_oid,
                        });
                },
                ContentEntry::Directory { path } => {
                    let mut current_entries = tree_entries.remove(path).unwrap_or_default();
                    current_entries.sort();
                    let tree_oid = repo
                        .write_object(gix::objs::Tree {
                            entries: current_entries,
                        })?
                        .detach();

                    let (parent, filename) = match path.rfind('/') {
                        Some(idx) => (&path[..idx], &path[idx + 1..]),
                        None => ("", path.as_str()),
                    };
                    tree_entries
                        .entry(parent.to_string())
                        .or_default()
                        .push(Entry {
                            mode: EntryMode::from(EntryKind::Tree),
                            filename: filename.into(),
                            oid: tree_oid,
                        });
                },
            }
        }

        // Now write the root tree
        let mut root_entries = tree_entries.remove("").unwrap_or_default();
        root_entries.sort();
        let root_tree_oid = repo
            .write_object(gix::objs::Tree {
                entries: root_entries,
            })?
            .detach();
        Ok(root_tree_oid)
    }
}

/// Recursively write directory tree entries from a path to a Git ODB.
fn write_tree_recursive(repo: &gix::Repository, path: &Path) -> Result<ObjectId, GitError> {
    use gix::object::tree::EntryKind;
    use gix::objs::tree::{Entry, EntryMode};

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
