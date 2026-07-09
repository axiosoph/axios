//! Tracer-bullet e2e harness: a hand-written lock fixture drives
//! [`eos::execution::drive_action`] through a mock [`ExecutionEngine`] and
//! lands a record in [`MemFacts`] — the permanent stub harness for
//! `docs/models/execution-model.md`'s driver loop (IBC N-tracer).
//!
//! ## Why this harness exists
//!
//! `drive_action` (`eos/eos/src/execution.rs`), the `ExecutionEngine` trait
//! (`eos-core/src/executor.rs`), and `FactChannel`/`MemFacts`
//! (`htc-exec/src/facts.rs`) all landed as skeletons before any concrete
//! engine or manifest/lock-driven `Action` type exists. This harness is the
//! permanent, green, end-to-end witness that those three seams compose —
//! independent of, and without inventing, the identity logic later phases
//! owe.
//!
//! ## Why the fixture is bespoke, not `ion_lock::v2`
//!
//! `ion-lock` is **not** a Cargo dependency of `eos` or `eos-core`. Adding
//! one here would be scope creep this IBC's Non-Goals forbid, and it would
//! tempt writing a real `atom_czd_closure_root` computation "to make the
//! fixture realistic" — exactly the invention this harness must not do.
//! [`LockFixtureEntry`] below is a hand-written struct shaped like
//! `ion_lock::v2::DepEntry` + its `(set, label)` nesting key
//! (`ion/ion-lock/src/v2.rs:58-68`); it names the real type in comments so
//! a future migration is traceable, but never constructs it.
//!
//! ## The de-stubbing ladder
//!
//! Every identity value this harness fabricates is listed here, phase-tagged,
//! per the tracer-honesty constraint (c7). None of these stubs are computed
//! by hashing or any other "looks real" logic — each is a raw byte lifted
//! from the fixture, and is called out at its point of use besides.
//!
//! | Stub | Where | Real deliverable | Phase |
//! | :--- | :--- | :--- | :--- |
//! | [`MockEngine::action_id`] | this file | `ActionId = H(atom_czd_closure_root, toolchain_composition_root, params)` (execution-model.md §2.4) | Phase 1 — blocked on `atom_czd_closure_root` (F3 identity seam; **open**, no computation defined anywhere yet — do not invent it here) |
//! | `CompositionRoot` in [`MockEngine::resolve`] | this file | the materialized view's real digest (execution-model.md §2.1) | Phase 1 — same F3 blocker |
//! | `ExecutionRecord::req_digest` in [`MockEngine::execute`] | this file | `ExecutionRequest::req_digest()`, `unimplemented!()` by design (`htc-exec/src/lib.rs`) | P5 — canonical request serialization (execution-model.md §2.1/§8) |
//! | [`LockFixtureEntry`] itself | this file | `ion_lock::v2::DepEntry` + `SetEntry` (`ion/ion-lock/src/v2.rs`) | no phase committed — only happens if/when `eos` takes `ion-lock` as a real dependency, a decision this IBC does not make |
//! | `LockFixtureEntry.publish` | this file | `atom_id::Czd` (Coz-signed publish transaction digest) | tracks the `ion_lock::v2::DepEntry` row above |
//!
//! The `MockEngine::action_id` / `CompositionRoot` stub precedent mirrors
//! the pre-existing `#[cfg(test)] MockEngine` in `eos/eos/src/execution.rs`
//! (which stubs the same values from a bare `u8` action) — this harness
//! extends that precedent to a lock-shaped fixture rather than replacing it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use eos::execution::{Fulfillment, drive_action};
use eos_core::executor::{
    ActionId, Digest, ExecuteReply, ExecutionEngine, ExecutionPlan, ExecutionRecord,
    ExecutionRequest, FactChannel, MemFacts, RecordId, ReqDigest,
};
use htc_exec::{CPath, Command, CompositionRoot, Policy, Signature};

/// Hand-written stand-in for `ion_lock::v2::DepEntry` plus its `(set,
/// label)` nesting key (`ion/ion-lock/src/v2.rs:58-68`) — see the module
/// docs above for why this is bespoke rather than the real type.
#[derive(Clone, Debug)]
struct LockFixtureEntry {
    set: String,
    label: String,
    /// Stand-in for the real `publish: atom_id::Czd` field's bytes — a
    /// raw byte array, not a `Czd`, and never treated as one.
    publish: [u8; 32],
    version: String,
}

fn fixture(seed: u8) -> LockFixtureEntry {
    LockFixtureEntry {
        set: "core".to_string(),
        label: "gcc".to_string(),
        publish: [seed; 32],
        version: "13.3.0".to_string(),
    }
}

fn record_template(exit_code: i32) -> ExecutionRecord {
    ExecutionRecord {
        req_digest: ReqDigest([0; 32]), // overwritten in `MockEngine::execute`
        exit_code,
        outputs: vec![],
        stdout: Digest([0; 32]),
        stderr: Digest([0; 32]),
        observed: None,
        context: None,
        signature: Signature(vec![exit_code as u8]),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("mock engine error")]
struct MockError;

/// Scripted engine over the bespoke lock fixture: `cached` short-circuits
/// `plan`; otherwise the resolved request "runs" and returns `exit_code`.
/// See the module-level de-stubbing ladder for what every stubbed value
/// here stands in for.
struct MockEngine {
    cached: Option<ExecutionRecord>,
    exit_code: i32,
    executions: AtomicUsize,
}

impl ExecutionEngine for MockEngine {
    type Action = LockFixtureEntry;
    type Error = MockError;

    fn action_id(&self, action: &LockFixtureEntry) -> ActionId {
        // STUB (Phase 1, F3 identity seam) — see the ladder above. This is
        // deliberately just the fixture's raw first byte, not a hash of
        // anything: `atom_czd_closure_root` has no defined computation to
        // hash, and inventing one here is exactly what this IBC forbids.
        ActionId([action.publish[0]; 32])
    }

    async fn resolve(&self, action: &LockFixtureEntry) -> Result<ExecutionRequest, MockError> {
        Ok(ExecutionRequest {
            // STUB — same ladder row as `action_id`: fabricated from the
            // fixture byte, not derived from any closure-materialization
            // logic.
            view: CompositionRoot(Digest([action.publish[0]; 32])),
            command: Command {
                argv: vec![
                    "build".into(),
                    format!("{}@{}", action.label, action.version),
                ],
                env: BTreeMap::new(),
                cwd: CPath::parse(&format!("/{}/{}", action.set, action.label)),
            },
            outputs: vec![],
            policy: Policy::default(),
        })
    }

    async fn lookup(&self, _req: ReqDigest) -> Result<Option<ExecutionRecord>, MockError> {
        Ok(self.cached.clone())
    }

    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecuteReply, MockError> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        let mut r = record_template(self.exit_code);
        // STUB (P5) — see the ladder above. `ExecutionRequest::req_digest()`
        // is `unimplemented!()` by design; this mock never calls it and
        // instead stamps `req_digest` directly from the view byte.
        r.req_digest = ReqDigest([request.view.0.0[0]; 32]);
        Ok(ExecuteReply::Scheduled(r))
    }

    async fn plan(&self, action: &LockFixtureEntry) -> Result<ExecutionPlan, MockError> {
        Ok(
            match self.lookup(ReqDigest([action.publish[0]; 32])).await? {
                Some(r) => ExecutionPlan::Cached(r),
                None => ExecutionPlan::NeedsBuild(self.resolve(action).await?),
            },
        )
    }
}

fn record_id(r: &ExecutionRecord) -> RecordId {
    RecordId([r.req_digest.0[0]; 32])
}

#[tokio::test]
async fn needs_build_records_a_fact_from_the_lock_fixture() {
    let action = fixture(7);
    let engine = MockEngine {
        cached: None,
        exit_code: 0,
        executions: AtomicUsize::new(0),
    };
    let mut facts = MemFacts::default();

    let (record, how) = drive_action(&engine, &mut facts, &action, record_id)
        .await
        .expect("mock engine never errors on this path");

    assert_eq!(how, Fulfillment::Executed);
    assert_eq!(record.exit_code, 0);
    assert_eq!(engine.executions.load(Ordering::SeqCst), 1);
    assert_eq!(
        facts.witnesses(ReqDigest([7; 32])).unwrap().len(),
        1,
        "the executed record must land as a fact in the channel"
    );
}

#[tokio::test]
async fn cache_hit_skips_execution_and_appends_nothing() {
    let action = fixture(9);
    let mut cached = record_template(0);
    cached.req_digest = ReqDigest([9; 32]);
    let engine = MockEngine {
        cached: Some(cached),
        exit_code: 0,
        executions: AtomicUsize::new(0),
    };
    let mut facts = MemFacts::default();

    let (record, how) = drive_action(&engine, &mut facts, &action, record_id)
        .await
        .expect("mock engine never errors on this path");

    assert_eq!(how, Fulfillment::CacheHit);
    assert_eq!(record.req_digest, ReqDigest([9; 32]));
    assert_eq!(
        engine.executions.load(Ordering::SeqCst),
        0,
        "a cache hit must execute nothing (execution-model.md §2.4)"
    );
    assert!(
        facts.witnesses(ReqDigest([9; 32])).unwrap().is_empty(),
        "a cache hit appends nothing — the witness it served already existed"
    );
}

#[test]
fn action_id_stub_is_visibly_not_a_closure_identity() {
    // Locks in the stub's actual behavior so a silent upgrade to a
    // hash-based computation cannot happen without this test noticing —
    // see the de-stubbing ladder above (Phase 1, F3).
    let engine = MockEngine {
        cached: None,
        exit_code: 0,
        executions: AtomicUsize::new(0),
    };
    let a = fixture(3);
    let b = LockFixtureEntry {
        set: a.set.clone(),
        label: "clang".to_string(),
        publish: a.publish, // same publish[0] == 3 — the stub's only input
        version: "18.1.0".to_string(),
    };
    // `a` and `b` differ in fields a real `H(atom_czd_closure_root, ...)`
    // would incorporate (label, version) but share the stub's one input
    // (`publish[0]`). A real hash would NOT collide here; the stub does —
    // that's the point: this is a placeholder, not a real identity function,
    // and the test says so.
    assert_eq!(engine.action_id(&a), engine.action_id(&b));
    assert_eq!(engine.action_id(&a), ActionId([3; 32]));
}
