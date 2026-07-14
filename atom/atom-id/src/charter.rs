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

/// Verify a succession chain of charters for linearity and authorization.
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
/// **Out of scope.** This function does not itself re-verify each
/// payload's Coz signature — a `CharterPayload` here is assumed to have
/// already passed [`verify_charter`]/`verify_signature` upstream (this
/// type carries no raw signature bytes to re-check). It also does not
/// implement `[chain-monotonicity]` (a served chain regressing below a
/// consumer's previously recorded head): that is inherently stateful —
/// this signature has no recorded-head input to compare against — and is
/// a separate, not-yet-designed obligation (see
/// `atom/atom-id/tests/charter/chain_monotonicity.rs`).
///
/// Spec constraints: `[charter-anchor]`, `[charter-succession]`,
/// `[charter-succession-linear]`.
#[cfg(feature = "serde")]
pub fn verify_succession_chain(chain: &[CharterPayload]) -> Result<(), crate::VerifyError> {
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

        let result = verify_succession_chain(&[founding, successor_a, successor_b]);
        assert!(result.is_err(), "divergent successors must fail closed");
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
