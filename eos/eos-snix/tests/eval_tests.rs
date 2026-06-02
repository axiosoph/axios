use std::sync::Arc;

use clap::Parser;
use eos_core::engine::BuildEngine;
use eos_core::eval::{EvalRequest, EvalTarget};
use eos_snix::{SandboxedEvalConfig, SnixEngine};
use snix_build::buildservice::DummyBuildService;
use snix_store::utils::{ServiceUrlsMemory, construct_services};

#[tokio::test]
async fn test_snix_engine_evaluate() {
    // 1. Initialize in-memory store services for testing
    let (blob_service, directory_service, path_info_service, nar_calculation_service) =
        construct_services(ServiceUrlsMemory::parse_from(std::iter::empty::<&str>()))
            .await
            .unwrap();

    let build_service = Arc::new(DummyBuildService::default());

    // 2. Instantiate the SnixEngine without sandbox (in-process eval)
    let engine = SnixEngine::new(
        blob_service,
        directory_service,
        path_info_service,
        nar_calculation_service.into(),
        build_service,
        None, // no sandbox — evaluate in-process
    );

    // 3. Construct an EvalRequest targeting a simple derivation expression
    let request = EvalRequest::new(EvalTarget::Expression(
        r#"builtins.derivation {
            name = "hello";
            builder = "/bin/sh";
            system = builtins.currentSystem;
        }"#
        .to_string(),
    ));

    // 4. Run evaluate and assert success
    let plan = engine.evaluate(request).await.unwrap();

    // 5. Verify the generated Derivation properties
    assert_eq!(plan.builder, "/bin/sh");
    assert_eq!(plan.environment.get("name").unwrap().as_slice(), b"hello");
}

#[tokio::test]
async fn test_snix_engine_evaluate_sandboxed() {
    // 1. Gate on Linux + bwrap availability
    if !cfg!(target_os = "linux") {
        return;
    }
    if std::process::Command::new("bwrap")
        .arg("--version")
        .status()
        .is_err()
    {
        return;
    }

    // 2. Locate the compiled eosd worker binary
    let cwd = std::env::current_dir().unwrap();
    let candidates = [
        cwd.join("target/debug/eosd"),
        cwd.join("../target/debug/eosd"),
        cwd.join("../../target/debug/eosd"),
    ];
    let Some(eosd_bin) = candidates.iter().find(|p| p.exists()).cloned() else {
        // Skip if the eosd binary has not been compiled
        return;
    };

    // 3. Initialize in-memory store services
    let (blob_service, directory_service, path_info_service, nar_calculation_service) =
        construct_services(ServiceUrlsMemory::parse_from(std::iter::empty::<&str>()))
            .await
            .unwrap();

    let build_service = Arc::new(DummyBuildService::default());

    // 4. Construct sandbox config with explicit worker binary path
    let sandbox_config = SandboxedEvalConfig {
        worker_bin: Some(eosd_bin),
        blob_service_addr: "memory://".to_string(),
        directory_service_addr: "memory://".to_string(),
        path_info_service_addr: "memory://".to_string(),
        workspace_dir: std::env::current_dir().unwrap(),
        sandbox_workdir: std::env::temp_dir().join("eos-test-sandbox"),
    };

    // 5. Instantiate the SnixEngine with sandbox enabled
    let engine = SnixEngine::new(
        blob_service,
        directory_service,
        path_info_service,
        nar_calculation_service.into(),
        build_service,
        Some(sandbox_config),
    );

    // 6. Construct an EvalRequest targeting a simple derivation expression
    let request = EvalRequest::new(EvalTarget::Expression(
        r#"builtins.derivation {
            name = "hello-sandboxed";
            builder = "/bin/sh";
            system = builtins.currentSystem;
        }"#
        .to_string(),
    ));

    // 7. Run evaluate — this MUST take the sandboxed subprocess path
    let plan = engine.evaluate(request).await.unwrap();

    // 8. Verify the generated Derivation properties
    assert_eq!(plan.builder, "/bin/sh");
    assert_eq!(
        plan.environment.get("name").unwrap().as_slice(),
        b"hello-sandboxed"
    );
}
