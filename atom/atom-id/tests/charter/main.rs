//! Charter attack corpus (N-charter-corpus).
//!
//! Four named attack scenarios against the landed `CharterPayload` /
//! `ClaimPayload` seam, built as real signed-and-verified transactions:
//!
//! 1. **Divergent succession** (`[charter-succession-linear]`) — a `CharterPayload` chain with two
//!    successors sharing a `prior`.
//! 2. **Prefix rollback** (`[chain-monotonicity]`) — a `CharterPayload` chain plus a regressed
//!    prefix of it.
//! 3. **Claim-replacement marking** (`[claim-replacement-authority]`) — CLAIM-level, not a charter
//!    chain: an owner replacement vs a governance replacement of the same claim.
//! 4. **Bootstrap seizure** (`[charter-transition]` PRE) — a founding charter racing a pre-existing
//!    claim on the same source.
//!
//! `construction.rs` holds the green suite: every transaction verifies
//! individually via `verify_charter`/`verify_claim` (construction
//! correctness only — no chain, succession, ancestry, or authorization
//! validation runs anywhere in this corpus; that is Phase 1, and
//! deliberately unimplemented in the landed seam).
//!
//! ## Red-test inventory
//!
//! This module adds the two red tests the corpus is missing:
//! [`chain_monotonicity::verify_succession_chain_rejects_prefix_rollback`]
//! (attack #2) and
//! [`bootstrap_gate::bootstrap_seizure_requires_incumbent_authorization`]
//! (attack #4). The other two attacks already have a landed red test
//! elsewhere and are NOT duplicated here:
//!
//! | Attack | Spec tag | Red test | Location |
//! |---|---|---|---|
//! | #1 divergent succession | `[charter-succession-linear]` | `verify_succession_chain_rejects_divergent_successors` | `atom/atom-id/src/charter.rs` |
//! | #2 prefix rollback | `[chain-monotonicity]` | `verify_succession_chain_rejects_prefix_rollback` | here, `chain_monotonicity` |
//! | #3 claim-replacement marking | `[claim-replacement-authority]` | `verify_claim_replacement_rejects_third_authority` | `atom/atom-id/src/tests.rs` |
//! | #4 bootstrap seizure | `[charter-transition]` PRE | `bootstrap_seizure_requires_incumbent_authorization` | here, `bootstrap_gate` |

mod bootstrap_gate;
mod chain_monotonicity;
mod construction;
mod fixtures;

/// Dev-only: regenerate `corpus/charter/*.json` from the deterministic
/// builders in `fixtures.rs`. NOT part of the green suite — the
/// committed corpus is the artifact under test; this only exists so a
/// future change to the fixture construction (e.g. a new attack, a key
/// scheme change) can re-derive it without hand-editing signed JSON.
///
/// Run explicitly: `cargo test -p atom-id --test charter -- --ignored \
/// regenerate_charter_corpus_fixtures`.
#[test]
#[ignore = "dev-only: regenerates the committed charter attack corpus"]
fn regenerate_charter_corpus_fixtures() {
    fixtures::save(
        "divergent_succession.json",
        &fixtures::build_divergent_succession(),
    );
    fixtures::save("prefix_rollback.json", &fixtures::build_prefix_rollback());
    fixtures::save(
        "claim_replacement.json",
        &fixtures::build_claim_replacement(),
    );
    fixtures::save(
        "bootstrap_seizure.json",
        &fixtures::build_bootstrap_seizure(),
    );
}
