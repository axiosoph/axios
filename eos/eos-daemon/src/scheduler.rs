//! Eos daemon build scheduler.
//!
//! Orchestrates job execution, deduplicates concurrent build submissions,
//! and enforces local concurrency limits.
//!
//! # Future Work
//!
//! The following spec invariants require a multi-worker architecture that
//! is not yet implemented. They are deferred until the daemon supports
//! remote worker registration via the RPC layer:
//!
//! - `[eos-scheduler-lease-expiry]`: Job leases granted to workers must expire after a configurable
//!   duration, triggering re-queuing.
//! - `[eos-scheduler-heartbeat-liveness]`: Workers must send periodic heartbeats; missed deadlines
//!   mark the worker unhealthy and revoke its active leases.
//! - `[eos-scheduler-input-affinity]`: Job placement should prefer workers whose local caches
//!   already hold the job's input closure.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use eos_core::digest::Blake3Digest;
use eos_core::job::{JobId, JobStatus, ProgressEvent};
use eos_snix::SnixEngine;
use tokio::sync::{Semaphore, broadcast};
use tracing::{error, info};

use crate::config::DaemonConfig;

/// The bounds the daemon requires of an injected atom source.
///
/// Beyond content observation ([`atom_core::AtomContent`]) and cloneability,
/// the source's entry type must be `Send`: build jobs are spawned onto the
/// multi-threaded Tokio runtime, so the orchestrator future — which holds a
/// resolved entry across `await` points — must itself be `Send`. This blanket
/// trait bundles those bounds so callers name a single contract.
pub trait InjectedSource:
    atom_core::AtomContent + atom_core::AtomSource<Entry: Send> + Clone
{
}

impl<T> InjectedSource for T where
    T: atom_core::AtomContent + atom_core::AtomSource<Entry: Send> + Clone
{
}

/// State for a single scheduled or running job.
#[derive(Clone)]
pub struct JobState {
    /// Opaque job identifier.
    pub id: JobId<Blake3Digest>,
    /// Current status of the job.
    pub status: JobStatus<Blake3Digest>,
    /// Broadcast sender for progress events.
    pub sender: broadcast::Sender<ProgressEvent<Blake3Digest>>,
    /// Task abort handle to cancel execution.
    pub abort_handle: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
}

/// The build job scheduler.
///
/// Generic over the injected atom source `S`. The source is bound by
/// [`atom_core::AtomContent`] — the same content/observation contract the
/// orchestrator and castore bridge are generic over — and is constructed once
/// in the daemon's composition layer (`main.rs`), never per job.
pub struct Scheduler<S> {
    config: Arc<DaemonConfig>,
    engine: Arc<SnixEngine>,
    index: Arc<eos::index::RequestIndex>,
    jobs: Arc<Mutex<HashMap<Blake3Digest, JobState>>>,
    semaphore: Arc<Semaphore>,
    /// Injected atom source, shared by every spawned build task.
    source: S,
}

impl<S: InjectedSource> Scheduler<S> {
    /// Creates a new `Scheduler` over an injected atom `source`.
    #[must_use]
    pub fn new(
        config: Arc<DaemonConfig>,
        engine: Arc<SnixEngine>,
        index: Arc<eos::index::RequestIndex>,
        source: S,
    ) -> Self {
        let max_concurrency = config.max_concurrency;

        Self {
            config,
            engine,
            index,
            jobs: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            source,
        }
    }

    /// Submits a BuildRequest to the scheduler.
    ///
    /// If the job is already active, returns the existing handle.
    /// Otherwise, schedules a new background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the scheduler lock is poisoned.
    pub fn submit(
        &self,
        request: eos_core::request::BuildRequest<Blake3Digest>,
    ) -> Result<JobState, String> {
        let plan_digest = request.plan_digest;
        let mut guard = self.jobs.lock().map_err(|e| e.to_string())?;

        // @spec-compliance[eos-scheduler-deduplication]
        // Mechanism: Prevents redundant scheduling by tracking active job handles in an in-memory
        // hash map and returning the existing handle. Verified-By:
        // eos/eos-daemon/src/scheduler.rs:submit
        if let Some(existing) = guard.get(&plan_digest) {
            info!("Deduplicating submission for job: {}", plan_digest);
            return Ok(existing.clone());
        }

        info!("Scheduling new job: {}", plan_digest);
        let (tx, _rx) = broadcast::channel(100);

        let job_state = JobState {
            id: JobId(plan_digest),
            status: JobStatus::Queued,
            sender: tx.clone(),
            abort_handle: Arc::new(Mutex::new(None)),
        };

        guard.insert(plan_digest, job_state.clone());

        // Spawn build task in background
        let engine = self.engine.clone();
        let config = self.config.clone();
        let semaphore = self.semaphore.clone();
        let jobs_map = self.jobs.clone();
        let abort_handle_clone = job_state.abort_handle.clone();
        let sender = tx.clone();
        let job_id = JobId(plan_digest);
        let index = self.index.clone();
        let source = self.source.clone();

        let join_handle = tokio::spawn(async move {
            // Send initial queued status
            let event = ProgressEvent {
                job_id,
                timestamp: SystemTime::now(),
                status: JobStatus::Queued,
                log_line: None,
            };
            let _ = sender.send(event);

            // @spec-compliance[eos-scheduler-concurrency-limits]
            // Mechanism: Restricts concurrent running build jobs using a Tokio Semaphore
            // initialized to daemon concurrency limits. Verified-By:
            // eos/eos-daemon/src/scheduler.rs:submit
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    let err_msg = format!("Failed to acquire concurrency permit: {}", e);
                    error!("{}", err_msg);
                    let event = ProgressEvent {
                        job_id,
                        timestamp: SystemTime::now(),
                        status: JobStatus::Failed {
                            error: err_msg,
                            exit_code: None,
                        },
                        log_line: None,
                    };
                    let _ = sender.send(event);
                    return;
                },
            };

            // Transition status to Evaluating
            let event = ProgressEvent {
                job_id,
                timestamp: SystemTime::now(),
                status: JobStatus::Evaluating {
                    message: "Initializing build...".to_string(),
                },
                log_line: None,
            };
            let _ = sender.send(event.clone());

            // Update status in jobs map
            if let Ok(mut guard) = jobs_map.lock()
                && let Some(j) = guard.get_mut(&plan_digest)
            {
                j.status = event.status;
            }

            // Populate AtomIndex with atoms from BuildRequest
            use eos_core::AtomIndex;
            for dep in &request.deps {
                if let eos_core::request::FetchDescriptor::Atom(atom_dep) = dep {
                    let meta = eos_core::index::AtomMeta {
                        id: atom_dep.id.clone(),
                        label: atom_dep.label.clone(),
                        versions: vec![eos_core::index::VersionInfo {
                            version: atom_dep.version.clone(),
                            rev: atom_dep.rev.clone().unwrap_or_default(),
                            set: atom_dep.set.clone(),
                        }],
                        sets: vec![atom_dep.set.clone()],
                    };
                    let _ = index.ingest(meta).await;
                }
            }

            // The atom source is injected once at composition time; every job
            // shares this clone instead of reopening the workspace repository.
            let ingest_service = eos_snix::SnixIngestService {
                blob_service: engine.blob_service.clone(),
                directory_service: engine.directory_service.clone(),
                path_info_service: engine.path_info_service.clone(),
                nar_calculation_service: engine.nar_calculation_service.clone(),
            };

            let bridge = eos::bridge::CastoreBridge::new(
                source.clone(),
                engine.blob_service.clone(),
                engine.directory_service.clone(),
                engine.path_info_service.clone(),
                engine.nar_calculation_service.clone(),
            );

            let build_ctx = eos::orchestrator::BuildContext {
                source: &source,
                bridge: &bridge,
                ingest: &ingest_service,
            };

            // Run build orchestration pipeline
            match eos::orchestrator::run_orchestrated_build(
                &request,
                build_ctx,
                engine,
                &config.workspace_dir,
                &config.sandbox_workdir,
                sender.clone(),
                job_id,
            )
            .await
            {
                Ok(outputs) => {
                    info!("Job {} completed successfully: {:?}", plan_digest, outputs);
                    // Update final state in jobs map
                    if let Ok(mut guard) = jobs_map.lock()
                        && let Some(j) = guard.get_mut(&plan_digest)
                    {
                        j.status = JobStatus::Completed { outputs: vec![] };
                    }
                },
                Err(e) => {
                    error!("Job {} failed: {}", plan_digest, e);
                    let event = ProgressEvent {
                        job_id,
                        timestamp: SystemTime::now(),
                        status: JobStatus::Failed {
                            error: e,
                            exit_code: None,
                        },
                        log_line: None,
                    };
                    let _ = sender.send(event.clone());
                    if let Ok(mut guard) = jobs_map.lock()
                        && let Some(j) = guard.get_mut(&plan_digest)
                    {
                        j.status = event.status;
                    }
                },
            }
        });

        // Store abort handle
        if let Ok(mut guard) = abort_handle_clone.lock() {
            *guard = Some(join_handle.abort_handle());
        }

        Ok(job_state)
    }

    /// Retrieves the status of a job.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal jobs lock is poisoned.
    pub fn get_status(
        &self,
        plan_digest: &Blake3Digest,
    ) -> Result<Option<JobStatus<Blake3Digest>>, String> {
        let guard = self.jobs.lock().map_err(|e| e.to_string())?;
        Ok(guard.get(plan_digest).map(|j| j.status.clone()))
    }

    /// Cancels a running job.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal jobs lock is poisoned.
    pub fn cancel(&self, plan_digest: &Blake3Digest) -> Result<bool, String> {
        let mut guard = self.jobs.lock().map_err(|e| e.to_string())?;
        if let Some(j) = guard.get_mut(plan_digest) {
            match j.status {
                JobStatus::Completed { .. } | JobStatus::Failed { .. } | JobStatus::Cancelled => {
                    return Ok(false);
                },
                _ => {},
            }

            j.status = JobStatus::Cancelled;
            let event = ProgressEvent {
                job_id: j.id,
                timestamp: SystemTime::now(),
                status: JobStatus::Cancelled,
                log_line: None,
            };
            let _ = j.sender.send(event);

            // Abort background task
            if let Ok(handle_guard) = j.abort_handle.lock()
                && let Some(ref handle) = *handle_guard
            {
                handle.abort();
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use atom_core::{AtomContent, AtomEntry, AtomSource, AtomVersion, ContentEntry, RawVersion};
    use atom_id::{Anchor, AtomId, Label};
    use clap::Parser;
    use eos::index::RequestIndex;
    use eos_core::request::{AtomFetchDescriptor, BuildRequest, ComposerSpec, FetchDescriptor};
    use eos_snix::SnixEngine;
    use snix_build::buildservice::DummyBuildService;
    use snix_store::utils::{ServiceUrlsMemory, construct_services};
    use tokio::sync::mpsc;

    use super::*;
    use crate::config::DaemonConfig;

    /// Error type for the test source. Never surfaced to a user.
    #[derive(Debug)]
    struct TestError;

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("test source error")
        }
    }

    impl std::error::Error for TestError {}

    /// Placeholder version observation. `RecordingSource::resolve` returns
    /// `Ok(None)`, so this is never constructed — it only satisfies the
    /// associated-type bounds.
    struct NeverVersion;

    impl AtomVersion for NeverVersion {
        fn version(&self) -> &RawVersion {
            unreachable!("RecordingSource never yields an entry")
        }

        fn dig(&self) -> &[u8] {
            unreachable!("RecordingSource never yields an entry")
        }

        fn czd(&self) -> Option<&atom_core::Czd> {
            None
        }

        fn claim_msg(&self) -> Option<&str> {
            None
        }

        fn publish_msg(&self) -> Option<&str> {
            None
        }
    }

    /// Placeholder entry observation; see [`NeverVersion`].
    struct NeverEntry;

    impl AtomEntry for NeverEntry {
        type Version = NeverVersion;
        type VersionIter<'a> = std::iter::Empty<&'a NeverVersion>;

        fn id(&self) -> &AtomId {
            unreachable!("RecordingSource never yields an entry")
        }

        fn versions(&self) -> Self::VersionIter<'_> {
            std::iter::empty()
        }
    }

    /// A non-git [`AtomSource`] that records every `resolve` call. Injecting it
    /// proves the scheduler drives the *injected* source rather than a
    /// hardcoded git backend.
    #[derive(Clone)]
    struct RecordingSource {
        resolved: mpsc::UnboundedSender<AtomId>,
    }

    impl AtomSource for RecordingSource {
        type Entry = NeverEntry;
        type Error = TestError;

        async fn resolve(&self, id: &AtomId) -> Result<Option<NeverEntry>, TestError> {
            let _ = self.resolved.send(id.clone());
            Ok(None)
        }

        async fn discover(&self, _query: &str) -> Result<Vec<AtomId>, TestError> {
            Ok(Vec::new())
        }
    }

    impl AtomContent for RecordingSource {
        async fn content(
            &self,
            _id: &AtomId,
            _dig: &[u8],
        ) -> Result<Option<Vec<ContentEntry>>, TestError> {
            Ok(None)
        }
    }

    fn test_atom_id() -> AtomId {
        AtomId::new(
            Anchor::new(vec![1, 2, 3, 4]),
            Label::try_from("regression").unwrap(),
        )
    }

    async fn memory_engine() -> Arc<SnixEngine> {
        let (blob_service, directory_service, path_info_service, nar_calculation_service) =
            construct_services(ServiceUrlsMemory::parse_from(std::iter::empty::<&str>()))
                .await
                .unwrap();
        Arc::new(SnixEngine::new(
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service.into(),
            Arc::new(DummyBuildService::default()),
            None,
        ))
    }

    /// Regression: a submitted build reaches the orchestrator through the
    /// injected (non-git) atom source. Mitigates F8 — the scheduler must no
    /// longer construct its own git backend.
    #[tokio::test]
    async fn injected_source_reaches_orchestrator() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let source = RecordingSource { resolved: tx };

        let config = Arc::new(DaemonConfig::parse_from(["eosd"]));
        let engine = memory_engine().await;
        let index = Arc::new(RequestIndex::new());
        let scheduler = Scheduler::new(config, engine, index, source);

        let id = test_atom_id();
        let request = BuildRequest {
            plan_digest: Blake3Digest([7u8; 32]),
            sets: HashMap::new(),
            deps: vec![FetchDescriptor::Atom(AtomFetchDescriptor {
                id: id.clone(),
                label: "regression".to_string(),
                version: "1.0.0".to_string(),
                set: "default".to_string(),
                rev: None,
                requires: Vec::new(),
                direct: true,
            })],
            composer: ComposerSpec::Static,
            eval_args: Vec::new(),
        };

        scheduler
            .submit(request)
            .expect("submit should schedule the job");

        // The spawned build task must drive the injected source's `resolve`.
        let observed = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("injected source was not reached within timeout")
            .expect("recording channel closed without a resolve call");

        assert_eq!(
            observed, id,
            "orchestrator resolved through the injected source"
        );
    }
}
