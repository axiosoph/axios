//! Cap'n Proto RPC server implementation for the Eos daemon.

use std::sync::Arc;

use atom_id::AtomId;
use capnp::capability::Promise;
use eos::index::RequestIndex;
use eos_core::digest::Blake3Digest;
use eos_core::index::{AtomMeta, AtomQuery};
use eos_core::job::JobStatus;
use eos_core::request::{
    AtomFetchDescriptor, AtomSetInfo, BuildRequest, ComposerSpec, FetchDescriptor,
    NixFetchDescriptor, NixGitFetchDescriptor, NixSrcFetchDescriptor, NixTarFetchDescriptor,
};
use eos_core::{AtomIndex, Digest};
use eos_proto::eos_capnp;

use crate::scheduler::{InjectedSource, JobState, Scheduler};

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
///
/// Generic over the injected atom source `S`, carried through from the
/// [`Scheduler`].
pub struct EosDaemonImpl<S> {
    scheduler: Arc<Scheduler<S>>,
    index: Arc<RequestIndex>,
}

impl<S: InjectedSource> EosDaemonImpl<S> {
    /// Creates a new `EosDaemonImpl`.
    #[must_use]
    pub fn new(scheduler: Arc<Scheduler<S>>, index: Arc<RequestIndex>) -> Self {
        Self { scheduler, index }
    }
}

fn deserialize_request(
    reader: eos_capnp::build_request::Reader<'_>,
) -> Result<BuildRequest<Blake3Digest>, capnp::Error> {
    // 1. planDigest
    let digest_bytes = reader.get_plan_digest()?;
    let plan_digest =
        Blake3Digest::try_from(digest_bytes).map_err(|e| capnp::Error::failed(e.to_string()))?;

    // 2. sets
    let sets_reader = reader.get_sets()?;
    let mut sets = std::collections::HashMap::new();
    for i in 0..sets_reader.len() {
        let set_entry = sets_reader.get(i);
        let anchor = set_entry.get_anchor()?.to_str()?.to_string();
        let tag = set_entry.get_tag()?.to_str()?.to_string();
        let mirrors_reader = set_entry.get_mirrors()?;
        let mut mirrors = Vec::new();
        for j in 0..mirrors_reader.len() {
            mirrors.push(mirrors_reader.get(j)?.to_str()?.to_string());
        }
        sets.insert(anchor, AtomSetInfo { tag, mirrors });
    }

    // 3. deps
    let deps_reader = reader.get_deps()?;
    let mut deps = Vec::new();
    for i in 0..deps_reader.len() {
        let dep_reader = deps_reader.get(i);
        use eos_capnp::dep_descriptor::Which;
        let fd = match dep_reader.which()? {
            Which::Atom(group) => {
                let id = resolve_atom_id(group.get_id()?)?;
                let label = group.get_label()?.to_str()?.to_string();
                let version = group.get_version()?.to_str()?.to_string();
                let set = group.get_set()?.to_str()?.to_string();
                let rev_str = group.get_rev()?.to_str()?;
                let rev = if rev_str.is_empty() {
                    None
                } else {
                    Some(rev_str.to_string())
                };

                let requires_reader = group.get_requires()?;
                let mut requires = Vec::new();
                for j in 0..requires_reader.len() {
                    requires.push(resolve_atom_id(requires_reader.get(j))?);
                }
                let direct = group.get_direct();
                FetchDescriptor::Atom(AtomFetchDescriptor {
                    id,
                    label,
                    version,
                    set,
                    rev,
                    requires,
                    direct,
                })
            },
            Which::Nix(group) => {
                let name = group.get_name()?.to_str()?.to_string();
                let url = group.get_url()?.to_str()?.to_string();
                let hash = group.get_hash()?.to_str()?.to_string();
                let owner = if group.has_owner() {
                    Some(resolve_atom_id(group.get_owner()?)?)
                } else {
                    None
                };
                FetchDescriptor::Nix(NixFetchDescriptor {
                    name,
                    url,
                    hash,
                    owner,
                })
            },
            Which::NixGit(group) => {
                let name = group.get_name()?.to_str()?.to_string();
                let url = group.get_url()?.to_str()?.to_string();
                let rev = group.get_rev()?.to_str()?.to_string();
                let ver_str = group.get_version()?.to_str()?;
                let version = if ver_str.is_empty() {
                    None
                } else {
                    Some(ver_str.to_string())
                };
                let owner = if group.has_owner() {
                    Some(resolve_atom_id(group.get_owner()?)?)
                } else {
                    None
                };
                FetchDescriptor::NixGit(NixGitFetchDescriptor {
                    name,
                    url,
                    rev,
                    version,
                    owner,
                })
            },
            Which::NixTar(group) => {
                let name = group.get_name()?.to_str()?.to_string();
                let url = group.get_url()?.to_str()?.to_string();
                let hash = group.get_hash()?.to_str()?.to_string();
                let owner = if group.has_owner() {
                    Some(resolve_atom_id(group.get_owner()?)?)
                } else {
                    None
                };
                FetchDescriptor::NixTar(NixTarFetchDescriptor {
                    name,
                    url,
                    hash,
                    owner,
                })
            },
            Which::NixSrc(group) => {
                let name = group.get_name()?.to_str()?.to_string();
                let url = group.get_url()?.to_str()?.to_string();
                let hash = group.get_hash()?.to_str()?.to_string();
                let owner = if group.has_owner() {
                    Some(resolve_atom_id(group.get_owner()?)?)
                } else {
                    None
                };
                FetchDescriptor::NixSrc(NixSrcFetchDescriptor {
                    name,
                    url,
                    hash,
                    owner,
                })
            },
        };
        deps.push(fd);
    }

    // 4. composer
    let comp_reader = reader.get_composer()?;
    use eos_capnp::composer_spec::Which as CompWhich;
    let composer = match comp_reader.which()? {
        CompWhich::Atom(group) => {
            let id = resolve_atom_id(group.get_id()?)?;
            let entry_str = group.get_entry()?.to_str()?;
            let entry = if entry_str.is_empty() {
                None
            } else {
                Some(entry_str.to_string())
            };
            let args_list = group.get_args()?;
            let mut args = std::collections::HashMap::new();
            for j in 0..args_list.len() {
                let kv = args_list.get(j);
                args.insert(
                    kv.get_key()?.to_str()?.to_string(),
                    kv.get_value()?.to_str()?.to_string(),
                );
            }
            ComposerSpec::Atom { id, entry, args }
        },
        CompWhich::NixTrivial(group) => {
            let expression = group.get_expression()?.to_str()?.to_string();
            let args_list = group.get_args()?;
            let mut args = std::collections::HashMap::new();
            for j in 0..args_list.len() {
                let kv = args_list.get(j);
                args.insert(
                    kv.get_key()?.to_str()?.to_string(),
                    kv.get_value()?.to_str()?.to_string(),
                );
            }
            ComposerSpec::NixTrivial { expression, args }
        },
        CompWhich::Static(()) => ComposerSpec::Static,
    };

    // 5. evalArgs
    let eval_args_reader = reader.get_eval_args()?;
    let mut eval_args = Vec::new();
    for j in 0..eval_args_reader.len() {
        let kv = eval_args_reader.get(j);
        eval_args.push((
            kv.get_key()?.to_str()?.to_string(),
            kv.get_value()?.to_str()?.to_string(),
        ));
    }

    Ok(BuildRequest {
        plan_digest,
        sets,
        deps,
        composer,
        eval_args,
    })
}

impl<S: InjectedSource> eos_capnp::eos_daemon::Server for EosDaemonImpl<S> {
    fn submit_build(
        &mut self,
        params: eos_capnp::eos_daemon::SubmitBuildParams,
        mut results: eos_capnp::eos_daemon::SubmitBuildResults,
    ) -> Promise<(), capnp::Error> {
        let params_reader = match params.get() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let request_reader = match params_reader.get_request() {
            Ok(r) => r,
            Err(e) => return Promise::err(e),
        };

        let request = match deserialize_request(request_reader) {
            Ok(req) => req,
            Err(e) => return Promise::err(e),
        };

        match self.scheduler.submit(request) {
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
pub struct BuildJobImpl<S> {
    job_state: JobState,
    scheduler: Arc<Scheduler<S>>,
}

impl<S: InjectedSource> BuildJobImpl<S> {
    /// Creates a new `BuildJobImpl`.
    pub fn new(job_state: JobState, scheduler: Arc<Scheduler<S>>) -> Self {
        Self {
            job_state,
            scheduler,
        }
    }
}

impl<S: InjectedSource> eos_capnp::build_job::Server for BuildJobImpl<S> {
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

    fn get_missing(
        &mut self,
        _params: eos_capnp::build_job::GetMissingParams,
        mut results: eos_capnp::build_job::GetMissingResults,
    ) -> Promise<(), capnp::Error> {
        let _list = results.get().init_missing_atoms(0);
        Promise::ok(())
    }
}

/// Implementation of the `AtomDiscovery` Cap'n Proto interface.
pub struct AtomDiscoveryImpl {
    index: Arc<RequestIndex>,
}

impl AtomDiscoveryImpl {
    /// Creates a new `AtomDiscoveryImpl`.
    pub fn new(index: Arc<RequestIndex>) -> Self {
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
