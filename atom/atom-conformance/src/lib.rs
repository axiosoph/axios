//! # Atom backend-conformance battery
//!
//! The executable rendering of `docs/specs/atom-backend-contract.md`'s
//! discharge table: one test (or documented test family) per constraint
//! tag, red where Appendix A says GAP/PARTIAL, green where it says
//! Discharged. This crate MEASURES conformance of the git instantiation
//! (`atom-git`'s `GitSource`/`GitStore`/`GitRegistry`); it does not close
//! gaps — that is n1/n2/n3 work and `n4-battery-green`.
//!
//! Honesty over green: a red/ignored test names the Appendix A row it
//! mirrors and the node expected to green it, in its `#[ignore = "..."]`
//! reason string. No GAP/PARTIAL row is asserted true by a weakened
//! check.
//!
//! ## Tag -> test -> verdict -> Appendix A row
//!
//! | Constraint tag | Test module | Verdict | Appendix A status |
//! | :--- | :--- | :--- | :--- |
//! | `backend-store-immutable` | `backend_store_immutable` | RED (ignored) | GAP (implicit) |
//! | `backend-store-injective` | `backend_store_injective` | GREEN | Stated; verification pending |
//! | `backend-ancestry-sound` | `backend_ancestry_sound` | RED (ignored) x2 | GAP + GAP (query path) |
//! | `backend-ancestry-queryable` | `backend_ancestry_queryable` | GREEN | Discharged |
//! | `backend-refs-linearizable` | `backend_refs_linearizable` | SPLIT (green + red) | PARTIAL (present correctness defect) |
//! | `backend-refs-atomic-multi` | `backend_refs_atomic_multi` | SPLIT (green + red) | PARTIAL (guidance, not a tagged constraint) |
//! | `backend-replica-reads` | `backend_replica_reads` | GREEN (out-of-backend-scope marker) | Discharged (protocol layer) |
//! | `backend-seam-typed` | `backend_seam_typed` | RED (ignored) x2 | GAP + GAP (ref encodings) |
//! | `backend-carriage-bit-perfect` | `backend_carriage_bit_perfect` | GREEN | Discharged |
//! | `backend-chain-append` | `backend_chain_append` | SPLIT (green + red) | Discharged (claim/publish) + GAP (charter) |
//! | `backend-enumeration` | `backend_enumeration` | SPLIT (green + red) | Discharged (except charter) |
//! | `backend-refs-sole-mutability` | `backend_refs_sole_mutability` | RED (ignored) | GAP (implicit) |
//! | `backend-liveness-protection` | `backend_liveness_protection` | GREEN | Discharged |
//! | `backend-hash-strength` | `backend_hash_strength` | RED (ignored) | GAP (review-residue, doc obligation) |
//! | `backend-substitutable` | `backend_substitutable` | RED (ignored) | **MISSING from Appendix A — spec defect finding** |
//! | `backend-verification-carried` | `backend_verification_carried` | RED (ignored) | **MISSING from Appendix A — spec defect finding** |
//!
//! The last two rows are a Reserved-predicate hit (IBC `n0-battery` S3):
//! the contract's own Verification table lists both tags with
//! `Method: integration-test`, but Appendix A's discharge map — which its
//! own preamble claims covers "every obligation" — has no row for
//! either. This battery does not resolve that disagreement; it freezes
//! each as an ignored stub naming the finding, for campaign escalation.

/// The full, canonical set of `[backend-*]` constraint tags this contract
/// defines (`docs/specs/atom-backend-contract.md`, Constraints section:
/// Invariants + Behavioral Properties). Hand-maintained rather than
/// parsed from the spec at build time — a deliberate skeleton-stage
/// choice (IBC `n0-battery` Decision Rights: "parse the spec's tags or
/// maintain a checked constant"). Keep in sync with the spec by hand;
/// the bijection meta-test (`tests/battery.rs::meta::tag_test_bijection`)
/// only catches drift between this list and the battery's own
/// `BATTERY_TAGS`, not drift against the markdown source.
pub const CONTRACT_TAGS: &[&str] = &[
    "backend-store-immutable",
    "backend-store-injective",
    "backend-ancestry-sound",
    "backend-ancestry-queryable",
    "backend-refs-linearizable",
    "backend-refs-atomic-multi",
    "backend-replica-reads",
    "backend-seam-typed",
    "backend-carriage-bit-perfect",
    "backend-chain-append",
    "backend-enumeration",
    "backend-refs-sole-mutability",
    "backend-liveness-protection",
    "backend-hash-strength",
    "backend-substitutable",
    "backend-verification-carried",
];
