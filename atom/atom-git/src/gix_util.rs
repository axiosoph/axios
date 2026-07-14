use std::collections::{HashSet, VecDeque};

use atom_id::PublishPayload;
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

/// Create a charter commit: empty tree, ALWAYS parentless.
///
/// Mirrors [`write_claim_commit`] exactly except for parenting: a charter
/// commit never has a git parent, even on succession — succession is
/// expressed entirely via the signed `prior` field on the next charter's
/// payload, never via git ancestry (`[charter-succession-via-prior]`).
///
/// Spec constraint: `[charter-commit-format]`.
pub fn write_charter_commit(
    repo: &gix::Repository,
    charter_message: String,
) -> Result<ObjectId, GitError> {
    // Write an empty tree to obtain the correct OID for the active hash algorithm
    let empty_tree = gix::objs::Tree {
        entries: Vec::new(),
    };
    let tree_oid = repo.write_object(empty_tree)?.detach();

    let blank = blank_signature();
    let commit = Commit {
        tree: tree_oid,
        parents: Vec::new().into(),
        author: blank.clone(),
        committer: blank,
        encoding: None,
        message: BString::from(charter_message),
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

/// Append a new publish tag onto an existing tag chain, enforcing
/// write-side semantic-immutability before creating the tag object.
///
/// Wraps [`write_publish_tag`], always targeting `previous_tag_oid` (the
/// existing chain tip) as a [`gix::object::Kind::Tag`] — that is what a
/// chain append always targets (`registry.rs::publish()`'s existing
/// version-ref-exists branch already does this implicitly, but performs
/// no check between the two payloads first). Rejects with
/// [`GitError::Validation`] if `(label, version, dig, src, path)` differ
/// between `previous_payload` and `new_payload`; `mode`, `meta`, `claim`,
/// `tmb`, `now`, and signing-key metadata MAY differ freely (e.g. a
/// `witnessed` -> `reproducible` mode transition on re-publish).
///
/// This closes the write-side half of `[tag-chain-semantic-immutable]`
/// (`docs/specs/git-storage-format.md:758-768`); the read-side half is
/// enforced separately in `source.rs`'s resolution walk.
pub fn write_chain_append_tag(
    repo: &gix::Repository,
    tag_name: &str,
    previous_tag_oid: ObjectId,
    previous_payload: &PublishPayload,
    new_payload: &PublishPayload,
    tagger: Signature,
    publish_message: String,
) -> Result<ObjectId, GitError> {
    if previous_payload.label != new_payload.label
        || previous_payload.version != new_payload.version
        || previous_payload.dig != new_payload.dig
        || previous_payload.src != new_payload.src
        || previous_payload.path != new_payload.path
    {
        return Err(GitError::Validation(format!(
            "Semantic immutability violation: chain-append payload for tag {tag_name} changes an \
             immutable field (label/version/dig/src/path) from the previous tag"
        )));
    }

    write_publish_tag(
        repo,
        tag_name,
        previous_tag_oid,
        gix::object::Kind::Tag,
        tagger,
        publish_message,
    )
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

/// The tip-stability result of re-checking a ref against an OID observed
/// earlier in a resolution walk.
///
/// Genuinely absent from the spec — neither "moved-tip" nor "acquisition
/// warning" has any hit in `docs/specs/atom-transactions.md` or
/// `docs/specs/git-storage-format.md` — so this shape (a signal alongside
/// the resolved value, not a hard error) is a judgment call grounded in
/// what a consumer needs: enough information to know their resolved view
/// may already be stale, without `source.rs`'s read-side resolution
/// failing outright over a race that may be entirely benign (e.g. an
/// unrelated version being published concurrently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipStability {
    /// The ref's tip is unchanged since it was first observed.
    Stable,
    /// The ref's tip moved to a new OID during resolution — the resolved
    /// view built from the OID first observed may already be stale.
    Moved(ObjectId),
    /// The ref no longer exists — it was deleted or force-updated away
    /// during resolution.
    Vanished,
}

/// Re-read `ref_name` and compare its current tip against `observed_tip`,
/// the OID first observed when a resolution walk over this ref began.
pub fn tip_stability(
    repo: &gix::Repository,
    ref_name: &str,
    observed_tip: ObjectId,
) -> Result<TipStability, GitError> {
    match repo.try_find_reference(ref_name)? {
        Some(r) if r.id().detach() == observed_tip => Ok(TipStability::Stable),
        Some(r) => Ok(TipStability::Moved(r.id().detach())),
        None => Ok(TipStability::Vanished),
    }
}

#[cfg(test)]
mod tests {
    use gix::actor::SignatureRef;
    use gix::objs::Tree;
    use tempfile::TempDir;

    use super::*;

    /// Set up a test Git repository with a genesis commit, matching the
    /// established convention (`tests/tag_chain.rs::setup_test_repo`).
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
            Vec::<ObjectId>::new(),
        )
        .unwrap();

        let repo = gix::open(dir.path()).unwrap();
        (dir, repo)
    }

    /// c1: `write_charter_commit` writes a parentless commit with the
    /// well-known empty tree.
    #[test]
    fn write_charter_commit_is_parentless_with_empty_tree() {
        let (_dir, repo) = setup_test_repo();

        let oid = write_charter_commit(&repo, "irrelevant body".to_string())
            .expect("writing a charter commit must succeed");

        let commit = repo.find_object(oid).unwrap().try_into_commit().unwrap();
        assert_eq!(
            commit.parent_ids().count(),
            0,
            "a charter commit must never have a git parent -- succession is via `prior`, not \
             ancestry"
        );

        let tree_oid = commit.tree_id().unwrap().detach();
        let tree = repo.find_object(tree_oid).unwrap().try_into_tree().unwrap();
        assert!(
            tree.iter().next().is_none(),
            "a charter commit must carry the well-known empty tree"
        );
    }
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
