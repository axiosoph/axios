use std::fs;

use atom_core::{
    AtomContent, AtomEntry, AtomId, AtomRegistry, AtomSource, AtomStore, AtomVersion, ContentEntry,
    Label, RawVersion,
};
use atom_git::{GitError, GitRegistry, GitSource, GitStore};
use coz_rs::{Alg, Ed25519, SigningKey};
use gix::actor::SignatureRef;
use gix::hash::ObjectId;
use gix::objs::tree::{Entry, EntryKind};
use gix::objs::{Blob, Tree};
use tempfile::TempDir;

/// Helper to set up a test Git repository with user config and a genesis commit
fn setup_test_repo() -> (TempDir, gix::Repository, ObjectId) {
    let dir = TempDir::new().unwrap();
    let repo = gix::init(dir.path()).unwrap();

    let sig = SignatureRef::default();
    let empty_tree = Tree {
        entries: Vec::new(),
    };
    let tree_oid = repo.write_object(empty_tree).unwrap().detach();

    let genesis_oid = repo
        .commit_as(
            sig,
            sig,
            "refs/heads/master",
            "genesis commit",
            tree_oid,
            Vec::<ObjectId>::new(),
        )
        .unwrap()
        .detach();

    // Re-open repo
    let repo = gix::open(dir.path()).unwrap();

    (dir, repo, genesis_oid)
}

/// Independently recompute a signed Coz message's real `czd` from its raw
/// JSON envelope — a code path deliberately separate from
/// `atom_id::czd_for_alg` (the production helper under test), used to prove
/// that atom-git returns the spec-defined digest of `(cad, sig)`, not the
/// git object id the message happens to be stored at.
fn independent_ed25519_czd(msg_str: &str) -> coz_rs::Czd {
    let envelope: atom_git::source::CozMessageEnvelope = serde_json::from_str(msg_str).unwrap();
    let pay_bytes = serde_json::to_vec(&envelope.pay).unwrap();
    let cad = coz_rs::canonical_hash::<coz_rs::Ed25519>(&pay_bytes, None).unwrap();
    coz_rs::Czd::compute::<coz_rs::Ed25519>(&cad, &envelope.sig)
}

/// Helper to create a commit with a single file to simulate workspace changes
fn create_commit(
    repo: &gix::Repository,
    message: &str,
    file_name: &str,
    file_content: &[u8],
    parents: Vec<ObjectId>,
) -> ObjectId {
    let blob_oid = repo
        .write_object(Blob {
            data: file_content.to_vec(),
        })
        .unwrap()
        .detach();

    let parts: Vec<&str> = file_name.split('/').collect();
    let mut current_oid = blob_oid;

    for i in (0..parts.len()).rev() {
        let name = parts[i];
        let mode = if i == parts.len() - 1 {
            EntryKind::Blob.into()
        } else {
            EntryKind::Tree.into()
        };
        let entry = Entry {
            mode,
            filename: name.into(),
            oid: current_oid,
        };
        let tree = Tree {
            entries: vec![entry],
        };
        current_oid = repo.write_object(tree).unwrap().detach();
    }
    let tree_oid = current_oid;

    let sig = SignatureRef::default();
    repo.commit_as(sig, sig, "refs/heads/master", message, tree_oid, parents)
        .unwrap()
        .detach()
}

/// Charter a virgin source via the real `charter()` API and return the
/// resulting founding anchor -- under `[anchor-resolvable]` this is the
/// only kind of anchor `claim()` will accept, since the anchor is given
/// and verified against a real charter, never derived from git ancestry.
fn found_anchor(registry: &GitRegistry, pub_key: &[u8], src: &[u8]) -> atom_core::Anchor {
    let czd = registry.charter(&[owner_ref(pub_key)], src, None).unwrap();
    atom_core::Anchor::new(czd.as_bytes().to_vec())
}

/// `GitStore::ingest` (atom-git/src/store.rs, outside this node's surface)
/// does not yet propagate the charter chain into the destination store --
/// only `refs/atom/claims/d/*`, `refs/atom/d/*`, and dev refs. Resolution
/// now correctly requires a resolvable charter
/// (`[claim-charter-authorization]`), so tests that ingest into a fresh
/// store and then resolve from it must replant the SAME founding charter
/// directly, to stay isolated from that separate, already-known gap.
fn replant_charter(
    source_repo: &gix::Repository,
    dest_repo: &gix::Repository,
    anchor: &atom_core::Anchor,
) {
    let ref_name = atom_git::charter_store::charter_ref_name(anchor.as_bytes());
    let charter_ref = source_repo
        .try_find_reference(&ref_name)
        .unwrap()
        .expect("the fixture's own charter() call must have written this ref");
    let charter_msg = source_repo
        .find_object(charter_ref.id().detach())
        .unwrap()
        .try_into_commit()
        .unwrap()
        .message_raw_sloppy()
        .to_string();
    let dest_oid = atom_git::gix_util::write_charter_commit(dest_repo, charter_msg).unwrap();
    let dest_ref_fullname = gix::refs::FullName::try_from(ref_name.as_str()).unwrap();
    dest_repo
        .edit_reference(gix::refs::transaction::RefEdit {
            change: gix::refs::transaction::Change::Update {
                log: gix::refs::transaction::LogChange::default(),
                expected: gix::refs::transaction::PreviousValue::Any,
                new: gix::refs::Target::Object(dest_oid),
            },
            name: dest_ref_fullname,
            deref: false,
        })
        .unwrap();
}

/// A `single-key` owner-reference authorizing the key with these raw
/// public-key bytes -- the common case throughout these tests, none of
/// which exercise multi-member sets or non-`single-key` tiers.
///
/// `[owner-authorization-delegated]`: a `single-key` `OwnerRef.value` MUST
/// be the key's THUMBPRINT (`signer.tmb == o.value`), never the raw public
/// key -- these are different byte strings, and using the raw key here
/// would make every authorization check comparing against this owner-ref
/// silently fail closed.
fn owner_ref(pub_key: &[u8]) -> atom_id::OwnerRef {
    let tmb = coz_rs::compute_thumbprint_for_alg("Ed25519", pub_key)
        .expect("Ed25519 is always supported");
    atom_id::OwnerRef::single_key(&tmb)
}

/// F23: `claim()`'s anchor check resolves a real founding charter
/// (`crate::charter_store::resolve_founding_charter`) rather than deriving
/// one from git ancestry. An anchor with no founding charter behind it must
/// be rejected cleanly, not accepted just because it happens to name a real
/// git commit.
#[test]
fn test_claim_rejects_unchartered_anchor() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    // A real commit, but never chartered -- no `refs/atom/charter/d/*`
    // entry exists for it.
    let anchor = atom_core::Anchor::new(genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    let res = registry.claim(&id, &owner_ref(&pub_key));
    assert!(
        matches!(&res, Err(GitError::Validation(msg)) if msg.contains("founding charter")),
        "claim into an unchartered anchor must be rejected with a clear error: {res:?}"
    );
}

/// F23: a properly chartered source's anchor resolves and `claim()`
/// succeeds.
#[test]
fn test_claim_accepts_chartered_anchor() {
    let (_dir, repo, _genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    let _claim_czd = registry
        .claim(&id, &owner_ref(&pub_key))
        .expect("claim against a properly chartered anchor must succeed");
}

#[test]
fn test_deterministic_commits() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let blob_oid = repo
        .write_object(Blob {
            data: b"file data".to_vec(),
        })
        .unwrap()
        .detach();
    let entry = Entry {
        mode: EntryKind::Blob.into(),
        filename: "test.txt".into(),
        oid: blob_oid,
    };
    let tree = Tree {
        entries: vec![entry],
    };
    let tree_oid = repo.write_object(tree).unwrap().detach();

    // Write deterministic commit twice with same input
    let commit1_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, tree_oid, genesis_oid).unwrap();
    let commit2_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, tree_oid, genesis_oid).unwrap();

    assert_eq!(commit1_oid, commit2_oid);
}

#[test]
fn test_claim_and_key_rotation() {
    let (_dir, repo, _genesis_oid) = setup_test_repo();

    // Set up coz credentials
    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // Initial claim
    let claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();

    // Verify claim reference was created — located by its ref name (label),
    // never by reinterpreting the returned czd as a git object id.
    let repo = registry.source.repo();
    let claim_ref = repo
        .try_find_reference("refs/atom/claims/pub/my-package")
        .unwrap()
        .unwrap();
    let claim_oid = claim_ref.id().detach();

    // Regression: `claim_czd` must be the spec-defined digest of (cad, sig),
    // independently recomputable from the claim's own signed bytes — and it
    // must NOT be the git object id the claim commit happens to be stored
    // at (the exact wrong value the original bug produced).
    let claim_commit = repo
        .find_object(claim_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let claim_msg_str = claim_commit.message_raw_sloppy().to_string();
    assert_eq!(claim_czd, independent_ed25519_czd(&claim_msg_str));
    assert_ne!(
        claim_czd.as_bytes(),
        claim_oid.as_bytes(),
        "czd must not be the git object id of the claim commit"
    );

    // Key rotation / parented claim update
    let next_sk = SigningKey::<Ed25519>::generate();
    let next_prv = next_sk.private_key_bytes().to_vec();
    let next_pub = next_sk.verifying_key().public_key_bytes().to_vec();

    let registry_rotated = GitRegistry::new(
        registry.source.repo(),
        next_prv,
        next_pub.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    // [claim-replacement-authority]'s owner-replacement path: the
    // replacement is signed by the OUTGOING owner's key (`registry`, still
    // holding `pub_key`), authorizing the transition -- naming `next_pub`
    // as the new owner going forward. `registry_rotated` cannot sign this
    // replacement itself: `next_pub` is not yet an authorized owner of
    // anything until this very call makes it one.
    let next_claim_czd = registry.claim(&id, &owner_ref(&next_pub)).unwrap();
    assert_ne!(claim_czd, next_claim_czd);

    // Check that the next claim has the previous claim as a parent (claim
    // rotation chain), located via the ref (git-native, unaffected by the
    // czd fix) rather than by reinterpreting a czd as an oid.
    let repo_rotated = registry_rotated.source.repo();
    let next_claim_ref = repo_rotated
        .try_find_reference("refs/atom/claims/pub/my-package")
        .unwrap()
        .unwrap();
    let next_claim_oid = next_claim_ref.id().detach();
    let next_claim_commit = repo_rotated
        .find_object(next_claim_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    assert_eq!(next_claim_commit.parent_ids().count(), 1);
    assert_eq!(
        next_claim_commit.parent_ids().next().unwrap().detach(),
        claim_oid
    );

    let next_claim_msg_str = next_claim_commit.message_raw_sloppy().to_string();
    assert_eq!(next_claim_czd, independent_ed25519_czd(&next_claim_msg_str));
    assert_ne!(next_claim_czd.as_bytes(), next_claim_oid.as_bytes());
}

#[test]
fn test_publish_and_tag_chain() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let repo = registry.source.repo();

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // 1. Claim package
    let claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();

    // Create a version workspace state tree
    let ver_commit_oid = create_commit(
        &repo,
        "v1.0.0 src",
        "lib.rs",
        b"fn test() {}",
        vec![genesis_oid],
    );
    let ver_commit_obj = repo
        .find_object(ver_commit_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let ver_tree_oid = ver_commit_obj.tree_id().unwrap();

    // 2. Publish package version 1.0.0
    let ver_1 = RawVersion::new("1.0.0".to_string());
    registry
        .publish(
            &id,
            &claim_czd,
            &ver_1,
            ver_tree_oid.as_bytes(),
            ver_commit_oid.as_bytes(),
            "Cargo.toml",
        )
        .unwrap();

    // Verify tag exists
    let tag_ref = repo
        .try_find_reference("refs/atom/pub/my-package/1.0.0")
        .unwrap()
        .unwrap();
    let tag_oid = tag_ref.id().detach();

    // Peel publish reference back to the deterministic commit
    let peeled = repo
        .find_object(tag_oid)
        .unwrap()
        .peel_to_kind(gix::object::Kind::Commit)
        .unwrap();

    // Verify it carries the src header matching our workspace commit OID
    let commit = peeled.try_into_commit().unwrap();
    let commit_decoded = commit.decode().unwrap();
    let src_header = commit_decoded
        .extra_headers
        .iter()
        .find(|(k, _)| *k == "src")
        .unwrap()
        .1
        .to_string();
    assert_eq!(src_header, ver_commit_oid.to_hex().to_string());
}

/// Regression: `publish()` must verify the caller-supplied claim identity
/// against the active claim's real, recomputed czd — never by
/// reinterpreting the supplied bytes as a git object id. A czd fabricated
/// from the claim commit's own git oid — the exact shape the original bug
/// produced — must be rejected, and the real czd must be accepted.
#[test]
fn test_publish_rejects_fabricated_oid_czd() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );
    let repo = registry.source.repo();

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    let claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();

    let claim_ref = repo
        .try_find_reference("refs/atom/claims/pub/my-package")
        .unwrap()
        .unwrap();
    let claim_oid = claim_ref.id().detach();

    // The exact wrong value the original bug produced: the claim commit's
    // own git oid, reinterpreted as a czd.
    let bogus_czd = coz_rs::Czd::from_bytes(claim_oid.as_bytes().to_vec());
    assert_ne!(
        bogus_czd, claim_czd,
        "sanity: fabricated oid-czd must differ from the real one"
    );

    let ver_commit_oid = create_commit(
        &repo,
        "v1.0.0 src",
        "lib.rs",
        b"fn test() {}",
        vec![genesis_oid],
    );
    let ver_commit_obj = repo
        .find_object(ver_commit_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let ver_tree_oid = ver_commit_obj.tree_id().unwrap();
    let ver_1 = RawVersion::new("1.0.0".to_string());

    let res = registry.publish(
        &id,
        &bogus_czd,
        &ver_1,
        ver_tree_oid.as_bytes(),
        ver_commit_oid.as_bytes(),
        "Cargo.toml",
    );
    assert!(
        matches!(res, Err(GitError::Validation(_))),
        "publish must reject a claim czd fabricated from the commit's git oid: {res:?}"
    );

    let res = registry.publish(
        &id,
        &claim_czd,
        &ver_1,
        ver_tree_oid.as_bytes(),
        ver_commit_oid.as_bytes(),
        "Cargo.toml",
    );
    assert!(
        res.is_ok(),
        "publish must accept the real, spec-correct claim czd: {res:?}"
    );
}

/// Regression: `GitSource::resolve`'s REGISTRY branch
/// (`refs/atom/claims/pub/*` + `refs/atom/pub/*/*`) must return the real,
/// independently-recomputable czd — not the active claim commit's git oid.
#[tokio::test]
async fn test_resolve_registry_branch_returns_real_czd() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );
    let repo = registry.source.repo();

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    let claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();
    let claim_ref = repo
        .try_find_reference("refs/atom/claims/pub/my-package")
        .unwrap()
        .unwrap();
    let claim_oid = claim_ref.id().detach();

    let ver_commit_oid = create_commit(
        &repo,
        "v1.0.0 src",
        "lib.rs",
        b"fn test() {}",
        vec![genesis_oid],
    );
    let ver_commit_obj = repo
        .find_object(ver_commit_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let ver_tree_oid = ver_commit_obj.tree_id().unwrap();
    let ver_1 = RawVersion::new("1.0.0".to_string());
    registry
        .publish(
            &id,
            &claim_czd,
            &ver_1,
            ver_tree_oid.as_bytes(),
            ver_commit_oid.as_bytes(),
            "Cargo.toml",
        )
        .unwrap();

    let source = GitSource::new(gix::open(repo.path()).unwrap());
    let entry = source.resolve(&id).await.unwrap().unwrap();
    let mut versions = entry.versions();
    let version_entry = versions.next().unwrap();
    let resolved_czd = version_entry.czd().unwrap();

    assert_eq!(
        resolved_czd, &claim_czd,
        "resolve() must return the same real czd claim() produced"
    );
    assert_ne!(
        resolved_czd.as_bytes(),
        claim_oid.as_bytes(),
        "resolve()'s czd must not be the claim commit's git oid"
    );
}

/// Adversarial (tmb-binding): a claim validly signed by an ATTACKER's own
/// key, but whose payload declares `tmb` equal to a LEGITIMATE charter
/// owner's thumbprint, must be rejected by `resolve()`'s REGISTRY branch
/// before `verify_claim_authorized_by_charter` ever trusts the declared
/// `tmb`. `verify_claim`'s signature check alone cannot catch this: the
/// attacker's own key validly signs the attacker's own payload regardless
/// of what `tmb` field it asserts.
#[tokio::test]
async fn test_resolve_registry_branch_rejects_claim_with_forged_tmb() {
    let (_dir, repo, _genesis_oid) = setup_test_repo();

    let victim_sk = SigningKey::<Ed25519>::generate();
    let victim_pub = victim_sk.verifying_key().public_key_bytes().to_vec();
    let victim_tmb = coz_rs::compute_thumbprint_for_alg("Ed25519", &victim_pub).unwrap();

    let attacker_sk = SigningKey::<Ed25519>::generate();
    let attacker_prv = attacker_sk.private_key_bytes().to_vec();
    let attacker_pub = attacker_sk.verifying_key().public_key_bytes().to_vec();

    // A real charter, whose owner set names the VICTIM as the sole
    // authorized principal.
    let registry_for_charter = GitRegistry::new(
        repo,
        attacker_prv.clone(), // irrelevant to founding; any key may found a virgin source
        attacker_pub.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );
    let founding_czd = registry_for_charter
        .charter(
            std::slice::from_ref(&atom_id::OwnerRef::single_key(&victim_tmb)),
            b"src-rev",
            None,
        )
        .unwrap();
    let repo = registry_for_charter.source.repo();
    let anchor = atom_core::Anchor::new(founding_czd.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // Forged claim: signed by the ATTACKER's own key, but declares `tmb`
    // equal to the VICTIM's -- the only charter-set member --
    // impersonating the one signer `verify_claim_authorized_by_charter`
    // would accept.
    let forged_claim = atom_id::ClaimPayload::new(
        Alg::Ed25519,
        id.clone(),
        1_000,
        atom_id::OwnerRef::single_key(&victim_tmb),
        "cargo".to_string(),
        b"src-rev".to_vec(),
        victim_tmb.clone(), // forged: declares the victim's tmb
    );
    let pay_val = serde_json::to_value(&forged_claim).unwrap();
    let pay_map: indexmap::IndexMap<String, serde_json::Value> =
        serde_json::from_value(pay_val).unwrap();
    let pay_bytes = serde_json::to_vec(&pay_map).unwrap();
    let (sig, _cad) =
        coz_rs::sign_json(&pay_bytes, "Ed25519", &attacker_prv, &attacker_pub).unwrap();
    let envelope = atom_git::source::CozMessageEnvelope {
        pay: pay_map,
        sig,
        key: Some(attacker_pub.clone()),
    };
    let claim_msg = serde_json::to_string(&envelope).unwrap();
    let claim_oid = atom_git::gix_util::write_claim_commit(&repo, claim_msg, None).unwrap();
    let claim_ref_fullname =
        gix::refs::FullName::try_from("refs/atom/claims/pub/my-package").unwrap();
    repo.edit_reference(gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(claim_oid),
        },
        name: claim_ref_fullname,
        deref: false,
    })
    .unwrap();

    let source = GitSource::new(gix::open(repo.path()).unwrap());
    let result = source.resolve(&id).await;
    assert!(
        matches!(
            result,
            Err(GitError::Verify(atom_id::VerifyError::ThumbprintMismatch))
        ),
        "a claim signed by an attacker's key but declaring the victim's tmb must be rejected \
         before authorization ever trusts the declared tmb: {result:?}"
    );
}

/// Regression: `GitSource::resolve`'s STORE branch
/// (`refs/atom/claims/d/*` + `refs/atom/d/*/*`) must also return the real,
/// independently-recomputable czd — not the claim commit's git oid, and
/// not the ref-name hex segment used for the store's internal addressing.
///
/// This exercises the store layout directly (rather than via
/// `GitStore::ingest`, exercised by `test_local_ingest`) by hand-signing and
/// hand-writing a claim + publish pair, mirroring what `GitRegistry` does
/// internally.
#[tokio::test]
async fn test_resolve_store_branch_returns_real_czd() {
    let dir = tempfile::tempdir().unwrap();
    let repo = gix::init_bare(dir.path()).unwrap();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    // `resolve()`'s store branch now resolves and checks the charter chain
    // (`[claim-charter-authorization]`) same as the registry branch --
    // hand-sign and hand-write a real founding charter (mirroring
    // `GitRegistry::charter()`'s construction, matching this test's own
    // "mirrors what GitRegistry does internally" idiom) rather than an
    // arbitrary stand-in anchor with no charter behind it.
    let tmb = coz_rs::compute_thumbprint_for_alg("Ed25519", &pub_key).unwrap();
    let charter_payload = atom_id::CharterPayload::new(
        Alg::Ed25519,
        500,
        vec![atom_id::OwnerRef::single_key(&tmb)],
        None,
        vec![0u8; 20],
        tmb.clone(),
    )
    .unwrap();
    let charter_pay_val = serde_json::to_value(&charter_payload).unwrap();
    let charter_pay_map: indexmap::IndexMap<String, serde_json::Value> =
        serde_json::from_value(charter_pay_val).unwrap();
    let charter_pay_bytes = serde_json::to_vec(&charter_pay_map).unwrap();
    let (charter_sig, _cad) =
        coz_rs::sign_json(&charter_pay_bytes, "Ed25519", &prv, &pub_key).unwrap();
    let charter_envelope = atom_git::source::CozMessageEnvelope {
        pay: charter_pay_map,
        sig: charter_sig.clone(),
        key: Some(pub_key.clone()),
    };
    let charter_msg = serde_json::to_string(&charter_envelope).unwrap();
    let charter_czd = atom_id::czd_for_alg(&charter_pay_bytes, &charter_sig, "Ed25519").unwrap();
    let charter_oid = atom_git::gix_util::write_charter_commit(&repo, charter_msg).unwrap();
    let charter_ref_name = atom_git::charter_store::charter_ref_name(charter_czd.as_bytes());
    let charter_ref_fullname = gix::refs::FullName::try_from(charter_ref_name.as_str()).unwrap();
    repo.edit_reference(gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(charter_oid),
        },
        name: charter_ref_fullname,
        deref: false,
    })
    .unwrap();

    let anchor = atom_core::Anchor::new(charter_czd.as_bytes().to_vec());
    let label = Label::try_from("store-pkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());

    // A standalone "src" commit to anchor claim/publish provenance to.
    let empty_tree = repo
        .write_object(gix::objs::Tree {
            entries: Vec::new(),
        })
        .unwrap()
        .detach();
    let blank = atom_git::gix_util::blank_signature();
    let src_oid = repo
        .write_object(gix::objs::Commit {
            tree: empty_tree,
            parents: Vec::new().into(),
            author: blank.clone(),
            committer: blank.clone(),
            encoding: None,
            message: gix::objs::bstr::BString::from("src"),
            extra_headers: Vec::new(),
        })
        .unwrap()
        .detach();

    // Hand-sign a claim, mirroring GitRegistry::claim's construction.
    let claim_payload = atom_id::ClaimPayload::new(
        Alg::Ed25519,
        AtomId::new(anchor.clone(), label.clone()),
        1_000,
        atom_id::OwnerRef::single_key(&tmb),
        "cargo".to_string(),
        src_oid.as_bytes().to_vec(),
        tmb.clone(),
    );
    let claim_pay_val = serde_json::to_value(&claim_payload).unwrap();
    let claim_pay_map: indexmap::IndexMap<String, serde_json::Value> =
        serde_json::from_value(claim_pay_val).unwrap();
    let claim_pay_bytes = serde_json::to_vec(&claim_pay_map).unwrap();
    let (claim_sig, _cad) = coz_rs::sign_json(&claim_pay_bytes, "Ed25519", &prv, &pub_key).unwrap();

    let expected_czd = independent_ed25519_czd(
        &serde_json::to_string(&atom_git::source::CozMessageEnvelope {
            pay: claim_pay_map.clone(),
            sig: claim_sig.clone(),
            key: Some(pub_key.clone()),
        })
        .unwrap(),
    );

    let claim_envelope = atom_git::source::CozMessageEnvelope {
        pay: claim_pay_map,
        sig: claim_sig,
        key: Some(pub_key.clone()),
    };
    let claim_msg = serde_json::to_string(&claim_envelope).unwrap();
    let claim_oid = atom_git::gix_util::write_claim_commit(&repo, claim_msg, None).unwrap();

    // Hand-sign a publish chaining to that claim, mirroring
    // GitRegistry::publish's construction.
    let atom_commit_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, empty_tree, src_oid).unwrap();
    let publish_payload = atom_id::PublishPayload::new(
        Alg::Ed25519,
        AtomId::new(anchor, label),
        expected_czd.clone(),
        atom_commit_oid.as_bytes().to_vec(),
        2_000,
        "Cargo.toml".to_string(),
        src_oid.as_bytes().to_vec(),
        tmb,
        RawVersion::new("1.0.0".to_string()),
    );
    let pub_pay_val = serde_json::to_value(&publish_payload).unwrap();
    let pub_pay_map: indexmap::IndexMap<String, serde_json::Value> =
        serde_json::from_value(pub_pay_val).unwrap();
    let pub_pay_bytes = serde_json::to_vec(&pub_pay_map).unwrap();
    let (pub_sig, _cad) = coz_rs::sign_json(&pub_pay_bytes, "Ed25519", &prv, &pub_key).unwrap();
    let pub_envelope = atom_git::source::CozMessageEnvelope {
        pay: pub_pay_map,
        sig: pub_sig,
        key: Some(pub_key.clone()),
    };
    let publish_msg = serde_json::to_string(&pub_envelope).unwrap();
    let publish_czd = independent_ed25519_czd(&publish_msg);
    let tag_oid = atom_git::gix_util::write_publish_tag(
        &repo,
        "store-pkg-1.0.0",
        atom_commit_oid,
        gix::object::Kind::Commit,
        atom_git::gix_util::blank_signature(),
        publish_msg,
    )
    .unwrap();

    // Write the store layout refs. The claim ref is keyed by the claim's
    // own real czd (`[store-claim-ref]`); the version ref is flat and
    // keyed by blake3(publish_czd) (`[store-ref-by-publish-czd]`) --
    // `GitSource::resolve`'s store branch discovers a version's owning
    // claim only via this real linkage now (the publish tag's own
    // `claim` field), unlike the old nested scheme where any shared
    // stable hex string sufficed structurally.
    let claim_key_hex = atom_git::store::hex_encode(expected_czd.as_bytes());
    let version_key_hex =
        atom_git::store::hex_encode(blake3::hash(publish_czd.as_bytes()).as_bytes());
    for (name, oid) in [
        (format!("refs/atom/claims/d/{}", claim_key_hex), claim_oid),
        (format!("refs/atom/d/{}", version_key_hex), tag_oid),
    ] {
        let fullname = gix::refs::FullName::try_from(name.as_str()).unwrap();
        let edit = gix::refs::transaction::RefEdit {
            change: gix::refs::transaction::Change::Update {
                log: gix::refs::transaction::LogChange::default(),
                expected: gix::refs::transaction::PreviousValue::Any,
                new: gix::refs::Target::Object(oid),
            },
            name: fullname,
            deref: false,
        };
        repo.edit_reference(edit).unwrap();
    }

    let source = GitSource::new(gix::open(dir.path()).unwrap());
    let entry = source.resolve(&id).await.unwrap().unwrap();
    let mut versions = entry.versions();
    let version_entry = versions.next().unwrap();
    let resolved_czd = version_entry.czd().unwrap();

    assert_eq!(
        resolved_czd, &expected_czd,
        "store-branch resolve() must return the real, independently-recomputable czd"
    );
    assert_ne!(
        resolved_czd.as_bytes(),
        claim_oid.as_bytes(),
        "store-branch czd must not be the claim commit's git oid"
    );
}

/// `GitStore::ingest` locates a destination claim commit through its own
/// ref family (`refs/atom/claims/d/{claim_czd}`,
/// `docs/specs/git-storage-format.md` `[store-claim-ref]`) rather than by
/// assuming the signed `PublishPayload.claim` field (a `Czd`) is literally a
/// git object id (issue #64). This regression-tests that ingest succeeds
/// with a spec-correct, independently-recomputable czd (sized to the
/// signing algorithm's hash — e.g. 64 bytes for Ed25519/SHA-512, which
/// `ObjectId::try_from` cannot even parse), and that the store layout it
/// writes round-trips back through `GitSource::resolve` to the real czd —
/// never the claim commit's git oid, and never the ref-name hex segment
/// used for the store's internal addressing (mirroring
/// `test_resolve_store_branch_returns_real_czd`, which exercises the same
/// store layout invariant by hand-writing the refs directly).
#[tokio::test]
async fn test_local_ingest() {
    // 1. Create registry repository and publish a package version
    let (_reg_dir, reg_repo, reg_genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        reg_repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let reg_repo = registry.source.repo();

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("pkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());

    let claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();

    let ver_commit_oid = create_commit(
        &reg_repo,
        "v1.0.0 src",
        "src/main.rs",
        b"main",
        vec![reg_genesis_oid],
    );
    let ver_commit_obj = reg_repo
        .find_object(ver_commit_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let ver_tree_oid = ver_commit_obj.tree_id().unwrap();

    let ver = RawVersion::new("1.0.0".to_string());
    registry
        .publish(
            &id,
            &claim_czd,
            &ver,
            ver_tree_oid.as_bytes(),
            ver_commit_oid.as_bytes(),
            "Cargo.toml",
        )
        .unwrap();

    // 2. Create store repository and ingest from registry source
    let (_store_dir, store_repo, _store_genesis_oid) = setup_test_repo();
    let store = GitStore::new(store_repo);

    // Ingest!
    store.ingest(&registry.source).await.unwrap();
    replant_charter(&reg_repo, &store.source.repo(), &anchor);

    // 3. Verify store references exist and target a real, independently
    // ingested claim commit — the ref-path hex segment is a store-internal
    // addressing key, never the czd bytes themselves (`[czd-oid-disjoint]`),
    // so it can't be reconstructed here from `claim_czd`; assert existence
    // and identity via the claim commit's own content instead.
    let repo_store = store.source.repo();
    let claim_refs: Vec<_> = repo_store
        .references()
        .unwrap()
        .prefixed("refs/atom/claims/d/")
        .unwrap()
        .map(|r| r.unwrap().id().detach())
        .collect();
    assert_eq!(
        claim_refs.len(),
        1,
        "expected exactly one ingested claim ref"
    );
    let store_claim_oid = claim_refs[0];

    let claim_commit = repo_store
        .find_object(store_claim_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let claim_envelope: atom_git::source::CozMessageEnvelope =
        serde_json::from_str(&claim_commit.message_raw_sloppy().to_string()).unwrap();
    let claim_pay_bytes = serde_json::to_vec(&claim_envelope.pay).unwrap();
    let recomputed_czd =
        atom_id::czd_for_alg(&claim_pay_bytes, &claim_envelope.sig, "Ed25519").unwrap();
    assert_eq!(
        recomputed_czd, claim_czd,
        "ingested claim commit's own signed content must recompute to the real claim czd"
    );
    assert_ne!(
        recomputed_czd.as_bytes(),
        store_claim_oid.as_bytes(),
        "the claim czd must not equal the destination claim commit's git oid"
    );

    // Verify resolving the store source yields the correct package info
    let query_source = GitSource::new(gix::open(repo_store.path()).unwrap());
    let resolved_entry = query_source.resolve(&id).await.unwrap().unwrap();
    let mut versions = resolved_entry.versions();
    let version_entry = versions.next().unwrap();
    assert_eq!(version_entry.version().as_str(), "1.0.0");
    assert_eq!(version_entry.czd().unwrap(), &claim_czd);
}

#[test]
fn test_fs_dev_ingest() {
    let (temp_dir, repo, _genesis_oid) = setup_test_repo();
    let store = GitStore::new(repo);

    // Create a local filesystem directory with package contents
    let local_dir = temp_dir.path().join("local_pkg");
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(local_dir.join("main.rs"), b"fn main() {}").unwrap();

    let label = Label::try_from("dev-pkg").unwrap();
    let dev_version = RawVersion::new("0.1.0-dev".to_string());

    // Import directory
    store.import_path(&label, &local_dir, &dev_version).unwrap();

    // Verify dev namespace reference exists (Step 6)
    let anchor = atom_core::Anchor::new(atom_git::store::FS_SENTINEL_ANCHOR.to_vec());
    let dev_id = AtomId::new(anchor, label.clone());
    let digest = atom_core::AtomDigest::compute(&dev_id, coz_rs::Alg::ES256.hash_alg());
    let digest_str = atom_git::store::dev_ref_digest(&digest);
    let dev_ref_name = format!("refs/atom/dev/{}/0.1.0-dev", digest_str);
    let repo = store.source.repo();
    let dev_ref = repo.try_find_reference(&dev_ref_name).unwrap().unwrap();

    // The ref should point to a commit carrying our files
    let peeled = repo
        .find_object(dev_ref.id().detach())
        .unwrap()
        .peel_to_kind(gix::object::Kind::Commit)
        .unwrap();
    let commit = peeled.try_into_commit().unwrap();
    let tree = commit.tree().unwrap();
    let decoded_tree = tree.decode().unwrap();
    assert_eq!(decoded_tree.entries.len(), 1);
    assert_eq!(decoded_tree.entries[0].filename, "main.rs");
}

#[test]
fn test_failures_and_forbidden_states() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        repo,
        prv,
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let repo = registry.source.repo();

    let anchor = found_anchor(&registry, &pub_key, b"src-rev");
    let label = Label::try_from("bad-package").unwrap();
    let id = AtomId::new(anchor, label);

    // 1. Attempting to publish without active claim should fail
    let claim_czd = coz_rs::Czd::from_bytes(vec![0; 32]);
    let ver = RawVersion::new("1.0.0".to_string());

    let res = registry.publish(
        &id,
        &claim_czd,
        &ver,
        &[0; 20],
        genesis_oid.as_bytes(),
        "Cargo.toml",
    );
    assert!(matches!(res, Err(GitError::NoActiveClaim(_))));

    // Now establish a claim
    let real_claim_czd = registry.claim(&id, &owner_ref(&pub_key)).unwrap();

    // 2. Attempting to publish with a backdated src commit (not a descendant of claim src) should
    //    fail
    // We create an independent root (another genesis) in the same repo to act as a non-descendant
    // src OID
    let other_sig = SignatureRef::default();
    let empty_tree = Tree {
        entries: Vec::new(),
    };
    let other_tree_oid = repo.write_object(empty_tree).unwrap().detach();
    let other_genesis_oid = repo
        .commit_as(
            other_sig,
            other_sig,
            "refs/heads/other-root",
            "other genesis",
            other_tree_oid,
            Vec::<ObjectId>::new(),
        )
        .unwrap()
        .detach();

    let res = registry.publish(
        &id,
        &real_claim_czd,
        &ver,
        &[0; 20],
        other_genesis_oid.as_bytes(),
        "Cargo.toml",
    );
    assert!(matches!(res, Err(GitError::InvalidTemporalVector { .. })));
}

#[test]
fn test_differential_git_cli() {
    let (dir, repo, genesis_oid) = setup_test_repo();

    let blob_oid = repo
        .write_object(Blob {
            data: b"differential testing content".to_vec(),
        })
        .unwrap()
        .detach();
    let entry = Entry {
        mode: EntryKind::Blob.into(),
        filename: "diff.txt".into(),
        oid: blob_oid,
    };
    let tree = Tree {
        entries: vec![entry],
    };
    let tree_oid = repo.write_object(tree).unwrap().detach();

    // 1. Write deterministic commit via gix
    let commit_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, tree_oid, genesis_oid).unwrap();

    // 2. Query the exact same commit using the canonical git binary
    let output = std::process::Command::new("git")
        .arg("cat-file")
        .arg("-p")
        .arg(commit_oid.to_hex().to_string())
        .current_dir(dir.path())
        .output()
        .expect("failed to execute git command");

    assert!(output.status.success());
    let stdout_str = String::from_utf8(output.stdout).unwrap();

    // Verify canonical git parses the tree correctly
    assert!(stdout_str.contains(&format!("tree {}", tree_oid.to_hex())));

    // Verify the extra header "src" is present and matches genesis_oid
    assert!(stdout_str.contains(&format!("src {}", genesis_oid.to_hex())));

    // Verify there are no author/committer names or timestamps
    assert!(stdout_str.contains(" 0 +0000"));
    assert!(stdout_str.contains("author "));
    assert!(stdout_str.contains("committer "));
}

/// Write a ref pointing directly at `target` -- a bare structural ref
/// with no real object content, used to exercise `evict_version`'s
/// behavior against store refs that carry no publish tag payload.
fn write_bare_ref(repo: &gix::Repository, name: &str, target: ObjectId) {
    let fullname = gix::refs::FullName::try_from(name).unwrap();
    let edit = gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(target),
        },
        name: fullname,
        deref: false,
    };
    repo.edit_reference(edit).unwrap();
}

/// `evict_version`'s claim-cleanup step (`[store-claim-cleanup]`) can
/// only key off a claim czd discovered from the evicted ref's own
/// publish tag payload -- under the flat scheme there is no shared
/// claim-prefix to scan, unlike the old nested
/// `refs/atom/d/{claim_czd}/{version}` shape this test originally
/// exercised. Against bare, non-tag store refs (no payload to read),
/// eviction must still succeed for each version ref, but ownership can
/// never be proven, so the claim ref is intentionally left alone rather
/// than guessed at -- see `store_keying.rs::evict_sibling_scan_flat_refs`
/// for the real-tag-payload case, where cleanup does fire.
#[test]
fn test_store_claim_cleanup_bare_refs_skip_orphan_check() {
    let dir = tempfile::tempdir().unwrap();
    let repo = gix::init_bare(dir.path()).unwrap();

    let empty_tree_oid = repo
        .write_object(gix::objs::Tree::empty())
        .unwrap()
        .detach();

    let store = GitStore::new(repo);
    let claim_czd_hex = "0123456789abcdef0123456789abcdef01234567";
    let v1_key = "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
    let v2_key = "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";

    let claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
    let v1_ref_name = format!("refs/atom/d/{}", v1_key);
    let v2_ref_name = format!("refs/atom/d/{}", v2_key);

    write_bare_ref(&store.source.repo(), &claim_ref_name, empty_tree_oid);
    write_bare_ref(&store.source.repo(), &v1_ref_name, empty_tree_oid);
    write_bare_ref(&store.source.repo(), &v2_ref_name, empty_tree_oid);

    // Verify all references exist
    let repo = store.source.repo();
    assert!(repo.try_find_reference(&claim_ref_name).unwrap().is_some());
    assert!(repo.try_find_reference(&v1_ref_name).unwrap().is_some());
    assert!(repo.try_find_reference(&v2_ref_name).unwrap().is_some());

    // Evict version 1
    store.evict_version(v1_key).unwrap();
    assert!(repo.try_find_reference(&v1_ref_name).unwrap().is_none());
    assert!(repo.try_find_reference(&v2_ref_name).unwrap().is_some());
    assert!(
        repo.try_find_reference(&claim_ref_name).unwrap().is_some(),
        "claim ref must not be touched when ownership can't be established"
    );

    // Evict version 2 -- still no ownership can be established, so the
    // claim ref must survive even though no version refs remain.
    store.evict_version(v2_key).unwrap();
    assert!(repo.try_find_reference(&v2_ref_name).unwrap().is_none());
    assert!(
        repo.try_find_reference(&claim_ref_name).unwrap().is_some(),
        "bare, non-tag store refs never trigger claim cleanup -- there is no payload to attribute \
         them to a claim"
    );
}

#[cfg(test)]
mod proptests {
    use proptest::prelude::*;
    use tempfile::TempDir;

    use super::*;

    proptest! {
        // Under the flat `refs/atom/d/{blake3(publish_czd)}` scheme,
        // claim-cleanup ownership is only discoverable from a real
        // publish tag payload (deterministically covered by
        // `store_keying.rs::evict_sibling_scan_flat_refs`). This
        // property test's bare, non-tag refs can no longer exercise that
        // linkage (see `test_store_claim_cleanup_bare_refs_skip_orphan_check`),
        // so it now fuzzes the orthogonal property it still can: given N
        // distinct flat version refs, evicting them in any order deletes
        // exactly the evicted ref and leaves every other ref untouched.
        #[test]
        fn test_store_evict_pbt(
            version_count in 1..10usize,
            shuffle_seed in 0..100usize,
        ) {
            let dir = TempDir::new().unwrap();
            let repo = gix::init_bare(dir.path()).unwrap();

            let empty_tree_oid = repo
                .write_object(gix::objs::Tree::empty())
                .unwrap()
                .detach();

            let store = GitStore::new(repo);

            // Distinct flat store keys, one per generated version -- a
            // stand-in for hex(blake3(publish_czd)); their real content
            // doesn't matter for this property, only that they're unique.
            let keys: Vec<String> = (0..version_count)
                .map(|i| format!("{:064x}", i))
                .collect();

            for key in &keys {
                let ref_name = format!("refs/atom/d/{}", key);
                let fullname = gix::refs::FullName::try_from(ref_name.as_str()).unwrap();
                let edit = gix::refs::transaction::RefEdit {
                    change: gix::refs::transaction::Change::Update {
                        log: gix::refs::transaction::LogChange::default(),
                        expected: gix::refs::transaction::PreviousValue::Any,
                        new: gix::refs::Target::Object(empty_tree_oid),
                    },
                    name: fullname,
                    deref: false,
                };
                store.source.repo().edit_reference(edit).unwrap();
            }

            // Verify all exist
            for key in &keys {
                let ref_name = format!("refs/atom/d/{}", key);
                let has_ref = store.source.repo().try_find_reference(&ref_name)
                    .unwrap()
                    .is_some();
                prop_assert!(has_ref);
            }

            // Evict them in pseudo-random order determined by shuffle_seed
            let mut to_evict = keys.clone();
            let n = to_evict.len();
            for i in 0..n {
                let j = (shuffle_seed + i) % n;
                to_evict.swap(i, j);
            }

            let mut remaining = keys.clone();

            for key in to_evict {
                store.evict_version(&key).unwrap();

                // Verify this version is gone
                let ref_name = format!("refs/atom/d/{}", key);
                let has_ver = store.source.repo().try_find_reference(&ref_name)
                    .unwrap()
                    .is_some();
                prop_assert!(!has_ver);

                // Remove from remaining list
                remaining.retain(|x| x != &key);

                // Verify all other remaining versions still exist
                for rem in &remaining {
                    let rem_ref_name = format!("refs/atom/d/{}", rem);
                    let has_rem = store.source.repo().try_find_reference(&rem_ref_name)
                        .unwrap()
                        .is_some();
                    prop_assert!(has_rem);
                }
            }
        }
    }
}

fn map_components_to_path(components: &[u8], kind: u8) -> Option<String> {
    if components.is_empty() {
        return None;
    }
    // Limit depth to avoid ridiculously deep trees or stack overflows
    let depth = std::cmp::min(components.len(), 5);
    let mut parts = Vec::new();
    for component in components.iter().take(depth) {
        let name = match component % 4 {
            0 => "dir_a",
            1 => "dir_b",
            2 => "dir_c",
            _ => "dir_d",
        };
        parts.push(name);
    }
    // The last component is the file name
    let file_name = match kind % 3 {
        0 => "file_x.txt",
        1 => "file_y.sh",
        _ => "file_z.lnk",
    };
    parts.push(file_name);
    Some(parts.join("/"))
}

#[derive(bolero::TypeGenerator, Debug, Clone)]
struct FuzzFile {
    components: Vec<u8>,
    content: Vec<u8>,
    kind: u8,
}

#[test]
fn test_atom_content_bolero() {
    bolero::check!()
        .with_type::<Vec<FuzzFile>>()
        .for_each(|fuzz_files| {
            if fuzz_files.is_empty() {
                return;
            }

            let temp_dir = TempDir::new().unwrap();
            let repo = gix::init(temp_dir.path()).unwrap();

            let mut files = std::collections::HashMap::new();
            for f in fuzz_files {
                if let Some(path) = map_components_to_path(&f.components, f.kind) {
                    files.insert(path, f.clone());
                }
            }

            if files.is_empty() {
                return;
            }

            let mut tree_entries = std::collections::HashMap::new();

            for (path, f) in &files {
                let kind_mod = f.kind % 3;
                let data = if f.content.is_empty() {
                    b"default".to_vec()
                } else {
                    f.content.clone()
                };

                let blob_oid = repo
                    .write_object(gix::objs::Blob { data })
                    .unwrap()
                    .detach();

                let mode = match kind_mod {
                    0 => EntryKind::Blob.into(),
                    1 => EntryKind::BlobExecutable.into(),
                    _ => EntryKind::Link.into(),
                };

                let parts: Vec<&str> = path.split('/').collect();

                let parent_path = parts[0..parts.len() - 1].join("/");
                let filename = parts.last().unwrap().to_string();

                tree_entries
                    .entry(parent_path)
                    .or_insert_with(Vec::new)
                    .push(Entry {
                        mode,
                        filename: filename.into(),
                        oid: blob_oid,
                    });
            }

            let mut parent_paths: Vec<String> = tree_entries.keys().cloned().collect();
            parent_paths.sort_by_key(|p| std::cmp::Reverse(p.len()));

            for p_path in parent_paths {
                if p_path.is_empty() {
                    continue;
                }
                let mut entries = tree_entries.remove(&p_path).unwrap();
                entries.sort();
                let tree_oid = repo
                    .write_object(gix::objs::Tree { entries })
                    .unwrap()
                    .detach();

                let parts: Vec<&str> = p_path.split('/').collect();
                let parent_of_p = parts[0..parts.len() - 1].join("/");
                let dirname = parts.last().unwrap().to_string();

                tree_entries
                    .entry(parent_of_p)
                    .or_insert_with(Vec::new)
                    .push(Entry {
                        mode: EntryKind::Tree.into(),
                        filename: dirname.into(),
                        oid: tree_oid,
                    });
            }

            let mut root_entries = tree_entries.remove("").unwrap_or_default();
            root_entries.sort();
            let root_tree_oid = repo
                .write_object(gix::objs::Tree {
                    entries: root_entries,
                })
                .unwrap()
                .detach();

            let source = GitSource::new(repo.clone());
            let id = AtomId::new(
                atom_core::Anchor::new(vec![0; 20]),
                Label::try_from("test-pkg").unwrap(),
            );

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let content_entries = rt.block_on(async {
                source
                    .content(&id, root_tree_oid.as_bytes())
                    .await
                    .unwrap()
                    .unwrap()
            });

            // 1. Verify children-before-parents ordering
            let path_indices: std::collections::HashMap<String, usize> = content_entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let p = match entry {
                        ContentEntry::Regular { path, .. } => path,
                        ContentEntry::Symlink { path, .. } => path,
                        ContentEntry::Directory { path } => path,
                    };
                    (p.clone(), i)
                })
                .collect();

            for entry in &content_entries {
                let path = match entry {
                    ContentEntry::Regular { path, .. } => path,
                    ContentEntry::Symlink { path, .. } => path,
                    ContentEntry::Directory { path } => path,
                };

                if let Some(idx) = path.rfind('/') {
                    let parent = &path[..idx];
                    if let Some(&parent_idx) = path_indices.get(parent) {
                        let self_idx = *path_indices.get(path).unwrap();
                        assert!(
                            self_idx < parent_idx,
                            "Child {} (idx {}) must be before parent {} (idx {})",
                            path,
                            self_idx,
                            parent,
                            parent_idx
                        );
                    }
                }
            }

            // 2. Verify write_content_tree produces identical OID
            let store = GitStore::new(repo);
            let dest_repo = store.source.repo();
            let reconstructed = store
                .write_content_tree(&dest_repo, &content_entries)
                .unwrap();
            assert_eq!(reconstructed, root_tree_oid);
        });
}

#[tokio::test]
async fn test_atom_content_walk_and_reconstruct() {
    let (_dir, repo, _genesis_oid) = setup_test_repo();

    let file1_data = b"hello from file1";
    let script_data = b"echo hello";
    let sym_target = b"a/b/file1.txt";

    let file1_blob = repo
        .write_object(Blob {
            data: file1_data.to_vec(),
        })
        .unwrap()
        .detach();
    let script_blob = repo
        .write_object(Blob {
            data: script_data.to_vec(),
        })
        .unwrap()
        .detach();
    let sym_blob = repo
        .write_object(Blob {
            data: sym_target.to_vec(),
        })
        .unwrap()
        .detach();

    // Build 'b' tree
    let b_tree = repo
        .write_object(Tree {
            entries: vec![Entry {
                mode: EntryKind::Blob.into(),
                filename: "file1.txt".into(),
                oid: file1_blob,
            }],
        })
        .unwrap()
        .detach();

    // Build 'a' tree
    let a_tree = repo
        .write_object(Tree {
            entries: vec![Entry {
                mode: EntryKind::Tree.into(),
                filename: "b".into(),
                oid: b_tree,
            }],
        })
        .unwrap()
        .detach();

    // Build root tree
    let root_tree = repo
        .write_object(Tree {
            entries: vec![
                Entry {
                    mode: EntryKind::Tree.into(),
                    filename: "a".into(),
                    oid: a_tree,
                },
                Entry {
                    mode: EntryKind::BlobExecutable.into(),
                    filename: "script.sh".into(),
                    oid: script_blob,
                },
                Entry {
                    mode: EntryKind::Link.into(),
                    filename: "sym.txt".into(),
                    oid: sym_blob,
                },
            ],
        })
        .unwrap()
        .detach();

    // Walk this tree using GitSource::content
    let source = GitSource::new(repo.clone());
    let id = AtomId::new(
        atom_core::Anchor::new(vec![0; 20]),
        Label::try_from("test-package").unwrap(),
    );

    let content_entries = source
        .content(&id, root_tree.as_bytes())
        .await
        .unwrap()
        .unwrap();

    let mut file1_idx = None;
    let mut b_idx = None;
    let mut a_idx = None;
    let mut script_idx = None;
    let mut sym_idx = None;

    for (i, entry) in content_entries.iter().enumerate() {
        match entry {
            ContentEntry::Regular {
                path,
                data,
                executable,
            } => {
                if path == "a/b/file1.txt" {
                    assert_eq!(data, file1_data);
                    assert!(!executable);
                    file1_idx = Some(i);
                } else if path == "script.sh" {
                    assert_eq!(data, script_data);
                    assert!(executable);
                    script_idx = Some(i);
                }
            },
            ContentEntry::Symlink { path, target } => {
                if path == "sym.txt" {
                    assert_eq!(target, sym_target);
                    sym_idx = Some(i);
                }
            },
            ContentEntry::Directory { path } => {
                if path == "a/b" {
                    b_idx = Some(i);
                } else if path == "a" {
                    a_idx = Some(i);
                }
            },
        }
    }

    assert!(file1_idx.is_some());
    assert!(b_idx.is_some());
    assert!(a_idx.is_some());
    assert!(script_idx.is_some());
    assert!(sym_idx.is_some());

    // Verify children-before-parents ordering
    assert!(
        file1_idx.unwrap() < b_idx.unwrap(),
        "file1.txt must be before dir a/b"
    );
    assert!(
        b_idx.unwrap() < a_idx.unwrap(),
        "dir a/b must be before dir a"
    );

    // Verify reconstruction:
    // Use GitStore to reconstruct the tree from the walked entries.
    let store = GitStore::new(repo);
    let dest_repo = store.source.repo();
    let reconstructed_tree = store
        .write_content_tree(&dest_repo, &content_entries)
        .unwrap();
    assert_eq!(
        reconstructed_tree, root_tree,
        "Reconstructed tree OID must be bit-identical to original"
    );
}
