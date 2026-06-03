//! Ion Eos Client Library.
//!
//! Exposes connection and RPC wrapper logic for talking to the Eos daemon.

pub mod discovery;
pub mod error;

use std::path::Path;

use atom_id::AtomId;
use capnp::capability::Promise;
use capnp_rpc::rpc_twoparty_capnp::Side;
use eos_core::Digest;
use eos_core::digest::Blake3Digest;
use eos_core::job::{JobStatus, ProgressEvent};
use eos_core::request::{
    AtomFetchDescriptor, AtomSetInfo, BuildRequest, ComposerSpec, FetchDescriptor,
    NixFetchDescriptor, NixGitFetchDescriptor, NixSrcFetchDescriptor, NixTarFetchDescriptor,
};
use eos_proto::eos_capnp;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::discovery::DiscoveryClient;
use crate::error::ClientError;

/// Client for communicating with the Eos build daemon.
#[derive(Clone)]
pub struct EosClient {
    daemon: eos_capnp::eos_daemon::Client,
}

impl EosClient {
    /// Connects to a running Eos daemon at the specified socket path.
    ///
    /// # Errors
    ///
    /// Returns a `ClientError::ConnectionFailed` if the socket connection cannot be established.
    pub async fn connect<P: AsRef<Path>>(socket_path: P) -> Result<Self, ClientError> {
        let path = socket_path.as_ref().to_path_buf();
        let stream = tokio::net::UnixStream::connect(&path).await.map_err(|e| {
            ClientError::ConnectionFailed {
                socket_path: path.to_string_lossy().to_string(),
                source: e,
            }
        })?;

        let (reader, writer) = stream.into_split();
        let compat_reader = reader.compat();
        let compat_writer = writer.compat_write();

        let network = capnp_rpc::twoparty::VatNetwork::new(
            compat_reader,
            compat_writer,
            Side::Client,
            Default::default(),
        );

        let mut rpc_system = capnp_rpc::RpcSystem::new(Box::new(network), None);
        let client: eos_capnp::eos_daemon::Client = rpc_system.bootstrap(Side::Server);

        // Spawn RPC system on local task
        tokio::task::spawn_local(async move {
            if let Err(e) = rpc_system.await {
                tracing::error!("RPC system error: {}", e);
            }
        });

        Ok(Self { daemon: client })
    }

    /// Submits a pre-translated BuildRequest to the Eos daemon.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC submission fails.
    pub async fn submit_build(
        &self,
        request: &BuildRequest<Blake3Digest>,
    ) -> Result<BuildHandle, ClientError> {
        let mut req = self.daemon.submit_build_request();
        {
            let mut params = req.get();
            let mut request_builder = params.reborrow().init_request();

            // a. planDigest
            request_builder.set_plan_digest(request.plan_digest.as_bytes());

            // b. sets
            let mut sets_list = request_builder
                .reborrow()
                .init_sets(request.sets.len() as u32);
            for (i, (anchor, info)) in request.sets.iter().enumerate() {
                let mut set_entry = sets_list.reborrow().get(i as u32);
                set_entry.set_anchor(anchor);
                set_entry.set_tag(&info.tag);
                let mut mirrors_list = set_entry.init_mirrors(info.mirrors.len() as u32);
                for (j, mirror) in info.mirrors.iter().enumerate() {
                    mirrors_list.reborrow().set(j as u32, mirror);
                }
            }

            // c. deps
            let mut deps_list = request_builder
                .reborrow()
                .init_deps(request.deps.len() as u32);
            for (i, dep) in request.deps.iter().enumerate() {
                let dep_desc = deps_list.reborrow().get(i as u32);
                match dep {
                    FetchDescriptor::Atom(d) => {
                        let mut atom_builder = dep_desc.init_atom();
                        let mut id_builder = atom_builder.reborrow().init_id();
                        id_builder.set_digest(d.id.to_string().as_bytes());
                        atom_builder.set_label(&d.label);
                        atom_builder.set_version(&d.version);
                        atom_builder.set_set(&d.set);
                        if let Some(rev) = &d.rev {
                            atom_builder.set_rev(rev);
                        }
                        let mut reqs_list = atom_builder
                            .reborrow()
                            .init_requires(d.requires.len() as u32);
                        for (j, req_id) in d.requires.iter().enumerate() {
                            let mut req_id_builder = reqs_list.reborrow().get(j as u32);
                            req_id_builder.set_digest(req_id.to_string().as_bytes());
                        }
                        atom_builder.set_direct(d.direct);
                    },
                    FetchDescriptor::Nix(d) => {
                        let mut nix_builder = dep_desc.init_nix();
                        nix_builder.set_name(&d.name);
                        nix_builder.set_url(&d.url);
                        nix_builder.set_hash(&d.hash);
                        if let Some(owner) = &d.owner {
                            let mut owner_builder = nix_builder.init_owner();
                            owner_builder.set_digest(owner.to_string().as_bytes());
                        }
                    },
                    FetchDescriptor::NixGit(d) => {
                        let mut git_builder = dep_desc.init_nix_git();
                        git_builder.set_name(&d.name);
                        git_builder.set_url(&d.url);
                        git_builder.set_rev(&d.rev);
                        if let Some(ver) = &d.version {
                            git_builder.set_version(ver);
                        }
                        if let Some(owner) = &d.owner {
                            let mut owner_builder = git_builder.init_owner();
                            owner_builder.set_digest(owner.to_string().as_bytes());
                        }
                    },
                    FetchDescriptor::NixTar(d) => {
                        let mut tar_builder = dep_desc.init_nix_tar();
                        tar_builder.set_name(&d.name);
                        tar_builder.set_url(&d.url);
                        tar_builder.set_hash(&d.hash);
                        if let Some(owner) = &d.owner {
                            let mut owner_builder = tar_builder.init_owner();
                            owner_builder.set_digest(owner.to_string().as_bytes());
                        }
                    },
                    FetchDescriptor::NixSrc(d) => {
                        let mut src_builder = dep_desc.init_nix_src();
                        src_builder.set_name(&d.name);
                        src_builder.set_url(&d.url);
                        src_builder.set_hash(&d.hash);
                        if let Some(owner) = &d.owner {
                            let mut owner_builder = src_builder.init_owner();
                            owner_builder.set_digest(owner.to_string().as_bytes());
                        }
                    },
                }
            }

            // d. composer
            let mut comp_builder = request_builder.reborrow().init_composer();
            match &request.composer {
                ComposerSpec::Atom { id, entry, args } => {
                    let mut atom_builder = comp_builder.init_atom();
                    let mut id_builder = atom_builder.reborrow().init_id();
                    id_builder.set_digest(id.to_string().as_bytes());
                    if let Some(ent) = entry {
                        atom_builder.set_entry(ent);
                    }
                    let mut args_list = atom_builder.init_args(args.len() as u32);
                    for (j, (k, v)) in args.iter().enumerate() {
                        let mut kv = args_list.reborrow().get(j as u32);
                        kv.set_key(k);
                        kv.set_value(v);
                    }
                },
                ComposerSpec::NixTrivial { expression, args } => {
                    let mut nix_builder = comp_builder.init_nix_trivial();
                    nix_builder.set_expression(expression);
                    let mut args_list = nix_builder.init_args(args.len() as u32);
                    for (j, (k, v)) in args.iter().enumerate() {
                        let mut kv = args_list.reborrow().get(j as u32);
                        kv.set_key(k);
                        kv.set_value(v);
                    }
                },
                ComposerSpec::Static => {
                    comp_builder.set_static(());
                },
            }

            // e. evalArgs
            let mut eval_args_list = request_builder.init_eval_args(request.eval_args.len() as u32);
            for (j, (k, v)) in request.eval_args.iter().enumerate() {
                let mut kv = eval_args_list.reborrow().get(j as u32);
                kv.set_key(k);
                kv.set_value(v);
            }
        }

        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("RPC error: {}", e),
            })?;

        let job_client = res
            .get()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .get_job()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?;

        Ok(BuildHandle {
            client: job_client,
            daemon: self.daemon.clone(),
            job_id: request.plan_digest,
        })
    }

    /// Returns the discovery client interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the discover capability cannot be retrieved.
    pub async fn discover(&self) -> Result<DiscoveryClient, ClientError> {
        let req = self.daemon.discover_request();
        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("Discover RPC failed: {}", e),
            })?;

        let client = res
            .get()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .get_discovery()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?;

        Ok(DiscoveryClient::new(client))
    }
}

/// Convenience method to parse lock content and translate it to `BuildRequest`.
///
/// # Errors
///
/// Returns an error if parsing or translation fails.
pub fn parse_and_translate(lock_content: &str) -> Result<BuildRequest<Blake3Digest>, ClientError> {
    let lock = ion_lock::LockFile::parse(lock_content).map_err(|e| ClientError::ProtocolError {
        detail: format!("Failed to parse lock TOML: {}", e),
    })?;
    lock.validate().map_err(|e| ClientError::ProtocolError {
        detail: format!("Invalid lock file: {}", e),
    })?;

    // 1. Plan digest (Blake3 digest of lock_content)
    let hash = blake3::hash(lock_content.as_bytes());
    let plan_digest = Blake3Digest::from(*hash.as_bytes());

    // 2. Map sets
    let mut sets = std::collections::HashMap::new();
    for (anchor, set_details) in lock.sets {
        sets.insert(
            anchor,
            AtomSetInfo {
                tag: set_details.tag,
                mirrors: set_details.mirrors,
            },
        );
    }

    // 3. Map deps
    let mut deps = Vec::new();
    for dep in lock.deps {
        let fd = match dep {
            ion_lock::Dependency::Atom(d) => FetchDescriptor::Atom(AtomFetchDescriptor {
                id: d.id,
                label: d.label,
                version: d.version,
                set: d.set,
                rev: d.rev,
                requires: d.requires,
                direct: d.direct,
            }),
            ion_lock::Dependency::Nix(d) => FetchDescriptor::Nix(NixFetchDescriptor {
                name: d.name,
                url: d.url,
                hash: d.hash,
                owner: d.owner,
            }),
            ion_lock::Dependency::NixGit(d) => FetchDescriptor::NixGit(NixGitFetchDescriptor {
                name: d.name,
                url: d.url,
                rev: d.rev,
                version: d.version,
                owner: d.owner,
            }),
            ion_lock::Dependency::NixTar(d) => FetchDescriptor::NixTar(NixTarFetchDescriptor {
                name: d.name,
                url: d.url,
                hash: d.hash,
                owner: d.owner,
            }),
            ion_lock::Dependency::NixSrc(d) => FetchDescriptor::NixSrc(NixSrcFetchDescriptor {
                name: d.name,
                url: d.url,
                hash: d.hash,
                owner: d.owner,
            }),
        };
        deps.push(fd);
    }

    // 4. Map eval args (compose args also serve as eval args for historical behavior)
    let mut eval_args = Vec::new();
    for (k, v) in &lock.compose.args {
        eval_args.push((k.clone(), v.clone()));
    }

    // 5. Map composer
    let composer = match lock.compose.r#use {
        Some(ref u) if u == "static" => ComposerSpec::Static,
        Some(ref u) if u == "nix" => {
            let expr = lock
                .compose
                .entry
                .clone()
                .unwrap_or_else(|| "default.nix".to_string());
            ComposerSpec::NixTrivial {
                expression: expr,
                args: lock.compose.args.clone(),
            }
        },
        Some(ref u) => {
            let atom_id = u
                .parse::<AtomId>()
                .map_err(|e| ClientError::ProtocolError {
                    detail: format!("Failed to parse composer atom ID: {}", e),
                })?;
            ComposerSpec::Atom {
                id: atom_id,
                entry: lock.compose.entry,
                args: lock.compose.args,
            }
        },
        None => ComposerSpec::Static,
    };

    Ok(BuildRequest {
        plan_digest,
        sets,
        deps,
        composer,
        eval_args,
    })
}

/// Handle to a submitted build job.
pub struct BuildHandle {
    client: eos_capnp::build_job::Client,
    daemon: eos_capnp::eos_daemon::Client,
    job_id: Blake3Digest,
}

impl BuildHandle {
    /// Returns the unique Job ID.
    #[must_use]
    pub fn job_id(&self) -> &Blake3Digest {
        &self.job_id
    }

    /// Queries the current status of the build job.
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon status query RPC fails.
    pub async fn status(&self) -> Result<JobStatus<Blake3Digest>, ClientError> {
        let mut req = self.daemon.query_status_request();
        req.get().set_job_id(self.job_id.as_bytes());

        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("QueryStatus RPC failed: {}", e),
            })?;

        let status_reader = res
            .get()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .get_status()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?;

        parse_job_status(status_reader)
    }

    /// Cancels the build job.
    ///
    /// # Errors
    ///
    /// Returns an error if the cancel RPC fails.
    pub async fn cancel(&self) -> Result<(), ClientError> {
        let req = self.client.cancel_request();
        req.send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("Cancel RPC failed: {}", e),
            })?;
        Ok(())
    }

    /// Attaches to the progress stream of the build job.
    ///
    /// # Errors
    ///
    /// Returns an error if the attach progress RPC fails.
    pub async fn attach_progress(&self) -> Result<ProgressStream, ClientError> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let progress_server = ProgressStreamImpl {
            sender: tx,
            job_id: eos_core::job::JobId(self.job_id),
        };
        let progress_client: eos_capnp::progress_stream::Client =
            capnp_rpc::new_client(progress_server);

        let mut req = self.client.attach_progress_request();
        req.get().set_callback(progress_client);

        req.send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("AttachProgress RPC failed: {}", e),
            })?;

        Ok(ProgressStream { receiver: rx })
    }

    /// Returns a list of AtomIds that the daemon could not resolve from its local store or mirrors.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn get_missing(&self) -> Result<Vec<AtomId>, ClientError> {
        let req = self.client.get_missing_request();
        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: format!("getMissing RPC failed: {}", e),
            })?;

        let res_reader = res.get().map_err(|e| ClientError::ProtocolError {
            detail: e.to_string(),
        })?;

        let missing_list =
            res_reader
                .get_missing_atoms()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?;

        let mut missing = Vec::new();
        for i in 0..missing_list.len() {
            let atom_id_reader = missing_list.get(i);
            let digest_bytes =
                atom_id_reader
                    .get_digest()
                    .map_err(|e| ClientError::ProtocolError {
                        detail: e.to_string(),
                    })?;
            let s = std::str::from_utf8(digest_bytes).map_err(|e| ClientError::ProtocolError {
                detail: format!("Invalid UTF-8 in AtomId from getMissing: {}", e),
            })?;
            let atom_id = s
                .parse::<AtomId>()
                .map_err(|e| ClientError::ProtocolError {
                    detail: format!("Failed to parse AtomId '{}' from getMissing: {}", s, e),
                })?;
            missing.push(atom_id);
        }

        Ok(missing)
    }
}

/// An asynchronous stream of progress events for a build job.
pub struct ProgressStream {
    receiver: tokio::sync::mpsc::UnboundedReceiver<ProgressEvent<Blake3Digest>>,
}

impl ProgressStream {
    /// Returns the next progress event, or `None` if the stream has finished.
    pub async fn next(&mut self) -> Option<ProgressEvent<Blake3Digest>> {
        self.receiver.recv().await
    }
}

struct ProgressStreamImpl {
    sender: tokio::sync::mpsc::UnboundedSender<ProgressEvent<Blake3Digest>>,
    job_id: eos_core::job::JobId<Blake3Digest>,
}

impl eos_capnp::progress_stream::Server for ProgressStreamImpl {
    fn update(
        &mut self,
        params: eos_capnp::progress_stream::UpdateParams,
    ) -> Promise<(), capnp::Error> {
        let status_reader = match params.get().and_then(|p| p.get_status()) {
            Ok(s) => s,
            Err(e) => return Promise::err(e),
        };

        let status = match parse_job_status(status_reader) {
            Ok(s) => s,
            Err(e) => return Promise::err(capnp::Error::failed(e.to_string())),
        };

        let event = ProgressEvent {
            job_id: self.job_id,
            timestamp: std::time::SystemTime::now(),
            status,
            log_line: None,
        };

        let _ = self.sender.send(event);
        Promise::ok(())
    }

    fn done(
        &mut self,
        _params: eos_capnp::progress_stream::DoneParams,
        _results: eos_capnp::progress_stream::DoneResults,
    ) -> Promise<(), capnp::Error> {
        Promise::ok(())
    }
}

fn parse_job_status(
    reader: eos_capnp::build_status::Reader,
) -> Result<JobStatus<Blake3Digest>, ClientError> {
    use eos_capnp::build_status::Which;
    match reader.which().map_err(|e| ClientError::ProtocolError {
        detail: e.to_string(),
    })? {
        Which::Queued(()) => Ok(JobStatus::Queued),
        Which::Evaluating(msg) => {
            let s = msg
                .get_message()?
                .to_str()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?
                .to_string();
            Ok(JobStatus::Evaluating { message: s })
        },
        Which::Building(group) => {
            let phase = group
                .get_phase()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?
                .to_str()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?
                .to_string();
            let progress = group.get_progress();
            let p = if progress > 0.0 { Some(progress) } else { None };
            Ok(JobStatus::Building { phase, progress: p })
        },
        Which::Completed(group) => {
            let output_paths_reader =
                group
                    .get_output_paths()
                    .map_err(|e| ClientError::ProtocolError {
                        detail: e.to_string(),
                    })?;
            let mut outputs = Vec::new();
            for i in 0..output_paths_reader.len() {
                let path_str = output_paths_reader
                    .get(i)
                    .map_err(|e| ClientError::ProtocolError {
                        detail: e.to_string(),
                    })?
                    .to_str()
                    .map_err(|e| ClientError::ProtocolError {
                        detail: e.to_string(),
                    })?;

                let store_path = eos_core::store::StorePath(path_str.to_string());
                outputs.push(eos_core::job::ArtifactInfo {
                    digest: Blake3Digest::from([0u8; 32]),
                    store_path,
                    size: 0,
                    references: vec![],
                    deriver: None,
                });
            }
            Ok(JobStatus::Completed { outputs })
        },
        Which::Failed(group) => {
            let error = group
                .get_error()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?
                .to_str()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?
                .to_string();
            let exit_code = group.get_exit_code();
            let ec = if exit_code == -1 {
                None
            } else {
                Some(exit_code)
            };
            Ok(JobStatus::Failed {
                error,
                exit_code: ec,
            })
        },
        Which::Cancelled(()) => Ok(JobStatus::Cancelled),
    }
}
