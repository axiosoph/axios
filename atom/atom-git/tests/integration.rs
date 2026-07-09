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

#[test]
fn test_anchor_discovery() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    // Create a commit path
    let c1 = create_commit(&repo, "commit 1", "f1.txt", b"c1", vec![genesis_oid]);
    let c2 = create_commit(&repo, "commit 2", "f2.txt", b"c2", vec![c1]);

    // Derive anchor from c2
    let anchor = atom_git::gix_util::derive_anchor(&repo, c2).unwrap();
    assert_eq!(anchor, genesis_oid);
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
    let (_dir, repo, genesis_oid) = setup_test_repo();

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

    let anchor = atom_core::Anchor::new(genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // Initial claim
    let claim_czd = registry.claim(&id, &pub_key).unwrap();

    // Verify claim reference was created
    let repo = registry.source.repo();
    let claim_ref = repo
        .try_find_reference("refs/atom/claims/pub/my-package")
        .unwrap()
        .unwrap();
    assert_eq!(
        claim_ref.id().detach(),
        ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
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

    let next_claim_czd = registry_rotated.claim(&id, &next_pub).unwrap();
    assert_ne!(claim_czd, next_claim_czd);

    // Check that the next claim has the previous claim as a parent (claim rotation chain)
    let repo_rotated = registry_rotated.source.repo();
    let claim_commit_obj = repo_rotated
        .find_object(ObjectId::from_bytes_or_panic(next_claim_czd.as_bytes()))
        .unwrap();
    let claim_commit = claim_commit_obj.try_into_commit().unwrap();
    assert_eq!(claim_commit.parent_ids().count(), 1);
    assert_eq!(
        claim_commit.parent_ids().next().unwrap().detach(),
        ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
    );
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

    let anchor = atom_core::Anchor::new(genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // 1. Claim package
    let claim_czd = registry.claim(&id, &pub_key).unwrap();

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

    let anchor = atom_core::Anchor::new(reg_genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("pkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());

    let claim_czd = registry.claim(&id, &pub_key).unwrap();

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

    // 3. Verify store references are written by claim czd (Step 5)
    let claim_czd_hex = ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
        .to_hex()
        .to_string();
    let store_claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
    let store_version_ref_name = format!("refs/atom/d/{}/1.0.0", claim_czd_hex);

    let repo_store = store.source.repo();
    let store_claim_ref = repo_store
        .try_find_reference(&store_claim_ref_name)
        .unwrap()
        .unwrap();
    let _store_version_ref = repo_store
        .try_find_reference(&store_version_ref_name)
        .unwrap()
        .unwrap();

    assert_eq!(
        store_claim_ref.id().detach(),
        ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
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

    let anchor = atom_core::Anchor::new(genesis_oid.as_bytes().to_vec());
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
    let real_claim_czd = registry.claim(&id, &pub_key).unwrap();

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

#[test]
fn test_store_claim_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let repo = gix::init_bare(dir.path()).unwrap();

    let empty_tree_oid = repo
        .write_object(gix::objs::Tree::empty())
        .unwrap()
        .detach();

    let store = GitStore::new(repo);
    let claim_czd_hex = "0123456789abcdef0123456789abcdef01234567";

    // 1. Write claim reference refs/atom/claims/d/{claim_czd_hex}
    let claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
    let claim_fullname = gix::refs::FullName::try_from(claim_ref_name.as_str()).unwrap();
    let claim_edit = gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(empty_tree_oid),
        },
        name: claim_fullname,
        deref: false,
    };
    store.source.repo().edit_reference(claim_edit).unwrap();

    // 2. Write version 1 refs/atom/d/{claim_czd_hex}/1.0.0
    let v1_ref_name = format!("refs/atom/d/{}/1.0.0", claim_czd_hex);
    let v1_fullname = gix::refs::FullName::try_from(v1_ref_name.as_str()).unwrap();
    let v1_edit = gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(empty_tree_oid),
        },
        name: v1_fullname,
        deref: false,
    };
    store.source.repo().edit_reference(v1_edit).unwrap();

    // 3. Write version 2 refs/atom/d/{claim_czd_hex}/2.0.0
    let v2_ref_name = format!("refs/atom/d/{}/2.0.0", claim_czd_hex);
    let v2_fullname = gix::refs::FullName::try_from(v2_ref_name.as_str()).unwrap();
    let v2_edit = gix::refs::transaction::RefEdit {
        change: gix::refs::transaction::Change::Update {
            log: gix::refs::transaction::LogChange::default(),
            expected: gix::refs::transaction::PreviousValue::Any,
            new: gix::refs::Target::Object(empty_tree_oid),
        },
        name: v2_fullname,
        deref: false,
    };
    store.source.repo().edit_reference(v2_edit).unwrap();

    // Verify all references exist
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&claim_ref_name)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&v1_ref_name)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&v2_ref_name)
            .unwrap()
            .is_some()
    );

    // 4. Evict version 1.0.0
    store.evict_version(claim_czd_hex, "1.0.0").unwrap();

    // Verify version 1.0.0 is gone, but version 2.0.0 and claim remain
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&v1_ref_name)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&v2_ref_name)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&claim_ref_name)
            .unwrap()
            .is_some()
    );

    // 5. Evict version 2.0.0
    store.evict_version(claim_czd_hex, "2.0.0").unwrap();

    // Verify version 2.0.0 is gone, and since no versions are left, the claim is also deleted
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&v2_ref_name)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .source
            .repo()
            .try_find_reference(&claim_ref_name)
            .unwrap()
            .is_none()
    );
}

#[cfg(test)]
mod proptests {
    use atom_git::gix_util;
    use gix::objs::{Commit, Tree};
    use proptest::prelude::*;
    use tempfile::TempDir;

    use super::*;

    proptest! {
        #[test]
        fn test_anchor_derivation_pbt(
            root_count in 1..5usize,
            extra_commits in 0..10usize,
            oldest_root_index in 0..5usize,
        ) {
            let dir = TempDir::new().unwrap();
            let repo = gix::init(dir.path()).unwrap();
            let empty_tree_oid = repo.write_object(Tree { entries: Vec::new() }).unwrap().detach();

            let actual_root_count = root_count;
            let target_oldest_index = oldest_root_index % actual_root_count;

            let mut roots = Vec::new();
            for i in 0..actual_root_count {
                let timestamp = if i == target_oldest_index {
                    1000 // Oldest timestamp
                } else {
                    2000 + i as u32 * 100 // Newer timestamps
                };

                let sig = gix::actor::Signature {
                    name: "test".into(),
                    email: "test@example.com".into(),
                    time: gix::date::Time {
                        seconds: timestamp as i64,
                        offset: 0,
                    },
                };

                let root_commit = Commit {
                    tree: empty_tree_oid,
                    parents: Vec::new().into(),
                    author: sig.clone(),
                    committer: sig,
                    encoding: None,
                    message: "root commit".into(),
                    extra_headers: Vec::new(),
                };

                let root_oid = repo.write_object(root_commit).unwrap().detach();
                roots.push(root_oid);
            }

            // Create branch tips linking back to the roots
            let mut branch_tips = roots.clone();
            for i in 0..extra_commits {
                let root_idx = i % branch_tips.len();
                let parent = branch_tips[root_idx];

                let sig = gix_util::blank_signature();
                let commit = Commit {
                    tree: empty_tree_oid,
                    parents: vec![parent].into(),
                    author: sig.clone(),
                    committer: sig,
                    encoding: None,
                    message: format!("commit {}", i).into(),
                    extra_headers: Vec::new(),
                };

                let commit_oid = repo.write_object(commit).unwrap().detach();
                branch_tips[root_idx] = commit_oid;
            }

            // Merge all branches to guarantee reachability from a single head
            let sig = gix_util::blank_signature();
            let final_merge_commit = Commit {
                tree: empty_tree_oid,
                parents: branch_tips.into(),
                author: sig.clone(),
                committer: sig,
                encoding: None,
                message: "final merge".into(),
                extra_headers: Vec::new(),
            };
            let final_merge_oid = repo.write_object(final_merge_commit).unwrap().detach();

            // Derive anchor from the final merge commit
            let derived = atom_git::gix_util::derive_anchor(&repo, final_merge_oid).unwrap();
            let expected_oldest_root = roots[target_oldest_index];
            prop_assert_eq!(derived, expected_oldest_root);
        }

        #[test]
        fn test_store_ingest_evict_pbt(
            major_versions in prop::collection::vec(0..10u32, 1..5),
            shuffle_seed in 0..100usize,
        ) {
            let dir = TempDir::new().unwrap();
            let repo = gix::init_bare(dir.path()).unwrap();

            let empty_tree_oid = repo
                .write_object(gix::objs::Tree::empty())
                .unwrap()
                .detach();

            let store = GitStore::new(repo);
            let claim_czd_hex = "abcdef0123456789abcdef0123456789abcdef01";

            // 1. Write claim reference refs/atom/claims/d/{claim_czd_hex}
            let claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
            let claim_fullname = gix::refs::FullName::try_from(claim_ref_name.as_str()).unwrap();
            let claim_edit = gix::refs::transaction::RefEdit {
                change: gix::refs::transaction::Change::Update {
                    log: gix::refs::transaction::LogChange::default(),
                    expected: gix::refs::transaction::PreviousValue::Any,
                    new: gix::refs::Target::Object(empty_tree_oid),
                },
                name: claim_fullname,
                deref: false,
            };
            store.source.repo().edit_reference(claim_edit).unwrap();

            // Deduplicate versions
            let mut versions = major_versions;
            versions.sort();
            versions.dedup();
            let version_strs: Vec<String> = versions.iter().map(|v| format!("{}.0.0", v)).collect();

            // Write version references
            for ver_str in &version_strs {
                let ref_name = format!("refs/atom/d/{}/{}", claim_czd_hex, ver_str);
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
            for ver_str in &version_strs {
                let ref_name = format!("refs/atom/d/{}/{}", claim_czd_hex, ver_str);
                let has_ref = store.source.repo().try_find_reference(&ref_name)
                    .unwrap()
                    .is_some();
                prop_assert!(has_ref);
            }
            let has_claim = store.source.repo().try_find_reference(&claim_ref_name)
                .unwrap()
                .is_some();
            prop_assert!(has_claim);

            // Evict them in pseudo-random order determined by shuffle_seed
            let mut to_evict = version_strs.clone();
            let n = to_evict.len();
            for i in 0..n {
                let j = (shuffle_seed + i) % n;
                to_evict.swap(i, j);
            }

            let mut remaining = version_strs.clone();

            for ver_str in to_evict {
                store.evict_version(claim_czd_hex, &ver_str).unwrap();

                // Verify this version is gone
                let ref_name = format!("refs/atom/d/{}/{}", claim_czd_hex, ver_str);
                let has_ver = store.source.repo().try_find_reference(&ref_name)
                    .unwrap()
                    .is_some();
                prop_assert!(!has_ver);

                // Remove from remaining list
                remaining.retain(|x| x != &ver_str);

                // Verify all other remaining versions still exist
                for rem in &remaining {
                    let rem_ref_name = format!("refs/atom/d/{}/{}", claim_czd_hex, rem);
                    let has_rem = store.source.repo().try_find_reference(&rem_ref_name)
                        .unwrap()
                        .is_some();
                    prop_assert!(has_rem);
                }

                // Verify claim reference presence matches remaining status
                let has_claim = store.source.repo().try_find_reference(&claim_ref_name)
                    .unwrap()
                    .is_some();
                if !remaining.is_empty() {
                    prop_assert!(has_claim);
                } else {
                    prop_assert!(!has_claim);
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
    for i in 0..depth {
        let name = match components[i] % 4 {
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
