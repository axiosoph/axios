//! Property tests derived from the TLA/Alloy invariants this crate's
//! surface (`atom-id`, no `atom-git`/`gix`) can actually reach.
//!
//! Two distinct, differently-weighted pieces:
//!
//! 1. **Charter constraint properties** (`c1`-`c3`, below) — well-grounded approximations, at the
//!    Rust level, of `docs/specs/tla/AtomCharter.tla`'s TLC-verified `[charter-anchor]`,
//!    `[charter-succession]`, `[charter-succession-linear]`, `[chain-monotonicity]` against
//!    `atom_id::verify_succession_chain` / `verify_bootstrap_gate`.
//! 2. **Digest-seam properties** (`c4`, below) — a genuinely different, WEAKER and NARROWER claim
//!    than `docs/specs/alloy/atom_backend_seam.als`'s `oid_disjoint_from_protocol_sorts` /
//!    `carrier_czd_seam` assertions, which are about `OID` (git's `ObjectId`, an `atom-git`-only
//!    type not reachable from this crate). The closest in-surface analog is
//!    `AtomDigest`/`HashAlg`'s multi-algorithm-family round-trip and cross-family disjointness. Do
//!    not read `c4` as having verified the Alloy model's actual OID-disjointness claims — it has
//!    not.

use atom_id::{
    Alg, Anchor, AtomDigest, AtomId, CharterPayload, ClaimPayload, Czd, HashAlg, Label, OwnerKind,
    OwnerRef, Thumbprint, VerifyError, verify_bootstrap_gate, verify_succession_chain,
};
use proptest::prelude::*;

// ============================================================================
// Charter constraint properties (c1-c3)
// ============================================================================
//
// `verify_succession_chain`/`verify_bootstrap_gate` never re-check Coz
// signatures (see their docstrings — signature verification is
// `verify_charter`'s job, out of scope here). So, matching the existing
// `charter.rs` `#[cfg(test)] mod tests` idiom (e.g.
// `founding_czd = Czd::from_bytes(vec![9, 9, 9])`), fixtures below use
// plain byte-vector stand-ins for owners/thumbprints/czds rather than real
// Ed25519 keys — the functions under test never look past those bytes.

/// A per-position owner-identity byte stand-in, injective over `i` in the
/// small ranges these properties generate.
fn owner_bytes(i: usize) -> Vec<u8> {
    vec![0xB0, (i >> 8) as u8, i as u8]
}

/// A per-position czd byte stand-in (as if it were `czd(chain[i])`),
/// injective over `i` in the small ranges these properties generate.
fn czd_bytes(i: usize) -> Vec<u8> {
    vec![0xC7, (i >> 8) as u8, i as u8]
}

/// Build a genuinely linear, correctly authorized chain of `len` charters:
/// `chain[0]` is the founding charter (no `prior`); each `chain[i]` (i>=1)
/// names `czd_bytes(i-1)` as its `prior` and is authorized by
/// `chain[i-1]`'s owner (`chain[i].tmb == chain[i-1].owner`).
fn build_valid_chain(len: usize) -> Vec<CharterPayload> {
    assert!(len >= 1, "a chain always has at least a founding charter");
    (0..len)
        .map(|i| {
            let owner = owner_bytes(i);
            let (prior, tmb) = if i == 0 {
                (None, Thumbprint::from_bytes(vec![0xFF]))
            } else {
                (
                    Some(Czd::from_bytes(czd_bytes(i - 1))),
                    Thumbprint::from_bytes(owner_bytes(i - 1)),
                )
            };
            CharterPayload::new(
                Alg::ES256,
                1_000 + i as u64,
                vec![OwnerRef::new(OwnerKind::SingleKey, owner)],
                prior,
                vec![0x11; 4],
                tmb,
            )
            .expect("owner_bytes(i) is always non-empty, so the set is non-empty")
        })
        .collect()
}

/// An injected divergence from an otherwise-valid chain, or none.
#[derive(Debug, Clone, Copy)]
enum Mutation {
    /// No injection — the chain stays genuinely linear and authorized.
    None,
    /// Two successors are made to share the same `prior` — a
    /// set-authority fork (`[charter-succession-linear]`). Requires at
    /// least two successors (`len >= 3`) to pick two distinct positions.
    Fork,
    /// A successor's `tmb` is overwritten so it no longer matches its
    /// predecessor's `owner` (`[charter-succession]`). Requires at least
    /// one successor (`len >= 2`).
    AuthBreak,
}

/// Generate `(len, mutation)` pairs, restricting which mutations are
/// structurally possible for a given `len` (`Fork` needs two successors,
/// `AuthBreak` needs one).
fn len_and_mutation() -> impl Strategy<Value = (usize, Mutation)> {
    (1..12usize).prop_flat_map(|len| {
        let opts = if len >= 3 {
            vec![Mutation::None, Mutation::Fork, Mutation::AuthBreak]
        } else if len == 2 {
            vec![Mutation::None, Mutation::AuthBreak]
        } else {
            vec![Mutation::None]
        };
        proptest::sample::select(opts).prop_map(move |m| (len, m))
    })
}

proptest! {
    /// [c1-succession-linear-property]: the implementation accepts exactly
    /// the chains that are genuinely linear and correctly authorized, and
    /// rejects every generated chain with an injected fork or
    /// authorization break.
    #[test]
    fn c1_succession_linear_and_authorized_property((len, mutation) in len_and_mutation()) {
        let mut chain = build_valid_chain(len);
        let expect_ok = matches!(mutation, Mutation::None);

        match mutation {
            Mutation::None => {},
            Mutation::Fork => {
                // Two distinct successor positions (len >= 3 guarantees
                // 1 != len-1) are made to name the same `prior`.
                let dup_prior = chain[1].prior.clone();
                let last = chain.len() - 1;
                chain[last].prior = dup_prior;
            },
            Mutation::AuthBreak => {
                // A byte pattern guaranteed to differ from any
                // `owner_bytes(_)` (which always starts `0xB0`).
                chain[1].tmb = Thumbprint::from_bytes(vec![0xDE, 0xAD]);
            },
        }

        let result = verify_succession_chain(&chain, None);
        prop_assert_eq!(
            result.is_ok(),
            expect_ok,
            "len={} mutation={:?} result={:?}",
            len,
            mutation,
            result
        );
        match mutation {
            Mutation::Fork => {
                prop_assert!(matches!(result, Err(VerifyError::DivergentSuccessors)));
            },
            Mutation::AuthBreak => {
                prop_assert!(matches!(result, Err(VerifyError::Unauthorized)));
            },
            Mutation::None => {},
        }
    }
}

/// Where a `recorded_head` scenario lands relative to a generated chain.
#[derive(Debug, Clone, Copy)]
enum RecordedHeadCase {
    /// `recorded_head` names an earlier position's stand-in czd
    /// (`czd_bytes(k)`, `k <= len - 2`), which some successor's `prior`
    /// genuinely names — the chain demonstrably grew past it.
    Growth(usize),
    /// `recorded_head` names a point this chain never reaches (strictly
    /// beyond every generated stand-in czd) — a genuine regression, never
    /// the ambiguous "chain unchanged since last observation" case.
    /// `verify_succession_chain`'s own docstring documents that case as a
    /// known limitation (it unconditionally rejects an unchanged chain as
    /// a false regression); deliberately not exercised here since it is
    /// not the behavior this property is meant to pin down.
    Regression,
}

fn len_and_head_case() -> impl Strategy<Value = (usize, RecordedHeadCase)> {
    (1..12usize).prop_flat_map(|len| {
        if len >= 2 {
            proptest::sample::select(
                (0..(len - 1))
                    .map(RecordedHeadCase::Growth)
                    .chain(std::iter::once(RecordedHeadCase::Regression))
                    .collect::<Vec<_>>(),
            )
            .prop_map(move |case| (len, case))
            .boxed()
        } else {
            Just((len, RecordedHeadCase::Regression)).boxed()
        }
    })
}

proptest! {
    /// [c2-monotonicity-property]: for the GROWTH case specifically, a
    /// chain that demonstrably extends past `recorded_head` is accepted;
    /// one that never reaches it (a genuine regression, not the
    /// already-tech-debt-tracked steady-state-unchanged limitation) is
    /// rejected.
    #[test]
    fn c2_monotonicity_growth_vs_regression_property(
        (len, case) in len_and_head_case()
    ) {
        let chain = build_valid_chain(len);
        let (head_bytes, expect_ok) = match case {
            RecordedHeadCase::Growth(k) => (czd_bytes(k), true),
            // One position past anything any generated chain in this
            // property (len < 12) can ever reach — never named by any
            // `prior` in `chain`, and never the chain's own tail
            // (avoiding the steady-state ambiguity entirely).
            RecordedHeadCase::Regression => (czd_bytes(100), false),
        };
        let recorded_head = Czd::from_bytes(head_bytes);

        let result = verify_succession_chain(&chain, Some(&recorded_head));
        prop_assert_eq!(
            result.is_ok(),
            expect_ok,
            "len={} case={:?} result={:?}",
            len,
            case,
            result
        );
        if !expect_ok {
            prop_assert!(matches!(result, Err(VerifyError::ChainRegression)));
        }
    }
}

fn bootstrap_atom_id() -> AtomId {
    AtomId::new(Anchor::new(vec![0xA0; 4]), Label::try_from("pkg").unwrap())
}

fn build_founding(tmb_bytes: Vec<u8>) -> CharterPayload {
    CharterPayload::new(
        Alg::ES256,
        1_000,
        vec![OwnerRef::new(OwnerKind::SingleKey, vec![0x22])], /* owner is irrelevant to the
                                                                * bootstrap gate check */
        None,
        vec![0x33; 4],
        Thumbprint::from_bytes(tmb_bytes),
    )
    .expect("non-empty owner set")
}

fn build_preexisting_claim(owner_bytes: Vec<u8>) -> ClaimPayload {
    ClaimPayload::new(
        Alg::ES256,
        bootstrap_atom_id(),
        500,
        OwnerRef::new(OwnerKind::SingleKey, owner_bytes),
        "cargo".to_string(),
        vec![0x44; 4],
        Thumbprint::from_bytes(vec![0x55]), // tmb is irrelevant to the gate check
    )
}

/// A randomly generated (founding-charter, pre-existing-claim) pair for
/// `verify_bootstrap_gate`, deliberately biased to exercise all three of
/// the function's outcome classes rather than relying on coincidental
/// byte-vector collision:
#[derive(Debug, Clone)]
enum ClaimCase {
    /// No pre-existing claim: the gate must pass trivially.
    NoClaim,
    /// The founding charter's signer matches the claim's owner: authorized.
    Authorized(Vec<u8>),
    /// The founding charter's signer does NOT match the claim's owner:
    /// unauthorized.
    Unauthorized(Vec<u8>, Vec<u8>),
}

fn claim_case_strategy() -> impl Strategy<Value = ClaimCase> {
    prop_oneof![
        Just(ClaimCase::NoClaim),
        proptest::collection::vec(any::<u8>(), 1..8).prop_map(ClaimCase::Authorized),
        (
            proptest::collection::vec(any::<u8>(), 1..8),
            proptest::collection::vec(any::<u8>(), 1..8),
        )
            .prop_filter(
                "founder and claim-owner bytes must differ for the unauthorized case",
                |(a, b)| a != b
            )
            .prop_map(|(a, b)| ClaimCase::Unauthorized(a, b)),
    ]
}

proptest! {
    /// [c3-bootstrap-gate-property]: the gate accepts iff the founding
    /// charter's signer matches the claim's owner or there's no
    /// pre-existing claim, and rejects every other generated combination.
    #[test]
    fn c3_bootstrap_gate_property(case in claim_case_strategy()) {
        match case.clone() {
            ClaimCase::NoClaim => {
                let founding = build_founding(vec![0x66]);
                let result = verify_bootstrap_gate(&founding, None);
                prop_assert!(result.is_ok(), "no pre-existing claim must pass trivially: {result:?}");
            },
            ClaimCase::Authorized(bytes) => {
                let founding = build_founding(bytes.clone());
                let claim = build_preexisting_claim(bytes);
                let result = verify_bootstrap_gate(&founding, Some(&claim));
                prop_assert!(result.is_ok(), "matching signer/owner must be authorized: {result:?}");
            },
            ClaimCase::Unauthorized(founder_bytes, claim_owner_bytes) => {
                let founding = build_founding(founder_bytes);
                let claim = build_preexisting_claim(claim_owner_bytes);
                let result = verify_bootstrap_gate(&founding, Some(&claim));
                prop_assert!(
                    matches!(result, Err(VerifyError::Unauthorized)),
                    "mismatched signer/owner must fail closed: {result:?}"
                );
            },
        }
    }
}

// ============================================================================
// Digest-seam properties (c4) — AtomDigest/HashAlg, NOT the Alloy OID model
// ============================================================================
//
// `atom-git`'s `mod proptests { proptest! { ... } }`
// (`atom/atom-git/tests/integration.rs:1025+`) is this codebase's sibling
// integration-property idiom, but `digest.rs`'s OWN existing property
// tests (`try_from_czd_dispatches_by_length_and_round_trips`,
// `try_from_czd_rejects_arbitrary_non_coz_lengths`) already establish
// `bolero::check!()` as the closer idiom for AtomDigest/Czd-length
// properties specifically — matched here rather than `proptest!`.

/// Dispatch-and-round-trip across the three coz-family digest lengths
/// (32/48/64 bytes -> sha256/sha384/sha512), constructed via the public
/// `TryFrom<Czd>` path (distinct from `digest.rs`'s own internal unit
/// tests, which exercise the same claim from inside the crate).
#[test]
fn c4_round_trip_and_dispatch_across_coz_families() {
    bolero::check!()
        .with_type::<(u8, Vec<u8>)>()
        .for_each(|(choice, seed)| {
            let (len, expected_alg) = match choice % 3 {
                0 => (32, HashAlg::Sha256),
                1 => (48, HashAlg::Sha384),
                _ => (64, HashAlg::Sha512),
            };
            let base: Vec<u8> = if seed.is_empty() {
                vec![0x11]
            } else {
                seed.clone()
            };
            let bytes: Vec<u8> = base.iter().copied().cycle().take(len).collect();

            let digest =
                AtomDigest::try_from(Czd::from_bytes(bytes)).expect("valid coz digest length");
            assert_eq!(digest.alg(), expected_alg);

            let round_tripped: AtomDigest = digest.to_string().parse().expect("round-trip parse");
            assert_eq!(
                digest, round_tripped,
                "AtomDigest must round-trip byte-identical"
            );
        });
}

/// [c4-digest-seam-property]: the sharpest cross-family disjointness case
/// -- sha256 (32B, base64url) and blake3 (32B, hex) are the only two
/// `HashAlg` variants sharing a digest length, so this is the one pair
/// where identical byte content is a genuine adversarial collision
/// *attempt* rather than structurally impossible via length mismatch
/// alone. Also covers round-trip for both.
#[test]
fn c4_cross_family_disjointness_same_length_adversarial() {
    bolero::check!().with_type::<Vec<u8>>().for_each(|seed| {
        let base: Vec<u8> = if seed.is_empty() {
            vec![0x5A]
        } else {
            seed.clone()
        };
        let bytes: Vec<u8> = base.iter().copied().cycle().take(32).collect();

        let sha256_digest = AtomDigest::try_from(Czd::from_bytes(bytes.clone()))
            .expect("32 bytes is a valid coz digest length");
        assert_eq!(sha256_digest.alg(), HashAlg::Sha256);

        let blake3_digest: AtomDigest = format!("blake3:{}", hex::encode(&bytes))
            .parse()
            .expect("well-formed blake3 digest string");
        assert_eq!(blake3_digest.alg(), HashAlg::Blake3);

        assert_eq!(
            sha256_digest.cad().as_bytes(),
            blake3_digest.cad().as_bytes(),
            "sanity: byte content is identical by construction"
        );
        assert_ne!(
            sha256_digest, blake3_digest,
            "sha256 and blake3 must never be equal, even with byte-identical 32B content"
        );

        let rt_sha256: AtomDigest = sha256_digest
            .to_string()
            .parse()
            .expect("sha256 round-trip");
        assert_eq!(sha256_digest, rt_sha256);
        let rt_blake3: AtomDigest = blake3_digest
            .to_string()
            .parse()
            .expect("blake3 round-trip");
        assert_eq!(blake3_digest, rt_blake3);
    });
}
