//! Implementations of [`AtomSource`] and observation types.

use atom_core::{AtomContent, AtomId, AtomSource, ContentEntry, RawVersion};
use atom_id::{ClaimPayload, PublishPayload};
use coz_rs::Czd;
use gix::hash::ObjectId;
use serde::{Deserialize, Serialize};

use crate::error::GitError;
use crate::gix_util::is_descendant;

/// Opaque module for custom base64 serialization.
mod option_b64 {
    use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        opt: &Option<Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match opt {
            Some(bytes) => serializer.serialize_str(&Base64UrlUnpadded::encode_string(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => Base64UrlUnpadded::decode_vec(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// JSON envelope for a signed Coz message.
///
/// Contains the payload, signature, and optional public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CozMessageEnvelope {
    /// The payload parsed as an ordered map of JSON values to preserve key order.
    pub pay: indexmap::IndexMap<String, serde_json::Value>,
    /// The signature bytes (base64url-encoded in JSON).
    #[serde(with = "coz_rs::b64")]
    pub sig: Vec<u8>,
    /// The optional public key (base64url-encoded in JSON).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "option_b64")]
    pub key: Option<Vec<u8>>,
}

/// Resolved version entry returned by a [`GitSource`].
#[derive(Debug, Clone)]
pub struct GitVersionEntry {
    /// Opaque version string.
    pub version: RawVersion,
    /// Git commit OID representing the content snapshot (`dig`).
    pub dig: Vec<u8>,
    /// The coz digest of the authorizing claim, if signed.
    pub czd: Option<Czd>,
    /// Deserialized claim payload, if signed.
    pub claim_payload: Option<ClaimPayload>,
    /// Raw claim CozMessage JSON envelope, if signed.
    pub claim_msg: Option<String>,
    /// Raw claim signature bytes, if signed.
    pub claim_sig: Option<Vec<u8>>,
    /// Raw claim public key bytes, if signed.
    pub claim_pubkey: Option<Vec<u8>>,
    /// Deserialized publish payload, if signed.
    pub publish_payload: Option<PublishPayload>,
    /// Raw publish CozMessage JSON envelope, if signed.
    pub publish_msg: Option<String>,
    /// Raw publish signature bytes, if signed.
    pub publish_sig: Option<Vec<u8>>,
    /// Raw publish public key bytes, if signed.
    pub publish_pubkey: Option<Vec<u8>>,
}

/// Resolved atom entry returned by a [`GitSource`].
#[derive(Debug, Clone)]
pub struct GitEntry {
    /// The unique identity of the atom.
    pub id: AtomId,
    /// All resolved versions of the atom.
    pub versions: Vec<GitVersionEntry>,
}

/// Read-only observation of a Git-backed Atom registry or store.
pub struct GitSource {
    /// The underlying Git repository.
    pub repo_ts: gix::ThreadSafeRepository,
}

impl GitSource {
    /// Create a new `GitSource` wrapping a Git repository.
    pub fn new(repo: gix::Repository) -> Self {
        Self {
            repo_ts: repo.into_sync(),
        }
    }

    /// Return a thread-local Repository handle.
    pub fn repo(&self) -> gix::Repository {
        self.repo_ts.to_thread_local()
    }
}

impl AtomSource for GitSource {
    type Entry = GitEntry;
    type Error = GitError;

    async fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error> {
        let repo = self.repo();
        let mut versions = Vec::new();

        // 1. REGISTRY RESOLUTION: refs/atom/pub/{label}/{version}
        // First check for active claim for this label
        let claim_ref_name = format!("refs/atom/claims/pub/{}", id.label());
        if let Some(claim_ref) = repo.try_find_reference(&claim_ref_name)? {
            let claim_oid = claim_ref.id().detach();
            let claim_obj = repo.find_object(claim_oid)?;
            let claim_commit = claim_obj.try_into_commit()?;
            let claim_msg_str = claim_commit.message_raw_sloppy().to_string();

            // Parse claim envelope
            let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg_str)?;
            let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
            let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
                GitError::Validation("Claim CozMessage is missing the key field".into())
            })?;

            // Verify claim signature
            let alg_str = claim_envelope
                .pay
                .get("alg")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    GitError::Validation("Claim alg field is missing or invalid".into())
                })?;

            let claim_payload = atom_id::verify_claim(
                &claim_pay_bytes,
                &claim_envelope.sig,
                alg_str,
                claim_pub_key,
            )?;

            // Verify that anchor matches
            if claim_payload.anchor == *id.anchor() {
                // Find all version refs in refs/atom/pub/{label}/*
                let prefix_str = format!("refs/atom/pub/{}/", id.label());
                let references = repo.references()?;
                for ref_res in references.prefixed(prefix_str.as_str())? {
                    let reference = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
                    let ref_name = reference.name().as_bstr().to_string();
                    let version_str = ref_name.strip_prefix(&prefix_str).unwrap_or("");
                    if version_str.is_empty() {
                        continue;
                    }

                    // Walk the publish tag chain starting from the reference OID
                    let mut current_oid = reference.id().detach();
                    let mut tag_messages = Vec::new();

                    let dig_oid = loop {
                        let obj = repo.find_object(current_oid)?;
                        match obj.kind {
                            gix::object::Kind::Tag => {
                                let tag = obj.try_into_tag()?;
                                let tag_decoded = tag.decode()?;
                                tag_messages.push((current_oid, tag_decoded.message.to_string()));
                                current_oid = tag.target_id()?.detach();
                            },
                            gix::object::Kind::Commit => {
                                break current_oid;
                            },
                            _ => {
                                return Err(GitError::Validation(format!(
                                    "Invalid object kind {} in tag chain for version {}",
                                    obj.kind, version_str
                                )));
                            },
                        }
                    };

                    // Verify that the atom commit has the src header matching publish
                    let atom_obj = repo.find_object(dig_oid)?;
                    let atom_commit = atom_obj.try_into_commit()?;
                    let atom_decoded = atom_commit.decode()?;
                    let atom_src_val = atom_decoded
                        .extra_headers
                        .iter()
                        .find(|(k, _)| *k == "src")
                        .map(|(_, v)| v.to_string())
                        .ok_or_else(|| {
                            GitError::Validation(format!(
                                "Atom commit {} is missing 'src' header",
                                dig_oid
                            ))
                        })?;
                    let atom_src_oid =
                        ObjectId::from_hex(atom_src_val.as_bytes()).map_err(|e| {
                            GitError::Validation(format!("Invalid src header OID: {}", e))
                        })?;

                    // Verify the publish coz messages (from tip to oldest)
                    let mut prev_publish_payload: Option<PublishPayload> = None;
                    let mut tip_publish_msg = None;
                    let mut tip_publish_sig = None;
                    let mut tip_publish_pubkey = None;

                    for (_tag_oid, msg_str) in &tag_messages {
                        let pub_envelope: CozMessageEnvelope = serde_json::from_str(msg_str)?;
                        let pub_pay_bytes = serde_json::to_vec(&pub_envelope.pay)?;

                        // Use public key in publish tag if present, otherwise fall back to claim
                        // key
                        let pub_key_bytes = pub_envelope.key.as_ref().unwrap_or(claim_pub_key);

                        let pub_alg_str = pub_envelope
                            .pay
                            .get("alg")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                GitError::Validation(
                                    "Publish alg field is missing or invalid".into(),
                                )
                            })?;

                        let pub_payload = atom_id::verify_publish(
                            &pub_pay_bytes,
                            &pub_envelope.sig,
                            pub_alg_str,
                            pub_key_bytes,
                        )?;

                        // Invariant [tag-chain-semantic-immutable]: immutable fields must match
                        // across updates
                        if prev_publish_payload.as_ref().is_some_and(|prev| {
                            pub_payload.label != prev.label
                                || pub_payload.version != prev.version
                                || pub_payload.dig != prev.dig
                                || pub_payload.src != prev.src
                                || pub_payload.path != prev.path
                        }) {
                            return Err(GitError::Validation(
                                "Semantic immutability violation in update tag chain".into(),
                            ));
                        }

                        // Validate that publish src matches the atom commit extra header
                        let pub_src_oid =
                            ObjectId::try_from(pub_payload.src.as_slice()).map_err(|e| {
                                GitError::Validation(format!("Invalid publish source OID: {}", e))
                            })?;
                        if pub_src_oid != atom_src_oid {
                            return Err(GitError::Validation(
                                "Publish payload src does not match atom commit extra header"
                                    .into(),
                            ));
                        }

                        // Check temporal vector: publish src must be descendant of claim src
                        let claim_src_oid = ObjectId::try_from(claim_payload.src.as_slice())
                            .map_err(|e| {
                                GitError::Validation(format!("Invalid claim source OID: {}", e))
                            })?;
                        if !is_descendant(&repo, pub_src_oid, claim_src_oid)? {
                            return Err(GitError::InvalidTemporalVector {
                                publish_src: pub_src_oid.to_hex().to_string(),
                                claim_src: claim_src_oid.to_hex().to_string(),
                            });
                        }

                        if prev_publish_payload.is_none() {
                            prev_publish_payload = Some(pub_payload.clone());
                            tip_publish_msg = Some(msg_str.clone());
                            tip_publish_sig = Some(pub_envelope.sig.clone());
                            tip_publish_pubkey = Some(pub_key_bytes.clone());
                        }
                    }

                    let czd = Czd::from_bytes(claim_oid.as_bytes().to_vec());

                    let pub_payload = prev_publish_payload.ok_or_else(|| {
                        GitError::Validation(format!(
                            "No publish tag found for version {}",
                            version_str
                        ))
                    })?;

                    versions.push(GitVersionEntry {
                        version: RawVersion::new(version_str.to_string()),
                        dig: dig_oid.as_bytes().to_vec(),
                        czd: Some(czd),
                        claim_payload: Some(claim_payload.clone()),
                        claim_msg: Some(claim_msg_str.clone()),
                        claim_sig: Some(claim_envelope.sig.clone()),
                        claim_pubkey: Some(claim_pub_key.clone()),
                        publish_payload: Some(pub_payload),
                        publish_msg: tip_publish_msg,
                        publish_sig: tip_publish_sig,
                        publish_pubkey: tip_publish_pubkey,
                    });
                }
            }
        }

        // 2. STORE RESOLUTION: refs/atom/claims/d/{claim_czd} and refs/atom/d/{claim_czd}/{version}
        // Scan all claims in refs/atom/claims/d/* to find matching (anchor, label)
        let store_claims_prefix = "refs/atom/claims/d/";
        let references = repo.references()?;
        for ref_res in references.prefixed(store_claims_prefix)? {
            let claim_ref = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
            let ref_name = claim_ref.name().as_bstr().to_string();
            let claim_czd_hex = ref_name.strip_prefix(store_claims_prefix).unwrap_or("");
            if claim_czd_hex.is_empty() {
                continue;
            }

            let claim_oid = claim_ref.id().detach();
            let claim_obj = repo.find_object(claim_oid)?;
            let claim_commit = claim_obj.try_into_commit()?;
            let claim_msg_str = claim_commit.message_raw_sloppy().to_string();

            // Parse claim envelope
            let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg_str)?;
            let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
            let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
                GitError::Validation("Claim CozMessage is missing the key field".into())
            })?;

            let alg_str = claim_envelope
                .pay
                .get("alg")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    GitError::Validation("Claim alg field is missing or invalid".into())
                })?;

            let claim_payload = atom_id::verify_claim(
                &claim_pay_bytes,
                &claim_envelope.sig,
                alg_str,
                claim_pub_key,
            )?;

            // If the claim matches this anchor and label
            if claim_payload.anchor == *id.anchor() && claim_payload.label == *id.label() {
                // Find all version refs under refs/atom/d/{claim_czd_hex}/*
                let prefix_str = format!("refs/atom/d/{}/", claim_czd_hex);
                let refs_iter = repo.references()?;
                for v_ref_res in refs_iter.prefixed(prefix_str.as_str())? {
                    let v_ref = v_ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
                    let v_ref_name = v_ref.name().as_bstr().to_string();
                    let version_str = v_ref_name.strip_prefix(&prefix_str).unwrap_or("");
                    if version_str.is_empty() {
                        continue;
                    }

                    // Walk the publish tag chain starting from the reference OID
                    let mut current_oid = v_ref.id().detach();
                    let mut tag_messages = Vec::new();

                    let dig_oid = loop {
                        let obj = repo.find_object(current_oid)?;
                        match obj.kind {
                            gix::object::Kind::Tag => {
                                let tag = obj.try_into_tag()?;
                                let tag_decoded = tag.decode()?;
                                tag_messages.push((current_oid, tag_decoded.message.to_string()));
                                current_oid = tag.target_id()?.detach();
                            },
                            gix::object::Kind::Commit => {
                                break current_oid;
                            },
                            _ => {
                                return Err(GitError::Validation(format!(
                                    "Invalid object kind {} in tag chain for version {}",
                                    obj.kind, version_str
                                )));
                            },
                        }
                    };

                    // Verify that the atom commit has the src header matching publish
                    let atom_obj = repo.find_object(dig_oid)?;
                    let atom_commit = atom_obj.try_into_commit()?;
                    let atom_decoded = atom_commit.decode()?;
                    let atom_src_val = atom_decoded
                        .extra_headers
                        .iter()
                        .find(|(k, _)| *k == "src")
                        .map(|(_, v)| v.to_string())
                        .ok_or_else(|| {
                            GitError::Validation(format!(
                                "Atom commit {} is missing 'src' header",
                                dig_oid
                            ))
                        })?;
                    let atom_src_oid =
                        ObjectId::from_hex(atom_src_val.as_bytes()).map_err(|e| {
                            GitError::Validation(format!("Invalid src header OID: {}", e))
                        })?;

                    // Verify the publish coz messages (from tip to oldest)
                    let mut prev_publish_payload: Option<PublishPayload> = None;
                    let mut tip_publish_msg = None;
                    let mut tip_publish_sig = None;
                    let mut tip_publish_pubkey = None;

                    for (_tag_oid, msg_str) in &tag_messages {
                        let pub_envelope: CozMessageEnvelope = serde_json::from_str(msg_str)?;
                        let pub_pay_bytes = serde_json::to_vec(&pub_envelope.pay)?;

                        // Use public key in publish tag if present, otherwise fall back to claim
                        // key
                        let pub_key_bytes = pub_envelope.key.as_ref().unwrap_or(claim_pub_key);

                        let pub_alg_str = pub_envelope
                            .pay
                            .get("alg")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                GitError::Validation(
                                    "Publish alg field is missing or invalid".into(),
                                )
                            })?;

                        let pub_payload = atom_id::verify_publish(
                            &pub_pay_bytes,
                            &pub_envelope.sig,
                            pub_alg_str,
                            pub_key_bytes,
                        )?;

                        // Invariant [tag-chain-semantic-immutable]: immutable fields must match
                        // across updates
                        if prev_publish_payload.as_ref().is_some_and(|prev| {
                            pub_payload.label != prev.label
                                || pub_payload.version != prev.version
                                || pub_payload.dig != prev.dig
                                || pub_payload.src != prev.src
                                || pub_payload.path != prev.path
                        }) {
                            return Err(GitError::Validation(
                                "Semantic immutability violation in update tag chain".into(),
                            ));
                        }

                        // Validate that publish src matches the atom commit extra header
                        let pub_src_oid =
                            ObjectId::try_from(pub_payload.src.as_slice()).map_err(|e| {
                                GitError::Validation(format!("Invalid publish source OID: {}", e))
                            })?;
                        if pub_src_oid != atom_src_oid {
                            return Err(GitError::Validation(
                                "Publish payload src does not match atom commit extra header"
                                    .into(),
                            ));
                        }

                        // Check temporal vector: publish src must be descendant of claim src
                        // NOTE: Store resolution does not verify descendant relation using graph
                        // traversal as the development history is not
                        // present in the store repository.

                        if prev_publish_payload.is_none() {
                            prev_publish_payload = Some(pub_payload.clone());
                            tip_publish_msg = Some(msg_str.clone());
                            tip_publish_sig = Some(pub_envelope.sig.clone());
                            tip_publish_pubkey = Some(pub_key_bytes.clone());
                        }
                    }

                    let claim_czd_oid = ObjectId::from_hex(claim_czd_hex.as_bytes())
                        .map_err(|e| GitError::Validation(e.to_string()))?;
                    let czd = Czd::from_bytes(claim_czd_oid.as_bytes().to_vec());

                    let pub_payload = prev_publish_payload.ok_or_else(|| {
                        GitError::Validation(format!(
                            "No publish tag found for version {}",
                            version_str
                        ))
                    })?;

                    versions.push(GitVersionEntry {
                        version: RawVersion::new(version_str.to_string()),
                        dig: dig_oid.as_bytes().to_vec(),
                        czd: Some(czd),
                        claim_payload: Some(claim_payload.clone()),
                        claim_msg: Some(claim_msg_str.clone()),
                        claim_sig: Some(claim_envelope.sig.clone()),
                        claim_pubkey: Some(claim_pub_key.clone()),
                        publish_payload: Some(pub_payload),
                        publish_msg: tip_publish_msg,
                        publish_sig: tip_publish_sig,
                        publish_pubkey: tip_publish_pubkey,
                    });
                }
            }
        }

        // 3. DEV RESOLUTION: refs/atom/dev/{atom_digest}/{dev_version}
        // Scan for dev versions for each of the 4 supported algorithms
        for alg in [
            coz_rs::Alg::ES256,
            coz_rs::Alg::ES384,
            coz_rs::Alg::ES512,
            coz_rs::Alg::Ed25519,
        ] {
            if let Some(digest) = atom_core::AtomDigest::compute(id, alg) {
                let digest_str = digest.to_string();
                let dev_prefix = format!("refs/atom/dev/{}/", digest_str);
                let refs_iter = repo.references()?;
                for dev_ref_res in refs_iter.prefixed(dev_prefix.as_str())? {
                    let dev_ref = dev_ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
                    let dev_ref_name = dev_ref.name().as_bstr().to_string();
                    let dev_version_str = dev_ref_name.strip_prefix(&dev_prefix).unwrap_or("");
                    if dev_version_str.is_empty() {
                        continue;
                    }

                    // Dev refs point directly to atom commit
                    let atom_oid = dev_ref.id().detach();

                    versions.push(GitVersionEntry {
                        version: RawVersion::new(dev_version_str.to_string()),
                        dig: atom_oid.as_bytes().to_vec(),
                        czd: None,
                        claim_payload: None,
                        claim_msg: None,
                        claim_sig: None,
                        claim_pubkey: None,
                        publish_payload: None,
                        publish_msg: None,
                        publish_sig: None,
                        publish_pubkey: None,
                    });
                }
            }
        }

        if versions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GitEntry {
                id: id.clone(),
                versions,
            }))
        }
    }

    async fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error> {
        let repo = self.repo();
        let mut ids = indexmap::IndexSet::new();

        // 1. Scan registry claims: refs/atom/claims/pub/{label}
        let claims_prefix = "refs/atom/claims/pub/";
        let references = repo.references()?;
        for ref_res in references.prefixed(claims_prefix)? {
            let claim_ref = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
            let ref_name = claim_ref.name().as_bstr().to_string();
            let label_str = ref_name.strip_prefix(claims_prefix).unwrap_or("");
            if label_str.is_empty() || !label_str.contains(query) {
                continue;
            }

            let claim_oid = claim_ref.id().detach();
            let claim_obj = repo.find_object(claim_oid)?;
            let claim_commit = claim_obj.try_into_commit()?;
            let claim_msg_str = claim_commit.message_raw_sloppy().to_string();

            let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg_str)?;
            let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
            let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
                GitError::Validation("Claim CozMessage is missing the key field".into())
            })?;
            let alg_str = claim_envelope
                .pay
                .get("alg")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    GitError::Validation("Claim alg field is missing or invalid".into())
                })?;

            let claim_payload = atom_id::verify_claim(
                &claim_pay_bytes,
                &claim_envelope.sig,
                alg_str,
                claim_pub_key,
            )?;
            ids.insert(AtomId::new(claim_payload.anchor, claim_payload.label));
        }

        // 2. Scan store claims: refs/atom/claims/d/{claim_czd}
        let store_claims_prefix = "refs/atom/claims/d/";
        let refs_iter = repo.references()?;
        for ref_res in refs_iter.prefixed(store_claims_prefix)? {
            let claim_ref = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
            let ref_name = claim_ref.name().as_bstr().to_string();
            let claim_czd_hex = ref_name.strip_prefix(store_claims_prefix).unwrap_or("");
            if claim_czd_hex.is_empty() {
                continue;
            }

            let claim_oid = claim_ref.id().detach();
            let claim_obj = repo.find_object(claim_oid)?;
            let claim_commit = claim_obj.try_into_commit()?;
            let claim_msg_str = claim_commit.message_raw_sloppy().to_string();

            let claim_envelope: CozMessageEnvelope = serde_json::from_str(&claim_msg_str)?;
            let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay)?;
            let claim_pub_key = claim_envelope.key.as_ref().ok_or_else(|| {
                GitError::Validation("Claim CozMessage is missing the key field".into())
            })?;
            let alg_str = claim_envelope
                .pay
                .get("alg")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    GitError::Validation("Claim alg field is missing or invalid".into())
                })?;

            let claim_payload = atom_id::verify_claim(
                &claim_pay_bytes,
                &claim_envelope.sig,
                alg_str,
                claim_pub_key,
            )?;
            if claim_payload.label.contains(query) {
                ids.insert(AtomId::new(claim_payload.anchor, claim_payload.label));
            }
        }

        Ok(ids.into_iter().collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl atom_core::AtomEntry for GitEntry {
    type Version = GitVersionEntry;
    type VersionIter<'a>
        = std::slice::Iter<'a, GitVersionEntry>
    where
        Self: 'a;

    fn id(&self) -> &AtomId {
        &self.id
    }

    fn versions(&self) -> Self::VersionIter<'_> {
        self.versions.iter()
    }
}

impl atom_core::AtomVersion for GitVersionEntry {
    fn version(&self) -> &RawVersion {
        &self.version
    }

    fn dig(&self) -> &[u8] {
        &self.dig
    }

    fn czd(&self) -> Option<&Czd> {
        self.czd.as_ref()
    }

    fn claim_msg(&self) -> Option<&str> {
        self.claim_msg.as_deref()
    }

    fn publish_msg(&self) -> Option<&str> {
        self.publish_msg.as_deref()
    }
}

impl AtomContent for GitSource {
    async fn content(
        &self,
        _id: &AtomId,
        _dig: &[u8],
    ) -> Result<Option<Vec<ContentEntry>>, Self::Error> {
        Ok(None)
    }
}
