//! Eos daemon build scheduler.
//!
//! Orchestrates job execution, deduplicates concurrent build submissions,
//! and enforces local concurrency limits.

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

/// Status and metadata for a worker node.
#[allow(dead_code)]
pub struct WorkerStatus {
    /// Cryptographic worker identity.
    pub id: String,
    /// Concurrency limits.
    pub max_concurrency: usize,
    /// Active jobs currently assigned.
    pub active_jobs: std::collections::HashSet<Blake3Digest>,
    /// Last recorded heartbeat timestamp.
    pub last_heartbeat: SystemTime,
    /// Whether the worker is healthy.
    pub healthy: bool,
}

/// Lease covering a running job.
#[allow(dead_code)]
pub struct Lease {
    /// Worker assigned to the job.
    pub worker_id: String,
    /// Lease grant timestamp.
    pub granted_at: SystemTime,
    /// Lease expiration timestamp.
    pub expires_at: SystemTime,
}

/// The build job scheduler.
pub struct Scheduler {
    config: Arc<DaemonConfig>,
    engine: Arc<SnixEngine>,
    index: Arc<eos::index::LockFileIndex>,
    jobs: Arc<Mutex<HashMap<Blake3Digest, JobState>>>,
    semaphore: Arc<Semaphore>,
    workers: Arc<Mutex<HashMap<String, WorkerStatus>>>,
    leases: Arc<Mutex<HashMap<Blake3Digest, Lease>>>,
    pub lease_duration: Arc<Mutex<std::time::Duration>>,
    pub heartbeat_deadline: Arc<Mutex<std::time::Duration>>,
}

impl Scheduler {
    /// Creates a new `Scheduler`.
    #[must_use]
    pub fn new(
        config: Arc<DaemonConfig>,
        engine: Arc<SnixEngine>,
        index: Arc<eos::index::LockFileIndex>,
    ) -> Self {
        let max_concurrency = config.max_concurrency;
        let lease_duration = Arc::new(Mutex::new(std::time::Duration::from_secs(30)));
        let heartbeat_deadline = Arc::new(Mutex::new(std::time::Duration::from_secs(10)));

        let scheduler = Self {
            config,
            engine,
            index,
            jobs: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            workers: Arc::new(Mutex::new(HashMap::new())),
            leases: Arc::new(Mutex::new(HashMap::new())),
            lease_duration,
            heartbeat_deadline,
        };

        // Spawn a background monitoring loop for leases and heartbeats
        let workers_clone = scheduler.workers.clone();
        let leases_clone = scheduler.leases.clone();
        let jobs_clone = scheduler.jobs.clone();
        let heartbeat_deadline_clone = scheduler.heartbeat_deadline.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let now = SystemTime::now();

                // 1. Check worker heartbeats [eos-scheduler-heartbeat-liveness]
                let mut unhealthy_workers = Vec::new();
                let deadline = if let Ok(guard) = heartbeat_deadline_clone.lock() {
                    *guard
                } else {
                    std::time::Duration::from_secs(10)
                };

                if let Ok(mut w_guard) = workers_clone.lock() {
                    for (id, worker) in w_guard.iter_mut() {
                        if worker.healthy {
                            if let Ok(elapsed) = now.duration_since(worker.last_heartbeat) {
                                if elapsed > deadline {
                                    worker.healthy = false;
                                    info!("Worker {} marked unhealthy due to missed heartbeat", id);
                                    unhealthy_workers.push(id.clone());
                                }
                            }
                        }
                    }
                }

                // 2. Revoke leases for unhealthy workers or expired leases
                //    [eos-scheduler-lease-expiry]
                let mut expired_jobs = Vec::new();
                if let Ok(mut l_guard) = leases_clone.lock() {
                    let mut to_remove = Vec::new();
                    for (job_id, lease) in l_guard.iter() {
                        let is_expired = now > lease.expires_at;
                        let is_worker_unhealthy = unhealthy_workers.contains(&lease.worker_id);
                        if is_expired || is_worker_unhealthy {
                            expired_jobs.push((*job_id, lease.worker_id.clone(), is_expired));
                            to_remove.push(*job_id);
                        }
                    }
                    for job_id in to_remove {
                        l_guard.remove(&job_id);
                    }
                }

                // 3. Process expired/unhealthy worker jobs (abort and re-queue)
                for (job_id, worker_id, is_expired) in expired_jobs {
                    if let Ok(mut jobs_guard) = jobs_clone.lock() {
                        if let Some(j) = jobs_guard.get_mut(&job_id) {
                            let msg = if is_expired {
                                format!("Lease expired for job {} on worker {}", job_id, worker_id)
                            } else {
                                format!("Worker {} became unhealthy for job {}", worker_id, job_id)
                            };
                            error!("{}", msg);

                            // Abort currently running task
                            if let Ok(handle_guard) = j.abort_handle.lock() {
                                if let Some(ref handle) = *handle_guard {
                                    handle.abort();
                                }
                            }

                            // Evict from assigned worker's active jobs list
                            if let Ok(mut w_guard) = workers_clone.lock() {
                                if let Some(w) = w_guard.get_mut(&worker_id) {
                                    w.active_jobs.remove(&job_id);
                                }
                            }

                            // Transition job back to queued for reassignment
                            j.status = JobStatus::Queued;
                            let event = ProgressEvent {
                                job_id: j.id,
                                timestamp: SystemTime::now(),
                                status: JobStatus::Queued,
                                log_line: Some(format!("Re-queueing: {}", msg)),
                            };
                            let _ = j.sender.send(event);
                        }
                    }
                }
            }
        });

        scheduler
    }

    /// Submits a lock file build job to the scheduler.
    ///
    /// If the job is already active, returns the existing handle.
    /// Otherwise, schedules a new background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the locks directory cannot be read or resolve socket fails.
    pub fn submit(
        &self,
        plan_digest: Blake3Digest,
        eval_args: Vec<(String, String)>,
    ) -> Result<JobState, String> {
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

            // Resolve lock path on host
            let locks_dir = config.resolve_locks_dir();
            let lock_path = locks_dir.join(format!("{}.lock", plan_digest));
            let lock_content = match tokio::fs::read_to_string(&lock_path).await {
                Ok(content) => content,
                Err(e) => {
                    let err_msg = format!("Failed to read lock file at {:?}: {}", lock_path, e);
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

            // Populate AtomIndex with atoms from lock file
            if let Ok(lock_file) = eos::lock::LockFile::parse(&lock_content) {
                use eos_core::AtomIndex;
                for dep in lock_file.deps {
                    if let eos::lock::Dependency::Atom(atom_dep) = dep {
                        let meta = eos_core::index::AtomMeta {
                            id: atom_dep.id,
                            label: atom_dep.label,
                            versions: vec![eos_core::index::VersionInfo {
                                version: atom_dep.version.clone(),
                                rev: atom_dep.rev.unwrap_or_default(),
                                set: atom_dep.set.clone(),
                            }],
                            sets: vec![atom_dep.set],
                        };
                        let _ = index.ingest(meta).await;
                    }
                }
            }

            // Run build orchestration pipeline
            match eos::orchestrator::run_orchestrated_build(
                &lock_content,
                eval_args,
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
                        // Find the completed event from sender history or construct it
                        // To be simple, retrieve final status from SnixOutput mappings
                        // Note: run_orchestrated_build broadcasts status updates, so status in
                        // jobs map is updated by it.
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

    /// Register a worker with the scheduler.
    pub fn register_worker(&self, worker_id: String, max_concurrency: usize) -> Result<(), String> {
        let mut guard = self.workers.lock().map_err(|e| e.to_string())?;
        guard.insert(
            worker_id.clone(),
            WorkerStatus {
                id: worker_id,
                max_concurrency,
                active_jobs: std::collections::HashSet::new(),
                last_heartbeat: SystemTime::now(),
                healthy: true,
            },
        );
        Ok(())
    }

    /// Record a heartbeat from a worker, updating its last_heartbeat and marking it healthy.
    pub fn record_heartbeat(&self, worker_id: &str) -> Result<(), String> {
        let mut guard = self.workers.lock().map_err(|e| e.to_string())?;
        if let Some(w) = guard.get_mut(worker_id) {
            w.last_heartbeat = SystemTime::now();
            if !w.healthy {
                w.healthy = true;
                info!("Worker {} returned to healthy status", worker_id);
            }
            Ok(())
        } else {
            Err(format!("Worker {} not registered", worker_id))
        }
    }

    /// Grant a lease for a job on a worker.
    pub fn grant_lease(&self, job_id: Blake3Digest, worker_id: String) -> Result<(), String> {
        let mut guard = self.leases.lock().map_err(|e| e.to_string())?;
        let now = SystemTime::now();
        let duration = if let Ok(g) = self.lease_duration.lock() {
            *g
        } else {
            std::time::Duration::from_secs(30)
        };
        guard.insert(
            job_id,
            Lease {
                worker_id,
                granted_at: now,
                expires_at: now + duration,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;
    use snix_build::buildservice::DummyBuildService;
    use snix_store::utils::{ServiceUrlsMemory, construct_services};

    use super::*;

    #[tokio::test]
    async fn test_scheduler_lease_and_heartbeat() {
        let (blob_service, directory_service, path_info_service, nar_calculation_service) =
            construct_services(ServiceUrlsMemory::parse_from(std::iter::empty::<&str>()))
                .await
                .unwrap();

        let build_service = Arc::new(DummyBuildService::default());
        let engine = Arc::new(SnixEngine::new(
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service.into(),
            build_service,
            None,
        ));

        let config = Arc::new(DaemonConfig {
            socket_path: None,
            blob_service_addr: "memory://".to_string(),
            directory_service_addr: "memory://".to_string(),
            path_info_service_addr: "memory://".to_string(),
            max_concurrency: 2,
            sandbox_workdir: PathBuf::from("/tmp/sandbox"),
            workspace_dir: PathBuf::from("/tmp/workspace"),
            locks_dir: PathBuf::from("/tmp/locks"),
            eval_worker: false,
            enable_eval_sandbox: false,
        });

        let index = Arc::new(eos::index::LockFileIndex::new());

        let scheduler = Scheduler::new(config, engine, index);
        // Reduce lease and heartbeat durations for faster test execution
        let scheduler = scheduler;
        *scheduler.lease_duration.lock().unwrap() = std::time::Duration::from_millis(500);
        *scheduler.heartbeat_deadline.lock().unwrap() = std::time::Duration::from_millis(100);

        let worker_id = "worker-1".to_string();
        scheduler.register_worker(worker_id.clone(), 2).unwrap();

        // Check registered status
        {
            let guard = scheduler.workers.lock().unwrap();
            let w = guard.get(&worker_id).unwrap();
            assert!(w.healthy);
            assert_eq!(w.max_concurrency, 2);
        }

        // Grant a lease
        let job_id = Blake3Digest([1; 32]);
        scheduler.grant_lease(job_id, worker_id.clone()).unwrap();

        {
            let guard = scheduler.leases.lock().unwrap();
            let lease = guard.get(&job_id).unwrap();
            assert_eq!(lease.worker_id, worker_id);
        }

        // 1. Test heartbeat deadline expiry (worker goes unhealthy and lease is revoked)
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        {
            let guard = scheduler.workers.lock().unwrap();
            let w = guard.get(&worker_id).unwrap();
            assert!(!w.healthy);
        }
        {
            let guard = scheduler.leases.lock().unwrap();
            assert!(guard.get(&job_id).is_none());
        }

        // 2. record heartbeat, verify it stays healthy
        scheduler.record_heartbeat(&worker_id).unwrap();
        {
            let guard = scheduler.workers.lock().unwrap();
            let w = guard.get(&worker_id).unwrap();
            assert!(w.healthy);
        }

        // 3. Grant a lease again, wait 600ms while keeping worker healthy
        scheduler.grant_lease(job_id, worker_id.clone()).unwrap();

        for _ in 0..6 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let _ = scheduler.record_heartbeat(&worker_id);
        }

        // Lease should expire and be gone
        {
            let guard = scheduler.leases.lock().unwrap();
            assert!(guard.get(&job_id).is_none());
        }
    }
}
