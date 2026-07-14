//! Implementation of [`AtomRegistry`] for the Git backend.
//!
//! Provides the write interface for establishing claims and publishing
//! new versions of packages inside a source Git repository.

use std::time::SystemTime;

use atom_core::{
    AtomContent, AtomId, AtomRegistry, AtomSource, ContentEntry, Czd, OwnerRef, RawVersion,
};
#[cfg(test)]
use atom_id::Anchor;
use atom_id::{CharterPayload, ClaimPayload, PublishPayload};
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

/// Parse a charter commit's `CozMessage` body, verify its signature and
/// shape, and recompute its czd from the actual signed bytes.
///
/// Mirrors `charter_store.rs`'s private `parse_and_verify_charter_commit`
/// idiom (never trust a ref key or bare payload as proof of identity) —
/// duplicated here rather than exposed from that module, since
/// `n3-charter-store`'s settled primitives don't include a "resolve any
/// charter, founding or not, by its own czd" operation and this node's
/// surface is `registry.rs` only.
fn parse_and_verify_charter(
    repo: &gix::Repository,
    oid: gix::hash::ObjectId,
) -> Result<(CharterPayload, Czd), GitError> {
    let obj = repo.find_object(oid)?;
    let commit = obj.try_into_commit()?;
    let msg_str = commit.message_raw_sloppy().to_string();

    let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str)?;
    let pay_bytes = serde_json::to_vec(&envelope.pay)?;
    let pub_key = envelope.key.as_ref().ok_or_else(|| {
        GitError::Validation("Charter CozMessage is missing the key field".into())
    })?;
    let alg_str = envelope
        .pay
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GitError::Validation("Charter alg field is missing or invalid".into()))?;

    let computed_czd = atom_id::czd_for_alg(&pay_bytes, &envelope.sig, alg_str)?;
    let payload = atom_id::verify_charter(&pay_bytes, &envelope.sig, alg_str, pub_key)?;

    Ok((payload, computed_czd))
}

/// Parse a claim commit's `CozMessage` body, verify its signature and
/// shape, and recompute its czd from the actual signed bytes.
///
/// Mirrors [`parse_and_verify_charter`] for claims — needed by `claim()`'s
/// replacement path to resolve and authorize against the PRIOR claim,
/// which `publish()`'s own inline claim-parsing block (this file) does not
/// expose as a reusable function.
fn parse_and_verify_claim(
    repo: &gix::Repository,
    oid: gix::hash::ObjectId,
) -> Result<(ClaimPayload, Czd), GitError> {
    let obj = repo.find_object(oid)?;
    let commit = obj.try_into_commit()?;
    let msg_str = commit.message_raw_sloppy().to_string();

    let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str)?;
    let pay_bytes = serde_json::to_vec(&envelope.pay)?;
    let pub_key = envelope
        .key
        .as_ref()
        .ok_or_else(|| GitError::Validation("Claim CozMessage is missing the key field".into()))?;
    let alg_str = envelope
        .pay
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GitError::Validation("Claim alg field is missing or invalid".into()))?;

    let computed_czd = atom_id::czd_for_alg(&pay_bytes, &envelope.sig, alg_str)?;
    let payload = atom_id::verify_claim(&pay_bytes, &envelope.sig, alg_str, pub_key)?;

    Ok((payload, computed_czd))
}

/// Resolve a single charter — founding or successor — directly by its own
/// czd via the shared `refs/atom/charter/d/{czd-hex}` seam
/// ([`crate::charter_store::charter_ref_name`]).
///
/// Unlike [`crate::charter_store::resolve_founding_charter`] (which
/// deliberately rejects any payload carrying a `prior`), this accepts
/// either shape — `charter()`'s successor path needs to resolve an
/// arbitrary `prior` link, not only a founding one. Returns `Ok(None)`
/// when the ref doesn't exist or the stored payload's recomputed czd does
/// not bind to the requested key (the same czd-binding hole every charter
/// resolver in this codebase guards against); a genuine I/O, parse, or
/// signature failure still propagates as `Err`.
fn resolve_charter_by_czd(
    repo: &gix::Repository,
    czd: &Czd,
) -> Result<Option<CharterPayload>, GitError> {
    let ref_name = crate::charter_store::charter_ref_name(czd.as_bytes());
    let Some(reference) = repo.try_find_reference(&ref_name)? else {
        return Ok(None);
    };
    let oid = reference.id().detach();

    let (payload, computed_czd) = parse_and_verify_charter(repo, oid)?;
    if computed_czd != *czd {
        return Ok(None);
    }
    Ok(Some(payload))
}

/// Write-side linear-successor guard (`[charter-succession-linear]`):
/// scan `refs/atom/charter/d/*` for any existing, validly-bound charter
/// that already names `prior` as its own `prior`.
///
/// A narrow, purpose-built scan rather than a reuse of
/// [`crate::charter_store::resolve_effective_charter`]'s full chain walk —
/// this only needs "does anything else already name this exact prior,"
/// not a resolved effective head, and the guard must fire even when
/// `prior` itself is not on the anchor's canonical chain (an isolated
/// fork attempt), which an anchor-scoped effective-chain walk would not
/// necessarily surface.
fn any_successor_names_prior(repo: &gix::Repository, prior: &Czd) -> Result<bool, GitError> {
    let prefix = "refs/atom/charter/d/";
    for ref_res in repo.references()?.prefixed(prefix)? {
        let reference = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
        let ref_name = reference.name().as_bstr().to_string();
        let oid = reference.id().detach();

        let (payload, computed_czd) = parse_and_verify_charter(repo, oid)?;
        // Bind each candidate to its own ref key -- an entry stored under
        // the wrong key is not a legitimate charter and must not
        // participate in the guard.
        if crate::charter_store::charter_ref_name(computed_czd.as_bytes()) != ref_name {
            continue;
        }
        if payload.prior.as_ref() == Some(prior) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Enumerate every claim under the flat `refs/atom/claims/pub/*` family
/// (repo-wide, across every label — `charter()` takes no `id`/`label`
/// parameter, so "pre-existing claims for this source" cannot be scoped
/// to a single label the way `claim()`/`publish()` scope their own ref
/// lookups) and return the one with the smallest `now`.
///
/// This is [`atom_id::verify_bootstrap_gate`]'s required input: per
/// `[charter-transition]` PRE, a founding charter over a source that
/// already carries claims must be authorized by the *earliest* such
/// claim's owner. `now` is the only available ordering signal — charter
/// and claim commits are written via the deterministic, blank-signature
/// idiom (`gix_util::blank_signature`, committer time fixed at zero), so
/// git commit timestamps carry no real chronological information, and
/// claims under different labels share no `prior`-style chain to order
/// them by. Ties (equal `now`) keep the first candidate encountered in
/// ref-iteration order — `now` collisions are not a spec-constrained
/// case and no stronger tiebreak signal exists.
fn find_earliest_claim(repo: &gix::Repository) -> Result<Option<ClaimPayload>, GitError> {
    let prefix = "refs/atom/claims/pub/";
    let mut earliest: Option<ClaimPayload> = None;
    for ref_res in repo.references()?.prefixed(prefix)? {
        let reference = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
        let oid = reference.id().detach();

        let obj = repo.find_object(oid)?;
        let commit = obj.try_into_commit()?;
        let msg_str = commit.message_raw_sloppy().to_string();

        let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str)?;
        let pay_bytes = serde_json::to_vec(&envelope.pay)?;
        let pub_key = envelope.key.as_ref().ok_or_else(|| {
            GitError::Validation("Claim CozMessage is missing the key field".into())
        })?;
        let alg_str = envelope
            .pay
            .get("alg")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GitError::Validation("Claim alg field is missing or invalid".into()))?;

        let claim_payload = atom_id::verify_claim(&pay_bytes, &envelope.sig, alg_str, pub_key)?;

        if earliest
            .as_ref()
            .is_none_or(|cur| claim_payload.now < cur.now)
        {
            earliest = Some(claim_payload);
        }
    }
    Ok(earliest)
}

impl AtomRegistry for GitRegistry {
    fn claim(&self, id: &AtomId, owner: &OwnerRef) -> Result<Czd, Self::Error> {
        let repo = self.source.repo();
        let head_oid = repo
            .head_id()
            .map_err(|e| GitError::Init(format!("Failed to resolve HEAD: {}", e)))?
            .detach();

        // 1. Resolve the effective charter for the ID's anchor.
        // `[anchor-resolvable]`: the anchor is given (in the `AtomId`),
        // never derived from git history -- claiming into a set with no
        // founding charter is a correct rejection, not a derivation
        // failure. Subsumes the old "founding charter exists" check: no
        // effective charter resolves iff no founding charter exists.
        let (effective_charter, _chain) =
            crate::charter_store::resolve_effective_charter(&repo, id.anchor(), None)?.ok_or_else(
                || {
                    GitError::Validation(format!(
                        "no founding charter exists for anchor {}",
                        id.anchor().to_b64()
                    ))
                },
            )?;

        // 2. Determine claim chain parenting
        let claim_ref_name = format!("refs/atom/claims/pub/{}", id.label());
        let parent_oid = repo
            .try_find_reference(&claim_ref_name)?
            .map(|claim_ref| claim_ref.id().detach());

        // 3. Construct ClaimPayload
        let tmb = coz_rs::compute_thumbprint_for_alg(self.alg.name(), &self.pub_key)
            .ok_or_else(|| GitError::Coz("Failed to compute key thumbprint".into()))?;

        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Verification Pipeline step 9 requires charter.now < claim.now
        // strictly; mirror publish()'s own now-bump idiom so a claim
        // authored within the same wall-clock second as its charter does
        // not spuriously fail temporal ordering at resolve time.
        let now = if current_time <= effective_charter.now {
            effective_charter.now + 1
        } else {
            current_time
        };

        // 3b. Construct + authorize, branching on founding vs. replacement.
        // `[claim-charter-authorization]` (founding): the signer must be
        // authorized by the effective charter's owner SET.
        // `[claim-replacement-authority]` (replacement): only the
        // owner-replacement path is reachable through this API -- `claim()`
        // has no `governance` parameter, so `verify_claim_replacement` is
        // called with `governance: false` fixed, which makes its
        // governance-branch unreachable here by construction (not a gap:
        // a governance seizure requires its own, separate entry point).
        let claim_payload = match parent_oid {
            None => {
                let candidate = ClaimPayload::new(
                    self.alg,
                    id.clone(),
                    now,
                    owner.clone(),
                    self.pkg.clone(),
                    head_oid.as_bytes().to_vec(),
                    tmb,
                );
                atom_id::verify_claim_authorized_by_charter(&candidate, &effective_charter)?;
                candidate
            },
            Some(parent) => {
                let (prior_payload, prior_czd) = parse_and_verify_claim(&repo, parent)?;
                // Mirror `publish()`'s own now-bump idiom: a replacement's
                // `now` MUST strictly exceed the prior's
                // (`[claim-replacement-transition]` PRE), and two claims
                // authored within the same wall-clock second would
                // otherwise collide.
                let now = if now <= prior_payload.now {
                    prior_payload.now + 1
                } else {
                    now
                };
                let candidate = ClaimPayload::new_replacement(
                    self.alg,
                    id.clone(),
                    now,
                    owner.clone(),
                    self.pkg.clone(),
                    prior_czd,
                    false, // governance seizure is not reachable through this API
                    head_oid.as_bytes().to_vec(),
                    tmb,
                );
                atom_id::verify_claim_replacement(
                    &candidate,
                    &prior_payload,
                    &effective_charter.owner,
                )?;
                candidate
            },
        };

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

        // The claim's identity is the spec-defined `czd`: the digest of
        // (cad, sig), independently recomputable by any party from the
        // signed message alone. It must never be the git object id the
        // claim commit happens to be stored at — that is a storage
        // accident, not a property of the signed content.
        let czd = atom_id::czd_for_alg(&pay_bytes, &sig, self.alg.name())?;

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

        Ok(czd)
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

        // The caller must name the active claim by its spec-defined czd —
        // the digest of (cad, sig) — never by the git object id the claim
        // commit happens to be stored at. Recompute it from the active
        // claim's own signed bytes and compare.
        let active_czd =
            atom_id::czd_for_alg(&claim_pay_bytes, &claim_envelope.sig, claim_alg_str)?;
        if active_czd != *claim {
            return Err(GitError::Validation(format!(
                "Active claim mismatch: active is {} but expected {}",
                active_czd.to_b64(),
                claim.to_b64()
            )));
        }

        // 3. Verify temporal vector (publish src must be a descendant of claim src)
        let publish_src_oid = crate::gix_util::seam::oid_from_src_field(src)
            .map_err(|e| GitError::Validation(format!("Invalid publish source OID: {}", e)))?;
        let claim_src_oid = crate::gix_util::seam::oid_from_src_field(claim_payload.src.as_slice())
            .map_err(|e| GitError::Validation(format!("Invalid claim source OID: {}", e)))?;

        if !crate::gix_util::is_descendant(&repo, publish_src_oid, claim_src_oid)? {
            return Err(GitError::InvalidTemporalVector {
                publish_src: publish_src_oid.to_hex().to_string(),
                claim_src: claim_src_oid.to_hex().to_string(),
            });
        }

        // 4. Create the deterministic, parentless atom commit
        let tree_oid = crate::gix_util::seam::oid_from_dig_field(dig)
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

    fn charter(
        &self,
        owner: &[OwnerRef],
        src: &[u8],
        prior: Option<&Czd>,
    ) -> Result<Czd, Self::Error> {
        let repo = self.source.repo();

        let tmb = coz_rs::compute_thumbprint_for_alg(self.alg.name(), &self.pub_key)
            .ok_or_else(|| GitError::Coz("Failed to compute key thumbprint".into()))?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let charter_payload = match prior {
            None => {
                // Founding path: the bootstrap gate needs the candidate's
                // own tmb to check against the earliest pre-existing
                // claim's owner (a virgin source passes trivially).
                let candidate = CharterPayload::new(
                    self.alg,
                    now,
                    owner.to_vec(),
                    None,
                    src.to_vec(),
                    tmb.clone(),
                )
                .map_err(|e| GitError::Validation(format!("invalid charter owner set: {e}")))?;
                let earliest_claim = find_earliest_claim(&repo)?;
                atom_id::verify_bootstrap_gate(&candidate, earliest_claim.as_ref())?;
                candidate
            },
            Some(prior_czd) => {
                // Successor path: resolve the prior charter directly by
                // its own czd (not necessarily the anchor -- `prior` may
                // name any link in the chain).
                let prior_payload = resolve_charter_by_czd(&repo, prior_czd)?.ok_or_else(|| {
                    GitError::Validation(format!(
                        "prior charter {} not found or fails czd-binding verification",
                        prior_czd.to_b64()
                    ))
                })?;

                // [charter-succession]: the signer must be authorized by
                // membership in the prior charter's owner SET
                // (`[owner-authorization-delegated]`'s set composition
                // rule) -- the same `owner_set_authorizes` helper
                // `verify_succession_chain`'s own per-link check calls, so
                // the two never drift apart. Applied directly here (rather
                // than via `verify_succession_chain` itself) since
                // `prior_payload` need not itself be the chain's founding
                // charter, which that function requires of `chain[0]`.
                if !atom_id::owner_set_authorizes(&prior_payload.owner, &tmb) {
                    return Err(GitError::Verify(atom_id::VerifyError::Unauthorized));
                }
                if now <= prior_payload.now {
                    return Err(GitError::Validation(format!(
                        "charter succession now ({now}) does not strictly exceed prior charter's \
                         now ({})",
                        prior_payload.now
                    )));
                }

                // Write-side linear-successor guard
                // ([charter-succession-linear]): reject a second successor
                // naming the same prior, mirroring
                // `gix_util::write_chain_append_tag`'s own semantic-
                // immutability enforcement.
                if any_successor_names_prior(&repo, prior_czd)? {
                    return Err(GitError::Validation(format!(
                        "charter {} already has a successor; writing a second successor would \
                         fork set authority",
                        prior_czd.to_b64()
                    )));
                }

                CharterPayload::new(
                    self.alg,
                    now,
                    owner.to_vec(),
                    Some(prior_czd.clone()),
                    src.to_vec(),
                    tmb.clone(),
                )
                .map_err(|e| GitError::Validation(format!("invalid charter owner set: {e}")))?
            },
        };

        // Serialize, sign, and envelope -- mirrors claim()/publish()'s
        // exact idiom.
        let pay_val = serde_json::to_value(&charter_payload)?;
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val)?;
        let pay_bytes = serde_json::to_vec(&pay_map)?;

        let (sig, _cad) = coz_rs::sign_json(
            &pay_bytes,
            self.alg.name(),
            &self.signing_key,
            &self.pub_key,
        )
        .ok_or_else(|| GitError::Coz("Failed to sign charter JSON".into()))?;

        // The charter's identity is the spec-defined czd -- the digest of
        // (cad, sig) -- independently recomputable by any party from the
        // signed message alone, never the git object id the charter
        // commit happens to be stored at.
        let czd = atom_id::czd_for_alg(&pay_bytes, &sig, self.alg.name())?;

        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(self.pub_key.clone()),
        };
        let charter_msg = serde_json::to_string(&envelope)?;

        let new_charter_oid = crate::gix_util::write_charter_commit(&repo, charter_msg)?;

        // Charter refs are write-once, never CAS-rotated the way claim
        // refs are -- each new charter (founding or successor) gets a
        // distinct czd-keyed ref via the shared `charter_ref_name` seam.
        let ref_name = crate::charter_store::charter_ref_name(czd.as_bytes());
        let ref_fullname = FullName::try_from(ref_name.as_str())
            .map_err(|e| GitError::Validation(e.to_string()))?;
        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "Create atom-set charter".into(),
                },
                expected: PreviousValue::MustNotExist,
                new: Target::Object(new_charter_oid),
            },
            name: ref_fullname,
            deref: false,
        })?;

        Ok(czd)
    }
}

#[cfg(test)]
mod charter_tests {
    use atom_core::Label;
    use atom_id::Thumbprint;
    use coz_rs::{Alg, Ed25519, SigningKey};
    use gix::actor::SignatureRef;
    use gix::objs::Tree;
    use tempfile::TempDir;

    use super::*;

    /// Set up a test Git repository with a genesis commit, matching the
    /// established convention (`charter_store.rs::tests::setup_test_repo`).
    fn setup_test_repo() -> (TempDir, gix::Repository) {
        let dir = TempDir::new().unwrap();
        let repo = gix::init(dir.path()).unwrap();

        let sig = SignatureRef::default();
        let empty_tree = Tree {
            entries: Vec::new(),
        };
        let tree_oid = repo.write_object(empty_tree).unwrap().detach();
        repo.commit_as(
            sig,
            sig,
            "refs/heads/master",
            "genesis commit",
            tree_oid,
            Vec::<gix::hash::ObjectId>::new(),
        )
        .unwrap();

        let repo = gix::open(dir.path()).unwrap();
        (dir, repo)
    }

    struct Keypair {
        prv: Vec<u8>,
        pub_key: Vec<u8>,
        tmb: Thumbprint,
    }

    fn gen_keypair() -> Keypair {
        let sk = SigningKey::<Ed25519>::generate();
        let prv = sk.private_key_bytes().to_vec();
        let pub_key = sk.verifying_key().public_key_bytes().to_vec();
        let tmb = coz_rs::compute_thumbprint_for_alg(Alg::Ed25519.name(), &pub_key).unwrap();
        Keypair { prv, pub_key, tmb }
    }

    fn registry_for(repo: &gix::Repository, kp: &Keypair) -> GitRegistry {
        GitRegistry::new(
            gix::open(repo.path()).unwrap(),
            kp.prv.clone(),
            kp.pub_key.clone(),
            Alg::Ed25519,
            "cargo".to_string(),
        )
    }

    /// A single-entry `single-key` owner set from raw bytes -- the common
    /// case throughout these tests, none of which exercise multi-member
    /// sets.
    fn single_owner(bytes: Vec<u8>) -> Vec<OwnerRef> {
        vec![atom_id::OwnerRef::new(atom_id::OwnerKind::SingleKey, bytes)]
    }

    /// Plant a charter directly via the write-side primitive, bypassing
    /// `GitRegistry::charter()` -- used to construct adversarial/synthetic
    /// prior charters (e.g. an artificial `now`) that the real API cannot
    /// produce on demand, matching `charter_store.rs::tests`'s own
    /// `write_and_ref_charter` idiom.
    fn plant_charter(repo: &gix::Repository, payload: &CharterPayload, kp: &Keypair) -> Czd {
        let pay_val = serde_json::to_value(payload).unwrap();
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val).unwrap();
        let pay_bytes = serde_json::to_vec(&pay_map).unwrap();

        let (sig, _cad) = coz_rs::sign_json(&pay_bytes, "Ed25519", &kp.prv, &kp.pub_key).unwrap();
        let czd = atom_id::czd_for_alg(&pay_bytes, &sig, "Ed25519").unwrap();

        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(kp.pub_key.clone()),
        };
        let msg = serde_json::to_string(&envelope).unwrap();

        let oid = crate::gix_util::write_charter_commit(repo, msg).unwrap();
        let ref_name = crate::charter_store::charter_ref_name(czd.as_bytes());
        let fullname = FullName::try_from(ref_name.as_str()).unwrap();
        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "plant charter".into(),
                },
                expected: PreviousValue::MustNotExist,
                new: Target::Object(oid),
            },
            name: fullname,
            deref: false,
        })
        .unwrap();
        czd
    }

    /// Plant an active claim directly at `refs/atom/claims/pub/{label}`,
    /// bypassing `GitRegistry::claim()` -- used to set up the bootstrap
    /// gate's pre-existing-claim scenarios without needing a real anchor
    /// (claims planted this way never go through anchor verification).
    fn plant_claim(
        repo: &gix::Repository,
        label: &str,
        now: u64,
        owner_tmb: &Thumbprint,
        kp: &Keypair,
    ) {
        let id = AtomId::new(Anchor::new(vec![0u8; 4]), Label::try_from(label).unwrap());
        let payload = ClaimPayload::new(
            Alg::Ed25519,
            id,
            now,
            atom_id::OwnerRef::single_key(owner_tmb),
            "cargo".to_string(),
            vec![0u8; 20],
            kp.tmb.clone(),
        );
        let pay_val = serde_json::to_value(&payload).unwrap();
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val).unwrap();
        let pay_bytes = serde_json::to_vec(&pay_map).unwrap();

        let (sig, _cad) = coz_rs::sign_json(&pay_bytes, "Ed25519", &kp.prv, &kp.pub_key).unwrap();
        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(kp.pub_key.clone()),
        };
        let msg = serde_json::to_string(&envelope).unwrap();

        let oid = crate::gix_util::write_claim_commit(repo, msg, None).unwrap();
        let ref_name = format!("refs/atom/claims/pub/{label}");
        let fullname = FullName::try_from(ref_name.as_str()).unwrap();
        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "plant claim".into(),
                },
                expected: PreviousValue::MustNotExist,
                new: Target::Object(oid),
            },
            name: fullname,
            deref: false,
        })
        .unwrap();
    }

    // -------------------------------------------------------------
    // c2: founding path
    // -------------------------------------------------------------

    #[test]
    fn charter_founds_virgin_source() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let registry = registry_for(&repo, &founder);

        let czd = registry
            .charter(&single_owner(founder.pub_key.clone()), b"src-rev", None)
            .expect("founding a virgin source must succeed trivially");

        let ref_name = crate::charter_store::charter_ref_name(czd.as_bytes());
        assert!(
            repo.try_find_reference(&ref_name).unwrap().is_some(),
            "the charter ref must exist after a successful founding"
        );

        let anchor = Anchor::new(czd.as_bytes().to_vec());
        let resolved = crate::charter_store::resolve_founding_charter(&repo, &anchor)
            .unwrap()
            .expect("the founding charter must resolve back through the read-side primitive");
        assert_eq!(resolved.prior, None);
    }

    #[test]
    fn charter_founds_authorized_over_preexisting_claim() {
        let (_dir, repo) = setup_test_repo();
        let incumbent = gen_keypair();
        plant_claim(&repo, "some-label", 500, &incumbent.tmb, &incumbent);

        let registry = registry_for(&repo, &incumbent);
        let result = registry.charter(&single_owner(incumbent.pub_key.clone()), b"src-rev", None);
        assert!(
            result.is_ok(),
            "a founder authorized by the earliest pre-existing claim's owner must succeed: \
             {result:?}"
        );
    }

    #[test]
    fn charter_founds_rejects_unauthorized_founder() {
        let (_dir, repo) = setup_test_repo();
        let incumbent = gen_keypair();
        let stranger = gen_keypair();
        plant_claim(&repo, "some-label", 500, &incumbent.tmb, &incumbent);

        let registry = registry_for(&repo, &stranger);
        let result = registry.charter(&single_owner(stranger.pub_key.clone()), b"src-rev", None);
        assert!(
            matches!(
                result,
                Err(GitError::Verify(atom_id::VerifyError::Unauthorized))
            ),
            "a founder not authorized by the earliest claim's owner must be rejected: {result:?}"
        );
    }

    // -------------------------------------------------------------
    // c3: successor path
    // -------------------------------------------------------------

    #[test]
    fn charter_succeeds_accepts_authorized_later_now() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let successor = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_000,
            single_owner(successor.tmb.as_bytes().to_vec()),
            None,
            b"src-rev".to_vec(),
            founder.tmb.clone(),
        )
        .unwrap();
        let founding_czd = plant_charter(&repo, &founding, &founder);

        let registry = registry_for(&repo, &successor);
        let result = registry.charter(
            &single_owner(successor.pub_key.clone()),
            b"src-rev-2",
            Some(&founding_czd),
        );
        assert!(
            result.is_ok(),
            "a successor authorized by the prior owner with a later now must succeed: {result:?}"
        );

        let successor_czd = result.unwrap();
        let resolved = resolve_charter_by_czd(&repo, &successor_czd)
            .unwrap()
            .expect("the successor charter must resolve back by its own czd");
        assert_eq!(resolved.prior, Some(founding_czd));
    }

    #[test]
    fn charter_succeeds_rejects_unauthorized_signer() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let intended_successor = gen_keypair();
        let stranger = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_000,
            single_owner(intended_successor.tmb.as_bytes().to_vec()),
            None,
            b"src-rev".to_vec(),
            founder.tmb.clone(),
        )
        .unwrap();
        let founding_czd = plant_charter(&repo, &founding, &founder);

        let registry = registry_for(&repo, &stranger);
        let result = registry.charter(
            &single_owner(stranger.pub_key.clone()),
            b"src-rev-2",
            Some(&founding_czd),
        );
        assert!(
            matches!(
                result,
                Err(GitError::Verify(atom_id::VerifyError::Unauthorized))
            ),
            "a successor not signed by the prior's named owner must be rejected: {result:?}"
        );
    }

    #[test]
    fn charter_succeeds_rejects_non_increasing_now() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let successor = gen_keypair();

        // An artificially far-future `now` that any real wall-clock call
        // cannot exceed -- isolates the now-ordering check from the
        // authorization check (which must otherwise pass here).
        let founding = CharterPayload::new(
            Alg::Ed25519,
            9_999_999_999,
            single_owner(successor.tmb.as_bytes().to_vec()),
            None,
            b"src-rev".to_vec(),
            founder.tmb.clone(),
        )
        .unwrap();
        let founding_czd = plant_charter(&repo, &founding, &founder);

        let registry = registry_for(&repo, &successor);
        let result = registry.charter(
            &single_owner(successor.pub_key.clone()),
            b"src-rev-2",
            Some(&founding_czd),
        );
        assert!(
            matches!(result, Err(GitError::Validation(_))),
            "a successor whose now does not strictly exceed the prior's must be rejected: \
             {result:?}"
        );
    }

    // -------------------------------------------------------------
    // c4: linear-successor guard
    // -------------------------------------------------------------

    #[test]
    fn charter_succeeds_rejects_second_successor_to_same_prior() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let successor = gen_keypair();

        // A small, synthetic `now` so both real successor attempts below
        // (real wall-clock time) trivially exceed it -- isolating this
        // test from the c3 now-ordering check.
        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_000,
            single_owner(successor.tmb.as_bytes().to_vec()),
            None,
            b"src-rev".to_vec(),
            founder.tmb.clone(),
        )
        .unwrap();
        let founding_czd = plant_charter(&repo, &founding, &founder);

        let registry = registry_for(&repo, &successor);
        let first = registry.charter(
            &single_owner(successor.pub_key.clone()),
            b"src-rev-2",
            Some(&founding_czd),
        );
        assert!(first.is_ok(), "the first successor must succeed: {first:?}");

        let second = registry.charter(
            &single_owner(successor.pub_key.clone()),
            b"src-rev-3",
            Some(&founding_czd),
        );
        assert!(
            matches!(second, Err(GitError::Validation(_))),
            "a second successor naming the same prior must be rejected at write time: {second:?}"
        );
    }

    // -------------------------------------------------------------
    // a2: end-to-end through the public trait interface alone
    // -------------------------------------------------------------

    #[test]
    fn charter_end_to_end_founding_then_succession_resolves_through_public_api() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let successor = gen_keypair();

        let registry = registry_for(&repo, &founder);
        let founding_czd = registry
            .charter(
                &single_owner(successor.tmb.as_bytes().to_vec()),
                b"src-rev",
                None,
            )
            .expect("founding must succeed");

        // `now` is second-granularity wall-clock time and
        // [charter-succession] requires the successor's `now` to
        // *strictly* exceed the prior's -- without this, founding and
        // succession could land in the same wall-clock second and the
        // real (correct) now-ordering check below would flake.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let registry_successor = registry_for(&repo, &successor);
        let successor_czd = registry_successor
            .charter(
                &single_owner(b"next-owner".to_vec()),
                b"src-rev-2",
                Some(&founding_czd),
            )
            .expect("succession must succeed");

        let anchor = Anchor::new(founding_czd.as_bytes().to_vec());
        let (effective, chain) =
            crate::charter_store::resolve_effective_charter(&repo, &anchor, None)
                .unwrap()
                .expect(
                    "the full founding+succession chain must resolve through n3-charter-store's \
                     read-side primitives, using only the czds returned by the public \
                     AtomRegistry::charter() calls above",
                );
        assert_eq!(chain.len(), 2, "founding + one succession");
        assert_eq!(
            effective.prior,
            Some(founding_czd),
            "the effective head must be the successor charter just written"
        );

        // Confirm the successor czd returned by the public API is exactly
        // the one the read-side chain walk independently resolved to.
        let resolved_via_own_czd = resolve_charter_by_czd(&repo, &successor_czd)
            .unwrap()
            .expect("the successor must also resolve directly by the czd charter() returned");
        assert_eq!(resolved_via_own_czd, effective);
    }
}
