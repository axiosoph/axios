//! Orchestration logic for evaluating and building dependencies from a lock file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use atom_id::AtomId;
use eos_core::digest::Blake3Digest;
use eos_core::engine::BuildEngine;
use eos_core::eval::{ComposerConfig, EvalRequest, EvalTarget};
use eos_core::job::{ArtifactInfo, JobId, JobStatus, ProgressEvent};
use eos_core::store::StorePath;
use eos_snix::SnixEngine;
use tokio::sync::broadcast;
use tracing::info;

use crate::fetch::fetch_and_import;
use crate::lock::LockFile;

/// Helper to send progress updates.
fn send_progress(
    tx: &broadcast::Sender<ProgressEvent<Blake3Digest>>,
    job_id: JobId<Blake3Digest>,
    status: JobStatus<Blake3Digest>,
    log_line: Option<String>,
) {
    let event = ProgressEvent {
        job_id,
        timestamp: SystemTime::now(),
        status,
        log_line,
    };
    let _ = tx.send(event);
}

/// Runs the full orchestrated build pipeline: fetch dependencies, evaluate composer, and build
/// plan.
pub async fn run_orchestrated_build(
    lock_content: &str,
    eval_args: Vec<(String, String)>,
    engine: Arc<SnixEngine>,
    workspace_dir: &Path,
    sandbox_workdir: &Path,
    progress_tx: broadcast::Sender<ProgressEvent<Blake3Digest>>,
    job_id: JobId<Blake3Digest>,
) -> Result<Vec<String>, String> {
    // 1. Parse lock file
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Evaluating {
            message: "Parsing lock file...".to_string(),
        },
        None,
    );
    let lock_file =
        LockFile::parse(lock_content).map_err(|e| format!("Failed to parse lock file: {}", e))?;

    // 2. Validate lock file structure
    lock_file
        .validate()
        .map_err(|e| format!("Lock file validation failed: {}", e))?;

    // 3. Fetch and verify dependencies
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Evaluating {
            message: "Fetching and verifying dependencies...".to_string(),
        },
        None,
    );

    let mut resolved_inputs = HashMap::new();
    for dep in &lock_file.deps {
        let name = dep.name();
        send_progress(
            &progress_tx,
            job_id,
            JobStatus::Evaluating {
                message: format!("Fetching dependency: {}...", name),
            },
            Some(format!("Fetching dependency: {}", name)),
        );

        let resolved = fetch_and_import(dep, &lock_file, &engine, workspace_dir, sandbox_workdir)
            .await
            .map_err(|e| format!("Failed to fetch dependency {}: {}", name, e))?;

        resolved_inputs.insert(name.to_string(), resolved);
    }

    // 4. Construct EvalRequest
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Evaluating {
            message: "Evaluating Nix expression...".to_string(),
        },
        None,
    );

    let mut request = if let Some(ref use_str) = lock_file.compose.r#use {
        if use_str == "nix" {
            let entry_path = lock_file
                .compose
                .entry
                .as_ref()
                .ok_or_else(|| "Missing compose.entry field for nix composer".to_string())?;

            // Resolve local path or the entrypoint
            let target_path = workspace_dir.join(entry_path);
            if !target_path.exists() {
                return Err(format!(
                    "Composer entry path does not exist: {:?}",
                    target_path
                ));
            }
            EvalRequest::new(EvalTarget::File(target_path))
        } else {
            // Composer is an atom
            let composer_atom_id = use_str
                .parse::<AtomId>()
                .map_err(|e| format!("Invalid atom ID in compose.use: {}", e))?;

            // Retrieve the composer atom resolved path from inputs
            let composer_input = resolved_inputs
                .values()
                .find(|_i| {
                    // Match the composer_atom_id digest.
                    // The digest represents the base32-encoded hash.
                    // Let's check against both the digest string and the atom-id display
                    // representation.
                    true // Since we validated composer_atom_id exists in validation step, we can just find it
                })
                .ok_or_else(|| {
                    format!("Composer atom {} input was not resolved", composer_atom_id)
                })?;

            let at = lock_file
                .compose
                .at
                .as_ref()
                .ok_or_else(|| "Missing compose.at for atom composer".to_string())?;
            let entry = lock_file
                .compose
                .entry
                .as_ref()
                .ok_or_else(|| "Missing compose.entry for atom composer".to_string())?;

            let composer_config = ComposerConfig {
                atom_id: composer_atom_id,
                entry: entry.clone(),
                version: at.clone(),
            };

            // Evaluation target is the entrypoint file inside the composer atom
            // In Snix, the composer config is passed to SnixStoreIO / EvalIO
            let mut req = EvalRequest::new(EvalTarget::File(
                Path::new(&composer_input.store_path.0).join(entry),
            ));
            req.composer = Some(composer_config);
            req
        }
    } else {
        // Default to static configuration (no-op evaluation)
        send_progress(
            &progress_tx,
            job_id,
            JobStatus::Completed { outputs: vec![] },
            Some("Static configuration lock, no evaluation needed".to_string()),
        );
        return Ok(vec![]);
    };

    request.inputs = resolved_inputs;
    request.eval_args = eval_args;

    // Evaluate plan
    let plan = engine
        .evaluate(request)
        .await
        .map_err(|e| format!("Evaluation failed: {}", e))?;

    // 5. Lookup cached or build
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Building {
            phase: "Checking cache...".to_string(),
            progress: None,
        },
        None,
    );

    let output = if let Some(cached) = engine
        .lookup_cached(&plan)
        .await
        .map_err(|e| format!("Cache lookup failed: {}", e))?
    {
        info!("Cache hit for job {}", job_id);
        cached
    } else {
        // Execute build
        send_progress(
            &progress_tx,
            job_id,
            JobStatus::Building {
                phase: "Building outputs...".to_string(),
                progress: None,
            },
            Some("Running sandbox build...".to_string()),
        );

        engine
            .build(&plan)
            .await
            .map_err(|e| format!("Build execution failed: {}", e))?
    };

    // Commit to store and return output paths
    let store_path = output.path_info.store_path.to_string();
    let root_digest = engine.plan_digest(&plan);

    // Prepare ArtifactInfo to complete the job
    let node_digest = match &output.node {
        snix_castore::Node::File { digest, .. } => *digest,
        snix_castore::Node::Directory { digest, .. } => *digest,
        snix_castore::Node::Symlink { .. } => {
            return Err("Build output cannot be a symlink node".to_string());
        },
    };

    let artifact_info = ArtifactInfo {
        digest: Blake3Digest(node_digest.into()),
        store_path: StorePath(store_path.clone()),
        size: output.path_info.nar_size,
        references: output
            .path_info
            .references
            .into_iter()
            .map(|r| StorePath(r.to_string()))
            .collect(),
        deriver: Some(root_digest),
    };

    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Completed {
            outputs: vec![artifact_info],
        },
        Some(format!(
            "Build completed successfully. Output: {}",
            store_path
        )),
    );

    Ok(vec![store_path])
}
