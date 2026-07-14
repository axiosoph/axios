//! Conformance tests for the store's `blake3(publish_czd)` version-ref
//! keying scheme (`docs/specs/git-storage-format.md`
//! `[store-ref-by-publish-czd]`, and the conformance-table rows
//! `store-claim-disambiguation` / `store-ref-by-publish-czd`,
//! `git-storage-format.md:947-951`).
//!
//! These exercise the real write path (`GitRegistry::claim`/`publish` +
//! `GitStore::ingest`) rather than hand-writing store refs, then
//! independently recompute each publish's `czd` from its own signed
//! content on the SOURCE side (never trusted from whatever key the store
//! wrote under) to derive the expected `refs/atom/d/{blake3(publish_czd)}`
//! path — mirroring `integration.rs`'s established
//! independent-recompute idiom.

use atom_core::{AtomId, AtomRegistry, AtomStore, Label, RawVersion};
use atom_git::{GitRegistry, GitStore};
use coz_rs::{Alg, Ed25519, SigningKey};
use gix::actor::SignatureRef;
use gix::hash::ObjectId;
use gix::objs::tree::{Entry, EntryKind};
use gix::objs::{Blob, Tree};
use tempfile::TempDir;

/// Set up a test Git repository with user config and a genesis commit.
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

    let repo = gix::open(dir.path()).unwrap();
    (dir, repo, genesis_oid)
}

/// Create a commit with a single file (possibly nested, `/`-separated),
/// parented on `parents`.
fn create_commit(
    repo: &gix::Repository,
    message: &str,
    file_name: &str,
    content: &[u8],
    parents: Vec<ObjectId>,
) -> ObjectId {
    let blob_oid = repo
        .write_object(Blob {
            data: content.to_vec(),
        })
        .unwrap()
        .detach();

    let parts: Vec<&str> = file_name.split('/').collect();
    let mut current_oid = blob_oid;
    for i in (0..parts.len()).rev() {
        let mode = if i == parts.len() - 1 {
            EntryKind::Blob.into()
        } else {
            EntryKind::Tree.into()
        };
        let tree = Tree {
            entries: vec![Entry {
                mode,
                filename: parts[i].into(),
                oid: current_oid,
            }],
        };
        current_oid = repo.write_object(tree).unwrap().detach();
    }
    let tree_oid = current_oid;

    let sig = SignatureRef::default();
    repo.commit_as(sig, sig, "refs/heads/master", message, tree_oid, parents)
        .unwrap()
        .detach()
}

/// Recompute a signed publish tag's `czd` independently from its own
/// wire bytes -- the spec-defined digest of `(cad, sig)`, never the git
/// object id it happens to be stored at.
fn publish_czd_of(msg_str: &str, alg: &str) -> coz_rs::Czd {
    let envelope: atom_git::source::CozMessageEnvelope = serde_json::from_str(msg_str).unwrap();
    let pay_bytes = serde_json::to_vec(&envelope.pay).unwrap();
    atom_id::czd_for_alg(&pay_bytes, &envelope.sig, alg).unwrap()
}

/// The store's flat ref-path segment for a given publish czd:
/// `hex(blake3(publish_czd))`.
fn store_key_hex(publish_czd: &coz_rs::Czd) -> String {
    atom_git::store::hex_encode(blake3::hash(publish_czd.as_bytes()).as_bytes())
}

/// Claim one atom identity and publish two distinct versions
/// ("1.0.0", "2.0.0") under it in a fresh registry, then ingest into a
/// fresh store. Returns the store, and each version's independently
/// recomputed flat store-ref key.
async fn ingest_two_versions() -> (TempDir, GitStore, String, String) {
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

    // `claim()`'s anchor check now resolves a real founding charter
    // (`[anchor-resolvable]`) rather than deriving one from git ancestry --
    // charter the source first via the real API. `[owner-authorization-
    // delegated]`: a `single-key` owner's `value` MUST be the key's
    // thumbprint, not the raw public key.
    let owner = atom_id::OwnerRef::single_key(sk.thumbprint());
    let founding_czd = registry
        .charter(std::slice::from_ref(&owner), b"src-rev", None)
        .unwrap();
    let anchor = atom_core::Anchor::new(founding_czd.as_bytes().to_vec());
    let label = Label::try_from("pkg").unwrap();
    let id = AtomId::new(anchor.clone(), label.clone());

    let claim_czd = registry.claim(&id, &owner).unwrap();

    let mut keys = Vec::new();
    let mut parent = reg_genesis_oid;
    for (idx, ver) in ["1.0.0", "2.0.0"].into_iter().enumerate() {
        let ver_commit_oid = create_commit(
            &reg_repo,
            &format!("v{ver} src"),
            "src/main.rs",
            format!("main-{idx}").as_bytes(),
            vec![parent],
        );
        parent = ver_commit_oid;
        let ver_commit_obj = reg_repo
            .find_object(ver_commit_oid)
            .unwrap()
            .try_into_commit()
            .unwrap();
        let ver_tree_oid = ver_commit_obj.tree_id().unwrap();

        registry
            .publish(
                &id,
                &claim_czd,
                &RawVersion::new(ver.to_string()),
                ver_tree_oid.as_bytes(),
                ver_commit_oid.as_bytes(),
                "Cargo.toml",
            )
            .unwrap();

        // Independently recompute this publish's czd from the registry's
        // own publish tag, never from whatever key the store later writes
        // under.
        let pub_ref_name = format!("refs/atom/pub/{}/{}", label, ver);
        let pub_ref = reg_repo.try_find_reference(&pub_ref_name).unwrap().unwrap();
        let tag_obj = reg_repo.find_object(pub_ref.id().detach()).unwrap();
        let tag = tag_obj.try_into_tag().unwrap();
        let msg_str = tag.decode().unwrap().message.to_string();
        let publish_czd = publish_czd_of(&msg_str, "Ed25519");
        keys.push(store_key_hex(&publish_czd));
    }

    let (_store_dir, store_repo, _store_genesis_oid) = setup_test_repo();
    let store = GitStore::new(store_repo);
    store.ingest(&registry.source).await.unwrap();

    (_store_dir, store, keys[0].clone(), keys[1].clone())
}

/// `[store-ref-by-publish-czd]`: a stored version is resolvable by
/// looking up `refs/atom/d/{blake3(publish_czd)}` directly.
#[tokio::test]
async fn store_ref_by_publish_czd() {
    let (_dir, store, key_v1, key_v2) = ingest_two_versions().await;
    let repo = store.source.repo();

    for key in [&key_v1, &key_v2] {
        let ref_name = format!("refs/atom/d/{}", key);
        assert!(
            repo.try_find_reference(&ref_name).unwrap().is_some(),
            "expected store ref {} (keyed by blake3(publish_czd)) to exist",
            ref_name
        );
    }
}

/// `[store-claim-disambiguation]`: two publishes sharing the same
/// `AtomId` but distinct versions key to distinct
/// `blake3(publish_czd)` refs -- no collision.
#[tokio::test]
async fn store_claim_disambiguation() {
    let (_dir, store, key_v1, key_v2) = ingest_two_versions().await;

    assert_ne!(
        key_v1, key_v2,
        "distinct publishes under the same claim must key to distinct blake3(publish_czd) refs"
    );

    let repo = store.source.repo();
    let v1_oid = repo
        .try_find_reference(&format!("refs/atom/d/{}", key_v1))
        .unwrap()
        .unwrap()
        .id()
        .detach();
    let v2_oid = repo
        .try_find_reference(&format!("refs/atom/d/{}", key_v2))
        .unwrap()
        .unwrap()
        .id()
        .detach();
    assert_ne!(
        v1_oid, v2_oid,
        "distinct publishes must target distinct tag objects"
    );
}

/// `evict_version`'s sibling-scan must work against the flat ref shape:
/// evicting one of two versions under a shared claim leaves the claim ref
/// intact; evicting the last one cleans it up.
#[tokio::test]
async fn evict_sibling_scan_flat_refs() {
    let (_dir, store, key_v1, key_v2) = ingest_two_versions().await;
    let repo = store.source.repo();

    let claim_refs: Vec<String> = repo
        .references()
        .unwrap()
        .prefixed("refs/atom/claims/d/")
        .unwrap()
        .map(|r| r.unwrap().name().as_bstr().to_string())
        .collect();
    assert_eq!(
        claim_refs.len(),
        1,
        "expected exactly one ingested claim ref"
    );
    let claim_ref_name = claim_refs[0].clone();

    store.evict_version(&key_v1).unwrap();
    assert!(
        repo.try_find_reference(&format!("refs/atom/d/{}", key_v1))
            .unwrap()
            .is_none(),
        "evicted version's ref must be gone"
    );
    assert!(
        repo.try_find_reference(&format!("refs/atom/d/{}", key_v2))
            .unwrap()
            .is_some(),
        "sibling version's ref must survive"
    );
    assert!(
        repo.try_find_reference(&claim_ref_name).unwrap().is_some(),
        "claim ref must survive while a sibling version remains"
    );

    store.evict_version(&key_v2).unwrap();
    assert!(
        repo.try_find_reference(&format!("refs/atom/d/{}", key_v2))
            .unwrap()
            .is_none(),
        "last evicted version's ref must be gone"
    );
    assert!(
        repo.try_find_reference(&claim_ref_name).unwrap().is_none(),
        "claim ref must be cleaned up once no versions reference it"
    );
}
