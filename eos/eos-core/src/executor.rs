//! The ADR-0006 successor to [`crate::engine::BuildEngine`] — skeleton.
//!
//! This module is **not yet consumed**: it compiles beside the legacy
//! trait so the migration (the orchestrator re-cut, removal-inventory
//! step 3) can move call sites one at a time. Once `eos/src/
//! orchestrator.rs` and `eos-daemon` dispatch through this trait,
//! `engine.rs`, `eval.rs`'s evaluator types, and the `eos-snix` crate
//! are deleted (ADR-0006 §3).
//!
//! What changed, and why (docs/models/execution-model.md):
//!
//! - **The engine-specific `Plan` type is gone.** Under the execution model there is exactly one
//!   plan representation — the [`ExecutionRequest`] — because build, test, fetch-discovery, and
//!   closure capture are one operation under different policies. The old `type Plan` existed to
//!   hold a Nix derivation; nothing holds a derivation anymore.
//! - **The plan coproduct has exactly two variants** ([`ExecutionPlan`]): `Cached | NeedsBuild`.
//!   `NeedsEvaluation` names a stage that no longer exists.
//! - **The digest generic is gone.** The substrate speaks the artifact store's digest
//!   ([`htc_exec::Digest`], blake3-shaped). Engines do not get to choose identity.
//! - **Cache lookup is witness selection** (model §2.2): a hit is "∃ a record acceptable under the
//!   consumer's trust anchors", and the pick is a recorded choice. There is no canonical witness
//!   and this trait must never grow API that implies one (no `the_record_for`, no reconcile).

pub use htc_exec::{
    Digest, ExecuteReply, ExecutionRecord, ExecutionRequest, PolicyError, ReqDigest, Stratum,
};
use trait_variant::make;

/// Action-level identity of *intent* (execution model §2.4):
/// `H(atom_czd_closure_root, toolchain_composition_root, params)`.
/// The scheduler/user-facing cache key; distinct from [`ReqDigest`]
/// (the executor-level key) and never conflated with it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ActionId(pub [u8; 32]);

/// The two-variant plan coproduct (ADR-0006 §3). This is the whole
/// state machine input the scheduler needs per node.
#[derive(Clone, Debug)]
pub enum ExecutionPlan {
    /// A trust-acceptable witness exists for the resolved request; its
    /// concrete output digests are what downstream views bind. Which
    /// witness was picked is recorded at request formation (model §2.2)
    /// — this variant carries the pick, not "the" answer.
    Cached(ExecutionRecord),
    /// No acceptable witness; the request must run.
    NeedsBuild(ExecutionRequest),
}

/// The scheduler-facing engine: resolve intent, select witnesses,
/// dispatch execution. Implementations delegate the actual run to an
/// [`htc_exec::Executor`] (or a remote worker speaking the session
/// protocol, model §6.2) — this trait adds the two arrows the substrate
/// executor deliberately does not own: intent→request resolution and
/// trust-filtered witness lookup.
#[make(Send)]
pub trait ExecutionEngine: Send + Sync + 'static {
    /// The intent payload the scheduler holds per node — the successor
    /// of `BuildRequest`'s per-atom slice. Its concrete shape is the
    /// manifest/lock redesign's deliverable (ADR-0006 consequences);
    /// the trait deliberately does not fix it.
    type Action: Clone + Send + Sync + 'static;

    type Error: std::error::Error + Send + Sync + 'static;

    /// Action-level identity of the intent. Two actions with equal ids
    /// share the action-level cache slot (cross-intent dedup is at the
    /// request level, model §2.4).
    fn action_id(&self, action: &Self::Action) -> ActionId;

    /// **P7's arrow**: deterministic resolution of intent into a
    /// concrete request — closure materialization layout, command
    /// assembly, pin/policy assembly, witness picks over the fact
    /// snapshot. Obligations (model §8, P7): (a) this is a *function*
    /// of (action, fact snapshot, choice policy); (b) the
    /// materializer's layout algorithm is versioned inside the
    /// identity; (c) totality is gated on the toolchain-pin lock entry
    /// (open manifest/lock work — until then this may legitimately
    /// error on actions whose toolchain is unpinned).
    async fn resolve(&self, action: &Self::Action) -> Result<ExecutionRequest, Self::Error>;

    /// Witness lookup under the consumer's trust anchors: `Some` iff a
    /// trust-acceptable record exists for this request. Multiple
    /// witnesses may exist; the implementation returns its recorded
    /// pick and MUST NOT attempt reconciliation (model §2.2, §7.3).
    async fn lookup(&self, req: ReqDigest) -> Result<Option<ExecutionRecord>, Self::Error>;

    /// The one dynamic operation, delegated to the substrate executor.
    /// Law: for `request.policy` in the deterministic stratum, a
    /// returned record is a fact; otherwise it is an attestation and
    /// MUST NOT be served from `lookup` as a cache value (model §2.3).
    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecuteReply, Self::Error>;

    /// Plan = resolve, then witness-select. Kept as a required method
    /// (not a default) so implementations can fuse the two round-trips,
    /// but the LAW is fixed: `plan(a)` is `Cached(r)` iff `lookup` on
    /// the resolved request yields `r`, else `NeedsBuild(request)` —
    /// no third outcome, no evaluation.
    async fn plan(&self, action: &Self::Action) -> Result<ExecutionPlan, Self::Error>;
}
