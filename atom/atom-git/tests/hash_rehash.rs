//! Integration tests for `content_hash`'s consumer-level
//! MUST-verify-when-present obligation (`docs/specs/atom-transactions.md`
//! `[content-hash-obligation]` tier 2) — `GitStore::ingest` recomputes
//! `atom_core::content_hash` over the resolved content whenever a
//! publish carries one, and rejects on mismatch.
//!
//! `GitRegistry::publish()` does not yet support setting `content_hash`
//! pre-signature (see `.ledger/state/hash-decision-brief.md` and this
//! node's own halt report on the publish-side seam) — these tests
//! hand-construct and sign a `PublishPayload` carrying `content_hash`
//! directly, using the same public primitives `GitRegistry::publish`
//! itself uses internally (`gix_util::write_publish_tag`,
//! `coz_rs::sign_json`), by re-signing a real `registry.publish()`
//! tag's payload with `content_hash` added. This never reaches into
//! `atom-git/src/registry.rs`.

use atom_core::{AtomId, AtomRegistry, AtomStore, Label, RawVersion};
use atom_git::source::CozMessageEnvelope;
use atom_git::{GitRegistry, GitStore};
use atom_id::PublishPayload;
use coz_rs::{Alg, Ed25519, SigningKey};
use gix::hash::ObjectId;
use gix::objs::bstr::BString;
use gix::objs::tree::{Entry, EntryKind};
use gix::objs::{Blob, Tree};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};
use tempfile::TempDir;

/// Set up a test Git repository with a genesis commit — mirrors
/// `integration.rs`/`store_keying.rs`'s own established fixture.
fn setup_test_repo() -> (TempDir, gix::Repository, ObjectId) {
    let dir = TempDir::new().unwrap();
    let repo = gix::init(dir.path()).unwrap();

    let sig = gix::actor::SignatureRef::default();
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

/// Create a commit with a single nested file, parented on `parents`.
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
    repo.commit_as(
        gix::actor::SignatureRef::default(),
        gix::actor::SignatureRef::default(),
        "refs/heads/master",
        message,
        current_oid,
        parents,
    )
    .unwrap()
    .detach()
}

/// The `ContentEntry` list `GitSource::content` will reconstruct for the
/// single-file fixture `create_commit` above writes — children before
/// parents, exactly mirroring `walk_git_tree_recursive`'s own order.
fn fixture_content_entries(data: &[u8]) -> Vec<atom_core::ContentEntry> {
    vec![
        atom_core::ContentEntry::Regular {
            path: "src/main.rs".to_string(),
            data: data.to_vec(),
            executable: false,
        },
        atom_core::ContentEntry::Directory {
            path: "src".to_string(),
        },
    ]
}

/// Claim, publish one version normally via the real `GitRegistry` API,
/// then re-sign that publish tag's payload with `content_hash` set to
/// `content_hash_override` (or removed, if `None`), overwriting the
/// version ref in place. Returns the registry (as an `AtomContent`
/// source for `GitStore::ingest`) and the label/version used.
fn publish_then_resign_with_content_hash(
    content_hash_override: Option<Vec<u8>>,
) -> (TempDir, GitRegistry, AtomId, RawVersion) {
    let (_reg_dir, reg_repo, reg_genesis_oid) = setup_test_repo();

    let sk = SigningKey::<Ed25519>::generate();
    let prv = sk.private_key_bytes().to_vec();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();

    let registry = GitRegistry::new(
        reg_repo,
        prv.clone(),
        pub_key.clone(),
        Alg::Ed25519,
        "cargo".to_string(),
    );
    let reg_repo = registry.source.repo();

    // `[owner-authorization-delegated]`: a `single-key` owner's `value`
    // MUST be the key's thumbprint, not the raw public key.
    let owner = atom_id::OwnerRef::single_key(sk.thumbprint());
    let founding_czd = registry
        .charter(std::slice::from_ref(&owner), b"src-rev", None)
        .unwrap();
    let anchor = atom_core::Anchor::new(founding_czd.as_bytes().to_vec());
    let label = Label::try_from("pkg").unwrap();
    let id = AtomId::new(anchor, label.clone());

    let claim_czd = registry.claim(&id, &owner).unwrap();

    let ver_commit_oid = create_commit(
        &reg_repo,
        "v1.0.0 src",
        "src/main.rs",
        b"fn main() {}",
        vec![reg_genesis_oid],
    );
    let ver_commit_obj = reg_repo
        .find_object(ver_commit_oid)
        .unwrap()
        .try_into_commit()
        .unwrap();
    let ver_tree_oid = ver_commit_obj.tree_id().unwrap();
    let version = RawVersion::new("1.0.0".to_string());

    registry
        .publish(
            &id,
            &claim_czd,
            &version,
            ver_tree_oid.as_bytes(),
            ver_commit_oid.as_bytes(),
            "Cargo.toml",
        )
        .unwrap();

    // Read back the real publish tag, decode its payload, set
    // content_hash, re-serialize, re-sign with the same key/alg, and
    // overwrite the version ref to point at a freshly-written tag object
    // carrying the modified, re-signed message. The tag's target
    // (the atom commit) and everything else about the transaction is
    // untouched -- only content_hash is added.
    let version_ref_name = format!("refs/atom/pub/{}/{}", label, version.as_str());
    let version_ref = reg_repo
        .try_find_reference(&version_ref_name)
        .unwrap()
        .unwrap();
    let tag_oid = version_ref.id().detach();
    let tag_obj = reg_repo.find_object(tag_oid).unwrap();
    let tag = tag_obj.try_into_tag().unwrap();
    let tag_decoded = tag.decode().unwrap();
    let target_oid = tag_decoded.target().to_owned();
    let msg_str = tag_decoded.message.to_string();

    let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str).unwrap();
    let pay_value = serde_json::to_value(&envelope.pay).unwrap();
    let mut payload: PublishPayload = serde_json::from_value(pay_value).unwrap();
    payload.content_hash = content_hash_override;

    let pay_val = serde_json::to_value(&payload).unwrap();
    let pay_map: indexmap::IndexMap<String, serde_json::Value> =
        serde_json::from_value(pay_val).unwrap();
    let pay_bytes = serde_json::to_vec(&pay_map).unwrap();
    let (sig, _cad) = coz_rs::sign_json(&pay_bytes, "Ed25519", &prv, &pub_key).unwrap();
    let new_envelope = CozMessageEnvelope {
        pay: pay_map,
        sig,
        key: Some(pub_key.clone()),
    };
    let new_msg = serde_json::to_string(&new_envelope).unwrap();

    let new_tag_oid = atom_git::gix_util::write_publish_tag(
        &reg_repo,
        &format!("{}-{}", id.label(), version.as_str()),
        target_oid,
        gix::object::Kind::Commit,
        atom_git::gix_util::blank_signature(),
        new_msg,
    )
    .unwrap();

    let version_ref_fullname = FullName::try_from(version_ref_name.as_str()).unwrap();
    reg_repo
        .edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: BString::from("test: re-sign with content_hash"),
                },
                expected: PreviousValue::Any,
                new: Target::Object(new_tag_oid),
            },
            name: version_ref_fullname,
            deref: false,
        })
        .unwrap();

    (_reg_dir, registry, id, version)
}

/// A resolved publish carrying a `content_hash` that matches the
/// recomputed digest of its resolved content is accepted — ingestion
/// succeeds and the version becomes resolvable from the store.
#[tokio::test]
async fn ingest_accepts_matching_content_hash() {
    let entries = fixture_content_entries(b"fn main() {}");
    let expected = atom_core::content_hash(&entries).unwrap().to_vec();

    let (_reg_dir, registry, id, version) = publish_then_resign_with_content_hash(Some(expected));

    let (_store_dir, store_repo, _genesis) = setup_test_repo();
    let store = GitStore::new(store_repo);
    store
        .ingest(&registry.source)
        .await
        .expect("ingest must accept a matching content_hash");

    // `GitStore::ingest` (atom-git/src/store.rs, outside this node's
    // surface) does not yet propagate the charter chain into the
    // destination store -- only `refs/atom/claims/d/*`, `refs/atom/d/*`,
    // and dev refs. Resolution now correctly requires a resolvable charter
    // (`[claim-charter-authorization]`), so replant the SAME founding
    // charter this fixture already wrote into the source registry,
    // directly into the store repo, to keep this content-hash-focused
    // test isolated from that separate, already-known gap.
    let reg_repo = registry.source.repo();
    let source_charter_ref_name = atom_git::charter_store::charter_ref_name(id.anchor().as_bytes());
    let charter_ref = reg_repo
        .try_find_reference(&source_charter_ref_name)
        .unwrap()
        .expect("the fixture's own charter() call must have written this ref");
    let charter_msg = reg_repo
        .find_object(charter_ref.id().detach())
        .unwrap()
        .try_into_commit()
        .unwrap()
        .message_raw_sloppy()
        .to_string();
    let store_repo = store.source.repo();
    let store_charter_oid =
        atom_git::gix_util::write_charter_commit(&store_repo, charter_msg).unwrap();
    let store_charter_ref_name = atom_git::charter_store::charter_ref_name(id.anchor().as_bytes());
    let store_charter_ref_fullname = FullName::try_from(store_charter_ref_name.as_str()).unwrap();
    store_repo
        .edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::Any,
                new: Target::Object(store_charter_oid),
            },
            name: store_charter_ref_fullname,
            deref: false,
        })
        .unwrap();

    use atom_core::AtomSource;
    let resolved = store.resolve(&id).await.unwrap();
    assert!(resolved.is_some(), "ingested version must be resolvable");
    let versions = resolved.unwrap().versions;
    assert!(versions.iter().any(|v| v.version == version));
}

/// A resolved publish carrying a `content_hash` that does NOT match the
/// recomputed digest of its resolved content is rejected —
/// `[content-hash-obligation]` tier 2 is not optional.
#[tokio::test]
async fn ingest_rejects_mismatched_content_hash() {
    let tampered = vec![0xFFu8; 32]; // never equals a real BLAKE3 digest of this fixture
    let (_reg_dir, registry, _id, _version) = publish_then_resign_with_content_hash(Some(tampered));

    let (_store_dir, store_repo, _genesis) = setup_test_repo();
    let store = GitStore::new(store_repo);
    let err = store
        .ingest(&registry.source)
        .await
        .expect_err("ingest must reject a mismatched content_hash");
    assert!(
        err.to_string().contains("content_hash"),
        "error must name content_hash as the cause, got: {err}"
    );
}

/// A resolved publish with `content_hash` absent is unaffected — no
/// rehash is ever forced (`[content-hash-obligation]` tier 1,
/// schema-optional). Regression guard for this node's own change: prior
/// behavior (no content_hash field at all) must be preserved.
#[tokio::test]
async fn ingest_unaffected_when_content_hash_absent() {
    let (_reg_dir, registry, _id, _version) = publish_then_resign_with_content_hash(None);

    let (_store_dir, store_repo, _genesis) = setup_test_repo();
    let store = GitStore::new(store_repo);
    store
        .ingest(&registry.source)
        .await
        .expect("ingest must succeed when content_hash is absent, exactly as before");
}
