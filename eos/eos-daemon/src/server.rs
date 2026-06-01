//! Cap'n Proto RPC server implementation for the Eos daemon.

use std::sync::Arc;

use atom_id::AtomId;
use capnp::capability::Promise;
use eos::index::LockFileIndex;
use eos_core::digest::Blake3Digest;
use eos_core::index::{AtomMeta, AtomQuery};
use eos_core::job::JobStatus;
use eos_core::{AtomIndex, Digest};
use eos_proto::eos_capnp;

use crate::scheduler::{JobState, Scheduler};

/// Helper to parse a Cap'n Proto `AtomId` reader into an `atom_id::AtomId`.
fn resolve_atom_id(capnp_id: eos_capnp::atom_id::Reader) -> Result<AtomId, capnp::Error> {
    let digest = capnp_id.get_digest()?;
    let s = std::str::from_utf8(digest)
        .map_err(|e| capnp::Error::failed(format!("Invalid UTF-8 in AtomId: {}", e)))?;
    s.parse::<AtomId>()
        .map_err(|e| capnp::Error::failed(format!("Failed to parse AtomId '{}': {}", s, e)))
}

/// Helper to populate a Cap'n Proto `AtomMeta` builder from core metadata.
fn populate_atom_meta(
    mut builder: eos_capnp::atom_meta::Builder,
    meta: &AtomMeta,
) -> Result<(), capnp::Error> {
    let mut id_builder = builder.reborrow().init_id();
    id_builder.set_digest(meta.id.to_string().as_bytes());
    builder.set_label(&meta.label);

    let mut versions_list = builder.reborrow().init_versions(meta.versions.len() as u32);
    for (i, v) in meta.versions.iter().enumerate() {
        let mut v_builder = versions_list.reborrow().get(i as u32);
        v_builder.set_version(&v.version);
        v_builder.set_rev(&v.rev);
        v_builder.set_set(&v.set);
    }

    let mut sets_list = builder.reborrow().init_sets(meta.sets.len() as u32);
    for (i, s) in meta.sets.iter().enumerate() {
        sets_list.set(i as u32, s);
    }

    Ok(())
}

/// Implementation of the `EosDaemon` Cap'n Proto interface.
pub struct EosDaemonImpl {
    scheduler: Arc<Scheduler>,
    index: Arc<LockFileIndex>,
}

impl EosDaemonImpl {
    /// Creates a new `EosDaemonImpl`.
    #[must_use]
    pub fn new(scheduler: Arc<Scheduler>, index: Arc<LockFileIndex>) -> Self {
        Self { scheduler, index }
    }
}

impl eos_capnp::eos_daemon::Server for EosDaemonImpl {
    fn submit_build(
        &mut self,
        params: eos_capnp::eos_daemon::SubmitBuildParams,
        mut results: eos_capnp::eos_daemon::SubmitBuildResults,
    ) -> Promise<(), capnp::Error> {
        let params_reader = match params.get() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let plan_digest_reader = match params_reader.get_plan_digest() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let digest_bytes = match plan_digest_reader.get_bytes() {
            Ok(b) => b,
            Err(e) => return Promise::err(e),
        };

        let plan_digest = match Blake3Digest::try_from(digest_bytes) {
            Ok(d) => d,
            Err(e) => return Promise::err(capnp::Error::failed(e.to_string())),
        };

        let eval_args_reader = match params_reader.get_eval_args() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let mut eval_args = Vec::new();
        for kv in eval_args_reader.iter() {
            let key = match kv.get_key() {
                Ok(k) => match k.to_str() {
                    Ok(s) => s.to_string(),
                    Err(e) => return Promise::err(capnp::Error::failed(e.to_string())),
                },
                Err(e) => return Promise::err(e),
            };
            let value = match kv.get_value() {
                Ok(v) => match v.to_str() {
                    Ok(s) => s.to_string(),
                    Err(e) => return Promise::err(capnp::Error::failed(e.to_string())),
                },
                Err(e) => return Promise::err(e),
            };
            eval_args.push((key, value));
        }

        match self.scheduler.submit(plan_digest, eval_args) {
            Ok(job_state) => {
                let job_server = BuildJobImpl::new(job_state, self.scheduler.clone());
                let job_client: eos_capnp::build_job::Client = capnp_rpc::new_client(job_server);
                results.get().set_job(job_client);
                Promise::ok(())
            },
            Err(e) => Promise::err(capnp::Error::failed(e)),
        }
    }

    fn query_status(
        &mut self,
        params: eos_capnp::eos_daemon::QueryStatusParams,
        mut results: eos_capnp::eos_daemon::QueryStatusResults,
    ) -> Promise<(), capnp::Error> {
        let job_id_bytes = match params.get().and_then(|p| p.get_job_id()) {
            Ok(bytes) => bytes,
            Err(e) => return Promise::err(e),
        };

        let digest = match Blake3Digest::try_from(job_id_bytes) {
            Ok(d) => d,
            Err(e) => return Promise::err(capnp::Error::failed(e.to_string())),
        };

        let status = match self.scheduler.get_status(&digest) {
            Ok(Some(s)) => s,
            Ok(None) => return Promise::err(capnp::Error::failed("Job not found".to_string())),
            Err(e) => return Promise::err(capnp::Error::failed(e)),
        };

        let mut status_builder = results.get().init_status();
        match status {
            JobStatus::Queued => {
                status_builder.set_queued(());
            },
            JobStatus::Evaluating { message } => {
                let mut group = status_builder.init_evaluating();
                group.set_message(&message);
            },
            JobStatus::Building { phase, progress } => {
                let mut group = status_builder.init_building();
                group.set_phase(&phase);
                group.set_progress(progress.unwrap_or(0.0));
            },
            JobStatus::Completed { outputs } => {
                let mut group = status_builder.init_completed();
                let mut list = group.reborrow().init_output_paths(outputs.len() as u32);
                for (i, path) in outputs.iter().enumerate() {
                    list.set(i as u32, path.store_path.as_ref());
                }
                group.set_output_digest(&[0u8; 32]);
            },
            JobStatus::Failed { error, exit_code } => {
                let mut group = status_builder.init_failed();
                group.set_error(&error);
                group.set_exit_code(exit_code.unwrap_or(-1));
            },
            JobStatus::Cancelled => {
                status_builder.set_cancelled(());
            },
        }

        Promise::ok(())
    }

    fn get_capabilities(
        &mut self,
        _params: eos_capnp::eos_daemon::GetCapabilitiesParams,
        mut results: eos_capnp::eos_daemon::GetCapabilitiesResults,
    ) -> Promise<(), capnp::Error> {
        let mut results_builder = results.get();
        results_builder.set_api_version(1);
        let mut backends = results_builder.init_supported_backends(1);
        backends.set(0, "snix");
        Promise::ok(())
    }

    fn discover(
        &mut self,
        _params: eos_capnp::eos_daemon::DiscoverParams,
        mut results: eos_capnp::eos_daemon::DiscoverResults,
    ) -> Promise<(), capnp::Error> {
        let discovery_server = AtomDiscoveryImpl::new(self.index.clone());
        let discovery_client: eos_capnp::atom_discovery::Client =
            capnp_rpc::new_client(discovery_server);
        results.get().set_discovery(discovery_client);
        Promise::ok(())
    }
}

/// Implementation of the `BuildJob` Cap'n Proto interface.
pub struct BuildJobImpl {
    job_state: JobState,
    scheduler: Arc<Scheduler>,
}

impl BuildJobImpl {
    /// Creates a new `BuildJobImpl`.
    pub fn new(job_state: JobState, scheduler: Arc<Scheduler>) -> Self {
        Self {
            job_state,
            scheduler,
        }
    }
}

impl eos_capnp::build_job::Server for BuildJobImpl {
    fn attach_progress(
        &mut self,
        params: eos_capnp::build_job::AttachProgressParams,
        _results: eos_capnp::build_job::AttachProgressResults,
    ) -> Promise<(), capnp::Error> {
        let callback = match params.get().and_then(|p| p.get_callback()) {
            Ok(c) => c,
            Err(e) => return Promise::err(e),
        };

        let mut rx = self.job_state.sender.subscribe();

        // [eos-progress-multiplexing]
        tokio::task::spawn_local(async move {
            while let Ok(event) = rx.recv().await {
                let mut req = callback.update_request();
                {
                    let mut status_builder = req.get().init_status();
                    match &event.status {
                        JobStatus::Queued => {
                            status_builder.set_queued(());
                        },
                        JobStatus::Evaluating { message } => {
                            let mut group = status_builder.init_evaluating();
                            group.set_message(message);
                        },
                        JobStatus::Building { phase, progress } => {
                            let mut group = status_builder.init_building();
                            group.set_phase(phase);
                            group.set_progress(progress.unwrap_or(0.0));
                        },
                        JobStatus::Completed { outputs } => {
                            let mut group = status_builder.init_completed();
                            let mut list = group.reborrow().init_output_paths(outputs.len() as u32);
                            for (i, path) in outputs.iter().enumerate() {
                                list.set(i as u32, path.store_path.as_ref());
                            }
                            group.set_output_digest(&[0u8; 32]);
                        },
                        JobStatus::Failed { error, exit_code } => {
                            let mut group = status_builder.init_failed();
                            group.set_error(error);
                            group.set_exit_code(exit_code.unwrap_or(-1));
                        },
                        JobStatus::Cancelled => {
                            status_builder.set_cancelled(());
                        },
                    }
                }
                if let Err(e) = req.send().await {
                    tracing::error!("Failed to send progress update: {}", e);
                    break;
                }

                // If final state reached, invoke done() and terminate.
                if matches!(
                    event.status,
                    JobStatus::Completed { .. } | JobStatus::Failed { .. } | JobStatus::Cancelled
                ) {
                    let done_req = callback.done_request();
                    let _ = done_req.send().promise.await;
                    break;
                }
            }
        });

        Promise::ok(())
    }

    fn cancel(
        &mut self,
        _params: eos_capnp::build_job::CancelParams,
        _results: eos_capnp::build_job::CancelResults,
    ) -> Promise<(), capnp::Error> {
        let plan_digest = self.job_state.id.0;
        match self.scheduler.cancel(&plan_digest) {
            Ok(_) => Promise::ok(()),
            Err(e) => Promise::err(capnp::Error::failed(e)),
        }
    }

    fn get_job_id(
        &mut self,
        _params: eos_capnp::build_job::GetJobIdParams,
        mut results: eos_capnp::build_job::GetJobIdResults,
    ) -> Promise<(), capnp::Error> {
        let digest_bytes = self.job_state.id.0.as_bytes();
        results.get().set_job_id(digest_bytes);
        Promise::ok(())
    }
}

/// Implementation of the `AtomDiscovery` Cap'n Proto interface.
pub struct AtomDiscoveryImpl {
    index: Arc<LockFileIndex>,
}

impl AtomDiscoveryImpl {
    /// Creates a new `AtomDiscoveryImpl`.
    pub fn new(index: Arc<LockFileIndex>) -> Self {
        Self { index }
    }
}

impl eos_capnp::atom_discovery::Server for AtomDiscoveryImpl {
    fn resolve(
        &mut self,
        params: eos_capnp::atom_discovery::ResolveParams,
        mut results: eos_capnp::atom_discovery::ResolveResults,
    ) -> Promise<(), capnp::Error> {
        let id_reader = match params.get().and_then(|p| p.get_id()) {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };
        let atom_id = match resolve_atom_id(id_reader) {
            Ok(id) => id,
            Err(e) => return Promise::err(e),
        };

        let index = self.index.clone();
        let p = async move {
            if let Some(meta) = index.resolve(&atom_id).await.unwrap() {
                let meta_builder = results.get().init_meta();
                populate_atom_meta(meta_builder, &meta)?;
                Ok(())
            } else {
                Err(capnp::Error::failed("Atom not found".to_string()))
            }
        };
        Promise::from_future(p)
    }

    fn contains(
        &mut self,
        params: eos_capnp::atom_discovery::ContainsParams,
        mut results: eos_capnp::atom_discovery::ContainsResults,
    ) -> Promise<(), capnp::Error> {
        let id_reader = match params.get().and_then(|p| p.get_id()) {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };
        let atom_id = match resolve_atom_id(id_reader) {
            Ok(id) => id,
            Err(e) => return Promise::err(e),
        };

        let index = self.index.clone();
        let p = async move {
            let exists = index.contains(&atom_id).await.unwrap();
            results.get().set_exists(exists);
            Ok(())
        };
        Promise::from_future(p)
    }

    fn search(
        &mut self,
        params: eos_capnp::atom_discovery::SearchParams,
        mut results: eos_capnp::atom_discovery::SearchResults,
    ) -> Promise<(), capnp::Error> {
        let index = self.index.clone();
        let p = async move {
            let query_reader = params.get()?.get_query()?;
            let label_pattern = query_reader
                .get_label_pattern()?
                .to_str()
                .map_err(|e| capnp::Error::failed(e.to_string()))?
                .to_string();
            let set_filter = if query_reader.has_set_filter() {
                let filter = query_reader
                    .get_set_filter()?
                    .to_str()
                    .map_err(|e| capnp::Error::failed(e.to_string()))?
                    .to_string();
                if filter.is_empty() {
                    None
                } else {
                    Some(filter)
                }
            } else {
                None
            };
            let limit = query_reader.get_limit();

            let query = AtomQuery {
                label_pattern,
                set_filter,
                limit,
            };

            let results_list = index.search(&query).await.unwrap();
            let mut results_builder = results.get().init_results(results_list.len() as u32);
            for (i, meta) in results_list.iter().enumerate() {
                let meta_builder = results_builder.reborrow().get(i as u32);
                populate_atom_meta(meta_builder, meta)?;
            }
            Ok(())
        };
        Promise::from_future(p)
    }
}
