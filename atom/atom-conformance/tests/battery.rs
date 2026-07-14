//! The backend-conformance battery: one test module per
//! `docs/specs/atom-backend-contract.md` constraint tag. See
//! `atom-conformance`'s crate-level docs (`src/lib.rs`) for the full
//! tag -> test -> verdict -> Appendix A mapping table.
//!
//! Every `#[ignore]` reason string names the Appendix A row it mirrors
//! and the node expected to green it (c2-honest-split). No GAP/PARTIAL
//! row is asserted true by a weakened check (Non-Goals).

mod common;

use atom_core::{AtomContent, AtomRegistry, ContentEntry};
use atom_git::{GitSource, GitStore};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

// ---------------------------------------------------------------------
// backend-store-immutable — Appendix A: GAP (implicit)
// ---------------------------------------------------------------------
mod backend_store_immutable {
    #[test]
    #[ignore = "GAP (implicit): backend-store-immutable is relied on throughout \
                git-storage-format.md ([tag-chain-immutable] and every 'the ref moved but the old \
                object persists' claim) but is never stated as an obligation anywhere. The battery \
                measures conformance to the CONTRACT, not incidental git behavior — asserting this \
                now would silently promote an unstated property to spec authority. Node: \
                doc-amendment sweep (Appendix B, unscheduled). Greened when the obligation is \
                stated and this stub is promoted to a real get(put(x))=x + mutation-rejection \
                assertion."]
    fn store_immutable_gap_undischarged() {
        unreachable!("ignored: see #[ignore] reason — GAP, no stated discharge point yet");
    }
}

// ---------------------------------------------------------------------
// backend-store-injective — Appendix A: Stated; verification pending
// ---------------------------------------------------------------------
mod backend_store_injective {
    use super::*;
    use crate::common::setup_repo;

    /// P16: canonical serialization is order-independent — two content
    /// trees built from the same entries in different insertion order
    /// MUST yield the same OID. Exercises `GitStore::write_content_tree`'s
    /// `root_entries.sort()` (store.rs), the concrete discharge point
    /// Appendix A names ("Tree construction", canonical byte-order
    /// sorting).
    #[test]
    fn canonical_tree_construction_is_order_independent() {
        let (_dir, repo, _genesis) = setup_repo();
        let store = GitStore::new(repo.clone());

        let forward = vec![
            ContentEntry::Regular {
                path: "a.txt".into(),
                data: b"a".to_vec(),
                executable: false,
            },
            ContentEntry::Regular {
                path: "b.txt".into(),
                data: b"b".to_vec(),
                executable: false,
            },
        ];
        let reversed = vec![forward[1].clone(), forward[0].clone()];

        let oid_forward = store
            .write_content_tree(&repo, &forward)
            .expect("write forward tree");
        let oid_reversed = store
            .write_content_tree(&repo, &reversed)
            .expect("write reversed tree");

        assert_eq!(
            oid_forward, oid_reversed,
            "canonical tree construction must be independent of entry insertion order"
        );
    }
}

// ---------------------------------------------------------------------
// backend-ancestry-sound — Appendix A: GAP + GAP (query path)
// ---------------------------------------------------------------------
mod backend_ancestry_sound {
    #[test]
    #[ignore = "GAP: backend-ancestry-sound (P15) is used by \
                [temporal-vector]/[no-backdated-src]/[charter-ancestry] but never stated as an \
                obligation. atom-git's ancestry primitive (gix_util::is_descendant) does walk \
                hash-committed commit.parent_ids() — the right shape — but no spec obligation or \
                machine-checked proof binds that fact yet. Node: n3-alloy-seam (P15's abstract \
                row) + a doc-amendment sweep item stating the git argument (parent OIDs are in the \
                hashed preimage)."]
    fn ancestry_sound_law_unstated() {
        unreachable!("ignored: see #[ignore] reason — GAP, founding finding of the contract");
    }

    #[test]
    #[ignore = "GAP (query path): Appendix A names no anti-replacement guidance — git ships \
                refs/replace/* and grafts that silently divert operational parent queries from the \
                hash-committed set without changing any OID, and atom-git's ancestry walk \
                (gix_util::is_descendant) does not configure --no-replace-objects semantics or \
                otherwise disable replacement resolution. Node: n3-alloy-seam (P15 model) + \
                doc-amendment sweep item ('protocol ancestry paths MUST resolve with \
                --no-replace-objects semantics')."]
    fn ancestry_query_path_replacement_not_disabled() {
        unreachable!("ignored: see #[ignore] reason — GAP, anti-replacement obligation absent");
    }
}

// ---------------------------------------------------------------------
// backend-ancestry-queryable — Appendix A: Discharged
// ---------------------------------------------------------------------
mod backend_ancestry_queryable {
    use crate::common::{commit_child, setup_repo};

    /// Exercises the local discharge point: `is_descendant` answers `⊑`
    /// purely from the commit-parent graph (`commit.parent_ids()`),
    /// never touching tree or blob objects — the local analogue of
    /// "revision metadata alone suffices". The networked treeless
    /// (`tree:0`) fetch-advertisement half of this obligation is a
    /// transport concern outside atom-git's local-repository surface
    /// and is not exercised here.
    #[test]
    fn ancestry_check_uses_only_commit_graph() {
        let (_dir, repo, genesis) = setup_repo();
        let c1 = commit_child(&repo, genesis, "c1");
        let c2 = commit_child(&repo, c1, "c2");

        assert!(atom_git::gix_util::is_descendant(&repo, c2, genesis).unwrap());
        assert!(atom_git::gix_util::is_descendant(&repo, c2, c2).unwrap());
        assert!(!atom_git::gix_util::is_descendant(&repo, genesis, c2).unwrap());
    }
}

// ---------------------------------------------------------------------
// backend-refs-linearizable — Appendix A: PARTIAL (present correctness defect)
// ---------------------------------------------------------------------
mod backend_refs_linearizable {
    use super::*;
    use crate::common::{chartered_atom_id, new_registry, registry_owner, setup_repo};

    /// The mechanism half: gix's per-ref transaction genuinely rejects a
    /// write whose `expected` no longer matches the ref's live value —
    /// real linearizable CAS at the git-transaction layer, the
    /// substrate `[publish-transition-git]`'s CAS check is built on.
    #[test]
    fn ref_cas_rejects_stale_expected() {
        let (_dir, repo, genesis) = setup_repo();
        let registry = new_registry(repo.clone(), "cargo");
        let id = chartered_atom_id(&registry, "pkg-a");

        let _ = registry
            .claim(&id, &registry_owner(&registry))
            .expect("initial claim");

        let claim_ref_name = "refs/atom/claims/pub/pkg-a";
        let live_oid = repo
            .try_find_reference(claim_ref_name)
            .unwrap()
            .unwrap()
            .id()
            .detach();
        let wrong_oid = genesis; // definitely not the live claim commit oid

        let edit = RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: "conformance: attempted stale CAS".into(),
                },
                expected: PreviousValue::MustExistAndMatch(Target::Object(wrong_oid)),
                new: Target::Object(live_oid),
            },
            name: FullName::try_from(claim_ref_name).unwrap(),
            deref: false,
        };

        let result = repo.edit_references(vec![edit]);
        assert!(
            result.is_err(),
            "a ref-transaction CAS with a stale `expected` value must be rejected"
        );
    }

    #[test]
    #[ignore = "PARTIAL — present correctness defect (Appendix A): the claim-then-publish flow \
                reads the active claim czd at publish() step 1 and only re-validates it by \
                application- level comparison later in the function, not by re- asserting a \
                git-level CAS against the originally-read claim ref state at the final \
                edit_references() write. A claim rotation landing in that window is not provably \
                rejected at the git-transaction layer. c4-deterministic forbids fabricating a \
                non-deterministic concurrency race to force this red mechanically; this stub \
                documents the gap Appendix A already names rather than simulating a flaky \
                interleaving. Node: unscheduled n2 remediation (Appendix B priority amendment: \
                raise the publish-time CAS from SHOULD to MUST)."]
    fn claim_rotation_toctou_window_at_publish() {
        unreachable!("ignored: see #[ignore] reason — PARTIAL, TOCTOU window named by Appendix A");
    }
}

// ---------------------------------------------------------------------
// backend-refs-atomic-multi — Appendix A: PARTIAL (guidance, not tagged)
// ---------------------------------------------------------------------
mod backend_refs_atomic_multi {
    use atom_core::AtomRegistry;

    use crate::common::{chartered_atom_id, new_registry, registry_owner, setup_repo};

    /// `claim()` writes the claim ref and its protective src ref through
    /// a single `gix::Repository::edit_references` batch
    /// (registry.rs). Exercises the all-or-nothing outcome on the
    /// success path: after one `claim()` call, both names exist.
    #[test]
    fn claim_writes_land_both_refs_together() {
        let (_dir, repo, genesis) = setup_repo();
        let registry = new_registry(repo.clone(), "cargo");
        let id = chartered_atom_id(&registry, "pkg-b");

        let _ = registry
            .claim(&id, &registry_owner(&registry))
            .expect("claim");

        let claim_ref = repo
            .try_find_reference("refs/atom/claims/pub/pkg-b")
            .unwrap();
        let src_ref_name = format!("refs/atom/src/{}", genesis.to_hex());
        let src_ref = repo.try_find_reference(src_ref_name.as_str()).unwrap();

        assert!(
            claim_ref.is_some(),
            "claim ref must exist after a successful claim"
        );
        assert!(
            src_ref.is_some(),
            "protective src ref must exist after a successful claim"
        );
    }

    #[test]
    #[ignore = "PARTIAL: Appendix A marks this row 'guidance, not a tagged constraint' — \
                git-storage-format.md's Implementation Guidance describes gix::refs::Transaction + \
                atomic push, but the per-store atomic-multi-ref law is not yet a tagged MUST \
                constraint in that spec, only promoted to law in atom-backend-contract.md itself. \
                This is a spec-text gap, not a runtime defect — atom-git's edit_references() usage \
                is real (see the green sibling test). Node: doc-amendment sweep (Appendix B: \
                'promote the Atomicity implementation guidance to a tagged constraint')."]
    fn atomic_multi_ref_not_yet_a_tagged_constraint() {
        unreachable!("ignored: see #[ignore] reason — PARTIAL, doc-only gap");
    }
}

// ---------------------------------------------------------------------
// backend-replica-reads — Appendix A: Discharged (protocol layer)
// ---------------------------------------------------------------------
mod backend_replica_reads {
    /// Non-Goal: "Do NOT test protocol-layer semantics the contract
    /// assigns elsewhere." Appendix A discharges this obligation at the
    /// protocol layer (`[mirror-staleness-tolerance]`,
    /// `[atom-version-identity]`, `[czd-divergence-handling]` in
    /// atom-sourcing; `[chain-monotonicity]` in atom-transactions), not
    /// at the git backend surface this crate exercises. This stub is a
    /// deliberate marker, not a fake pass: it proves nothing about
    /// atom-git and claims nothing more than "out of backend-battery
    /// scope, see the protocol-layer conformance tests."
    #[test]
    fn discharged_at_protocol_layer_out_of_backend_scope() {
        // Intentionally backend-inert. See doc comment above.
    }
}

// ---------------------------------------------------------------------
// backend-seam-typed — Appendix A: GAP + GAP (ref encodings)
// ---------------------------------------------------------------------
mod backend_seam_typed {
    #[test]
    #[ignore = "GAP: no general seam law exists at the type level yet — OID and Czd surfaces are \
                mixed as raw &[u8]/ObjectId/Czd at call sites (registry.rs, source.rs) with no \
                disjoint newtype boundary preventing cross-sort comparison or construction outside \
                the named payload/carrier surfaces. Node: n0-seam-types (typed boundary + retrofit \
                of the four existing conversion sites)."]
    fn seam_law_no_typed_boundary() {
        unreachable!("ignored: see #[ignore] reason — GAP, the landed bug class's root cause");
    }

    #[test]
    #[ignore = "GAP (ref encodings): ref paths are built by raw format!() string interpolation \
                (e.g. 'refs/atom/claims/pub/{label}', 'refs/atom/src/{oid_hex}') with no typed \
                parser that rehydrates a segment to its family's sort before use — 'RefPath = \
                String' is exactly the insufficient shape Appendix A names. Node: n0-seam-types \
                (or its ref-path-typing follow-on)."]
    fn ref_path_segments_not_typed_encodings() {
        unreachable!("ignored: see #[ignore] reason — GAP, typed-encoding statement absent");
    }
}

// ---------------------------------------------------------------------
// backend-carriage-bit-perfect — Appendix A: Discharged
// ---------------------------------------------------------------------
mod backend_carriage_bit_perfect {
    use super::*;
    use crate::common::{atom_id, setup_repo};

    /// store -> retrieve -> byte-compare, extending `[coz-bit-perfect]`
    /// to the generic content-tree carriage path
    /// (`GitStore::write_content_tree` + `GitSource::content`).
    #[tokio::test]
    async fn content_round_trips_byte_exact() {
        let (_dir, repo, genesis) = setup_repo();
        let store = GitStore::new(repo.clone());
        let source = GitSource::new(repo.clone());
        let id = atom_id(genesis, "pkg-c");

        let original = vec![ContentEntry::Regular {
            path: "file.txt".into(),
            data: b"hello world, byte for byte".to_vec(),
            executable: false,
        }];

        let tree_oid = store
            .write_content_tree(&repo, &original)
            .expect("write content tree");

        let retrieved = source
            .content(&id, tree_oid.as_bytes())
            .await
            .expect("content retrieval")
            .expect("content present");

        assert_eq!(retrieved.len(), 1);
        match &retrieved[0] {
            ContentEntry::Regular {
                path,
                data,
                executable,
            } => {
                assert_eq!(path, "file.txt");
                assert_eq!(data, b"hello world, byte for byte");
                assert!(!executable);
            },
            other => panic!("expected a Regular entry, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------
// backend-chain-append — Appendix A: Discharged (claim/publish) + GAP (charter)
// ---------------------------------------------------------------------
mod backend_chain_append {
    use atom_core::AtomRegistry;

    use crate::common::{chartered_atom_id, new_registry, registry_owner, setup_repo};

    /// Claim rotation is append-only: the prior claim commit remains a
    /// retrievable git object after a successor claim lands.
    #[test]
    fn claim_rotation_preserves_prior_chain_object() {
        let (_dir, repo, _genesis) = setup_repo();
        let registry = new_registry(repo.clone(), "cargo");
        let id = chartered_atom_id(&registry, "pkg-d");

        let _ = registry
            .claim(&id, &registry_owner(&registry))
            .expect("first claim");
        let first_claim_oid = repo
            .try_find_reference("refs/atom/claims/pub/pkg-d")
            .unwrap()
            .unwrap()
            .id()
            .detach();

        // Second claim with the same signing identity — a legitimate
        // rotation/update against the same active claim chain.
        let _ = registry
            .claim(&id, &registry_owner(&registry))
            .expect("second claim");

        assert!(
            repo.find_object(first_claim_oid).is_ok(),
            "prior claim commit must remain retrievable after chain append"
        );
    }

    #[test]
    #[ignore = "GAP: charter chain-append is untestable at skeleton stage — no charter git \
                object/ref encoding exists in atom-git yet (Open Questions #6, \
                git-storage-format.md). Node: n1-charter-encoding."]
    fn charter_chain_append_untestable() {
        unreachable!("ignored: see #[ignore] reason — GAP, charter encoding not yet defined");
    }
}

// ---------------------------------------------------------------------
// backend-enumeration — Appendix A: Discharged except charter
// ---------------------------------------------------------------------
mod backend_enumeration {
    use atom_core::{AtomRegistry, AtomSource, RawVersion};

    use crate::common::{chartered_atom_id, new_registry, registry_owner, setup_repo};

    /// A claimed+published atom is discoverable and resolvable purely
    /// from ref/commit-message scans (`GitSource::discover`/`resolve`),
    /// with no tree/blob content download — object-free enumeration.
    #[tokio::test]
    async fn labels_and_versions_enumerable_without_content() {
        let (_dir, repo, genesis) = setup_repo();
        let registry = new_registry(repo.clone(), "cargo");
        let id = chartered_atom_id(&registry, "pkg-e");

        let claim_czd = registry
            .claim(&id, &registry_owner(&registry))
            .expect("claim");
        let empty_tree = repo
            .write_object(gix::objs::Tree {
                entries: Vec::new(),
            })
            .expect("write empty tree")
            .detach();

        registry
            .publish(
                &id,
                &claim_czd,
                &RawVersion::new("1.0.0".to_string()),
                empty_tree.as_bytes(),
                genesis.as_bytes(),
                "",
            )
            .expect("publish");

        let discovered = registry.discover("pkg-e").await.expect("discover");
        assert!(
            discovered.contains(&id),
            "discover must surface the claimed/published atom"
        );

        let resolved = registry
            .resolve(&id)
            .await
            .expect("resolve")
            .expect("entry present");
        let versions: Vec<_> = atom_core::AtomEntry::versions(&resolved).collect();
        assert!(
            versions
                .iter()
                .any(|v| atom_core::AtomVersion::version(*v).as_str() == "1.0.0"),
            "resolve must surface the published version"
        );
    }

    #[test]
    #[ignore = "GAP: charter enumeration is untestable at skeleton stage — no charter ref layout \
                exists to enumerate (Open Questions #6, git-storage-format.md). Node: \
                n1-charter-encoding."]
    fn charter_enumeration_untestable() {
        unreachable!("ignored: see #[ignore] reason — GAP, charter encoding not yet defined");
    }
}

// ---------------------------------------------------------------------
// backend-refs-sole-mutability — Appendix A: GAP (implicit)
// ---------------------------------------------------------------------
mod backend_refs_sole_mutability {
    #[test]
    #[ignore = "GAP (implicit): true of atom-git's design (git object DB immutability + all \
                protocol mutation flowing through refs/atom/*) but stated as an obligation \
                nowhere. Same reasoning as backend-store-immutable: asserting it now would promote \
                an unstated property to spec authority. Node: doc-amendment sweep (Appendix B, \
                unscheduled)."]
    fn refs_sole_mutability_gap_undischarged() {
        unreachable!("ignored: see #[ignore] reason — GAP, implicit in the design, unstated");
    }
}

// ---------------------------------------------------------------------
// backend-liveness-protection — Appendix A: Discharged
// ---------------------------------------------------------------------
mod backend_liveness_protection {
    use atom_core::AtomRegistry;

    use crate::common::{chartered_atom_id, new_registry, registry_owner, setup_repo};

    /// After `claim()`, the protective `refs/atom/src/{oid}` ref exists
    /// and its target (the claim's src revision) is retrievable — the
    /// mechanism `[store-claim-ref]` relies on to keep GC from
    /// collecting protocol-reachable state.
    #[test]
    fn protective_src_ref_exists_and_target_retrievable() {
        let (_dir, repo, genesis) = setup_repo();
        let registry = new_registry(repo.clone(), "cargo");
        let id = chartered_atom_id(&registry, "pkg-f");

        let _ = registry
            .claim(&id, &registry_owner(&registry))
            .expect("claim");

        let src_ref_name = format!("refs/atom/src/{}", genesis.to_hex());
        let src_ref = repo
            .try_find_reference(src_ref_name.as_str())
            .unwrap()
            .expect("protective src ref must exist");

        assert!(repo.find_object(src_ref.id().detach()).is_ok());
    }
}

// ---------------------------------------------------------------------
// backend-hash-strength — Appendix A: GAP (review-residue, doc obligation)
// ---------------------------------------------------------------------
mod backend_hash_strength {
    #[test]
    #[ignore = "Untestable at skeleton stage: the Verification table grades this obligation \
                'review-residue', not integration-test — it is a documentation obligation ('a \
                conforming backend MUST document its object hash and its collision-resistance \
                status') plus a SHOULD-grade re-anchor hardening recommendation, neither of which \
                has a runtime assertion surface in atom-git today. Node: n1-hash-decision \
                (decision brief) -> a future, as-yet-undrafted n2-hash-amendment (gated on a human \
                decision per that node's own IBC)."]
    fn hash_strength_is_review_residue_not_runtime() {
        unreachable!(
            "ignored: see #[ignore] reason — GAP, doc obligation, not integration-testable"
        );
    }
}

// ---------------------------------------------------------------------
// backend-substitutable — PARTIAL, inherently cross-backend
// ---------------------------------------------------------------------
mod backend_substitutable {
    #[test]
    #[ignore = "PARTIAL (atom-backend-contract.md Appendix A, backend-substitutable row): the \
                property is inherently cross-backend (interchangeability WITH another backend); \
                git's own determinism (snapshot-deterministic + canonical serialization) \
                discharges its git-side half, but there is no second conforming backend to compare \
                against yet, so a golden-trace conformance test cannot run. This row was MISSING \
                from Appendix A when this crate first landed (a genuine spec defect this Reserved \
                predicate correctly froze rather than resolved) — the composer added it directly \
                afterward (PR #77). The ignore verdict is unchanged by that fix; only the reason \
                has been corrected to match the row that now exists."]
    fn substitutable_missing_from_appendix_a() {
        unreachable!(
            "ignored: see #[ignore] reason — PARTIAL, no second backend to compare against"
        );
    }
}

// ---------------------------------------------------------------------
// backend-verification-carried — Discharged (implicit)
// ---------------------------------------------------------------------
mod backend_verification_carried {
    #[test]
    #[ignore = "Discharged (implicit) (atom-backend-contract.md Appendix A, \
                backend-verification-carried row): true of git's local object model by \
                construction — once fetched, every object reads from the local database with zero \
                network round-trips — but no git-storage-format.md constraint states this as a \
                formal obligation, so there is nothing to test against as an integration check. \
                This row was MISSING from Appendix A when this crate first landed (the same spec \
                defect as backend-substitutable, correctly frozen rather than resolved) — the \
                composer added it directly afterward (PR #77). The ignore verdict is unchanged by \
                that fix; only the reason has been corrected to match the row that now exists."]
    fn verification_carried_missing_from_appendix_a() {
        unreachable!(
            "ignored: see #[ignore] reason — Discharged (implicit), nothing to test against"
        );
    }
}

// ---------------------------------------------------------------------
// c1: tag <-> test bijection meta-test
// ---------------------------------------------------------------------
mod meta {
    /// The battery's own tag inventory, one entry per test module above.
    /// Hand-maintained alongside `atom_conformance::CONTRACT_TAGS`
    /// (IBC Decision Rights: "parse the spec's tags or maintain a
    /// checked constant"). This mechanically catches drift between the
    /// two lists; it does not (Rust has no test-level reflection) prove
    /// that a listed tag's test module still exists or still compiles —
    /// that's enforced by the crate simply failing to build if a module
    /// is renamed without updating its `mod` declaration above.
    const BATTERY_TAGS: &[&str] = &[
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

    #[test]
    fn tag_test_bijection() {
        let contract: std::collections::BTreeSet<&str> =
            atom_conformance::CONTRACT_TAGS.iter().copied().collect();
        let battery: std::collections::BTreeSet<&str> = BATTERY_TAGS.iter().copied().collect();

        let missing_from_battery: Vec<_> = contract.difference(&battery).collect();
        let missing_from_contract: Vec<_> = battery.difference(&contract).collect();

        assert!(
            missing_from_battery.is_empty(),
            "contract tags with no battery test module: {missing_from_battery:?}"
        );
        assert!(
            missing_from_contract.is_empty(),
            "battery test modules for tags absent from CONTRACT_TAGS: {missing_from_contract:?}"
        );
        assert_eq!(
            contract.len(),
            atom_conformance::CONTRACT_TAGS.len(),
            "CONTRACT_TAGS must not contain duplicate tags"
        );
        assert_eq!(
            battery.len(),
            BATTERY_TAGS.len(),
            "BATTERY_TAGS must not contain duplicate tags"
        );
    }
}
