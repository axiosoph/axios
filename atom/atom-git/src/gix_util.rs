use std::collections::{HashSet, VecDeque};

use gix::actor::Signature;
use gix::date::Time;
use gix::hash::ObjectId;
use gix::objs::bstr::BString;
use gix::objs::{Commit, Tag};

use crate::error::GitError;

/// The blank author/committer identity used for deterministic protocol commits.
pub fn blank_signature() -> Signature {
    Signature {
        name: BString::from(""),
        email: BString::from(""),
        time: Time {
            seconds: 0,
            offset: 0,
        },
    }
}

/// Derive the unique anchor from the oldest parentless genesis commit reachable from `src_oid`.
///
/// Follows parent headers in the commit history to find all ancestral roots,
/// selecting the chronologically oldest by committer timestamp.
///
/// Spec constraint: `[anchor-is-genesis]`, `[anchor-oldest-root]`.
pub fn derive_anchor(repo: &gix::Repository, src_oid: ObjectId) -> Result<ObjectId, GitError> {
    let mut roots = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(src_oid);
    visited.insert(src_oid);

    while let Some(oid) = queue.pop_front() {
        let obj = repo.find_object(oid)?;
        let commit = obj.try_into_commit()?;

        let parent_ids: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
        if parent_ids.is_empty() {
            roots.push(oid);
        } else {
            for parent_oid in parent_ids {
                if visited.insert(parent_oid) {
                    queue.push_back(parent_oid);
                }
            }
        }
    }

    if roots.is_empty() {
        return Err(GitError::Validation("No genesis commits found".into()));
    }

    // Select the oldest genesis commit by committer timestamp
    let mut oldest_oid = roots[0];
    let mut oldest_time = u64::MAX;

    for oid in roots {
        let obj = repo.find_object(oid)?;
        let commit = obj.try_into_commit()?;
        let committer = commit.committer()?;
        let decoded_time = committer
            .time()
            .map_err(|e| GitError::Validation(format!("Failed to parse committer time: {}", e)))?;

        let seconds = decoded_time.seconds as u64;
        if seconds < oldest_time {
            oldest_time = seconds;
            oldest_oid = oid;
        }
    }

    Ok(oldest_oid)
}

/// Create a deterministic, parentless commit representing the content snapshot of the atom.
///
/// Fixes timestamps, author, and committer information, and appends the `src` extra header.
///
/// Spec constraint: `[snapshot-deterministic]`, `[snapshot-parentless]`, `[snapshot-src-header]`.
pub fn write_deterministic_commit(
    repo: &gix::Repository,
    tree_oid: ObjectId,
    src_oid: ObjectId,
) -> Result<ObjectId, GitError> {
    let blank = blank_signature();
    let commit = Commit {
        tree: tree_oid,
        parents: Vec::new().into(),
        author: blank.clone(),
        committer: blank,
        encoding: None,
        message: BString::from(""),
        extra_headers: vec![(
            BString::from("src"),
            BString::from(src_oid.to_hex().to_string()),
        )],
    };

    let oid = repo.write_object(commit)?.detach();
    Ok(oid)
}

/// Create a claim commit in the registry's claim history.
///
/// If `parent_oid` is provided, it is set as the parent of this claim commit,
/// establishing the update/rotation audit chain.
///
/// Spec constraint: `[claim-detached]`, `[claim-message-is-coz]`, `[no-non-empty-claim]`.
pub fn write_claim_commit(
    repo: &gix::Repository,
    claim_message: String,
    parent_oid: Option<ObjectId>,
) -> Result<ObjectId, GitError> {
    // Write an empty tree to obtain the correct OID for the active hash algorithm
    let empty_tree = gix::objs::Tree {
        entries: Vec::new(),
    };
    let tree_oid = repo.write_object(empty_tree)?.detach();

    let blank = blank_signature();
    let parents = match parent_oid {
        Some(p) => vec![p],
        None => Vec::new(),
    };

    let commit = Commit {
        tree: tree_oid,
        parents: parents.into(),
        author: blank.clone(),
        committer: blank,
        encoding: None,
        message: BString::from(claim_message),
        extra_headers: Vec::new(),
    };

    let oid = repo.write_object(commit)?.detach();
    Ok(oid)
}

/// Create an annotated tag object carrying a signed publish transaction payload.
///
/// Targets either the atom commit (for initial publish) or the previous publish tag (for updates).
///
/// Spec constraint: `[publish-tag-targets-correct]`, `[publish-tag-message-is-coz]`.
pub fn write_publish_tag(
    repo: &gix::Repository,
    tag_name: &str,
    target_oid: ObjectId,
    target_kind: gix::object::Kind,
    tagger: Signature,
    publish_message: String,
) -> Result<ObjectId, GitError> {
    let tag = Tag {
        target: target_oid,
        target_kind,
        name: BString::from(tag_name),
        tagger: Some(tagger),
        message: BString::from(publish_message),
        pgp_signature: None,
    };

    let oid = repo.write_object(tag)?.detach();
    Ok(oid)
}

/// Walk the parent chain of a commit lineage to assert descendants.
///
/// Returns `true` if `descendant` is at or after `ancestor` in Git history.
pub fn is_descendant(
    repo: &gix::Repository,
    descendant: ObjectId,
    ancestor: ObjectId,
) -> Result<bool, GitError> {
    if descendant == ancestor {
        return Ok(true);
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(descendant);
    visited.insert(descendant);

    while let Some(oid) = queue.pop_front() {
        if oid == ancestor {
            return Ok(true);
        }
        let obj = repo.find_object(oid)?;
        let commit = obj.try_into_commit()?;
        for parent in commit.parent_ids() {
            let parent_oid = parent.detach();
            if visited.insert(parent_oid) {
                queue.push_back(parent_oid);
            }
        }
    }
    Ok(false)
}

/// Typed boundary between protocol content digests and git `ObjectId`s.
///
/// `[backend-seam-typed]` (`docs/specs/atom-backend-contract.md`): `Czd`
/// and `OID` are disjoint sorts — no implicit conversion between them is
/// permitted. The OID-sorted protocol surfaces are exactly the `dig` and
/// `src` fields of transaction payloads (plus the carrier-level `src`
/// extra header written by [`write_deterministic_commit`] and the
/// OID-rendering ref-path segment families); `anchor`, `czd`,
/// `publish_czd`, `claim`, `tmb`, and `owner` are NEVER OID-sorted. The
/// three constructors below are the only place in `atom-git` permitted
/// to call `ObjectId::try_from` on protocol-payload bytes — every other
/// call site MUST route through one of them, naming which sort it
/// asserts of its input.
pub mod seam {
    use gix::hash::{Error, ObjectId};

    /// Interpret a transaction payload's `src` field as a git [`ObjectId`].
    ///
    /// `src` is one of the two OID-sorted protocol surfaces named by
    /// `[backend-seam-typed]`, so this conversion is LEGAL. Rejects any
    /// input whose length does not match a supported git hash; never
    /// panics.
    pub fn oid_from_src_field(src: &[u8]) -> Result<ObjectId, Error> {
        ObjectId::try_from(src)
    }

    /// Interpret a transaction payload's `dig` field as a git [`ObjectId`].
    ///
    /// `dig` is the other OID-sorted protocol surface named by
    /// `[backend-seam-typed]`, so this conversion is LEGAL. Rejects any
    /// input whose length does not match a supported git hash; never
    /// panics.
    pub fn oid_from_dig_field(dig: &[u8]) -> Result<ObjectId, Error> {
        ObjectId::try_from(dig)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // atom-git compiles gix with both `sha1` and `sha256` (see
        // atom-git/Cargo.toml), so `ObjectId::try_from` legitimately
        // accepts either a 20-byte (sha1) or 32-byte (sha256) input — the
        // repository's configured object format decides which. Neither
        // length is "wrong"; only lengths matching NEITHER hash are.
        const VALID_OID_LENS: [usize; 2] = [20, 32];
        const INVALID_LENS: [usize; 7] = [0, 1, 19, 21, 33, 47, 64];

        #[test]
        fn oid_from_src_field_accepts_valid_oid_lengths() {
            for len in VALID_OID_LENS {
                let bytes = vec![0u8; len];
                assert!(
                    oid_from_src_field(&bytes).is_ok(),
                    "expected Ok for {len}-byte input"
                );
            }
        }

        #[test]
        fn oid_from_src_field_rejects_wrong_length() {
            for len in INVALID_LENS {
                let bytes = vec![0u8; len];
                assert!(
                    oid_from_src_field(&bytes).is_err(),
                    "expected Err for {len}-byte input"
                );
            }
        }

        #[test]
        fn oid_from_dig_field_accepts_valid_oid_lengths() {
            for len in VALID_OID_LENS {
                let bytes = vec![0u8; len];
                assert!(
                    oid_from_dig_field(&bytes).is_ok(),
                    "expected Ok for {len}-byte input"
                );
            }
        }

        #[test]
        fn oid_from_dig_field_rejects_wrong_length() {
            for len in INVALID_LENS {
                let bytes = vec![0u8; len];
                assert!(
                    oid_from_dig_field(&bytes).is_err(),
                    "expected Err for {len}-byte input"
                );
            }
        }

        #[test]
        fn constructors_never_panic_on_empty_input() {
            assert!(oid_from_src_field(&[]).is_err());
            assert!(oid_from_dig_field(&[]).is_err());
        }
    }
}
