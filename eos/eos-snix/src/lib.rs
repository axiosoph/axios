//! Snix backend bridge for Eos.
//!
//! Provides the [`BuildEngine`] and [`ArtifactStore`] implementation via Snix.

pub mod convert;
pub mod error;
pub mod eval;

use std::sync::Arc;

use eos_core::digest::Blake3Digest;
use eos_core::engine::BuildEngine;
use eos_core::eval::EvalRequest;
use nix_compat::derivation::Derivation;
use snix_build::buildservice::BuildService;
use snix_castore::Node;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};
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
}

impl SnixEngine {
    /// Creates a new `SnixEngine` wrapping the specified services.
    pub fn new(
        blob_service: Arc<dyn BlobService>,
        directory_service: Arc<dyn DirectoryService>,
        path_info_service: Arc<dyn PathInfoService>,
        nar_calculation_service: Arc<dyn NarCalculationService>,
        build_service: Arc<dyn BuildService>,
    ) -> Self {
        Self {
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
            build_service,
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

    async fn build(&self, _plan: &Self::Plan) -> Result<Self::Output, Self::Error> {
        Err(SnixError::SandboxError {
            platform: "unimplemented",
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "build is not yet implemented",
            )),
        })
    }

    async fn lookup_cached(&self, _plan: &Self::Plan) -> Result<Option<Self::Output>, Self::Error> {
        Ok(None)
    }

    fn plan_digest(&self, plan: &Self::Plan) -> Self::Digest {
        let bytes = plan.to_aterm_bytes();
        Blake3Digest(blake3::hash(&bytes).into())
    }
}
