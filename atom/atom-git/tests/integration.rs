use std::fs;

use atom_core::{
    AtomEntry, AtomId, AtomRegistry, AtomSource, AtomStore, AtomVersion, Label, RawVersion,
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

    let entry = Entry {
        mode: EntryKind::Blob.into(),
        filename: file_name.into(),
        oid: blob_oid,
    };

    let tree = Tree {
        entries: vec![entry],
    };
    let tree_oid = repo.write_object(tree).unwrap().detach();

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
    let claim_ref = registry
        .source
        .repo
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
        registry.source.repo,
        next_prv,
        next_pub.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );

    let next_claim_czd = registry_rotated.claim(&id, &next_pub).unwrap();
    assert_ne!(claim_czd, next_claim_czd);

    // Check that the next claim has the previous claim as a parent (claim rotation chain)
    let claim_commit_obj = registry_rotated
        .source
        .repo
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

    let anchor = atom_core::Anchor::new(genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    // 1. Claim package
    let claim_czd = registry.claim(&id, &pub_key).unwrap();

    // Create a version workspace state tree
    let ver_commit_oid = create_commit(
        &registry.source.repo,
        "v1.0.0 src",
        "lib.rs",
        b"fn test() {}",
        vec![genesis_oid],
    );
    let ver_commit_obj = registry
        .source
        .repo
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
    let tag_ref = registry
        .source
        .repo
        .try_find_reference("refs/atom/pub/my-package/1.0.0")
        .unwrap()
        .unwrap();
    let tag_oid = tag_ref.id().detach();

    // Peel publish reference back to the deterministic commit
    let peeled = registry
        .source
        .repo
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

#[test]
fn test_local_ingest() {
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

    let anchor = atom_core::Anchor::new(reg_genesis_oid.as_bytes().to_vec());
    let label = Label::try_from("pkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());

    let claim_czd = registry.claim(&id, &pub_key).unwrap();

    let ver_commit_oid = create_commit(
        &registry.source.repo,
        "v1.0.0 src",
        "src/main.rs",
        b"main",
        vec![reg_genesis_oid],
    );
    let ver_commit_obj = registry
        .source
        .repo
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
    store.ingest(&registry.source).unwrap();

    // 3. Verify store references are written by claim czd (Step 5)
    let claim_czd_hex = ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
        .to_hex()
        .to_string();
    let store_claim_ref_name = format!("refs/atom/claims/d/{}", claim_czd_hex);
    let store_version_ref_name = format!("refs/atom/d/{}/1.0.0", claim_czd_hex);

    let store_claim_ref = store
        .source
        .repo
        .try_find_reference(&store_claim_ref_name)
        .unwrap()
        .unwrap();
    let _store_version_ref = store
        .source
        .repo
        .try_find_reference(&store_version_ref_name)
        .unwrap()
        .unwrap();

    assert_eq!(
        store_claim_ref.id().detach(),
        ObjectId::from_bytes_or_panic(claim_czd.as_bytes())
    );

    // Verify resolving the store source yields the correct package info
    let query_source = GitSource::new(gix::open(store.source.repo.path()).unwrap());
    let resolved_entry = query_source.resolve(&id).unwrap().unwrap();
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
    let digest = atom_core::AtomDigest::compute(&dev_id, coz_rs::Alg::ES256).unwrap();
    let digest_str = digest.to_string();
    let dev_ref_name = format!("refs/atom/dev/{}/0.1.0-dev", digest_str);
    let dev_ref = store
        .source
        .repo
        .try_find_reference(&dev_ref_name)
        .unwrap()
        .unwrap();

    // The ref should point to a commit carrying our files
    let peeled = store
        .source
        .repo
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
    let other_tree_oid = registry
        .source
        .repo
        .write_object(empty_tree)
        .unwrap()
        .detach();
    let other_genesis_oid = registry
        .source
        .repo
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

#[cfg(test)]
mod proptests {
    use atom_git::gix_util;
    use gix::objs::{Commit, Tree};
    use proptest::prelude::*;
    use tempfile::TempDir;

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
    }
}
