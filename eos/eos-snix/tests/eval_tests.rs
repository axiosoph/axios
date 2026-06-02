use std::sync::Arc;

use clap::Parser;
use eos_core::engine::BuildEngine;
use eos_core::eval::{EvalRequest, EvalTarget};
use eos_snix::SnixEngine;
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

    // 2. Instantiate the SnixEngine
    let engine = SnixEngine::new(
        blob_service,
        directory_service,
        path_info_service,
        nar_calculation_service.into(),
        build_service,
        "memory://".to_string(),
        "memory://".to_string(),
        "memory://".to_string(),
        std::env::current_dir().unwrap(),
        std::env::temp_dir(),
        false, // disable eval sandbox for unit test
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
    // 1. Check if bwrap is installed on the host and we're on Linux
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

    // Determine target/debug/eosd binary path
    let eosd_path = std::env::current_dir().unwrap();
    let paths_to_try = vec![
        eosd_path.join("target/debug/eosd"),
        eosd_path.join("../target/debug/eosd"),
        eosd_path.join("../../target/debug/eosd"),
    ];
    let mut resolved_eosd = None;
    for p in paths_to_try {
        if p.exists() {
            resolved_eosd = Some(p);
            break;
        }
    }

    let Some(eosd_bin) = resolved_eosd else {
        // Skip test if we cannot find compiled eosd binary
        return;
    };

    // Set environment variable so the sandboxed evaluator uses the compiled daemon
    unsafe {
        std::env::set_var("EOS_EVAL_WORKER_BIN", &eosd_bin);
    }

    // 2. Initialize in-memory store services
    let (blob_service, directory_service, path_info_service, nar_calculation_service) =
        construct_services(ServiceUrlsMemory::parse_from(std::iter::empty::<&str>()))
            .await
            .unwrap();

    let build_service = Arc::new(DummyBuildService::default());

    // 3. Instantiate the SnixEngine with eval sandbox enabled
    let engine = SnixEngine::new(
        blob_service,
        directory_service,
        path_info_service,
        nar_calculation_service.into(),
        build_service,
        "memory://".to_string(),
        "memory://".to_string(),
        "memory://".to_string(),
        std::env::current_dir().unwrap(),
        std::env::temp_dir(),
        true, // enable eval sandbox!
    );

    // 4. Construct an EvalRequest targeting a simple derivation expression
    let request = EvalRequest::new(EvalTarget::Expression(
        r#"builtins.derivation {
            name = "hello-sandboxed";
            builder = "/bin/sh";
            system = builtins.currentSystem;
        }"#
        .to_string(),
    ));

    // 5. Run evaluate and assert success
    let plan = engine.evaluate(request).await.unwrap();

    // 6. Verify the generated Derivation properties
    assert_eq!(plan.builder, "/bin/sh");
    assert_eq!(
        plan.environment.get("name").unwrap().as_slice(),
        b"hello-sandboxed"
    );
}
