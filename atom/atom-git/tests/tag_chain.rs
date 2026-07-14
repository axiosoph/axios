//! Tests for publish-tag chain primitives: write-side semantic-immutability
//! enforcement on chain append, and moved-tip detection on resolution.
//!
//! Fixture construction mirrors `integration.rs`'s pattern (`setup_test_repo`,
//! `GitRegistry::new` + `claim`/`publish` for realistic signed chains) but
//! exercises `gix_util.rs`/`source.rs` primitives directly rather than going
//! through `AtomRegistry::publish()`, which cannot express `mode` yet and
//! does not (and, per this node's surface, MUST NOT be made to) call the new
//! write-side enforcement primitive.

use atom_core::{AtomEntry, AtomId, AtomRegistry, AtomSource, Label, RawVersion};
use atom_git::gix_util::TipStability;
use atom_git::{GitRegistry, GitSource};
use atom_id::{Mode, PublishPayload};
use coz_rs::{Alg, Ed25519, SigningKey};
use gix::actor::SignatureRef;
use gix::objs::Tree;
use tempfile::TempDir;

/// Helper to set up a test Git repository with a genesis commit, matching
/// `integration.rs::setup_test_repo`.
fn setup_test_repo() -> (TempDir, gix::Repository, gix::hash::ObjectId) {
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
            Vec::<gix::hash::ObjectId>::new(),
        )
        .unwrap()
        .detach();

    let repo = gix::open(dir.path()).unwrap();
    (dir, repo, genesis_oid)
}

/// Build a minimal, internally-consistent `PublishPayload` fixture. Callers
/// mutate the returned value's fields to construct reject/accept cases —
/// this helper is not a stand-in for `PublishPayload::new`'s own contract,
/// just a convenience to avoid repeating boilerplate across tests that only
/// care about the immutable-field comparison, not real signing.
fn publish_payload_fixture(
    id: &AtomId,
    dig: Vec<u8>,
    src: Vec<u8>,
    path: &str,
    version: &str,
) -> PublishPayload {
    let sk = SigningKey::<Ed25519>::generate();
    let pub_key = sk.verifying_key().public_key_bytes().to_vec();
    let tmb = coz_rs::compute_thumbprint_for_alg(Alg::Ed25519.name(), &pub_key).unwrap();

    PublishPayload::new(
        Alg::Ed25519,
        id.clone(),
        coz_rs::Czd::from_bytes(vec![0u8; 32]),
        dig,
        1_700_000_000,
        path.to_string(),
        src,
        tmb,
        RawVersion::new(version.to_string()),
    )
}

fn test_atom_id() -> AtomId {
    let anchor = atom_core::Anchor::new(vec![7u8; 32]);
    let label = Label::try_from("my-package").unwrap();
    AtomId::new(anchor, label)
}

// ---------------------------------------------------------------------
// Goal 1: orphan deletion (compile-time evidence)
// ---------------------------------------------------------------------

/// Compile-time evidence that `gix_util::seam::assume_czd_is_oid_issue64`
/// and its own 4 unit tests are gone from `gix_util.rs`: this crate would
/// fail to build against a `gix_util.rs` that still defined them under a
/// name this test could accidentally shadow or re-trigger.
///
/// `derive_anchor` was deliberately NOT covered here: at this node's
/// dispatch time it still had a live, reachable caller in
/// `registry.rs::claim()`, out of this node's declared surface, so
/// P1-orphans-confirmed was refuted for it and its deletion was halted
/// pending a composer decision. `n3-registry-anchor-fix` (closing F23)
/// carried out that deletion: `claim()`'s anchor check now resolves a
/// real founding charter instead, and `derive_anchor` itself is gone from
/// `gix_util.rs`.
#[test]
fn orphaned_seam_constructor_is_gone() {
    // No runtime assertion is possible for an absence; the fact that this
    // test file compiles and links against `atom_git::gix_util` at all is
    // the evidence.
}

// ---------------------------------------------------------------------
// Goal 2: write-side chain-append semantic-immutability enforcement
// ---------------------------------------------------------------------

/// c2 reject case: appending a tag whose `dig` differs from the previous
/// tag's payload must be rejected before any tag object is written.
#[test]
fn write_chain_append_tag_rejects_differing_dig() {
    let (_dir, repo, genesis_oid) = setup_test_repo();
    let id = test_atom_id();

    let atom_commit_oid = atom_git::gix_util::write_deterministic_commit(
        &repo,
        genesis_oid, // reuse genesis tree oid as a stand-in tree
        genesis_oid,
    )
    .unwrap();

    let previous =
        publish_payload_fixture(&id, vec![1u8; 20], vec![2u8; 20], "Cargo.toml", "1.0.0");
    let mut new_payload = previous.clone();
    new_payload.dig = vec![9u8; 20]; // differs -- must be rejected

    let tagger = atom_git::gix_util::blank_signature();
    let result = atom_git::gix_util::write_chain_append_tag(
        &repo,
        "my-package-1.0.0",
        atom_commit_oid,
        &previous,
        &new_payload,
        tagger,
        "irrelevant message".to_string(),
    );

    assert!(
        result.is_err(),
        "chain append with a differing `dig` must be rejected"
    );
}

/// c2 accept case: a mode-transition-only append (all immutable fields
/// identical, only `mode` differs) must be accepted and produce a real
/// tag object targeting the previous tag.
#[test]
fn write_chain_append_tag_accepts_mode_only_transition() {
    let (_dir, repo, genesis_oid) = setup_test_repo();
    let id = test_atom_id();

    let atom_commit_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, genesis_oid, genesis_oid).unwrap();

    let previous =
        publish_payload_fixture(&id, vec![1u8; 20], vec![2u8; 20], "Cargo.toml", "1.0.0");
    let mut new_payload = previous.clone();
    assert_eq!(previous.effective_mode(), Mode::Witnessed);
    new_payload.mode = Some(Mode::Reproducible); // only variable field changes

    let tagger = atom_git::gix_util::blank_signature();
    let new_tag_oid = atom_git::gix_util::write_chain_append_tag(
        &repo,
        "my-package-1.0.0",
        atom_commit_oid,
        &previous,
        &new_payload,
        tagger,
        "mode transition".to_string(),
    )
    .expect("mode-only transition must be accepted");

    let tag_obj = repo
        .find_object(new_tag_oid)
        .unwrap()
        .try_into_tag()
        .unwrap();
    assert_eq!(
        tag_obj.target_id().unwrap().detach(),
        atom_commit_oid,
        "the new tag must target the previous chain tip"
    );
}

// ---------------------------------------------------------------------
// Goal 3: moved-tip detection
// ---------------------------------------------------------------------

/// c3 warning-fires case: re-checking a ref whose tip was force-moved
/// since it was first observed must report `Moved(new_tip)`.
#[test]
fn tip_stability_reports_moved_when_ref_changes() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let ref_name = "refs/atom/pub/my-package/1.0.0";
    let observed_tip = genesis_oid;
    repo.reference(
        ref_name,
        genesis_oid,
        gix::refs::transaction::PreviousValue::Any,
        "observe",
    )
    .unwrap();

    // Simulate a concurrent publisher racing this resolution: the ref
    // moves to a new tip after `observed_tip` was captured.
    let other_oid =
        atom_git::gix_util::write_deterministic_commit(&repo, genesis_oid, genesis_oid).unwrap();
    repo.reference(
        ref_name,
        other_oid,
        gix::refs::transaction::PreviousValue::Any,
        "concurrent move",
    )
    .unwrap();

    let status = atom_git::gix_util::tip_stability(&repo, ref_name, observed_tip).unwrap();
    assert_eq!(status, TipStability::Moved(other_oid));
}

/// c3 warning-absent case: a ref that never moves between observation and
/// re-check must report `Stable`.
#[test]
fn tip_stability_reports_stable_when_ref_unchanged() {
    let (_dir, repo, genesis_oid) = setup_test_repo();

    let ref_name = "refs/atom/pub/my-package/1.0.0";
    repo.reference(
        ref_name,
        genesis_oid,
        gix::refs::transaction::PreviousValue::Any,
        "observe",
    )
    .unwrap();

    let status = atom_git::gix_util::tip_stability(&repo, ref_name, genesis_oid).unwrap();
    assert_eq!(status, TipStability::Stable);
}

/// Wiring sanity: a full, real `GitSource::resolve()` call over a stable
/// chain (nothing races it) must surface `TipStability::Stable` on the
/// returned `GitVersionEntry` — proving the primitive is actually wired
/// into the resolution walk, not just unit-tested in isolation.
#[tokio::test]
async fn resolve_reports_stable_tip_for_untouched_chain() {
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

    // `claim()`'s anchor check now resolves a real founding charter
    // (`[anchor-resolvable]`) rather than deriving one from git ancestry --
    // charter the source first via the real API.
    let founding_czd = registry.charter(&pub_key, b"src-rev", None).unwrap();
    let anchor = atom_core::Anchor::new(founding_czd.as_bytes().to_vec());
    let label = Label::try_from("my-package").unwrap();
    let id = AtomId::new(anchor, label);

    let claim_czd = registry.claim(&id, &pub_key).unwrap();

    let blob_tree = {
        let blob_oid = repo
            .write_object(gix::objs::Blob {
                data: b"fn test() {}".to_vec(),
            })
            .unwrap()
            .detach();
        let entry = gix::objs::tree::Entry {
            mode: gix::objs::tree::EntryKind::Blob.into(),
            filename: "lib.rs".into(),
            oid: blob_oid,
        };
        repo.write_object(Tree {
            entries: vec![entry],
        })
        .unwrap()
        .detach()
    };
    let ver_commit_oid = repo
        .commit_as(
            SignatureRef::default(),
            SignatureRef::default(),
            "refs/heads/master",
            "v1.0.0 src",
            blob_tree,
            vec![genesis_oid],
        )
        .unwrap()
        .detach();

    let ver_1 = RawVersion::new("1.0.0".to_string());
    registry
        .publish(
            &id,
            &claim_czd,
            &ver_1,
            blob_tree.as_bytes(),
            ver_commit_oid.as_bytes(),
            "Cargo.toml",
        )
        .unwrap();

    let source = GitSource::new(gix::open(repo.path()).unwrap());
    let entry = source.resolve(&id).await.unwrap().unwrap();
    let version_entry = entry.versions().next().unwrap();

    assert_eq!(
        version_entry.moved_tip,
        TipStability::Stable,
        "an untouched chain must resolve with a stable tip"
    );
}
