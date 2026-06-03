//! Implementation of [`AtomRegistry`] for the Git backend.
//!
//! Provides the write interface for establishing claims and publishing
//! new versions of packages inside a source Git repository.

use std::time::SystemTime;

use atom_core::{AtomContent, AtomId, AtomRegistry, AtomSource, ContentEntry, Czd, RawVersion};
use atom_id::{Anchor, ClaimPayload, PublishPayload};
use gix::hash::ObjectId;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

use crate::error::GitError;
use crate::source::{CozMessageEnvelope, GitEntry, GitSource};

/// Write-enabled Git registry.
///
/// Implements [`AtomRegistry`] to allow claiming package identities
/// and publishing package versions against active claims within a source
/// Git repository.
pub struct GitRegistry {
    /// Read-only source interface for resolving and discovering references.
    pub source: GitSource,
    /// Private signing key bytes.
    pub signing_key: Vec<u8>,
    /// Public verifying key bytes.
    pub pub_key: Vec<u8>,
    /// Cryptographic signing algorithm.
    pub alg: coz_rs::Alg,
    /// Package ecosystem format identifier (e.g., "cargo", "npm", "ion").
    pub pkg: String,
}

impl GitRegistry {
    /// Create a new `GitRegistry` instance wrapping a Git repository.
    pub fn new(
        repo: gix::Repository,
        signing_key: Vec<u8>,
        pub_key: Vec<u8>,
        alg: coz_rs::Alg,
        pkg: String,
    ) -> Self {
        Self {
            source: GitSource::new(repo),
            signing_key,
            pub_key,
            alg,
            pkg,
        }
    }
}

impl AtomSource for GitRegistry {
    type Entry = GitEntry;
    type Error = GitError;

    async fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error> {
        self.source.resolve(id).await
    }

    async fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error> {
        self.source.discover(query).await
    }
}

impl AtomContent for GitRegistry {
    async fn content(
        &self,
        id: &AtomId,
        dig: &[u8],
    ) -> Result<Option<Vec<ContentEntry>>, Self::Error> {
        self.source.content(id, dig).await
    }
}

impl AtomRegistry for GitRegistry {
    fn claim(&self, id: &AtomId, owner: &[u8]) -> Result<Czd, Self::Error> {
        let repo = self.source.repo();
        let head_oid = repo
            .head_id()
            .map_err(|e| GitError::Init(format!("Failed to resolve HEAD: {}", e)))?
            .detach();

        // 1. Derive anchor and verify it matches the ID
        let derived_anchor_oid = crate::gix_util::derive_anchor(&repo, head_oid)?;
        let expected_anchor = Anchor::new(derived_anchor_oid.as_bytes().to_vec());
        if expected_anchor != *id.anchor() {
            return Err(GitError::InvalidAnchor {
                derived: expected_anchor.to_b64(),
                expected: id.anchor().to_b64(),
            });
        }

        // 2. Determine claim chain parenting
        let claim_ref_name = format!("refs/atom/claims/pub/{}", id.label());
        let parent_oid = repo
            .try_find_reference(&claim_ref_name)?
            .map(|claim_ref| claim_ref.id().detach());

        // 3. Construct ClaimPayload
        let tmb = coz_rs::compute_thumbprint_for_alg(self.alg.name(), &self.pub_key)
            .ok_or_else(|| GitError::Coz("Failed to compute key thumbprint".into()))?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let claim_payload = ClaimPayload::new(
            self.alg,
            id.clone(),
            now,
            owner.to_vec(),
            self.pkg.clone(),
            head_oid.as_bytes().to_vec(),
            tmb,
        );

        // 4. Serialize, sign, and envelope
        let pay_val = serde_json::to_value(&claim_payload)?;
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val)?;
        let pay_bytes = serde_json::to_vec(&pay_map)?;

        let (sig, _cad) = coz_rs::sign_json(
            &pay_bytes,
            self.alg.name(),
            &self.signing_key,
            &self.pub_key,
        )
        .ok_or_else(|| GitError::Coz("Failed to sign claim JSON".into()))?;

        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(self.pub_key.clone()),
        };

        let claim_msg = serde_json::to_string(&envelope)?;

        // 5. Write claim commit
        let new_claim_oid = crate::gix_util::write_claim_commit(&repo, claim_msg, parent_oid)?;

        // 6. Atomically update references using a transaction
        let mut edits = Vec::new();

        let claim_ref_fullname = FullName::try_from(claim_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        edits.push(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Create or update atom identity claim".into(),
                },
                expected: match parent_oid {
                    Some(p) => PreviousValue::MustExistAndMatch(Target::Object(p)),
                    None => PreviousValue::MustNotExist,
                },
                new: Target::Object(new_claim_oid),
            },
            name: claim_ref_fullname,
            deref: false,
        });

        let src_ref_name = format!("refs/atom/src/{}", head_oid.to_hex());
        let src_ref_fullname = FullName::try_from(src_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        edits.push(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Pin claim source revision".into(),
                },
                expected: PreviousValue::Any,
                new: Target::Object(head_oid),
            },
            name: src_ref_fullname,
            deref: false,
        });

        repo.edit_references(edits)?;

        Ok(Czd::from_bytes(new_claim_oid.as_bytes().to_vec()))
    }

    fn publish(
        &self,
        id: &AtomId,
        claim: &Czd,
        version: &RawVersion,
        dig: &[u8],
        src: &[u8],
        path: &str,
    ) -> Result<(), Self::Error> {
        let repo = self.source.repo();

        // 1. Resolve and verify the active claim
        let claim_ref_name = format!("refs/atom/claims/pub/{}", id.label());
        let claim_ref = repo
            .try_find_reference(&claim_ref_name)?
            .ok_or_else(|| GitError::NoActiveClaim(id.label().to_string()))?;
        let claim_oid = claim_ref.id().detach();

        let expected_claim_oid = ObjectId::try_from(claim.as_bytes())
            .map_err(|e| GitError::Validation(format!("Invalid claim object ID: {}", e)))?;
        if claim_oid != expected_claim_oid {
            return Err(GitError::Validation(format!(
                "Active claim mismatch: active is {} but expected {}",
                claim_oid.to_hex(),
                expected_claim_oid.to_hex()
            )));
        }

        // 2. Parse claim payload to obtain claim source revision
        let claim_obj = repo.find_object(claim_oid)?;
        let claim_commit = claim_obj.try_into_commit()?;
        let claim_msg_str = claim_commit.message_raw_sloppy().to_string();

        let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg_str)?;
        let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
        let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
            GitError::Validation("Claim CozMessage is missing the key field".into())
        })?;

        let claim_alg_str = claim_envelope
            .pay
            .get("alg")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GitError::Validation("Claim alg field is missing or invalid".into()))?;

        let claim_payload = atom_id::verify_claim(
            &claim_pay_bytes,
            &claim_envelope.sig,
            claim_alg_str,
            claim_pub_key,
        )?;

        // 3. Verify temporal vector (publish src must be a descendant of claim src)
        let publish_src_oid = ObjectId::try_from(src)
            .map_err(|e| GitError::Validation(format!("Invalid publish source OID: {}", e)))?;
        let claim_src_oid = ObjectId::try_from(claim_payload.src.as_slice())
            .map_err(|e| GitError::Validation(format!("Invalid claim source OID: {}", e)))?;

        if !crate::gix_util::is_descendant(&repo, publish_src_oid, claim_src_oid)? {
            return Err(GitError::InvalidTemporalVector {
                publish_src: publish_src_oid.to_hex().to_string(),
                claim_src: claim_src_oid.to_hex().to_string(),
            });
        }

        // 4. Create the deterministic, parentless atom commit
        let tree_oid = ObjectId::try_from(dig)
            .map_err(|e| GitError::Validation(format!("Invalid tree OID: {}", e)))?;
        let atom_commit_oid =
            crate::gix_util::write_deterministic_commit(&repo, tree_oid, publish_src_oid)?;

        // 5. Determine version reference state and update tag target
        let version_ref_name = format!("refs/atom/pub/{}/{}", id.label(), version.as_str());
        let (target_oid, target_kind, version_ref_constraint) =
            if let Some(version_ref) = repo.try_find_reference(&version_ref_name)? {
                let prev_tag_oid = version_ref.id().detach();
                (
                    prev_tag_oid,
                    gix::object::Kind::Tag,
                    PreviousValue::MustExistAndMatch(Target::Object(prev_tag_oid)),
                )
            } else {
                (
                    atom_commit_oid,
                    gix::object::Kind::Commit,
                    PreviousValue::MustNotExist,
                )
            };

        // 6. Construct PublishPayload
        let tmb = coz_rs::compute_thumbprint_for_alg(self.alg.name(), &self.pub_key)
            .ok_or_else(|| GitError::Coz("Failed to compute key thumbprint".into()))?;

        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Ensure publish timestamp is strictly after claim timestamp
        let now = if current_time <= claim_payload.now {
            claim_payload.now + 1
        } else {
            current_time
        };

        let publish_payload = PublishPayload::new(
            self.alg,
            id.clone(),
            claim.clone(),
            dig.to_vec(),
            now,
            path.to_string(),
            src.to_vec(),
            tmb,
            version.clone(),
        );

        // 7. Serialize, sign, and envelope
        let pay_val = serde_json::to_value(&publish_payload)?;
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val)?;
        let pay_bytes = serde_json::to_vec(&pay_map)?;

        let (sig, _cad) = coz_rs::sign_json(
            &pay_bytes,
            self.alg.name(),
            &self.signing_key,
            &self.pub_key,
        )
        .ok_or_else(|| GitError::Coz("Failed to sign publish JSON".into()))?;

        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(self.pub_key.clone()),
        };

        let publish_msg = serde_json::to_string(&envelope)?;

        // 8. Write publish tag
        let tagger = crate::gix_util::blank_signature();
        let tag_name = format!("{}-{}", id.label(), version.as_str());
        let new_tag_oid = crate::gix_util::write_publish_tag(
            &repo,
            &tag_name,
            target_oid,
            target_kind,
            tagger,
            publish_msg,
        )?;

        // 9. Execute transaction: update version ref, verify claim ref CAS, and pin src
        let mut edits = Vec::new();

        let version_ref_fullname = FullName::try_from(version_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        edits.push(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: format!("Publish version {}", version.as_str()).into(),
                },
                expected: version_ref_constraint,
                new: Target::Object(new_tag_oid),
            },
            name: version_ref_fullname,
            deref: false,
        });

        let claim_ref_fullname = FullName::try_from(claim_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        edits.push(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Claim verification check".into(),
                },
                expected: PreviousValue::MustExistAndMatch(Target::Object(claim_oid)),
                new: Target::Object(claim_oid),
            },
            name: claim_ref_fullname,
            deref: false,
        });

        let src_ref_name = format!("refs/atom/src/{}", publish_src_oid.to_hex());
        let src_ref_fullname = FullName::try_from(src_ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        edits.push(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Pin publish source revision".into(),
                },
                expected: PreviousValue::Any,
                new: Target::Object(publish_src_oid),
            },
            name: src_ref_fullname,
            deref: false,
        });

        repo.edit_references(edits)?;

        Ok(())
    }
}
