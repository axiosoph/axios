//! Shared fixtures for the backend-conformance battery.
//!
//! Deliberately independent of `atom-git/tests/integration.rs`'s
//! helpers (private to that crate's own test binary) but matching its
//! idiom: a fresh `TempDir`-backed git repo with a deterministic empty
//! genesis commit, one blank-signature actor, no network, no clock
//! reads baked into any assertion (c4-deterministic).

use atom_core::{AtomId, Label};
use atom_git::GitRegistry;
use atom_id::Anchor;
use coz_rs::{Alg, Ed25519, SigningKey};
use gix::actor::SignatureRef;
use gix::hash::ObjectId;
use gix::objs::Tree;

/// A fresh git repo with one empty genesis commit on `refs/heads/master`.
pub fn setup_repo() -> (tempfile::TempDir, gix::Repository, ObjectId) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let repo = gix::init(dir.path()).expect("gix init");

    let sig = SignatureRef::default();
    let empty_tree = Tree {
        entries: Vec::new(),
    };
    let tree_oid = repo
        .write_object(empty_tree)
        .expect("write empty tree")
        .detach();

    let genesis_oid = repo
        .commit_as(
            sig,
            sig,
            "refs/heads/master",
            "genesis commit",
            tree_oid,
            Vec::<ObjectId>::new(),
        )
        .expect("commit genesis")
        .detach();

    let repo = gix::open(dir.path()).expect("reopen repo");
    (dir, repo, genesis_oid)
}

/// A `GitRegistry` wrapping `repo`, signing as a fresh Ed25519 keypair.
pub fn new_registry(repo: gix::Repository, pkg: &str) -> GitRegistry {
    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();
    GitRegistry::new(repo, prv, pub_key, Alg::Ed25519, pkg.to_string())
}

/// An `AtomId` anchored at `genesis_oid` under `label`.
pub fn atom_id(genesis_oid: ObjectId, label: &str) -> AtomId {
    let anchor = Anchor::new(genesis_oid.as_bytes().to_vec());
    AtomId::new(anchor, Label::try_from(label).expect("valid label"))
}

/// A parentless-tree-reusing child commit of `parent`, for ancestry-chain
/// fixtures that don't care about tree content.
pub fn commit_child(repo: &gix::Repository, parent: ObjectId, message: &str) -> ObjectId {
    let sig = SignatureRef::default();
    let tree_oid = repo
        .find_object(parent)
        .expect("find parent")
        .try_into_commit()
        .expect("parent is a commit")
        .tree_id()
        .expect("parent tree id")
        .detach();
    repo.commit_as(
        sig,
        sig,
        "refs/heads/master",
        message,
        tree_oid,
        vec![parent],
    )
    .expect("commit child")
    .detach()
}
