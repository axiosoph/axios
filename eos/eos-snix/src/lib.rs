//! Snix backend bridge for Eos.
//!
//! Provides the [`BuildEngine`] and [`ArtifactStore`] implementation via Snix.

pub mod build;
pub mod convert;
pub mod error;
pub mod eval;
pub mod sandbox;
pub mod store;

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

/// A Snix-backed implementation of the [`BuildEngine`] trait.
pub struct SnixEngine {
    /// Underlying blob storage service.
    pub blob_service: Arc<dyn BlobService>,
    /// Underlying directory directory tree service.
    pub directory_service: Arc<dyn DirectoryService>,
    /// Underlying Nix path information metadata service.
    pub path_info_service: Arc<dyn PathInfoService>,
    /// Underlying Nix Archive calculation service.
    pub nar_calculation_service: Arc<dyn NarCalculationService>,
    /// Sandbox execution build service.
    pub build_service: Arc<dyn BuildService>,
    /// Address for the blob service.
    pub blob_service_addr: String,
    /// Address for the directory service.
    pub directory_service_addr: String,
    /// Address for the path info service.
    pub path_info_service_addr: String,
    /// Working directory for local workspace.
    pub workspace_dir: std::path::PathBuf,
    /// Sandbox working directory.
    pub sandbox_workdir: std::path::PathBuf,
    /// Whether evaluation sandboxing is enabled.
    pub enable_eval_sandbox: bool,
}

impl SnixEngine {
    /// Creates a new `SnixEngine` wrapping the specified services.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
        nar_calculation_service: Arc<dyn NarCalculationService>,
        build_service: Arc<dyn BuildService>,
        blob_service_addr: String,
        directory_service_addr: String,
        path_info_service_addr: String,
        workspace_dir: std::path::PathBuf,
        sandbox_workdir: std::path::PathBuf,
        enable_eval_sandbox: bool,
    ) -> Self {
        Self {
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
            build_service,
            blob_service_addr,
            directory_service_addr,
            path_info_service_addr,
            workspace_dir,
            sandbox_workdir,
            enable_eval_sandbox,
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
        let is_daemon = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .map(|name| name.starts_with("eosd"))
            .unwrap_or(false);

        if self.enable_eval_sandbox && is_daemon {
            eval::evaluate_sandboxed(self, request).await
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
