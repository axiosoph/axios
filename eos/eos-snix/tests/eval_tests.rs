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
