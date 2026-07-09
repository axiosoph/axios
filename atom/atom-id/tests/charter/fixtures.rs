//! Deterministic construction + corpus I/O for the charter attack corpus.
//!
//! Every key, timestamp, and content byte below is FIXED: keys come from
//! `coz_rs::signing_key_from_bytes` over a repeated seed byte rather than
//! `SigningKey::generate()`, so re-running the `build_*` functions here
//! byte-for-byte reproduces `atom/atom-id/corpus/charter/*.json`. The
//! committed JSON is the artifact under test in `construction.rs`; this
//! module is its (deterministic) provenance, exercised only by the
//! `#[ignore]`d `regenerate_charter_corpus_fixtures` test in `main.rs`.
//!
//! Spec: `docs/specs/atom-transactions.md` §CharterPayload/§ClaimPayload,
//! `[charter-succession-linear]`, `[chain-monotonicity]`,
//! `[claim-replacement-authority]`, `[charter-transition]`.

use atom_id::{Alg, Anchor, AtomId, CharterPayload, ClaimPayload, Czd, Label, Thumbprint};
use coz_rs::Ed25519;
use serde::{Deserialize, Serialize};

// ============================================================================
// Deterministic key + signing helpers
// ============================================================================

/// Derive a deterministic Ed25519 keypair from a repeated seed byte.
///
/// A fixture-only shortcut — real keys are never constructed this way.
pub fn key(seed: u8) -> (Vec<u8>, Vec<u8>, Thumbprint) {
    let sk = coz_rs::signing_key_from_bytes::<Ed25519>(&[seed; 32])
        .expect("a fixed 32-byte seed is always a valid Ed25519 signing key");
    let prv = sk.private_key_bytes();
    let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
    let tmb = sk.thumbprint().clone();
    (prv, pub_bytes, tmb)
}

/// Sign a payload's canonical JSON with a fixture key and compute its czd.
pub fn sign<T: Serialize>(payload: &T, prv: &[u8], pub_bytes: &[u8]) -> (Vec<u8>, Czd) {
    let pay_json = serde_json::to_vec(payload).expect("fixture payload always serializes");
    let (sig, cad) = coz_rs::sign_json(&pay_json, "Ed25519", prv, pub_bytes)
        .expect("fixed Ed25519 fixture keys always sign");
    let czd = Czd::compute::<Ed25519>(&cad, &sig);
    (sig, czd)
}

// ============================================================================
// Shared identity fixtures
// ============================================================================

pub fn anchor() -> Anchor {
    Anchor::new(vec![0xA0; 4])
}

pub fn label() -> Label {
    Label::try_from("corpus-atom").expect("valid label")
}

pub fn test_atom_id() -> AtomId {
    AtomId::new(anchor(), label())
}

// ============================================================================
// Corpus envelope — the on-disk shape of `corpus/charter/*.json`
// ============================================================================

/// A signed `atom/charter` transaction, as committed to the corpus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedCharter {
    pub alg: String,
    pub pub_key: Vec<u8>,
    pub sig: Vec<u8>,
    pub payload: CharterPayload,
}

impl SignedCharter {
    /// Re-verify this transaction exactly as a consumer would: reserialize
    /// the payload and check the signature/typ via [`atom_id::verify_charter`].
    pub fn verify(&self) -> Result<CharterPayload, atom_id::VerifyError> {
        let pay_json = serde_json::to_vec(&self.payload).expect("payload always serializes");
        atom_id::verify_charter(&pay_json, &self.sig, &self.alg, &self.pub_key)
    }

    /// The czd a successor's `prior` would name if it chained to this
    /// transaction — recomputed independently from the committed bytes,
    /// not carried alongside them.
    pub fn czd(&self) -> Czd {
        czd_of(&self.payload, &self.sig)
    }
}

/// A signed `atom/claim` transaction, as committed to the corpus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedClaim {
    pub alg: String,
    pub pub_key: Vec<u8>,
    pub sig: Vec<u8>,
    pub payload: ClaimPayload,
}

impl SignedClaim {
    pub fn verify(&self) -> Result<ClaimPayload, atom_id::VerifyError> {
        let pay_json = serde_json::to_vec(&self.payload).expect("payload always serializes");
        atom_id::verify_claim(&pay_json, &self.sig, &self.alg, &self.pub_key)
    }

    /// The czd a replacement's `prior` would name if it replaced this
    /// transaction — recomputed independently from the committed bytes.
    pub fn czd(&self) -> Czd {
        czd_of(&self.payload, &self.sig)
    }
}

/// Recompute a transaction's czd from its (reserialized) payload and
/// signature — independent of whatever `Czd` a fixture builder happened
/// to thread through at construction time.
fn czd_of<T: Serialize>(payload: &T, sig: &[u8]) -> Czd {
    let pay_json = serde_json::to_vec(payload).expect("payload always serializes");
    let cad = atom_id::canonical_hash_for_alg(&pay_json, "Ed25519", None)
        .expect("Ed25519 is a supported algorithm");
    Czd::compute::<Ed25519>(&cad, sig)
}

/// One attack scenario: a named, spec-tagged bundle of signed transactions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CorpusFile {
    pub attack: String,
    pub spec: String,
    pub description: String,
    #[serde(default)]
    pub charters: Vec<SignedCharter>,
    #[serde(default)]
    pub claims: Vec<SignedClaim>,
}

/// Path of a named corpus file under `atom/atom-id/corpus/charter/`.
fn corpus_path(file_name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/charter")
        .join(file_name)
}

/// Load a committed corpus file.
pub fn load(file_name: &str) -> CorpusFile {
    let bytes = std::fs::read(corpus_path(file_name))
        .unwrap_or_else(|e| panic!("read corpus/charter/{file_name}: {e}"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse corpus/charter/{file_name}: {e}"))
}

/// Overwrite a committed corpus file. Dev-only: see the `#[ignore]`d
/// `regenerate_charter_corpus_fixtures` test in `main.rs`.
pub fn save(file_name: &str, file: &CorpusFile) {
    let json = serde_json::to_string_pretty(file).expect("corpus file always serializes");
    std::fs::write(corpus_path(file_name), json + "\n")
        .unwrap_or_else(|e| panic!("write corpus/charter/{file_name}: {e}"));
}

// ============================================================================
// Attack construction — attacks #1, #2, #4 (charter chains)
// ============================================================================

/// Attack #1 — divergent succession (`[charter-succession-linear]`).
///
/// A founding charter with TWO successors naming the same `prior`: a
/// set-authority fork. Both successors are signed by the founding
/// owner's own key — per spec commentary, "nothing can prevent a key
/// from signing two successors naming the same prior"; the constraint
/// binds consumers, not signers.
pub fn build_divergent_succession() -> CorpusFile {
    let (prv0, pub0, tmb0) = key(1);
    let founding = CharterPayload::new(Alg::Ed25519, 1_000, pub0.clone(), None, vec![0; 32], tmb0);
    let (sig0, czd0) = sign(&founding, &prv0, &pub0);

    let (_prv_a, pub_a, tmb_a) = key(2);
    let successor_a = CharterPayload::new(
        Alg::Ed25519,
        2_000,
        pub_a,
        Some(czd0.clone()),
        vec![1; 32],
        tmb_a,
    );
    let (sig_a, _) = sign(&successor_a, &prv0, &pub0);

    let (_prv_b, pub_b, tmb_b) = key(3);
    let successor_b =
        CharterPayload::new(Alg::Ed25519, 2_001, pub_b, Some(czd0), vec![1; 32], tmb_b);
    let (sig_b, _) = sign(&successor_b, &prv0, &pub0);

    CorpusFile {
        attack: "divergent-succession".into(),
        spec: "[charter-succession-linear]".into(),
        description: "Two successor charters name the same `prior` czd — a set-authority fork. \
                      Individually each charter verifies (construction-correctness); no validator \
                      ships to reject the fork itself (Phase 1)."
            .into(),
        charters: vec![
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub0.clone(),
                sig: sig0,
                payload: founding,
            },
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub0.clone(),
                sig: sig_a,
                payload: successor_a,
            },
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub0,
                sig: sig_b,
                payload: successor_b,
            },
        ],
        claims: vec![],
    }
}

/// Attack #2 — prefix rollback (`[chain-monotonicity]`).
///
/// A valid 3-charter succession chain (`c0 -> c1 -> c2`), plus the
/// 2-charter prefix `[c0, c1]` that a malicious or stale server could
/// replay to a consumer who already recorded head `czd(c2)` — a
/// detectable rollback below previously observed state.
pub fn build_prefix_rollback() -> CorpusFile {
    let (prv0, pub0, tmb0) = key(4);
    let c0 = CharterPayload::new(Alg::Ed25519, 1_000, pub0.clone(), None, vec![0; 32], tmb0);
    let (sig0, czd0) = sign(&c0, &prv0, &pub0);

    let (prv1, pub1, tmb1) = key(5);
    let c1 = CharterPayload::new(
        Alg::Ed25519,
        2_000,
        pub1.clone(),
        Some(czd0),
        vec![1; 32],
        tmb1,
    );
    let (sig1, czd1) = sign(&c1, &prv0, &pub0); // signed by c0's owner, authorizing transfer

    let (_prv2, pub2, tmb2) = key(6);
    let c2 = CharterPayload::new(Alg::Ed25519, 3_000, pub2, Some(czd1), vec![2; 32], tmb2);
    let (sig2, _czd2) = sign(&c2, &prv1, &pub1); // signed by c1's owner, authorizing transfer

    CorpusFile {
        attack: "prefix-rollback".into(),
        spec: "[chain-monotonicity]".into(),
        description: "A full chain c0->c1->c2 plus the [c0, c1] prefix a consumer who already \
                      recorded head czd(c2) must refuse. Individually each charter verifies \
                      (construction-correctness); no validator ships to reject the regression \
                      (Phase 1)."
            .into(),
        charters: vec![
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub0.clone(),
                sig: sig0,
                payload: c0,
            },
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub0,
                sig: sig1,
                payload: c1,
            },
            SignedCharter {
                alg: "Ed25519".into(),
                pub_key: pub1,
                sig: sig2,
                payload: c2,
            },
        ],
        claims: vec![],
    }
}

/// Attack #3 — claim-replacement marking (`[claim-replacement-authority]`).
///
/// An ordinary claim, an **owner replacement** (unmarked, signed by the
/// replaced claim's own owner), and a **governance replacement** (marked
/// `governance: true`, signed by the effective charter's owner instead).
pub fn build_claim_replacement() -> CorpusFile {
    let (prv_owner, pub_owner, tmb_owner) = key(7);
    let claim0 = ClaimPayload::new(
        Alg::Ed25519,
        test_atom_id(),
        1_000,
        pub_owner.clone(),
        "cargo".into(),
        vec![0; 32],
        tmb_owner,
    );
    let (sig0, czd0) = sign(&claim0, &prv_owner, &pub_owner);

    let (_prv_owner2, pub_owner2, tmb_owner2) = key(8);
    let owner_replacement = ClaimPayload::new_replacement(
        Alg::Ed25519,
        test_atom_id(),
        2_000,
        pub_owner2,
        "cargo".into(),
        czd0.clone(),
        false,
        vec![1; 32],
        tmb_owner2,
    );
    // Signed by claim0's OWN owner key — the ordinary, unmarked path.
    let (sig_owner_repl, _) = sign(&owner_replacement, &prv_owner, &pub_owner);

    let (prv_charter, pub_charter, tmb_charter) = key(9);
    let governance_replacement = ClaimPayload::new_replacement(
        Alg::Ed25519,
        test_atom_id(),
        2_001,
        pub_charter.clone(),
        "cargo".into(),
        czd0,
        true,
        vec![1; 32],
        tmb_charter,
    );
    // Signed by the EFFECTIVE CHARTER's owner, not claim0's owner — a
    // marked governance seizure.
    let (sig_gov_repl, _) = sign(&governance_replacement, &prv_charter, &pub_charter);

    CorpusFile {
        attack: "claim-replacement-marking".into(),
        spec: "[claim-replacement-authority]".into(),
        description: "An ordinary claim plus its two replacement authorities: an unmarked owner \
                      replacement (signed by the replaced claim's own owner) and a `governance: \
                      true` replacement (signed by the effective charter's owner instead). All \
                      three verify individually; the two-authority check itself is \
                      `verify_claim_replacement`, landed as an honest Phase-1 stub \
                      (`atom/atom-id/src/lib.rs`, `#[should_panic]`-pinned) — not re-declared \
                      here."
            .into(),
        charters: vec![],
        claims: vec![
            SignedClaim {
                alg: "Ed25519".into(),
                pub_key: pub_owner.clone(),
                sig: sig0,
                payload: claim0,
            },
            SignedClaim {
                alg: "Ed25519".into(),
                pub_key: pub_owner,
                sig: sig_owner_repl,
                payload: owner_replacement,
            },
            SignedClaim {
                alg: "Ed25519".into(),
                pub_key: pub_charter,
                sig: sig_gov_repl,
                payload: governance_replacement,
            },
        ],
    }
}

/// Attack #4 — bootstrap seizure (`[charter-transition]` PRE, founding).
///
/// A pre-existing claim (the incumbent's) on a source that a stranger
/// then attempts to charter over, signed by the STRANGER's key rather
/// than the incumbent claim owner's — the bootstrap gate this founding
/// charter must satisfy and currently cannot be checked (no
/// `CharterStore`/ancestry lookups ship here; Non-Goal).
pub fn build_bootstrap_seizure() -> CorpusFile {
    let (prv_incumbent, pub_incumbent, tmb_incumbent) = key(10);
    let pre_existing_claim = ClaimPayload::new(
        Alg::Ed25519,
        test_atom_id(),
        500, // predates the founding charter's `now` below
        pub_incumbent.clone(),
        "cargo".into(),
        vec![0xEE; 32], // source revision predating the charter's `src`
        tmb_incumbent,
    );
    let (sig_claim, _) = sign(&pre_existing_claim, &prv_incumbent, &pub_incumbent);

    let (prv_attacker, pub_attacker, tmb_attacker) = key(11);
    let founding_charter = CharterPayload::new(
        Alg::Ed25519,
        1_000,
        pub_attacker.clone(),
        None,
        vec![0xEE; 32], // same source: the attacker charters over the live, claimed set
        tmb_attacker,
    );
    let (sig_charter, _) = sign(&founding_charter, &prv_attacker, &pub_attacker);

    CorpusFile {
        attack: "bootstrap-seizure".into(),
        spec: "[charter-transition] PRE (founding, bootstrap gate)".into(),
        description: "A pre-existing claim (the incumbent's) plus a founding charter signed by an \
                      unrelated stranger's key over the same source. Both transactions verify \
                      individually (construction-correctness); no bootstrap-gate check ships \
                      (Phase 1) — see `bootstrap_seizure_requires_incumbent_authorization` in \
                      `bootstrap_gate.rs`."
            .into(),
        charters: vec![SignedCharter {
            alg: "Ed25519".into(),
            pub_key: pub_attacker,
            sig: sig_charter,
            payload: founding_charter,
        }],
        claims: vec![SignedClaim {
            alg: "Ed25519".into(),
            pub_key: pub_incumbent,
            sig: sig_claim,
            payload: pre_existing_claim,
        }],
    }
}
