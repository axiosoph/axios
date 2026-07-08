//! The two-variant execution driver — successor to the evaluator-shaped
//! state machine in [`crate::orchestrator`] (removal-inventory step 3).
//!
//! Not yet consumed: it compiles beside the legacy orchestrator so the
//! daemon can migrate call sites once a concrete
//! [`ExecutionEngine`] implementation exists. What this module fixes in
//! advance is the *state machine*: the legacy path matches three
//! `BuildPlan` variants (including `NeedsEvaluation`) and drives an
//! evaluator; this one has exactly the two lawful outcomes
//! (ADR-0006 §3) and no third branch to reintroduce one.
//!
//! Laws encoded here (docs/models/execution-model.md):
//! - a cache hit executes nothing (§2.4);
//! - **failure is admissible**: an execution that ran and exited non-zero produced a *record* — it
//!   is appended and returned, never converted into a driver error. Driver errors are reserved for
//!   the machinery (resolution failure, refusal, channel faults) — the distinction a failed *test*
//!   gates advertisement, never rebuilds (§3.2), depends on this;
//! - every produced record is appended to the fact channel before the driver returns — facts exist
//!   the moment the action completes, independent of any gate's later verdict (§3.2). Trial records
//!   are appended too: they accumulate as attestations, and their weaker epistemic status is a
//!   *read-side* judgment derived from the request's policy stratum, not a write-side filter.

use eos_core::executor::{
    ExecuteReply, ExecutionEngine, ExecutionPlan, ExecutionRecord, FactChannel, RecordId,
};

/// How the returned record was obtained — callers gating advertisement
/// or counting reproducibility evidence need the provenance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fulfillment {
    /// Served from an existing trust-acceptable witness; nothing ran.
    CacheHit,
    /// Executed by this driver; the record is a fresh fact/attestation.
    Executed,
}

/// Driver failure — machinery only, never a non-zero exit code.
#[derive(Debug, thiserror::Error)]
pub enum DriveError<E: std::error::Error> {
    #[error("engine error: {0}")]
    Engine(#[from] E),
    #[error("executor refused the request: {0}")]
    Refused(String),
    #[error("fact channel append failed: {0}")]
    Facts(String),
}

/// Drive one action to a record: plan → (cache hit | execute) → append.
///
/// `record_id` derives the record's content identity for idempotent
/// append; in production it is the digest of the signed record object
/// (the store dedupes re-appends of the same witness by it).
pub async fn drive_action<E, F>(
    engine: &E,
    facts: &mut F,
    action: &E::Action,
    record_id: impl Fn(&ExecutionRecord) -> RecordId,
) -> Result<(ExecutionRecord, Fulfillment), DriveError<E::Error>>
where
    E: ExecutionEngine,
    F: FactChannel,
    F::Error: std::fmt::Display,
{
    match engine.plan(action).await? {
        ExecutionPlan::Cached(record) => Ok((record, Fulfillment::CacheHit)),
        ExecutionPlan::NeedsBuild(request) => {
            let record = match engine.execute(&request).await? {
                ExecuteReply::Known(r) | ExecuteReply::Scheduled(r) => r,
                ExecuteReply::Refused(e) => return Err(DriveError::Refused(e.0)),
            };
            facts
                .append(record_id(&record), record.clone())
                .map_err(|e| DriveError::Facts(e.to_string()))?;
            Ok((record, Fulfillment::Executed))
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use eos_core::executor::{ActionId, Digest, ExecutionRequest, MemFacts, ReqDigest};
    use htc_exec::{Command, CompositionRoot, Policy, Signature};

    use super::*;

    fn request(seed: u8) -> ExecutionRequest {
        ExecutionRequest {
            view: CompositionRoot(Digest([seed; 32])),
            command: Command {
                argv: vec!["make".into()],
                env: BTreeMap::new(),
                cwd: htc_exec::CPath::parse("/build"),
            },
            outputs: vec![],
            policy: Policy::default(),
        }
    }

    fn record(seed: u8, exit_code: i32) -> ExecutionRecord {
        ExecutionRecord {
            req_digest: ReqDigest([seed; 32]),
            exit_code,
            outputs: vec![Digest([seed; 32])],
            stdout: Digest([0; 32]),
            stderr: Digest([0; 32]),
            observed: None,
            context: None,
            signature: Signature(vec![seed]),
        }
    }

    /// Scripted engine: `cached` short-circuits plan; otherwise the
    /// request runs and returns `exit_code`.
    struct MockEngine {
        cached: Option<ExecutionRecord>,
        exit_code: i32,
        refuse: bool,
        executions: AtomicUsize,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("mock")]
    struct MockError;

    impl ExecutionEngine for MockEngine {
        type Action = u8;
        type Error = MockError;

        fn action_id(&self, action: &u8) -> ActionId {
            ActionId([*action; 32])
        }

        async fn resolve(&self, action: &u8) -> Result<ExecutionRequest, MockError> {
            Ok(request(*action))
        }

        async fn lookup(&self, _req: ReqDigest) -> Result<Option<ExecutionRecord>, MockError> {
            Ok(self.cached.clone())
        }

        async fn execute(&self, request: &ExecutionRequest) -> Result<ExecuteReply, MockError> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            if self.refuse {
                return Ok(ExecuteReply::Refused(htc_exec::PolicyError(
                    "no netns".into(),
                )));
            }
            let mut r = record(0, self.exit_code);
            r.req_digest = ReqDigest([request.view.0.0[0]; 32]);
            Ok(ExecuteReply::Scheduled(r))
        }

        async fn plan(&self, action: &u8) -> Result<ExecutionPlan, MockError> {
            Ok(match self.lookup(ReqDigest([*action; 32])).await? {
                Some(r) => ExecutionPlan::Cached(r),
                None => ExecutionPlan::NeedsBuild(self.resolve(action).await?),
            })
        }
    }

    fn engine(cached: Option<ExecutionRecord>, exit_code: i32, refuse: bool) -> MockEngine {
        MockEngine {
            cached,
            exit_code,
            refuse,
            executions: AtomicUsize::new(0),
        }
    }

    fn rid(r: &ExecutionRecord) -> RecordId {
        RecordId([r.req_digest.0[0]; 32])
    }

    #[tokio::test]
    async fn cache_hit_executes_nothing_and_appends_nothing() {
        let e = engine(Some(record(7, 0)), 0, false);
        let mut facts = MemFacts::default();
        let (r, how) = drive_action(&e, &mut facts, &7, rid).await.unwrap();
        assert_eq!(how, Fulfillment::CacheHit);
        assert_eq!(r.exit_code, 0);
        assert_eq!(e.executions.load(Ordering::SeqCst), 0);
        assert!(facts.witnesses(ReqDigest([7; 32])).unwrap().is_empty());
    }

    #[tokio::test]
    async fn needs_build_executes_once_and_appends_the_record() {
        let e = engine(None, 0, false);
        let mut facts = MemFacts::default();
        let (_, how) = drive_action(&e, &mut facts, &7, rid).await.unwrap();
        assert_eq!(how, Fulfillment::Executed);
        assert_eq!(e.executions.load(Ordering::SeqCst), 1);
        assert_eq!(facts.witnesses(ReqDigest([7; 32])).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failure_is_admissible_a_nonzero_exit_is_a_record_not_an_error() {
        let e = engine(None, 2, false);
        let mut facts = MemFacts::default();
        let (r, how) = drive_action(&e, &mut facts, &7, rid).await.unwrap();
        assert_eq!(how, Fulfillment::Executed);
        assert_eq!(r.exit_code, 2);
        // the failed run is a fact in the channel, same as a success
        assert_eq!(facts.witnesses(ReqDigest([7; 32])).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refusal_is_a_driver_error() {
        let e = engine(None, 0, true);
        let mut facts = MemFacts::default();
        let err = drive_action(&e, &mut facts, &7, rid).await.unwrap_err();
        assert!(matches!(err, DriveError::Refused(_)));
        assert!(facts.witnesses(ReqDigest([7; 32])).unwrap().is_empty());
    }
}
