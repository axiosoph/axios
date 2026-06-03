//! Orchestration logic for evaluating and building dependencies from a BuildRequest.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use eos_core::digest::Blake3Digest;
use eos_core::engine::BuildEngine;
use eos_core::eval::{ComposerConfig, EvalRequest, EvalTarget};
use eos_core::job::{JobId, JobStatus, ProgressEvent};
use tokio::sync::broadcast;

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

/// Runs the orchestrated build pipeline: fetch dependencies, evaluate composer, and build plan.
pub async fn run_orchestrated_build<
    E: BuildEngine<Digest = Blake3Digest>,
    S: atom_core::AtomSource,
    B: eos_core::bridge::AtomContentBridge<Digest = Blake3Digest>,
>(
    request: &eos_core::request::BuildRequest<Blake3Digest>,
    source: &S,
    bridge: &B,
    engine: Arc<E>,
    workspace_dir: &Path,
    sandbox_workdir: &Path,
    progress_tx: broadcast::Sender<ProgressEvent<Blake3Digest>>,
    job_id: JobId<Blake3Digest>,
) -> Result<Vec<String>, String> {
    // 1. Fetch and verify dependencies concurrently
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Evaluating {
            message: "Fetching and verifying dependencies...".to_string(),
        },
        None,
    );

    // The engine downcast is needed only for non-atom deps (URL-based fetches
    // that require snix-specific import_path_as_nar_ca). Atom deps use the
    // bridge, which was constructed with the concrete backend at the wiring site.
    let snix_engine = engine
        .as_any()
        .downcast_ref::<eos_snix::SnixEngine>()
        .ok_or_else(|| "Engine must be a SnixEngine".to_string())?;

    let mut futures = Vec::new();
    for dep in &request.deps {
        let name = dep.name().to_string();
        let dep_clone = dep.clone();
        let sandbox_workdir_buf = sandbox_workdir.to_path_buf();
        let progress_tx_clone = progress_tx.clone();

        let fut = async move {
            send_progress(
                &progress_tx_clone,
                job_id,
                JobStatus::Evaluating {
                    message: format!("Fetching dependency: {}...", name),
                },
                Some(format!("Fetching dependency: {}", name)),
            );

            let resolved = match dep_clone {
                eos_core::request::FetchDescriptor::Atom(atom_desc) => {
                    crate::fetch::fetch_atom(&atom_desc, source, bridge).await
                },
                other => {
                    crate::fetch::fetch_external(&other, snix_engine, &sandbox_workdir_buf).await
                },
            };

            resolved.map(|r| (name, r))
        };
        futures.push(fut);
    }

    let results = futures::future::join_all(futures).await;
    let mut resolved_inputs = HashMap::new();
    for res in results {
        let (name, resolved) = res.map_err(|e| format!("Failed to fetch dependency: {}", e))?;
        resolved_inputs.insert(name, resolved);
    }

    // 2. Construct EvalRequest
    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Evaluating {
            message: "Evaluating Nix expression...".to_string(),
        },
        None,
    );

    let mut eval_request = match &request.composer {
        eos_core::request::ComposerSpec::NixTrivial { expression, .. } => {
            // Resolve local path or the entrypoint
            let target_path = workspace_dir.join(expression);
            if !target_path.exists() {
                return Err(format!(
                    "Composer entry path does not exist: {:?}",
                    target_path
                ));
            }
            EvalRequest::new(EvalTarget::File(target_path))
        },
        eos_core::request::ComposerSpec::Atom { id, entry, .. } => {
            // Find the atom fetch descriptor
            let atom_dep = request
                .deps
                .iter()
                .filter_map(|dep| {
                    if let eos_core::request::FetchDescriptor::Atom(atom_dep) = dep
                        && atom_dep.id == *id
                    {
                        Some(atom_dep)
                    } else {
                        None
                    }
                })
                .next()
                .ok_or_else(|| format!("Composer atom {} not found in request deps", id))?;

            let composer_input = resolved_inputs.get(&atom_dep.label).ok_or_else(|| {
                format!(
                    "Composer atom {} (label '{}') was not resolved",
                    id, atom_dep.label
                )
            })?;

            let entry_file = entry
                .as_ref()
                .ok_or_else(|| "Missing entry file for atom composer".to_string())?;

            let composer_config = ComposerConfig {
                atom_id: id.clone(),
                entry: entry_file.clone(),
                version: atom_dep.version.clone(),
            };

            let mut req = EvalRequest::new(EvalTarget::File(
                Path::new(&composer_input.store_path.0).join(entry_file),
            ));
            req.composer = Some(composer_config);
            req
        },
        eos_core::request::ComposerSpec::Static => {
            send_progress(
                &progress_tx,
                job_id,
                JobStatus::Completed { outputs: vec![] },
                Some("Static configuration, no evaluation needed".to_string()),
            );
            return Ok(vec![]);
        },
    };

    eval_request.inputs = resolved_inputs;
    eval_request.eval_args = request.eval_args.clone();

    // 3. Lookup cached or build
    let build_plan = engine
        .plan(eval_request.clone())
        .await
        .map_err(|e| format!("Planning failed: {}", e))?;

    let (plan, output) = match build_plan {
        eos_core::engine::BuildPlan::Cached(ref _paths) => {
            let plan = engine
                .evaluate(eval_request)
                .await
                .map_err(|e| format!("Evaluation failed: {}", e))?;
            let output = engine
                .lookup_cached(&plan)
                .await
                .map_err(|e| format!("Cache lookup failed: {}", e))?
                .ok_or_else(|| "Cache lookup failed after positive plan lookup".to_string())?;
            (plan, output)
        },
        eos_core::engine::BuildPlan::NeedsBuild(plan) => {
            send_progress(
                &progress_tx,
                job_id,
                JobStatus::Building {
                    phase: "Building outputs...".to_string(),
                    progress: None,
                },
                Some("Running sandbox build...".to_string()),
            );
            let output = engine
                .build(&plan)
                .await
                .map_err(|e| format!("Build execution failed: {}", e))?;
            (plan, output)
        },
        eos_core::engine::BuildPlan::NeedsEvaluation(_atom_ref) => {
            let plan = engine
                .evaluate(eval_request)
                .await
                .map_err(|e| format!("Evaluation failed: {}", e))?;

            if let Some(cached) = engine
                .lookup_cached(&plan)
                .await
                .map_err(|e| format!("Cache lookup failed: {}", e))?
            {
                (plan, cached)
            } else {
                send_progress(
                    &progress_tx,
                    job_id,
                    JobStatus::Building {
                        phase: "Building outputs...".to_string(),
                        progress: None,
                    },
                    Some("Running sandbox build...".to_string()),
                );
                let output = engine
                    .build(&plan)
                    .await
                    .map_err(|e| format!("Build execution failed: {}", e))?;
                (plan, output)
            }
        },
    };

    // Extract artifact metadata
    let artifacts = engine.output_artifacts(&output, &plan);
    let store_paths: Vec<String> = artifacts.iter().map(|a| a.store_path.0.clone()).collect();

    send_progress(
        &progress_tx,
        job_id,
        JobStatus::Completed { outputs: artifacts },
        Some(format!(
            "Build completed successfully. Outputs: {:?}",
            store_paths
        )),
    );

    Ok(store_paths)
}
