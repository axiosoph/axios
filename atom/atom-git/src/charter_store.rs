//! Charter storage: resolve charter commits from the czd-keyed
//! `refs/atom/charter/d/*` ref family (`docs/specs/git-storage-format.md`
//! `[charter-ref-by-czd]`).
//!
//! Writing the ref itself (the `PreviousValue::MustNotExist` ref-edit that
//! pairs with [`crate::gix_util::write_charter_commit`]) is
//! `n3-charter-registry`'s job — this module is read-side resolution plus
//! the commit primitive's write-side counterpart lives in `gix_util.rs`.
//!
//! **The czd-binding hole.** `CharterStore::get_charter`'s declared return
//! type carries no signature bytes, and `refs/atom/charter/d/{czd}` is a
//! publisher-controlled ref key — nothing at the git layer stops a ref from
//! serving a payload under the WRONG czd key. Every function here that
//! resolves a charter from a ref MUST recompute the czd from the actual
//! signed envelope bytes (`atom_id::czd_for_alg`) and confirm it binds to
//! the key it was looked up under before trusting the payload at all.

use atom_id::{Anchor, CharterPayload, Czd};

use crate::error::GitError;
use crate::source::CozMessageEnvelope;

/// The czd-keyed charter ref path for a given czd's raw bytes.
///
/// **Critical shared seam**: this encoding must be IDENTICAL between the
/// write side (`n3-charter-registry`'s ref-edit) and the read side
/// (`resolve_founding_charter`'s direct lookup) — if they ever disagree,
/// every resolution silently misses with no error. `hex_encode` is the
/// same helper already shared by every other czd-keyed ref family in this
/// crate (`refs/atom/claims/d/*`, `refs/atom/d/*`).
pub fn charter_ref_name(czd_bytes: &[u8]) -> String {
    format!(
        "refs/atom/charter/d/{}",
        crate::store::hex_encode(czd_bytes)
    )
}

/// Parse a charter commit's `CozMessage` body and recompute its czd from
/// the actual signed bytes.
///
/// Returns `(payload, recomputed_czd)` — the caller decides how to use the
/// recomputed czd (bind-check against an expected key, or record it while
/// walking a chain). Never trusts the ref key the commit was found under.
///
/// Also closes the tmb-binding soundness gap (Verification Pipeline step
/// 6, charter side) before returning: `payload.tmb` is a self-declared
/// field, and `verify_charter` alone only proves SOME valid signature
/// exists over the payload for the embedded key -- not that the embedded
/// key is the one `tmb` claims. This is the single choke point every
/// charter resolver in this module goes through
/// ([`resolve_founding_charter`] and [`resolve_effective_charter`]'s
/// chain-candidate enumeration alike), so fixing it here closes the gap
/// for every charter [`atom_id::verify_succession_chain`] is ever asked
/// to trust: an unbound `tmb` would otherwise let an attacker sign a
/// forged successor with their OWN key while declaring a legitimate
/// charter-set member's thumbprint, defeating the per-link authorization
/// check -- a full anchor takeover, not a narrow gap.
fn parse_and_verify_charter_commit(
    repo: &gix::Repository,
    oid: gix::hash::ObjectId,
) -> Result<(CharterPayload, Czd), GitError> {
    let obj = repo.find_object(oid)?;
    let commit = obj.try_into_commit()?;
    let msg_str = commit.message_raw_sloppy().to_string();

    let envelope: CozMessageEnvelope = serde_json::from_str(&msg_str)?;
    let pay_bytes = serde_json::to_vec(&envelope.pay)?;
    let pub_key = envelope.key.as_ref().ok_or_else(|| {
        GitError::Validation("Charter CozMessage is missing the key field".into())
    })?;
    let alg_str = envelope
        .pay
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GitError::Validation("Charter alg field is missing or invalid".into()))?;

    // The czd-binding hole: recompute from the actual signed bytes, never
    // trust a ref key or a bare deserialized payload as proof of identity.
    let computed_czd = atom_id::czd_for_alg(&pay_bytes, &envelope.sig, alg_str)?;
    let payload = atom_id::verify_charter(&pay_bytes, &envelope.sig, alg_str, pub_key)?;
    atom_id::verify_charter_key_thumbprint(&payload, alg_str, pub_key)?;

    Ok((payload, computed_czd))
}

/// Resolve the founding charter of the atom-set anchored at `anchor`.
///
/// Since `anchor == czd(charter₀)` by definition (`[charter-anchor]`),
/// this is a direct keyed lookup at `refs/atom/charter/d/{anchor-hex}`.
/// Returns `Ok(None)` — not an error — when: the ref doesn't exist, the
/// stored payload's recomputed czd does not bind to `anchor` (the
/// czd-binding hole, adversarially rejected), or the payload carries a
/// non-`None` `prior` (it is a real charter, just not a founding one for
/// this anchor). A genuine I/O, parse, or signature-verification failure
/// still propagates as `Err` — those are not "not found," they are "the
/// store or the data is broken."
pub fn resolve_founding_charter(
    repo: &gix::Repository,
    anchor: &Anchor,
) -> Result<Option<CharterPayload>, GitError> {
    let ref_name = charter_ref_name(anchor.as_bytes());
    let Some(reference) = repo.try_find_reference(&ref_name)? else {
        return Ok(None);
    };
    let oid = reference.id().detach();

    let (payload, computed_czd) = parse_and_verify_charter_commit(repo, oid)?;

    if computed_czd.as_bytes() != anchor.as_bytes() {
        // Binding mismatch: the ref key lied about what it serves.
        return Ok(None);
    }
    if payload.prior.is_some() {
        // A real, validly-signed, correctly-bound charter -- just not the
        // founding one (it names a `prior`).
        return Ok(None);
    }

    Ok(Some(payload))
}

/// Resolve the effective (most recent valid) charter for the atom-set
/// anchored at `anchor`, plus the full ordered succession chain from the
/// founding charter to that head.
///
/// Enumerates the flat, repo-wide `refs/atom/charter/d/*` family
/// (`[charter-ref-by-czd]` — no atom-set-scoping segment exists in the ref
/// path), bind-verifies every candidate the same way
/// [`resolve_founding_charter`] does, then walks forward from the founding
/// charter by following `prior` links. `verify_succession_chain` is the
/// authority on linearity/authorization/divergence — if the walk finds two
/// distinct candidates naming the same `prior` (a set-authority fork), both
/// are included in the assembled chain (rather than the walk picking a
/// branch) so `verify_succession_chain`'s own scan detects and rejects the
/// divergence, propagated here as `Err`.
///
/// `recorded_head` is threaded straight through to
/// [`atom_id::verify_succession_chain`]'s own `[chain-monotonicity]` check
/// — `None` when the caller has no previously recorded head (or is not yet
/// persisting one; this function does no persistence of its own, only the
/// threading), `Some` to enforce that the resolved chain demonstrably
/// extends past it. See that function's doc comment for the
/// steady-state-unchanged caveat a caller passing a real recorded head
/// MUST account for.
pub fn resolve_effective_charter(
    repo: &gix::Repository,
    anchor: &Anchor,
    recorded_head: Option<&Czd>,
) -> Result<Option<(CharterPayload, Vec<CharterPayload>)>, GitError> {
    let Some(founding) = resolve_founding_charter(repo, anchor)? else {
        return Ok(None);
    };

    // Enumerate every entry in the flat charter ref family, keeping each
    // candidate's own recomputed czd alongside its payload so the
    // prior-chain walk below can match successors by identity.
    let prefix = "refs/atom/charter/d/";
    let mut candidates: Vec<(Czd, CharterPayload)> = Vec::new();
    for ref_res in repo.references()?.prefixed(prefix)? {
        let reference = ref_res.map_err(|e| GitError::Validation(e.to_string()))?;
        let ref_name = reference.name().as_bstr().to_string();
        let ref_czd_hex = ref_name.strip_prefix(prefix).unwrap_or("").to_string();
        let oid = reference.id().detach();

        let (payload, computed_czd) = parse_and_verify_charter_commit(repo, oid)?;

        // Bind each enumerated entry to its own ref key too -- an entry
        // stored under the wrong key is not a legitimate chain member and
        // must not silently participate in chain assembly.
        if crate::store::hex_encode(computed_czd.as_bytes()) != ref_czd_hex {
            continue;
        }

        candidates.push((computed_czd, payload));
    }

    // Walk forward from the founding charter by following `prior` links.
    // A step with zero matches simply ends the chain there (linear, no
    // divergence). A step with 2+ matches is a genuine set-authority fork:
    // push all of them into the chain and stop advancing -- `prior` no
    // longer picks a unique next step, so there is nothing further to walk
    // deterministically, but the fork itself must not be silently dropped.
    let mut chain = vec![founding];
    let mut current_czd_bytes = anchor.as_bytes().to_vec();
    loop {
        let matches: Vec<&(Czd, CharterPayload)> = candidates
            .iter()
            .filter(|(_, c)| {
                c.prior
                    .as_ref()
                    .is_some_and(|p| p.as_bytes() == current_czd_bytes.as_slice())
            })
            .collect();

        match matches.as_slice() {
            [] => break,
            [(czd, payload)] => {
                current_czd_bytes = czd.as_bytes().to_vec();
                chain.push(payload.clone());
            },
            many => {
                for (_, payload) in many {
                    chain.push((*payload).clone());
                }
                break;
            },
        }
    }

    // Linearity, authorization, and divergence are `verify_succession_chain`'s
    // authority -- it fails closed (`VerifyError::DivergentSuccessors`) on
    // exactly the forked-chain shape assembled above. `recorded_head` is
    // threaded through (not hardcoded `None`) so a caller with a real
    // persisted head can actually get rollback detection
    // (`[chain-monotonicity]`) rather than the check being permanently
    // unreachable in production.
    atom_id::verify_succession_chain(&chain, recorded_head)?;

    let effective = chain
        .last()
        .expect("chain always contains at least the founding charter")
        .clone();
    Ok(Some((effective, chain)))
}

/// Thin async wrapper delegating to the sync resolvers above, for
/// consumers that want the [`atom_id::CharterStore`] trait interface.
///
/// The trait's `get_charter` returns a bare `Option<CharterPayload>` with
/// no error channel (`[charter-transition]` POST's Phase-1 seam), so any
/// I/O, parse, or verification failure here collapses to `None` rather
/// than propagating -- an intentional, documented consequence of the
/// trait's fixed signature, not a silent swallow: from this trait's
/// perspective, "the store is broken" and "no charter resolves" are
/// indistinguishable outcomes, and `None` is the only value the signature
/// can express for either.
impl atom_id::CharterStore for crate::source::GitSource {
    fn get_charter(
        &self,
        czd: &Czd,
    ) -> impl std::future::Future<Output = Option<CharterPayload>> + Send {
        let repo = self.repo();
        let anchor = Anchor::new(czd.as_bytes().to_vec());
        async move { resolve_founding_charter(&repo, &anchor).ok().flatten() }
    }
}

#[cfg(test)]
mod tests {
    use atom_id::Thumbprint;
    use coz_rs::{Alg, Ed25519, SigningKey};
    use gix::actor::SignatureRef;
    use gix::objs::Tree;
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
    use gix::refs::{FullName, Target};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn charter_ref_name_is_hex_keyed() {
        let bytes = [0xde, 0xad, 0xbe, 0xef];
        assert_eq!(charter_ref_name(&bytes), "refs/atom/charter/d/deadbeef");
    }

    /// Set up a test Git repository with a genesis commit, matching the
    /// established convention (`tests/tag_chain.rs::setup_test_repo`).
    fn setup_test_repo() -> (TempDir, gix::Repository) {
        let dir = TempDir::new().unwrap();
        let repo = gix::init(dir.path()).unwrap();

        let sig = SignatureRef::default();
        let empty_tree = Tree {
            entries: Vec::new(),
        };
        let tree_oid = repo.write_object(empty_tree).unwrap().detach();
        repo.commit_as(
            sig,
            sig,
            "refs/heads/master",
            "genesis commit",
            tree_oid,
            Vec::<gix::hash::ObjectId>::new(),
        )
        .unwrap();

        let repo = gix::open(dir.path()).unwrap();
        (dir, repo)
    }

    struct Keypair {
        prv: Vec<u8>,
        pub_key: Vec<u8>,
        tmb: Thumbprint,
    }

    fn gen_keypair() -> Keypair {
        let sk = SigningKey::<Ed25519>::generate();
        let prv = sk.private_key_bytes().to_vec();
        let pub_key = sk.verifying_key().public_key_bytes().to_vec();
        let tmb = coz_rs::compute_thumbprint_for_alg(Alg::Ed25519.name(), &pub_key).unwrap();
        Keypair { prv, pub_key, tmb }
    }

    /// A single-entry `single-key` owner set from raw bytes -- the common
    /// case throughout these tests, none of which exercise multi-member
    /// sets.
    fn single_owner(bytes: Vec<u8>) -> Vec<atom_id::OwnerRef> {
        vec![atom_id::OwnerRef::new(atom_id::OwnerKind::SingleKey, bytes)]
    }

    /// Sign a `CharterPayload` and build its `CozMessage` envelope JSON,
    /// mirroring `registry.rs::claim()`'s exact serialize/sign/envelope
    /// idiom.
    fn sign_charter(payload: &CharterPayload, prv: &[u8], pub_key: &[u8]) -> (String, Czd) {
        let pay_val = serde_json::to_value(payload).unwrap();
        let pay_map: indexmap::IndexMap<String, serde_json::Value> =
            serde_json::from_value(pay_val).unwrap();
        let pay_bytes = serde_json::to_vec(&pay_map).unwrap();

        let (sig, _cad) = coz_rs::sign_json(&pay_bytes, "Ed25519", prv, pub_key).unwrap();
        let czd = atom_id::czd_for_alg(&pay_bytes, &sig, "Ed25519").unwrap();

        let envelope = CozMessageEnvelope {
            pay: pay_map,
            sig,
            key: Some(pub_key.to_vec()),
        };
        (serde_json::to_string(&envelope).unwrap(), czd)
    }

    /// Write a charter commit via the real primitive and ref-edit it into
    /// place at `refs/atom/charter/d/{ref_czd}` -- `ref_czd` is a separate
    /// parameter from the payload's *actual* signed czd so adversarial
    /// tests can plant a deliberate binding mismatch.
    fn write_and_ref_charter(repo: &gix::Repository, charter_msg: String, ref_czd: &Czd) {
        let oid = crate::gix_util::write_charter_commit(repo, charter_msg).unwrap();
        let ref_name = charter_ref_name(ref_czd.as_bytes());
        let fullname = FullName::try_from(ref_name.as_str()).unwrap();
        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "write charter".into(),
                },
                expected: PreviousValue::MustNotExist,
                new: Target::Object(oid),
            },
            name: fullname,
            deref: false,
        })
        .unwrap();
    }

    // -------------------------------------------------------------
    // c2: resolve_founding_charter
    // -------------------------------------------------------------

    #[test]
    fn resolve_founding_charter_accepts_real_founding_charter() {
        let (_dir, repo) = setup_test_repo();
        let owner = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(owner.pub_key.clone()),
            None,
            vec![9u8; 20],
            owner.tmb.clone(),
        )
        .unwrap();
        let (msg, czd) = sign_charter(&founding, &owner.prv, &owner.pub_key);
        write_and_ref_charter(&repo, msg, &czd);

        let anchor = Anchor::new(czd.as_bytes().to_vec());
        let resolved = resolve_founding_charter(&repo, &anchor)
            .expect("resolution must not error")
            .expect("a real founding charter must resolve");
        assert_eq!(resolved, founding);
    }

    /// Adversarial (tmb-binding, founding): a charter validly signed by an
    /// ATTACKER's own key, but whose payload declares `tmb` equal to some
    /// other (victim) key's thumbprint, must be rejected -- signature
    /// verification alone cannot catch this, since any key can validly
    /// sign its own payload while lying about which key that is.
    #[test]
    fn resolve_founding_charter_rejects_forged_tmb() {
        let (_dir, repo) = setup_test_repo();
        let attacker = gen_keypair();
        let victim = gen_keypair();

        let forged = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(victim.pub_key.clone()),
            None,
            vec![9u8; 20],
            victim.tmb.clone(), // declares the VICTIM's tmb
        )
        .unwrap();
        // ...but is signed by the ATTACKER's own key.
        let (msg, czd) = sign_charter(&forged, &attacker.prv, &attacker.pub_key);
        write_and_ref_charter(&repo, msg, &czd);

        let anchor = Anchor::new(czd.as_bytes().to_vec());
        let result = resolve_founding_charter(&repo, &anchor);
        assert!(
            matches!(
                result,
                Err(GitError::Verify(atom_id::VerifyError::ThumbprintMismatch))
            ),
            "a charter signed by one key but declaring another key's tmb must be rejected, not \
             silently trusted on its declared tmb alone: {result:?}"
        );
    }

    #[test]
    fn resolve_founding_charter_rejects_missing_ref() {
        let (_dir, repo) = setup_test_repo();
        let anchor = Anchor::new(vec![1u8; 32]); // no ref written for this anchor

        let resolved = resolve_founding_charter(&repo, &anchor).expect("no I/O error");
        assert!(resolved.is_none(), "a nonexistent ref must resolve to None");
    }

    /// Adversarial: a ref key that does NOT match its payload's actual
    /// recomputed czd must be rejected -- this is the czd-binding hole
    /// (a2). A malicious/misconfigured ref cannot lie its way into being
    /// treated as the founding charter for an anchor it doesn't actually
    /// bind to.
    #[test]
    fn resolve_founding_charter_rejects_czd_binding_mismatch() {
        let (_dir, repo) = setup_test_repo();
        let owner = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(owner.pub_key.clone()),
            None,
            vec![9u8; 20],
            owner.tmb.clone(),
        )
        .unwrap();
        let (msg, real_czd) = sign_charter(&founding, &owner.prv, &owner.pub_key);

        // Plant the commit under a DIFFERENT czd-keyed ref than its actual
        // signed identity -- the binding hole this whole check exists for.
        let wrong_czd = Czd::from_bytes(vec![0xEE; real_czd.as_bytes().len()]);
        assert_ne!(real_czd, wrong_czd);
        write_and_ref_charter(&repo, msg, &wrong_czd);

        let anchor = Anchor::new(wrong_czd.as_bytes().to_vec());
        let resolved = resolve_founding_charter(&repo, &anchor).expect("no I/O error");
        assert!(
            resolved.is_none(),
            "a payload whose recomputed czd disagrees with its ref key must be rejected, not \
             trusted from the ref key alone"
        );
    }

    #[test]
    fn resolve_founding_charter_rejects_non_founding_payload() {
        let (_dir, repo) = setup_test_repo();
        let founder = gen_keypair();
        let successor = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(successor.pub_key.clone()),
            None,
            vec![9u8; 20],
            founder.tmb.clone(),
        )
        .unwrap();
        let (founding_msg, founding_czd) = sign_charter(&founding, &founder.prv, &founder.pub_key);
        write_and_ref_charter(&repo, founding_msg, &founding_czd);

        // A real, validly-signed, correctly-bound successor charter
        // (`prior` set) -- not a founding charter, even though it
        // verifies cleanly.
        let successor_payload = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_100,
            single_owner(vec![7u8; 4]),
            Some(founding_czd.clone()),
            vec![9u8; 20],
            successor.tmb.clone(),
        )
        .unwrap();
        let (successor_msg, successor_czd) =
            sign_charter(&successor_payload, &successor.prv, &successor.pub_key);
        write_and_ref_charter(&repo, successor_msg, &successor_czd);

        let anchor = Anchor::new(successor_czd.as_bytes().to_vec());
        let resolved = resolve_founding_charter(&repo, &anchor).expect("no I/O error");
        assert!(
            resolved.is_none(),
            "a charter with a non-None `prior` is not a founding charter, even if it verifies"
        );
    }

    // -------------------------------------------------------------
    // c3: resolve_effective_charter
    // -------------------------------------------------------------

    #[test]
    fn resolve_effective_charter_walks_multilink_chain_to_correct_tail() {
        let (_dir, repo) = setup_test_repo();

        // founding -> mid -> tail, each successor authorized by its
        // prior's owner.
        let owner0 = gen_keypair();
        let owner1 = gen_keypair();
        let owner2 = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1000,
            single_owner(owner1.tmb.as_bytes().to_vec()), /* founding names owner1 (by
                                                           * thumbprint) as next owner */
            None,
            vec![1u8; 20],
            owner0.tmb.clone(),
        )
        .unwrap();
        let (founding_msg, founding_czd) = sign_charter(&founding, &owner0.prv, &owner0.pub_key);
        write_and_ref_charter(&repo, founding_msg, &founding_czd);

        let mid = CharterPayload::new(
            Alg::Ed25519,
            2000,
            single_owner(owner2.tmb.as_bytes().to_vec()),
            Some(founding_czd.clone()),
            vec![1u8; 20],
            owner1.tmb.clone(), // authorized by founding's owner (owner1's tmb)
        )
        .unwrap();
        let (mid_msg, mid_czd) = sign_charter(&mid, &owner1.prv, &owner1.pub_key);
        write_and_ref_charter(&repo, mid_msg, &mid_czd);

        let tail = CharterPayload::new(
            Alg::Ed25519,
            3000,
            single_owner(vec![9u8; 4]),
            Some(mid_czd.clone()),
            vec![1u8; 20],
            owner2.tmb.clone(), // authorized by mid's owner (owner2's tmb)
        )
        .unwrap();
        let (tail_msg, tail_czd) = sign_charter(&tail, &owner2.prv, &owner2.pub_key);
        write_and_ref_charter(&repo, tail_msg, &tail_czd);

        let anchor = Anchor::new(founding_czd.as_bytes().to_vec());
        let (effective, chain) = resolve_effective_charter(&repo, &anchor, None)
            .expect("resolution must not error")
            .expect("a real chain must resolve");

        assert_eq!(
            effective, tail,
            "the effective head must be the chain's tail"
        );
        assert_eq!(
            chain,
            vec![founding, mid, tail],
            "the full chain must be in order"
        );
    }

    /// c3 reject case: two charters both naming the same `prior` is a
    /// set-authority fork (`[charter-succession-linear]`) -- the walk
    /// must fail closed via `verify_succession_chain`'s own divergence
    /// check, not silently pick a branch.
    #[test]
    fn resolve_effective_charter_fails_closed_on_divergent_successor() {
        let (_dir, repo) = setup_test_repo();
        let owner0 = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1000,
            single_owner(owner0.tmb.as_bytes().to_vec()), /* self-authorizes both forks for this
                                                           * test */
            None,
            vec![1u8; 20],
            owner0.tmb.clone(),
        )
        .unwrap();
        let (founding_msg, founding_czd) = sign_charter(&founding, &owner0.prv, &owner0.pub_key);
        write_and_ref_charter(&repo, founding_msg, &founding_czd);

        // Both successors are signed by owner0's OWN key and correctly
        // declare owner0's own tmb -- per spec commentary, "nothing can
        // prevent a key from signing two successors naming the same
        // prior"; the fork itself is the thing under test here, not a
        // forged tmb (a distinct attack, covered by
        // `verify_charter_key_thumbprint_rejects_forged_tmb`).
        let successor_a = CharterPayload::new(
            Alg::Ed25519,
            2000,
            single_owner(vec![1u8; 4]),
            Some(founding_czd.clone()),
            vec![1u8; 20],
            owner0.tmb.clone(),
        )
        .unwrap();
        let (msg_a, czd_a) = sign_charter(&successor_a, &owner0.prv, &owner0.pub_key);
        write_and_ref_charter(&repo, msg_a, &czd_a);

        let successor_b = CharterPayload::new(
            Alg::Ed25519,
            2001,
            single_owner(vec![2u8; 4]),
            Some(founding_czd.clone()),
            vec![1u8; 20],
            owner0.tmb.clone(),
        )
        .unwrap();
        let (msg_b, czd_b) = sign_charter(&successor_b, &owner0.prv, &owner0.pub_key);
        write_and_ref_charter(&repo, msg_b, &czd_b);

        let anchor = Anchor::new(founding_czd.as_bytes().to_vec());
        let result = resolve_effective_charter(&repo, &anchor, None);
        assert!(
            matches!(
                result,
                Err(GitError::Verify(atom_id::VerifyError::DivergentSuccessors))
            ),
            "a planted divergent successor must be rejected specifically as DivergentSuccessors \
             (verify_succession_chain's own fail-closed check), not silently resolved to one \
             branch or rejected for an unrelated reason: {result:?}"
        );
    }

    /// Adversarial (tmb-binding, succession -- the anchor-takeover case):
    /// a successor charter validly signed by an ATTACKER's own key, but
    /// declaring `tmb` equal to the founding owner's thumbprint, must be
    /// rejected. Left unchecked, `verify_succession_chain`'s per-link
    /// authorization check (`owner_set_authorizes(&previous.owner,
    /// &successor.tmb)`) would trust the declared `tmb` alone and
    /// authorize the attacker as if they held the legitimate owner's key
    /// -- full re-anchoring authority over the atom-set, not a narrow
    /// resolution bug.
    #[test]
    fn resolve_effective_charter_rejects_successor_with_forged_tmb() {
        let (_dir, repo) = setup_test_repo();
        let owner0 = gen_keypair();
        let attacker = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1000,
            single_owner(owner0.tmb.as_bytes().to_vec()),
            None,
            vec![1u8; 20],
            owner0.tmb.clone(),
        )
        .unwrap();
        let (founding_msg, founding_czd) = sign_charter(&founding, &owner0.prv, &owner0.pub_key);
        write_and_ref_charter(&repo, founding_msg, &founding_czd);

        // Forged successor: signed by the ATTACKER's own key, but the
        // payload declares `tmb` equal to owner0's (the only legitimate
        // charter-set member) -- exactly the forgery
        // `verify_charter_key_thumbprint` exists to catch.
        let forged_successor = CharterPayload::new(
            Alg::Ed25519,
            2000,
            single_owner(attacker.pub_key.clone()), // attacker names themself sole future owner
            Some(founding_czd.clone()),
            vec![1u8; 20],
            owner0.tmb.clone(), // forged: declares owner0's tmb, not the attacker's own
        )
        .unwrap();
        let (msg, czd) = sign_charter(&forged_successor, &attacker.prv, &attacker.pub_key);
        write_and_ref_charter(&repo, msg, &czd);

        let anchor = Anchor::new(founding_czd.as_bytes().to_vec());
        let result = resolve_effective_charter(&repo, &anchor, None);
        assert!(
            matches!(
                result,
                Err(GitError::Verify(atom_id::VerifyError::ThumbprintMismatch))
            ),
            "a successor signed by an attacker's key but declaring the legitimate owner's tmb \
             must be rejected before authorization ever trusts the declared tmb -- an anchor \
             takeover, not a narrow gap, if this is missed: {result:?}"
        );

        // Confirm the takeover genuinely didn't happen: the founding
        // charter, not the forged successor, must remain the only
        // resolvable state for this anchor.
        let founding_only = resolve_founding_charter(&repo, &anchor)
            .expect("the founding charter alone must still resolve cleanly")
            .expect("founding charter must exist");
        assert_eq!(founding_only, founding);
    }

    // -------------------------------------------------------------
    // c4/a2: impl CharterStore for GitSource
    // -------------------------------------------------------------

    #[tokio::test]
    async fn charterstore_get_charter_accepts_through_trait_interface() {
        let (_dir, repo) = setup_test_repo();
        let owner = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(owner.pub_key.clone()),
            None,
            vec![9u8; 20],
            owner.tmb.clone(),
        )
        .unwrap();
        let (msg, czd) = sign_charter(&founding, &owner.prv, &owner.pub_key);
        write_and_ref_charter(&repo, msg, &czd);

        let source = crate::source::GitSource::new(gix::open(repo.path()).unwrap());
        let resolved = atom_id::CharterStore::get_charter(&source, &czd).await;
        assert_eq!(
            resolved,
            Some(founding),
            "get_charter must resolve a real founding charter through the trait interface"
        );
    }

    /// a2 through the trait boundary: the same czd-binding adversarial
    /// case as `resolve_founding_charter_rejects_czd_binding_mismatch`,
    /// but driven through `CharterStore::get_charter` -- the check
    /// applies through this path too, not just the sync internals.
    #[tokio::test]
    async fn charterstore_get_charter_rejects_czd_binding_mismatch() {
        let (_dir, repo) = setup_test_repo();
        let owner = gen_keypair();

        let founding = CharterPayload::new(
            Alg::Ed25519,
            1_700_000_000,
            single_owner(owner.pub_key.clone()),
            None,
            vec![9u8; 20],
            owner.tmb.clone(),
        )
        .unwrap();
        let (msg, real_czd) = sign_charter(&founding, &owner.prv, &owner.pub_key);

        let wrong_czd = Czd::from_bytes(vec![0xEE; real_czd.as_bytes().len()]);
        assert_ne!(real_czd, wrong_czd);
        write_and_ref_charter(&repo, msg, &wrong_czd);

        let source = crate::source::GitSource::new(gix::open(repo.path()).unwrap());
        let resolved = atom_id::CharterStore::get_charter(&source, &wrong_czd).await;
        assert_eq!(
            resolved, None,
            "get_charter must reject a payload whose recomputed czd disagrees with the requested \
             key, through the trait interface too"
        );
    }
}
