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
pub struct Scheduler {
    config: Arc<DaemonConfig>,
    engine: Arc<SnixEngine>,
    index: Arc<eos::index::RequestIndex>,
    jobs: Arc<Mutex<HashMap<Blake3Digest, JobState>>>,
    semaphore: Arc<Semaphore>,
}

impl Scheduler {
    /// Creates a new `Scheduler`.
    #[must_use]
    pub fn new(
        config: Arc<DaemonConfig>,
        engine: Arc<SnixEngine>,
        index: Arc<eos::index::RequestIndex>,
    ) -> Self {
        let max_concurrency = config.max_concurrency;

        Self {
            config,
            engine,
            index,
            jobs: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
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

        // 1. Deduplication [eos-scheduler-deduplication]
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

        let join_handle = tokio::spawn(async move {
            // Send initial queued status
            let event = ProgressEvent {
                job_id,
                timestamp: SystemTime::now(),
                status: JobStatus::Queued,
                log_line: None,
            };
            let _ = sender.send(event);

            // Enforce concurrency limit [eos-scheduler-concurrency-limits]
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

            // Open the local workspace git repository to act as an AtomSource
            let repo = match gix::open(&config.workspace_dir) {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("Failed to open workspace git repository: {}", e);
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
                    let _ = sender.send(event.clone());
                    if let Ok(mut guard) = jobs_map.lock()
                        && let Some(j) = guard.get_mut(&plan_digest)
                    {
                        j.status = event.status;
                    }
                    return;
                },
            };
            let source = atom_git::GitSource::new(repo);

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

            // Run build orchestration pipeline
            match eos::orchestrator::run_orchestrated_build(
                &request,
                &source,
                &bridge,
                &ingest_service,
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
