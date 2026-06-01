//! Ion Eos Client Library.
//!
//! Exposes connection and RPC wrapper logic for talking to the Eos daemon.

pub mod discovery;
pub mod error;

use std::path::Path;

use capnp::capability::Promise;
use capnp_rpc::rpc_twoparty_capnp::Side;
use eos_core::Digest;
use eos_core::digest::Blake3Digest;
use eos_core::job::{JobStatus, ProgressEvent};
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

    /// Submits a lock file content to be built.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock file cannot be parsed or if the RPC submission fails.
    pub async fn submit_build(&self, lock_content: &str) -> Result<BuildHandle, ClientError> {
        // 1. Compute plan digest
        let hash = blake3::hash(lock_content.as_bytes());
        let digest_bytes = hash.as_bytes();
        let plan_digest = Blake3Digest::from(*digest_bytes);

        // 2. Parse compose args
        let value: toml::Value =
            toml::from_str(lock_content).map_err(|e| ClientError::ProtocolError {
                detail: format!("Failed to parse TOML: {}", e),
            })?;
        let mut eval_args = Vec::new();
        if let Some(compose) = value.get("compose")
            && let Some(args) = compose.get("args")
            && let Some(table) = args.as_table()
        {
            for (k, v) in table {
                if let Some(s) = v.as_str() {
                    eval_args.push((k.clone(), s.to_string()));
                }
            }
        }

        // 3. Invoke submitBuild RPC
        let mut req = self.daemon.submit_build_request();
        {
            let mut params = req.get();
            let mut plan_digest_builder = params.reborrow().init_plan_digest();
            plan_digest_builder.set_bytes(digest_bytes);

            let mut eval_args_list = params.init_eval_args(eval_args.len() as u32);
            for (i, (k, v)) in eval_args.iter().enumerate() {
                let mut kv_builder = eval_args_list.reborrow().get(i as u32);
                kv_builder.set_key(k);
                kv_builder.set_value(v);
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
            job_id: plan_digest,
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
