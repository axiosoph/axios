//! Golden vectors across the czd digest-algorithm axis (32/48/64 bytes) +
//! RED unknown-field round-trip fixtures.
//!
//! Every fixture prior to this node used 4-byte toy anchors and a single
//! algorithm (Ed25519), so the historic digest bug that mislabeled a
//! 48-byte `Czd` as `sha256` survived undetected — every committed vector
//! happened to be 32 bytes (`atom/atom-id/src/digest.rs`,
//! `TryFrom<Czd> for AtomDigest`). This module closes that gap: one
//! committed vector file per coz signing algorithm (`ES256`/32,
//! `ES384`/48, `Ed25519`/64), each carrying a full charter -> claim ->
//! publish chain signed and hashed at that algorithm's native length.
//!
//! Keys are derived deterministically from a repeated seed byte via
//! `coz_rs::signing_key_from_bytes`, mirroring the fixture idiom in
//! `atom/atom-id/tests/charter/fixtures.rs` — regeneration is
//! byte-identical (`regenerate_golden_vectors`, `#[ignore]`d, dev-only).
//!
//! Spec: `docs/specs/atom-transactions.md` `[charter-anchor]`,
//! `[symmetric-payloads]`, `[publish-chains-claim]`, `[czd-recalculatable]`,
//! `[publish-payload-extensible]`.

use atom_id::{
    Alg, Anchor, AtomId, CharterPayload, ClaimPayload, Czd, Label, PublishPayload, RawVersion,
    Thumbprint,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Deterministic key + signing helpers
// ============================================================================

/// Derive a deterministic signing key from a repeated seed byte, generic
/// over the coz algorithm marker type so each algorithm's native private
/// key length (`ES256`: 32, `ES384`: 48, `Ed25519`: 32) is used correctly.
///
/// A fixture-only shortcut — real keys are never constructed this way.
fn derive_key<A>(seed: u8) -> (Vec<u8>, Vec<u8>, Thumbprint)
where
    A: coz_rs::Algorithm + coz_rs::key::ops::KeyOps,
{
    let seed_bytes = vec![seed; A::PRV_SIZE];
    let sk = coz_rs::signing_key_from_bytes::<A>(&seed_bytes)
        .expect("a fixed seed of the algorithm's native length is always a valid signing key");
    let prv = sk.private_key_bytes();
    let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
    let tmb = sk.thumbprint().clone();
    (prv, pub_bytes, tmb)
}

/// Sign a payload's canonical JSON with a fixture key.
///
/// Returns the exact `pay_json` bytes that were signed alongside the
/// signature — the pair a consumer needs to recompute `czd`.
fn sign<T: Serialize>(payload: &T, alg: &str, prv: &[u8], pub_bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let pay_json = serde_json::to_vec(payload).expect("fixture payload always serializes");
    let (sig, _cad) = coz_rs::sign_json(&pay_json, alg, prv, pub_bytes)
        .unwrap_or_else(|| panic!("fixed {alg} fixture keys always sign"));
    (pay_json, sig)
}

// ============================================================================
// Vector file schema — the on-disk shape of `corpus/vectors/*.json`
// ============================================================================

/// One signed transaction plus its independently-committed `czd` — the
/// golden byte-exact expectation a test recomputes and compares against,
/// never the other way around.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedVector<P> {
    alg: String,
    pub_key: Vec<u8>,
    sig: Vec<u8>,
    payload: P,
    czd: Czd,
}

/// A full charter -> claim -> publish chain, signed and hashed at one
/// coz algorithm's native digest length.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlgVectorFile {
    alg: String,
    digest_len: usize,
    charter: SignedVector<CharterPayload>,
    claim: SignedVector<ClaimPayload>,
    publish: SignedVector<PublishPayload>,
}

fn corpus_path(file_name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/vectors")
        .join(file_name)
}

fn load(file_name: &str) -> AlgVectorFile {
    let bytes = std::fs::read(corpus_path(file_name))
        .unwrap_or_else(|e| panic!("read corpus/vectors/{file_name}: {e}"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse corpus/vectors/{file_name}: {e}"))
}

fn save(file_name: &str, file: &AlgVectorFile) {
    let json = serde_json::to_string_pretty(file).expect("vector file always serializes");
    std::fs::write(corpus_path(file_name), json + "\n")
        .unwrap_or_else(|e| panic!("write corpus/vectors/{file_name}: {e}"));
}

// ============================================================================
// Deterministic construction — one chain per algorithm
// ============================================================================

fn label() -> Label {
    Label::try_from("golden-vector-atom").expect("valid label")
}

fn build_vector_file<A>(alg: Alg, seed: u8, digest_len: usize) -> AlgVectorFile
where
    A: coz_rs::Algorithm + coz_rs::key::ops::KeyOps,
{
    let alg_name = alg.name();
    let (prv, pub_bytes, tmb) = derive_key::<A>(seed);

    // Charter: the founding charter's own czd BECOMES the atom-set anchor
    // (`[charter-anchor]`).
    let charter = CharterPayload::new(
        alg,
        1_000,
        pub_bytes.clone(),
        None,
        vec![0xC0; 32],
        tmb.clone(),
    );
    let (charter_pay_json, charter_sig) = sign(&charter, alg_name, &prv, &pub_bytes);
    let charter_czd = atom_id::czd_for_alg(&charter_pay_json, &charter_sig, alg_name)
        .unwrap_or_else(|e| panic!("{alg_name} charter czd: {e}"));
    assert_eq!(
        charter_czd.as_bytes().len(),
        digest_len,
        "{alg_name} charter czd must be {digest_len} bytes"
    );

    let anchor = Anchor::new(charter_czd.as_bytes().to_vec());
    let id = AtomId::new(anchor, label());

    // Claim: founding claim on the atom-set the charter just defined.
    let claim = ClaimPayload::new(
        alg,
        id,
        2_000,
        pub_bytes.clone(),
        "cargo".into(),
        vec![0xC1; 32],
        tmb.clone(),
    );
    let (claim_pay_json, claim_sig) = sign(&claim, alg_name, &prv, &pub_bytes);
    let claim_czd = atom_id::czd_for_alg(&claim_pay_json, &claim_sig, alg_name)
        .unwrap_or_else(|e| panic!("{alg_name} claim czd: {e}"));
    assert_eq!(
        claim_czd.as_bytes().len(),
        digest_len,
        "{alg_name} claim czd must be {digest_len} bytes"
    );

    // Publish: chains back to the claim via `[publish-chains-claim]`.
    let anchor_for_publish = Anchor::new(claim.anchor.as_bytes().to_vec());
    let publish_id = AtomId::new(anchor_for_publish, claim.label.clone());
    let publish = PublishPayload::new(
        alg,
        publish_id,
        claim_czd.clone(),
        vec![0xC2; digest_len],
        3_000,
        "src".into(),
        vec![0xC3; 32],
        tmb,
        RawVersion::new("1.0.0".into()),
    );
    let (publish_pay_json, publish_sig) = sign(&publish, alg_name, &prv, &pub_bytes);
    let publish_czd = atom_id::czd_for_alg(&publish_pay_json, &publish_sig, alg_name)
        .unwrap_or_else(|e| panic!("{alg_name} publish czd: {e}"));
    assert_eq!(
        publish_czd.as_bytes().len(),
        digest_len,
        "{alg_name} publish czd must be {digest_len} bytes"
    );

    AlgVectorFile {
        alg: alg_name.to_string(),
        digest_len,
        charter: SignedVector {
            alg: alg_name.to_string(),
            pub_key: pub_bytes.clone(),
            sig: charter_sig,
            payload: charter,
            czd: charter_czd,
        },
        claim: SignedVector {
            alg: alg_name.to_string(),
            pub_key: pub_bytes.clone(),
            sig: claim_sig,
            payload: claim,
            czd: claim_czd,
        },
        publish: SignedVector {
            alg: alg_name.to_string(),
            pub_key: pub_bytes,
            sig: publish_sig,
            payload: publish,
            czd: publish_czd,
        },
    }
}

/// Dev-only: regenerate `corpus/vectors/*.json` from the deterministic
/// builders above. NOT part of the green suite — the committed vectors
/// are the artifact under test; this only exists to re-derive them
/// byte-identically (e.g. after a fixture-construction change).
///
/// Run explicitly: `cargo test -p atom-id --test golden_vectors -- \
/// --ignored regenerate_golden_vectors`.
#[test]
#[ignore = "dev-only: regenerates the committed golden-vector corpus"]
fn regenerate_golden_vectors() {
    save(
        "es256.json",
        &build_vector_file::<coz_rs::ES256>(Alg::ES256, 1, 32),
    );
    save(
        "es384.json",
        &build_vector_file::<coz_rs::ES384>(Alg::ES384, 2, 48),
    );
    save(
        "ed25519.json",
        &build_vector_file::<coz_rs::Ed25519>(Alg::Ed25519, 3, 64),
    );
}

// ============================================================================
// c1/c2 — table-driven axis coverage, byte-exact
// ============================================================================

/// One vector file's expected shape, named for the axis table below.
struct AxisCase {
    file: &'static str,
    digest_len: usize,
}

const AXIS: [AxisCase; 3] = [
    AxisCase {
        file: "es256.json",
        digest_len: 32,
    },
    AxisCase {
        file: "es384.json",
        digest_len: 48,
    },
    AxisCase {
        file: "ed25519.json",
        digest_len: 64,
    },
];

/// Recompute a signed vector's `czd` independently from its committed
/// `(payload, sig, alg)` and compare byte-for-byte against the committed
/// `czd` — never a structural/derived comparison.
fn assert_czd_byte_exact<P: Serialize>(vector: &SignedVector<P>, digest_len: usize, what: &str) {
    let pay_json = serde_json::to_vec(&vector.payload).expect("payload always serializes");
    let recomputed = atom_id::czd_for_alg(&pay_json, &vector.sig, &vector.alg)
        .unwrap_or_else(|e| panic!("{what}: czd recompute: {e}"));
    assert_eq!(
        recomputed.as_bytes(),
        vector.czd.as_bytes(),
        "{what}: recomputed czd must equal the committed golden czd byte-for-byte"
    );
    assert_eq!(
        recomputed.as_bytes().len(),
        digest_len,
        "{what}: czd must be exactly {digest_len} bytes for {}",
        vector.alg
    );
}

/// Payload serialization round-trip: deserialize -> reserialize must be a
/// fixed point, byte-for-byte — never compared via `PartialEq` on the
/// parsed struct.
fn assert_round_trip_byte_exact<P>(pay_json_once: &[u8], what: &str)
where
    P: Serialize + for<'de> Deserialize<'de>,
{
    let parsed: P =
        serde_json::from_slice(pay_json_once).unwrap_or_else(|e| panic!("{what}: parse: {e}"));
    let reserialized = serde_json::to_vec(&parsed).expect("payload always serializes");
    assert_eq!(
        reserialized, pay_json_once,
        "{what}: serialize(deserialize(pay_json)) must equal pay_json byte-for-byte"
    );
}

// c-golden-vectors-axis: charter/claim/publish czd, all three coz digest
// lengths (32/48/64), byte-exact.
#[test]
fn golden_vectors_axis_is_byte_exact_at_all_three_lengths() {
    let mut cases_run = 0usize;

    for case in &AXIS {
        let vectors = load(case.file);
        assert_eq!(
            vectors.digest_len, case.digest_len,
            "{}: committed digest_len must match the axis table",
            case.file
        );

        assert_czd_byte_exact(&vectors.charter, case.digest_len, "charter czd");
        cases_run += 1;
        assert_czd_byte_exact(&vectors.claim, case.digest_len, "claim czd");
        cases_run += 1;
        assert_czd_byte_exact(&vectors.publish, case.digest_len, "publish czd");
        cases_run += 1;
    }

    assert_eq!(
        cases_run,
        AXIS.len() * 3,
        "must exercise charter+claim+publish czd at every axis length"
    );
}

// c-golden-vectors-round-trip: payload serialization round-trip, all
// three coz digest lengths, byte-exact.
#[test]
fn golden_vectors_payload_round_trip_is_byte_exact_at_all_three_lengths() {
    let mut cases_run = 0usize;

    for case in &AXIS {
        let vectors = load(case.file);

        let charter_json = serde_json::to_vec(&vectors.charter.payload).unwrap();
        assert_round_trip_byte_exact::<CharterPayload>(&charter_json, "charter payload round-trip");
        cases_run += 1;

        let claim_json = serde_json::to_vec(&vectors.claim.payload).unwrap();
        assert_round_trip_byte_exact::<ClaimPayload>(&claim_json, "claim payload round-trip");
        cases_run += 1;

        let publish_json = serde_json::to_vec(&vectors.publish.payload).unwrap();
        assert_round_trip_byte_exact::<PublishPayload>(&publish_json, "publish payload round-trip");
        cases_run += 1;
    }

    assert_eq!(
        cases_run,
        AXIS.len() * 3,
        "must exercise charter+claim+publish payload round-trip at every axis length"
    );
}

// Bonus construction-correctness check, mirroring the charter corpus
// idiom: every committed vector's signature actually verifies against
// its committed pub_key via the public verify_* entry points.
#[test]
fn golden_vectors_signatures_verify() {
    for case in &AXIS {
        let vectors = load(case.file);

        let charter_json = serde_json::to_vec(&vectors.charter.payload).unwrap();
        atom_id::verify_charter(
            &charter_json,
            &vectors.charter.sig,
            &vectors.charter.alg,
            &vectors.charter.pub_key,
        )
        .unwrap_or_else(|e| panic!("{}: charter signature must verify: {e}", case.file));

        let claim_json = serde_json::to_vec(&vectors.claim.payload).unwrap();
        atom_id::verify_claim(
            &claim_json,
            &vectors.claim.sig,
            &vectors.claim.alg,
            &vectors.claim.pub_key,
        )
        .unwrap_or_else(|e| panic!("{}: claim signature must verify: {e}", case.file));

        let publish_json = serde_json::to_vec(&vectors.publish.payload).unwrap();
        atom_id::verify_publish(
            &publish_json,
            &vectors.publish.sig,
            &vectors.publish.alg,
            &vectors.publish.pub_key,
        )
        .unwrap_or_else(|e| panic!("{}: publish signature must verify: {e}", case.file));
    }
}

// ============================================================================
// c3 — RED extensibility fixtures (n2-payload-fields greens these)
// ============================================================================

/// A minimal, valid `PublishPayload` for the extensibility fixtures below
/// — any single algorithm suffices; the axis is not the point here. The
/// signature is irrelevant to a serde round-trip fixture, so only the
/// owner thumbprint is drawn from a deterministic key.
fn extensibility_publish_payload() -> PublishPayload {
    let (.., tmb) = derive_key::<coz_rs::ES256>(0xE5);
    let id = AtomId::new(Anchor::new(vec![0xE5; 32]), label());
    PublishPayload::new(
        Alg::ES256,
        id,
        Czd::from_bytes(vec![0xE6; 32]),
        vec![0xE7; 32],
        4_000,
        "src".into(),
        vec![0xE8; 32],
        tmb,
        RawVersion::new("1.0.0".into()),
    )
}

/// A minimal, valid `ClaimPayload` for the extensibility fixtures below.
fn extensibility_claim_payload() -> ClaimPayload {
    let (_prv, pub_bytes, tmb) = derive_key::<coz_rs::ES256>(0xE9);
    let id = AtomId::new(Anchor::new(vec![0xE9; 32]), label());
    ClaimPayload::new(
        Alg::ES256,
        id,
        4_001,
        pub_bytes,
        "cargo".into(),
        vec![0xEA; 32],
        tmb,
    )
}

/// Inject a root-level `meta` object into a payload's JSON and return the
/// injected bytes — simulating an ecosystem-specific extension field per
/// `[publish-payload-extensible]`.
fn inject_meta<P: Serialize>(payload: &P) -> Vec<u8> {
    let mut value = serde_json::to_value(payload).expect("payload always serializes");
    value
        .as_object_mut()
        .expect("payload serializes as a JSON object")
        .insert(
            "meta".to_string(),
            serde_json::json!({ "note": "ecosystem-specific extension" }),
        );
    serde_json::to_vec(&value).expect("value with injected meta always serializes")
}

// F8: PublishPayload silently drops the unknown `meta` field under serde
// — the exact opposite of `[publish-payload-extensible]`'s preserve-all
// mandate. Greens once n2-payload-fields lands the `meta` field.
#[test]
#[ignore = "F8: greens at n2-payload-fields"]
fn publish_payload_preserves_unknown_meta_field() {
    let injected = inject_meta(&extensibility_publish_payload());

    let round_tripped: PublishPayload =
        serde_json::from_slice(&injected).expect("unknown fields must not be a parse error");
    let re_serialized =
        serde_json::to_value(&round_tripped).expect("round-tripped payload always serializes");

    assert_eq!(
        re_serialized.get("meta"),
        Some(&serde_json::json!({ "note": "ecosystem-specific extension" })),
        "[publish-payload-extensible]: an unknown 'meta' field MUST survive a \
         deserialize/reserialize round trip, not be silently dropped"
    );
}

// F8: ClaimPayload silently drops the unknown `meta` field under serde —
// `[symmetric-payloads]` ties claim and publish payload shape together,
// so the same preservation obligation applies. Greens once
// n2-payload-fields lands the `meta` field on both payload types.
#[test]
#[ignore = "F8: greens at n2-payload-fields"]
fn claim_payload_preserves_unknown_meta_field() {
    let injected = inject_meta(&extensibility_claim_payload());

    let round_tripped: ClaimPayload =
        serde_json::from_slice(&injected).expect("unknown fields must not be a parse error");
    let re_serialized =
        serde_json::to_value(&round_tripped).expect("round-tripped payload always serializes");

    assert_eq!(
        re_serialized.get("meta"),
        Some(&serde_json::json!({ "note": "ecosystem-specific extension" })),
        "[symmetric-payloads] x [publish-payload-extensible]: an unknown 'meta' field MUST \
         survive a deserialize/reserialize round trip, not be silently dropped"
    );
}
