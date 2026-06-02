//! Snix backend bridge for Eos.
//!
//! Provides the [`BuildEngine`] and [`ArtifactStore`] implementation via Snix.

pub mod build;
pub mod convert;
pub mod error;
pub mod eval;
pub mod sandbox;
pub mod store;

use std::path::PathBuf;
use std::sync::Arc;

use eos_core::digest::Blake3Digest;
use eos_core::engine::BuildEngine;
use eos_core::eval::EvalRequest;
use nix_compat::derivation::Derivation;
pub use sandbox::{SandboxConfig, select_sandbox};
use snix_build::buildservice::BuildService;
use snix_castore::Node;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};
pub use store::SnixStore;
use tokio::runtime::Handle;

use crate::error::SnixError;
use crate::eval::evaluate_on_thread;

/// Output representation of a Snix build, containing path metadata and its root node.
#[derive(Clone, Debug, PartialEq)]
pub struct SnixOutput {
    /// Nix path info carrying dependencies and metadata.
    pub path_info: PathInfo,
    /// Root node in the content-addressed blob/directory store.
    pub node: Node,
}

/// Configuration for out-of-process sandboxed evaluation.
///
/// When present on [`SnixEngine`], evaluations are dispatched to a
/// restricted subprocess using platform-native containerization
/// (Bubblewrap on Linux, Birdcage on macOS). When absent, evaluations
/// run in-process on a dedicated OS thread.
#[derive(Clone, Debug)]
pub struct SandboxedEvalConfig {
    /// Override path to the worker binary. Falls back to
    /// `std::env::current_exe()` if `None`. Settable via
    /// the `EOS_EVAL_WORKER_BIN` environment variable.
    pub worker_bin: Option<PathBuf>,
    /// Address for the blob service (forwarded to the worker).
    pub blob_service_addr: String,
    /// Address for the directory service (forwarded to the worker).
    pub directory_service_addr: String,
    /// Address for the path info service (forwarded to the worker).
    pub path_info_service_addr: String,
    /// Workspace root mounted read-only inside the sandbox.
    pub workspace_dir: PathBuf,
    /// Temporary directory mounted read-write for evaluation state.
    pub sandbox_workdir: PathBuf,
}

impl SandboxedEvalConfig {
    /// Resolves the worker binary path.
    ///
    /// Checks the `EOS_EVAL_WORKER_BIN` environment variable first,
    /// then falls back to the configured `worker_bin`, and finally to
    /// `std::env::current_exe()`.
    pub fn resolve_worker_bin(&self) -> Result<PathBuf, SnixError> {
        if let Ok(bin_str) = std::env::var("EOS_EVAL_WORKER_BIN") {
            return Ok(PathBuf::from(bin_str));
        }
        if let Some(ref bin) = self.worker_bin {
            return Ok(bin.clone());
        }
        std::env::current_exe().map_err(|e| SnixError::SandboxError {
            platform: "worker_bin",
            source: Box::new(e),
        })
    }
}

/// A Snix-backed implementation of the [`BuildEngine`] trait.
pub struct SnixEngine {
    /// Underlying blob storage service.
    pub blob_service: Arc<dyn BlobService>,
    /// Underlying directory tree service.
    pub directory_service: Arc<dyn DirectoryService>,
    /// Underlying Nix path information metadata service.
    pub path_info_service: Arc<dyn PathInfoService>,
    /// Underlying Nix Archive calculation service.
    pub nar_calculation_service: Arc<dyn NarCalculationService>,
    /// Sandbox execution build service.
    pub build_service: Arc<dyn BuildService>,
    /// Evaluation sandbox configuration.
    ///
    /// `Some` dispatches evaluations to an isolated subprocess.
    /// `None` evaluates in-process on a dedicated OS thread.
    pub eval_sandbox: Option<SandboxedEvalConfig>,
}

impl SnixEngine {
    /// Creates a new `SnixEngine` wrapping the specified services.
    ///
    /// Pass `Some(config)` for `eval_sandbox` to enable out-of-process
    /// sandboxed evaluation, or `None` for in-process evaluation.
    pub fn new(
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
        nar_calculation_service: Arc<dyn NarCalculationService>,
        build_service: Arc<dyn BuildService>,
        eval_sandbox: Option<SandboxedEvalConfig>,
    ) -> Self {
        Self {
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
            build_service,
            eval_sandbox,
        }
    }
}

impl BuildEngine for SnixEngine {
    type Digest = Blake3Digest;
    type Error = SnixError;
    type Output = SnixOutput;
    type Plan = Derivation;

    async fn evaluate(
        &self,
        request: EvalRequest<Self::Digest>,
    ) -> Result<Self::Plan, Self::Error> {
        if let Some(ref sandbox_config) = self.eval_sandbox {
            eval::evaluate_sandboxed(sandbox_config, request).await
        } else {
            let tokio_handle = Handle::current();
            let rx = evaluate_on_thread(
                request.expression,
                request.inputs,
                request.eval_args,
                self.blob_service.clone(),
                self.directory_service.clone(),
                self.path_info_service.clone(),
                self.nar_calculation_service.clone(),
                self.build_service.clone(),
                tokio_handle,
            );
            rx.await.map_err(|_| SnixError::EvalThreadPanic)?
        }
    }

    async fn build(&self, plan: &Self::Plan) -> Result<Self::Output, Self::Error> {
        build::do_engine_build(self, plan).await
    }

    async fn lookup_cached(&self, plan: &Self::Plan) -> Result<Option<Self::Output>, Self::Error> {
        build::do_engine_lookup_cached(self, plan).await
    }

    fn plan_digest(&self, plan: &Self::Plan) -> Self::Digest {
        let bytes = plan.to_aterm_bytes();
        Blake3Digest(blake3::hash(&bytes).into())
    }
}
