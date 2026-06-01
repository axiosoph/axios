//! Compilation and type-satisfiability tests for `eos-core` traits.

use std::convert::Infallible;

use atom_id::{Anchor, AtomId, Label};
use eos_core::{
    Blake3Digest, BuildEngine, ComposerConfig, EvalRequest, EvalTarget, JobId, ResolvedInput,
    StorePath,
};

/// A compile-time assertion that a type implements a trait.
fn assert_impl<T: ?Sized + Send + Sync>() {}

#[test]
fn test_trait_bounds_satisfiable() {
    // Assert that core traits can be implemented by types that are Send + Sync
    struct MockEngine;
    impl BuildEngine for MockEngine {
        type Digest = Blake3Digest;
        type Error = Infallible;
        type Output = Vec<StorePath>;
        type Plan = String;

        async fn evaluate(
            &self,
            _request: EvalRequest<Self::Digest>,
        ) -> Result<Self::Plan, Self::Error> {
            Ok(String::new())
        }

        async fn build(&self, _plan: &Self::Plan) -> Result<Self::Output, Self::Error> {
            Ok(Vec::new())
        }

        async fn lookup_cached(
            &self,
            _plan: &Self::Plan,
        ) -> Result<Option<Self::Output>, Self::Error> {
            Ok(None)
        }

        fn plan_digest(&self, _plan: &Self::Plan) -> Self::Digest {
            Blake3Digest([0; 32])
        }
    }

    assert_impl::<MockEngine>();
}

#[test]
fn test_eval_request_construction() {
    let target = EvalTarget::Expression("builtins.currentSystem".to_string());
    let mut request = EvalRequest::new(target);

    // Populate inputs
    let input_digest = Blake3Digest([1; 32]);
    let input_path = StorePath("/nix/store/11111111111111111111111111111111-hello".to_string());
    let resolved = ResolvedInput {
        digest: input_digest,
        store_path: input_path,
    };
    request.inputs.insert("hello".to_string(), resolved);

    // Populate composer
    let anchor_bytes = [0u8; 20];
    let anchor = Anchor::new(anchor_bytes.to_vec());
    let label = Label::try_from("composer").unwrap();
    let atom_id = AtomId::new(anchor, label);

    request.composer = Some(ComposerConfig {
        atom_id,
        entry: "default.nix".to_string(),
        version: "1.0.0".to_string(),
    });

    // Populate eval args
    request
        .eval_args
        .push(("system".to_string(), "x86_64-linux".to_string()));

    assert_eq!(request.eval_args[0].1, "x86_64-linux");
}

#[test]
fn test_job_id_and_status() {
    let digest = Blake3Digest([3; 32]);
    let job_id = JobId(digest);
    assert_eq!(
        job_id.to_string(),
        "0303030303030303030303030303030303030303030303030303030303030303"
    );
}
