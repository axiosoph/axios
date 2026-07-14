//! Charter transactions — atom-set chartering and succession.
//!
//! A charter defines and re-anchors atom-set identity: the founding
//! charter's coz digest IS the atom-set's `Anchor` (`[charter-anchor]`,
//! `[charter-transition]`). Unlike claims and publishes, a charter has
//! no `anchor`/`label` field — it *defines* the anchor rather than
//! referencing one.
//!
//! Spec: `docs/specs/atom-transactions.md` §CharterPayload,
//! `[charter-*]`, `[chain-*]`.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Alg, Czd, Thumbprint};

/// Transaction type for atom-set charters.
///
/// Spec constraint: `[charter-typ]`.
pub const TYP_CHARTER: &str = "atom/charter";

// ============================================================================
// CharterPayload
// ============================================================================

/// Payload for an `atom/charter` transaction.
///
/// Charters define and re-anchor atom-set identity. The founding charter
/// (no `prior`) has no pre-existing anchor to reference — its own coz
/// digest BECOMES the atom-set's [`Anchor`](crate::Anchor):
/// `Anchor == czd(charter₀)` (`[charter-anchor]`). A successor charter
/// (with `prior`) transfers ownership without changing the anchor
/// (`[charter-succession]`).
///
/// Unlike [`ClaimPayload`](crate::ClaimPayload) and
/// [`PublishPayload`](crate::PublishPayload), `CharterPayload` carries no
/// `anchor`/`label` field — it defines the anchor rather than referencing
/// one.
///
/// Spec constraints: `[charter-typ]`, `[charter-anchor]`,
/// `[charter-succession]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CharterPayload {
    /// The signing algorithm.
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_alg"))]
    pub alg: Alg,
    /// Timestamp (seconds since Unix epoch). Untrusted for authority
    /// ordering — chain position (`prior` links) governs precedence
    /// (`[charter-succession-linear]`).
    pub now: u64,
    /// Opaque identity digest of the owner (e.g., Coz thumbprint or Cyphr PR).
    ///
    /// Spec constraint: `[owner-abstract]`.
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_b64"))]
    pub owner: Vec<u8>,
    /// The czd of the charter this one succeeds. `None` for the founding
    /// charter, which defines the atom-set's anchor.
    ///
    /// Spec constraint: `[charter-succession]`.
    pub prior: Option<Czd>,
    /// Source revision demarking the chartering point. History prior to
    /// this point is unowned by the set unless re-claimed after it.
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_b64"))]
    pub src: Vec<u8>,
    /// Coz key thumbprint of the signing key.
    pub tmb: Thumbprint,
    /// Transaction type — always [`TYP_CHARTER`].
    pub typ: String,
}

impl CharterPayload {
    /// Construct a new charter payload.
    ///
    /// Sets `typ` to [`TYP_CHARTER`] automatically. Pass `prior: None` to
    /// construct a founding charter, or `prior: Some(czd)` for a successor.
    pub fn new(
        alg: Alg,
        now: u64,
        owner: Vec<u8>,
        prior: Option<Czd>,
        src: Vec<u8>,
        tmb: Thumbprint,
    ) -> Self {
        Self {
            alg,
            now,
            owner,
            prior,
            src,
            tmb,
            typ: TYP_CHARTER.to_owned(),
        }
    }
}

// ============================================================================
// Verification
// ============================================================================

/// Verify a signed `atom/charter` transaction.
///
/// Validates the Coz signature, deserializes the payload, and checks
/// that `typ` is [`TYP_CHARTER`]. Returns the parsed [`CharterPayload`]
/// on success.
///
/// This verifies a **single** charter's signature and shape only. It does
/// NOT walk or validate a succession chain — see [`verify_succession_chain`]
/// for that.
///
/// Spec constraints: `[sig-over-pay]`, `[charter-typ]`.
#[cfg(feature = "serde")]
pub fn verify_charter(
    pay_json: &[u8],
    sig: &[u8],
    alg: &str,
    pub_key: &[u8],
) -> Result<CharterPayload, crate::VerifyError> {
    crate::verify_signature(pay_json, sig, alg, pub_key)?;
    let payload: CharterPayload = serde_json::from_slice(pay_json)?;
    if payload.typ != TYP_CHARTER {
        return Err(crate::VerifyError::WrongTyp {
            expected: TYP_CHARTER,
            actual: payload.typ,
        });
    }
    Ok(payload)
}

/// Verify a succession chain of charters for linearity, authorization,
/// and (given a recorded head) monotonicity.
///
/// Given a chain already resolved and ordered by the caller (position `i`
/// is the successor of position `i-1` — resolving that ordering from the
/// underlying storage/encoding is the caller's job, not this function's;
/// see `[charter-succession-via-prior]`), checks:
///
/// - `chain[0]` carries no `prior` — it is the founding charter (`[charter-anchor]`).
/// - No two charters in the chain name the same `prior`: a **set-authority fork**, per
///   `[charter-succession-linear]`, MUST fail closed rather than pick a branch.
/// - Each successor's signing key (`tmb`) is authorized by its `prior` charter's `owner`
///   (single-key identity, `[owner-authorization-delegated]`), per `[charter-succession]`.
/// - If `recorded_head` is `Some`, the chain demonstrably extends past it (`[chain-monotonicity]`)
///   — see below.
///
/// **Dual-signed transfers are chained, not multi-signed.**
/// `[charter-succession-linear]` requires an ownership transfer to be
/// authorized by both the outgoing owner (the `prior` charter's signer)
/// and the incoming owner (proof of possession). `coz_rs::Coz<A>` carries
/// exactly one signature per message, so this is NOT expressed as multiple
/// signatures embedded in a single Coz message — it is expressed the same
/// way succession itself is: as a chain of independently-signed
/// transactions linked via `prior`. Applying the owner-authorization check
/// to every consecutive pair already captures this: the incoming owner's
/// proof of possession is exactly their own key signing the *next* link.
///
/// **`[chain-monotonicity]` — recorded-head check.** A consumer that has
/// previously recorded a chain head's czd passes it as `recorded_head`.
/// The served chain must demonstrably extend past that point: some
/// successor's `prior` must equal `recorded_head`, proving the chain
/// progressed beyond it. A chain that never mentions `recorded_head` is
/// treated as a regression (rollback) and rejected.
///
/// *Known limitation — a false-positive footgun, not a mere ambiguity.*
/// When the served chain is legitimately unchanged since the caller's
/// last observation (`recorded_head` equals the chain's own current tail
/// czd — the common steady-state poll, not a corner case), **no element
/// of the chain can name its own tail as a `prior`**, so the
/// extends-past check below is unconditionally `false` and this function
/// returns `Err(VerifyError::ChainRegression)` — a false "rollback
/// detected" — on every such call. This parameter proves only "the chain
/// grew past `recorded_head`"; it can NEVER confirm "no regression
/// happened" for an unchanged chain. Distinguishing the two would
/// require the served chain's own tail czd, which this function cannot
/// compute (`CharterPayload` carries no raw signature bytes; see below).
///
/// **A caller MUST peel off the unchanged case itself before calling:**
/// compare `recorded_head` against the served chain's own tail czd
/// (which the caller — having resolved the chain from its git-ref-keyed
/// storage — already has for free) and skip this call entirely when they
/// match. Passing an unchanged chain's own tail as `recorded_head` and
/// trusting this function to report "no regression" WILL misfire.
///
/// **Out of scope.** This function does not itself re-verify each
/// payload's Coz signature — a `CharterPayload` here is assumed to have
/// already passed [`verify_charter`]/`verify_signature` upstream (this
/// type carries no raw signature bytes to re-check).
///
/// Spec constraints: `[charter-anchor]`, `[charter-succession]`,
/// `[charter-succession-linear]`, `[chain-monotonicity]`.
#[cfg(feature = "serde")]
pub fn verify_succession_chain(
    chain: &[CharterPayload],
    recorded_head: Option<&Czd>,
) -> Result<(), crate::VerifyError> {
    let Some((founding, successors)) = chain.split_first() else {
        return Err(crate::VerifyError::EmptyChain);
    };
    if founding.prior.is_some() {
        return Err(crate::VerifyError::NotFoundingCharter);
    }

    // [charter-succession-linear]: at most one valid successor per prior.
    for (i, a) in chain.iter().enumerate() {
        let Some(a_prior) = &a.prior else { continue };
        if chain[i + 1..]
            .iter()
            .any(|b| b.prior.as_ref() == Some(a_prior))
        {
            return Err(crate::VerifyError::DivergentSuccessors);
        }
    }

    // [charter-succession]: each successor's signer authorized by its
    // prior charter's owner.
    let mut previous = founding;
    for successor in successors {
        if successor.tmb.as_bytes() != previous.owner.as_slice() {
            return Err(crate::VerifyError::Unauthorized);
        }
        previous = successor;
    }

    // [chain-monotonicity]: the chain must demonstrably extend past a
    // previously recorded head (see "Known limitation" above).
    if let Some(head) = recorded_head {
        let extends_past_head = successors.iter().any(|c| c.prior.as_ref() == Some(head));
        if !extends_past_head {
            return Err(crate::VerifyError::ChainRegression);
        }
    }

    Ok(())
}

/// Verify the founding-charter bootstrap gate.
///
/// Per `[charter-transition]` PRE (founding): if the source already
/// carries claims predating any charter, the founding charter's signing
/// key MUST be authorized by the owner of the earliest such claim —
/// chartering over a live, claimed set is a migration act reserved to
/// its incumbent, not a race open to strangers. A virgin source (no
/// pre-existing claims) is first-to-charter by design
/// (`[charter-fork-distinction]`) and passes trivially.
///
/// Resolving *which* claim is earliest (walking a source's claim history
/// to find the one predating any charter, if any) is the caller's job —
/// this function checks authorization only, given that resolution as
/// input, the same division of labor [`verify_succession_chain`] uses
/// for its already-resolved chain.
///
/// Spec constraint: `[charter-transition]`.
#[cfg(feature = "serde")]
pub fn verify_bootstrap_gate(
    founding: &CharterPayload,
    earliest_preexisting_claim: Option<&crate::ClaimPayload>,
) -> Result<(), crate::VerifyError> {
    let Some(claim) = earliest_preexisting_claim else {
        return Ok(());
    };
    if founding.tmb.as_bytes() != claim.owner.as_slice() {
        return Err(crate::VerifyError::Unauthorized);
    }
    Ok(())
}

// ============================================================================
// Charter ref-storage seam
// ============================================================================

/// Minimal seam declaring how a charter is addressed and retrieved by its
/// czd.
///
/// This is the storage seam parent AC4 requires: a later phase provides a
/// concrete `impl CharterStore` (e.g. backed by git refs, per
/// `[charter-transition]` POST: "stored in the source's atom refs ...
/// retrievable by its czd"). Declaration only — no working
/// storage/persistence ships with this trait.
pub trait CharterStore {
    /// Retrieve a charter by its czd.
    ///
    /// Async per `[trait-async-io]`: a real store is backed by git refs
    /// (or similar), and ref lookup is I/O — potentially over the network
    /// for a remote source — not an in-memory lookup.
    ///
    /// **Deliberately unimplemented — Phase 1.** The default body is an
    /// honest stub; a concrete store implementation MUST override this to
    /// provide real retrieval rather than inheriting the panic.
    fn get_charter(
        &self,
        _czd: &Czd,
    ) -> impl std::future::Future<Output = Option<CharterPayload>> + Send {
        async move {
            unimplemented!(
                "Phase 1: charter ref-storage retrieval is a specified deliverable, not a default \
                 — see docs/specs/atom-transactions.md [charter-transition] POST"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_ed25519_key() -> (Vec<u8>, Vec<u8>, crate::Thumbprint) {
        use coz_rs::Ed25519;

        let sk = coz_rs::SigningKey::<Ed25519>::generate();
        let prv = sk.private_key_bytes();
        let pub_bytes = sk.verifying_key().public_key_bytes().to_vec();
        let tmb = sk.thumbprint().clone();
        (prv, pub_bytes, tmb)
    }

    #[test]
    fn charter_payload_typ_constant() {
        let charter = CharterPayload::new(
            crate::Alg::ES256,
            1000,
            vec![99],
            None,
            vec![0; 32],
            crate::Thumbprint::from_bytes(vec![10, 20, 30]),
        );
        assert_eq!(charter.typ, TYP_CHARTER);
        assert_eq!(charter.typ, "atom/charter");
    }

    #[test]
    fn charter_payload_prior_optional() {
        let founding = CharterPayload::new(
            crate::Alg::ES256,
            1000,
            vec![99],
            None,
            vec![0; 32],
            crate::Thumbprint::from_bytes(vec![10, 20, 30]),
        );
        assert_eq!(founding.prior, None);

        let successor = CharterPayload::new(
            crate::Alg::ES256,
            2000,
            vec![100],
            Some(crate::Czd::from_bytes(vec![1, 2, 3])),
            vec![1; 32],
            crate::Thumbprint::from_bytes(vec![40, 50, 60]),
        );
        assert_eq!(successor.prior, Some(crate::Czd::from_bytes(vec![1, 2, 3])));
    }

    #[test]
    fn charter_payload_serde_roundtrip() {
        let charter = CharterPayload::new(
            crate::Alg::ES256,
            1000,
            vec![99],
            Some(crate::Czd::from_bytes(vec![1, 2, 3])),
            vec![0; 32],
            crate::Thumbprint::from_bytes(vec![10, 20, 30]),
        );
        let json = serde_json::to_string(&charter).unwrap();
        let back: CharterPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, charter);
    }

    #[test]
    fn verify_charter_roundtrip() {
        let (prv, pub_bytes, tmb) = gen_ed25519_key();
        let charter =
            CharterPayload::new(crate::Alg::Ed25519, 1000, vec![99], None, vec![0; 32], tmb);
        let pay_json = serde_json::to_vec(&charter).unwrap();
        let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

        let result = verify_charter(&pay_json, &sig, "Ed25519", &pub_bytes);
        assert!(result.is_ok(), "valid charter should verify: {result:?}");
        let verified = result.unwrap();
        assert_eq!(verified.typ, TYP_CHARTER);
    }

    #[test]
    fn verify_charter_wrong_typ() {
        let (prv, pub_bytes, tmb) = gen_ed25519_key();
        let charter =
            CharterPayload::new(crate::Alg::Ed25519, 1000, vec![99], None, vec![0; 32], tmb);
        let mut json_val: serde_json::Value = serde_json::to_value(&charter).unwrap();
        json_val["typ"] = serde_json::Value::String("atom/claim".into());
        let pay_json = serde_json::to_vec(&json_val).unwrap();
        let (sig, _cad) = coz_rs::sign_json(&pay_json, "Ed25519", &prv, &pub_bytes).unwrap();

        let result = verify_charter(&pay_json, &sig, "Ed25519", &pub_bytes);
        assert!(
            matches!(result, Err(crate::VerifyError::WrongTyp { .. })),
            "tampered typ should fail with WrongTyp: {result:?}"
        );
    }

    #[test]
    fn verify_succession_chain_rejects_divergent_successors() {
        // Two successors both naming the same `prior` is a set-authority
        // fork per [charter-succession-linear] — the walk MUST fail
        // closed rather than pick either branch.
        let (_prv0, _pub0, tmb0) = gen_ed25519_key();
        let founding =
            CharterPayload::new(crate::Alg::Ed25519, 1000, vec![1], None, vec![0; 32], tmb0);
        let founding_czd = crate::Czd::from_bytes(vec![9, 9, 9]); // stand-in for czd(founding)

        let (_prv1, _pub1, tmb1) = gen_ed25519_key();
        let successor_a = CharterPayload::new(
            crate::Alg::Ed25519,
            2000,
            vec![2],
            Some(founding_czd.clone()),
            vec![1; 32],
            tmb1,
        );
        let (_prv2, _pub2, tmb2) = gen_ed25519_key();
        let successor_b = CharterPayload::new(
            crate::Alg::Ed25519,
            2001,
            vec![3],
            Some(founding_czd),
            vec![1; 32],
            tmb2,
        );

        let result = verify_succession_chain(&[founding, successor_a, successor_b], None);
        assert!(result.is_err(), "divergent successors must fail closed");
    }

    #[test]
    fn verify_succession_chain_accepts_progression_past_recorded_head() {
        // A chain that genuinely extends past `recorded_head` (some
        // successor's `prior` names it) is not a regression.
        let founding_owner = vec![1];
        let founding = CharterPayload::new(
            crate::Alg::Ed25519,
            1000,
            founding_owner.clone(),
            None,
            vec![0; 32],
            crate::Thumbprint::from_bytes(vec![0]),
        );
        let founding_czd = crate::Czd::from_bytes(vec![9, 9, 9]); // stand-in for czd(founding)

        let successor = CharterPayload::new(
            crate::Alg::Ed25519,
            2000,
            vec![2],
            Some(founding_czd.clone()),
            vec![1; 32],
            crate::Thumbprint::from_bytes(founding_owner), // authorized by founding's owner
        );

        let result = verify_succession_chain(&[founding, successor], Some(&founding_czd));
        assert!(
            result.is_ok(),
            "chain extending past the recorded head is not a regression: {result:?}"
        );
    }

    #[test]
    fn verify_succession_chain_rejects_regression_below_recorded_head() {
        // A chain that never mentions `recorded_head` (a prefix rollback,
        // per [chain-monotonicity]) must fail closed.
        let (_prv0, _pub0, tmb0) = gen_ed25519_key();
        let founding =
            CharterPayload::new(crate::Alg::Ed25519, 1000, vec![1], None, vec![0; 32], tmb0);
        let unreached_head = crate::Czd::from_bytes(vec![7, 7, 7]); // never named by this chain

        let result = verify_succession_chain(&[founding], Some(&unreached_head));
        assert!(
            result.is_err(),
            "a chain never reaching the recorded head must be rejected"
        );
    }

    #[test]
    fn verify_bootstrap_gate_passes_with_no_preexisting_claim() {
        let (_prv, _pub, tmb) = gen_ed25519_key();
        let founding =
            CharterPayload::new(crate::Alg::Ed25519, 1000, vec![1], None, vec![0; 32], tmb);
        let result = verify_bootstrap_gate(&founding, None);
        assert!(
            result.is_ok(),
            "a virgin source is first-to-charter: {result:?}"
        );
    }

    #[test]
    fn verify_bootstrap_gate_accepts_incumbent_authorized_founder() {
        let (_prv, _pub, tmb) = gen_ed25519_key();
        let founding = CharterPayload::new(
            crate::Alg::Ed25519,
            1000,
            vec![1],
            None,
            vec![0; 32],
            tmb.clone(), // founding signed by the incumbent's own key
        );
        let pre_existing_claim = crate::ClaimPayload::new(
            crate::Alg::Ed25519,
            crate::AtomId::new(
                crate::Anchor::new(vec![0; 4]),
                crate::Label::try_from("x").unwrap(),
            ),
            500,
            tmb.as_bytes().to_vec(), // claim owner == founding's signing tmb
            "cargo".to_string(),
            vec![0; 32],
            tmb,
        );
        let result = verify_bootstrap_gate(&founding, Some(&pre_existing_claim));
        assert!(
            result.is_ok(),
            "incumbent-authorized founding must pass: {result:?}"
        );
    }

    #[test]
    fn verify_bootstrap_gate_rejects_unauthorized_founder() {
        let (_prv_incumbent, _pub_incumbent, tmb_incumbent) = gen_ed25519_key();
        let (_prv_stranger, _pub_stranger, tmb_stranger) = gen_ed25519_key();
        let founding = CharterPayload::new(
            crate::Alg::Ed25519,
            1000,
            vec![1],
            None,
            vec![0; 32],
            tmb_stranger, // signed by a stranger, not the incumbent
        );
        let pre_existing_claim = crate::ClaimPayload::new(
            crate::Alg::Ed25519,
            crate::AtomId::new(
                crate::Anchor::new(vec![0; 4]),
                crate::Label::try_from("x").unwrap(),
            ),
            500,
            tmb_incumbent.as_bytes().to_vec(),
            "cargo".to_string(),
            vec![0; 32],
            tmb_incumbent,
        );
        let result = verify_bootstrap_gate(&founding, Some(&pre_existing_claim));
        assert!(result.is_err(), "unauthorized founding must fail closed");
    }

    struct NullCharterStore;
    impl CharterStore for NullCharterStore {}

    #[tokio::test]
    #[should_panic(expected = "Phase 1")]
    async fn charter_store_stub_is_honest() {
        let store = NullCharterStore;
        let czd = crate::Czd::from_bytes(vec![1, 2, 3]);
        let _ = store.get_charter(&czd).await;
    }
}
