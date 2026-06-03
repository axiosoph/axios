//! Snix backend bridge for Eos.
//!
//! Provides the [`BuildEngine`] and [`ArtifactStore`] implementation via Snix.

pub mod build;
pub mod convert;
pub mod error;
pub mod eval;
pub mod ingest;
pub mod sandbox;
pub mod store;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use eos_core::digest::Blake3Digest;
use eos_core::engine::BuildEngine;
use eos_core::eval::EvalRequest;
pub use ingest::{SnixIngestError, SnixIngestService};
use nix_compat::derivation::Derivation;
use redb::ReadableDatabase;
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
    /// Evaluation cache database.
    eval_cache: Mutex<Option<Arc<redb::Database>>>,
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
            eval_cache: Mutex::new(None),
        }
    }

    fn get_eval_cache(&self) -> Result<Arc<redb::Database>, SnixError> {
        let mut cache_guard = self
            .eval_cache
            .lock()
            .map_err(|_| SnixError::CacheLockError)?;
        if let Some(ref db) = *cache_guard {
            return Ok(db.clone());
        }

        // Determine DB path
        let db_path = if let Ok(path_str) = std::env::var("EOS_EVAL_CACHE_DB") {
            PathBuf::from(path_str)
        } else if let Some(ref sandbox_config) = self.eval_sandbox {
            sandbox_config.sandbox_workdir.join("eval-cache.redb")
        } else {
            std::env::temp_dir().join("eos-eval-cache.redb")
        };

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SnixError::CacheError(e.to_string()))?;
        }

        let db =
            redb::Database::create(&db_path).map_err(|e| SnixError::CacheError(e.to_string()))?;

        // Initialize the table
        let write_txn = db
            .begin_write()
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(EVAL_CACHE_TABLE)
                .map_err(|e| SnixError::CacheError(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| SnixError::CacheError(e.to_string()))?;

        let arc_db = Arc::new(db);
        *cache_guard = Some(arc_db.clone());
        Ok(arc_db)
    }

    fn get_cached_plan(&self, cache_key: [u8; 32]) -> Result<Option<Derivation>, SnixError> {
        let db = self.get_eval_cache()?;
        let read_txn = db
            .begin_read()
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        let table = read_txn
            .open_table(EVAL_CACHE_TABLE)
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        let result = table
            .get(cache_key)
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        if let Some(val) = result {
            let bytes = val.value();
            let drv =
                Derivation::from_aterm_bytes(bytes).map_err(|e| SnixError::ConversionError {
                    from: "ATerm",
                    to: "Derivation",
                    detail: format!("{:?}", e),
                })?;
            Ok(Some(drv))
        } else {
            Ok(None)
        }
    }

    fn set_cached_plan(&self, cache_key: [u8; 32], plan: &Derivation) -> Result<(), SnixError> {
        let db = self.get_eval_cache()?;
        let write_txn = db
            .begin_write()
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        {
            let mut table = write_txn
                .open_table(EVAL_CACHE_TABLE)
                .map_err(|e| SnixError::CacheError(e.to_string()))?;
            let bytes = plan.to_aterm_bytes();
            table
                .insert(cache_key, bytes.as_slice())
                .map_err(|e| SnixError::CacheError(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| SnixError::CacheError(e.to_string()))?;
        Ok(())
    }
}

const EVAL_CACHE_TABLE: redb::TableDefinition<[u8; 32], &[u8]> =
    redb::TableDefinition::new("eval_cache");

impl BuildEngine for SnixEngine {
    type Digest = Blake3Digest;
    type Error = SnixError;
    type Output = SnixOutput;
    type Plan = Derivation;

    // @spec-compliance[engine-eval]
    // Mechanism: Evaluates the Nix target (File or Expression) to generate a Derivation plan.
    // Verified-By: eos/eos-snix/tests/eval_tests.rs:test_snix_engine_evaluate
    // @spec-compliance[eos-eval-cache-determinism]
    // Mechanism: Computes a deterministic cache key from the EvalRequest and retrieves cached
    // Derivations from redb database. Verified-By:
    // eos/eos-snix/tests/eval_tests.rs:test_snix_engine_evaluate
    async fn evaluate(
        &self,
        request: EvalRequest<Self::Digest>,
    ) -> Result<Self::Plan, Self::Error> {
        let cache_key = eval::compute_eval_cache_key(&request);
        if let Some(cached) = self.get_cached_plan(cache_key)? {
            return Ok(cached);
        }

        let plan = if let Some(ref sandbox_config) = self.eval_sandbox {
            eval::evaluate_sandboxed(sandbox_config, request).await?
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
            rx.await.map_err(|_| SnixError::EvalThreadPanic)??
        };

        self.set_cached_plan(cache_key, &plan)?;
        Ok(plan)
    }

    async fn plan(
        &self,
        request: EvalRequest<Self::Digest>,
    ) -> Result<eos_core::engine::BuildPlan<Self::Digest, Self::Plan>, Self::Error> {
        let cache_key = eval::compute_eval_cache_key(&request);
        if let Some(plan) = self.get_cached_plan(cache_key)? {
            if let Some(output) = self.lookup_cached(&plan).await? {
                let store_path = output.path_info.store_path.to_string();
                return Ok(eos_core::engine::BuildPlan::Cached(vec![
                    eos_core::store::StorePath(store_path),
                ]));
            } else {
                return Ok(eos_core::engine::BuildPlan::NeedsBuild(plan));
            }
        }

        // Cache miss -> NeedsEvaluation
        let id = if let Some(ref composer) = request.composer {
            composer.atom_id.clone()
        } else {
            atom_id::AtomId::new(
                atom_id::Anchor::new(vec![0; 32]),
                atom_id::Label::from_str("composer").unwrap(),
            )
        };
        let digest = if let Some(input) = request.inputs.values().next() {
            input.digest
        } else {
            Blake3Digest([0; 32])
        };

        Ok(eos_core::engine::BuildPlan::NeedsEvaluation(
            eos_core::engine::AtomRef { id, digest },
        ))
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

    fn output_artifacts(
        &self,
        output: &Self::Output,
        plan: &Self::Plan,
    ) -> Vec<eos_core::job::ArtifactInfo<Self::Digest>> {
        let store_path = output.path_info.store_path.to_string();
        let root_digest = self.plan_digest(plan);

        let node_digest = match &output.node {
            snix_castore::Node::File { digest, .. } => (*digest).into(),
            snix_castore::Node::Directory { digest, .. } => (*digest).into(),
            snix_castore::Node::Symlink { .. } => [0u8; 32],
        };

        vec![eos_core::job::ArtifactInfo {
            digest: Blake3Digest(node_digest),
            store_path: eos_core::store::StorePath(store_path),
            size: output.path_info.nar_size,
            references: output
                .path_info
                .references
                .iter()
                .map(|r| eos_core::store::StorePath(r.to_string()))
                .collect(),
            deriver: Some(root_digest),
        }]
    }
}
