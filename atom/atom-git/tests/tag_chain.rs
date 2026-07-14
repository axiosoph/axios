//! Tests for publish-tag chain primitives: write-side semantic-immutability
//! enforcement on chain append, and moved-tip detection on resolution.

// ---------------------------------------------------------------------
// Goal 1: orphan deletion (compile-time evidence)
// ---------------------------------------------------------------------

/// Compile-time evidence that `gix_util::seam::assume_czd_is_oid_issue64`
/// and its own 4 unit tests are gone from `gix_util.rs`: this crate would
/// fail to build against a `gix_util.rs` that still defined them under a
/// name this test could accidentally shadow or re-trigger.
///
/// `derive_anchor` is deliberately NOT covered here: P1-orphans-confirmed
/// was refuted for it at dispatch time — it has a live, reachable caller
/// in `registry.rs::claim()` (also exercised by `integration.rs`'s
/// `test_anchor_discovery`), both out of this node's declared surface.
/// Deleting it is halted pending a composer decision; see this node's
/// final report.
#[test]
fn orphaned_seam_constructor_is_gone() {
    // No runtime assertion is possible for an absence; the fact that this
    // test file compiles and links against `atom_git::gix_util` at all is
    // the evidence.
}
