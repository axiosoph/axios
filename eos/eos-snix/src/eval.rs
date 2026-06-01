//! Dedicated OS thread evaluation bridge for the Snix evaluator.
//!
//! The Snix evaluator is `!Send` because it relies internally on `Rc` pointers
//! (e.g., `Rc<Closure>`). This module isolates the evaluator on a dedicated thread,
//! executing evaluation synchronously and returning only `Send` data (the final
//! `Derivation`) to the async runtime.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

use bstr::ByteSlice;
use eos_core::digest::Blake3Digest;
use eos_core::eval::{EvalTarget, ResolvedInput};
use nix_compat::derivation::Derivation;
use nix_compat::store_path::StorePath as NixStorePath;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use snix_build::buildservice::BuildService;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_eval::{EvalIO, EvalMode, Evaluation, NixString, Value};
use snix_glue::builtins::{add_derivation_builtins, add_fetcher_builtins, add_import_builtins};
use snix_glue::configure_nix_path;
use snix_glue::snix_io::SnixIO;
use snix_glue::snix_store_io::SnixStoreIO;
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::PathInfoService;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

use crate::error::SnixError;

/// Helper function to recursively extract a string from a potentially thunk-wrapped `Value`.
fn extract_string(val: &Value) -> Result<String, SnixError> {
    match val {
        Value::String(s) => {
            s.as_str()
                .map(|r| r.to_owned())
                .map_err(|e| SnixError::ConversionError {
                    from: "NixString",
                    to: "str",
                    detail: e.to_string(),
                })
        },
        Value::Thunk(t) => {
            let inner = t.value();
            extract_string(&inner)
        },
        other => Err(SnixError::ConversionError {
            from: "Value",
            to: "String",
            detail: format!("expected a String or Thunk, got {:?}", other),
        }),
    }
}

/// Spawns a dedicated OS thread to evaluate the Nix expression/file synchronously.
///
/// Returns a oneshot receiver containing the evaluated [`Derivation`] on success,
/// or a [`SnixError`] on failure.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_on_thread(
    expression: EvalTarget,
    inputs: HashMap<String, ResolvedInput<Blake3Digest>>,
    eval_args: Vec<(String, String)>,
    blob_service: Arc<dyn BlobService>,
    directory_service: Arc<dyn DirectoryService>,
    path_info_service: Arc<dyn PathInfoService>,
    nar_calculation_service: Arc<dyn NarCalculationService>,
    build_service: Arc<dyn BuildService>,
    tokio_handle: Handle,
) -> oneshot::Receiver<Result<Derivation, SnixError>> {
    let (tx, rx) = oneshot::channel();

    thread::spawn(move || {
        // Construct SnixStoreIO (requires Rc for the evaluator builtins)
        let store_io = Rc::new(SnixStoreIO::new(
            blob_service,
            directory_service,
            path_info_service,
            nar_calculation_service,
            build_service,
            tokio_handle.clone(),
            Vec::new(), // hashed_mirrors
        ));

        // Wrap with SnixIO for <nix/fetchurl.nix> imports
        let eval_io =
            Box::new(SnixIO::new(Rc::clone(&store_io) as Rc<dyn EvalIO>)) as Box<dyn EvalIO>;

        // Build the evaluation environment map (env)
        let mut env_map = FxHashMap::default();
        for (name, input) in inputs {
            let nix_str = NixString::from(input.store_path.0.as_str());
            env_map.insert(SmolStr::new(name), Value::String(nix_str));
        }
        for (name, val) in eval_args {
            let nix_str = NixString::from(val.as_str());
            env_map.insert(SmolStr::new(name), Value::String(nix_str));
        }

        // Create the EvaluationBuilder
        let mut eval_builder = Evaluation::builder(eval_io)
            .enable_import()
            .mode(EvalMode::Strict)
            .env(Some(&env_map));

        eval_builder = add_derivation_builtins(eval_builder, Rc::clone(&store_io));
        eval_builder = add_fetcher_builtins(eval_builder, Rc::clone(&store_io));
        eval_builder = add_import_builtins(eval_builder, Rc::clone(&store_io));
        eval_builder = configure_nix_path(eval_builder, &None);

        let eval = eval_builder.build();

        // Perform the evaluation
        let (code, path) = match &expression {
            EvalTarget::File(p) => match std::fs::read_to_string(p) {
                Ok(c) => (c, Some(p.clone())),
                Err(e) => {
                    let _ = tx.send(Err(SnixError::EvalFailed {
                        expression: p.display().to_string(),
                        source: Box::new(e),
                    }));
                    return;
                },
            },
            EvalTarget::Expression(expr) => (expr.clone(), None),
        };

        let result = eval.evaluate(&code, path);

        if !result.errors.is_empty() {
            // Fancy format errors
            let err_msg = result
                .errors
                .iter()
                .map(|e| e.fancy_format_str())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = tx.send(Err(SnixError::EvalFailed {
                expression: match &expression {
                    EvalTarget::File(p) => p.display().to_string(),
                    EvalTarget::Expression(expr) => expr.clone(),
                },
                source: Box::new(std::io::Error::other(err_msg)),
            }));
            return;
        }

        // Extract the Derivation from evaluation result
        let Some(val) = result.value else {
            let _ = tx.send(Err(SnixError::EvalFailed {
                expression: match &expression {
                    EvalTarget::File(p) => p.display().to_string(),
                    EvalTarget::Expression(expr) => expr.clone(),
                },
                source: Box::new(std::io::Error::other("evaluation returned no value")),
            }));
            return;
        };

        // The returned value can be either:
        // 1. A string representing the store path to the derivation
        // 2. An attribute set (derivation representation) containing `drvPath`
        let drv_path_str = match &val {
            Value::Attrs(attrs) => {
                if let Some(drv_path_val) = attrs.select(b"drvPath".as_bstr()) {
                    match extract_string(drv_path_val) {
                        Ok(s) => s,
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return;
                        },
                    }
                } else {
                    let _ = tx.send(Err(SnixError::ConversionError {
                        from: "NixAttrs",
                        to: "StorePath",
                        detail: "attribute set has no drvPath attribute".to_string(),
                    }));
                    return;
                }
            },
            _ => match extract_string(&val) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                },
            },
        };

        // Parse drv_path_str as absolute store path
        let nix_store_path = match NixStorePath::from_absolute_path(drv_path_str.as_bytes()) {
            Ok(sp) => sp,
            Err(e) => {
                let _ = tx.send(Err(SnixError::ConversionError {
                    from: "str",
                    to: "nix_compat::store_path::StorePath",
                    detail: e.to_string(),
                }));
                return;
            },
        };

        // Retrieve Derivation from known_paths
        let known_paths_ref = store_io.known_paths.borrow();
        let Some(derivation) = known_paths_ref.get_drv_by_drvpath(&nix_store_path) else {
            let _ = tx.send(Err(SnixError::EvalFailed {
                expression: drv_path_str,
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "derivation not found in known paths map",
                )),
            }));
            return;
        };

        let _ = tx.send(Ok(derivation.clone()));
    });

    rx
}
