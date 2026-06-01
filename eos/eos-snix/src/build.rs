//! Build execution and store registration for the Snix engine.

use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;

use eos_core::BuildEngine;
use futures::stream::TryStreamExt;
use nix_compat::derivation::Derivation;
use nix_compat::nixhash::CAHash;
use nix_compat::store_path::StorePath as NixStorePath;
use snix_castore::Node;
use snix_castore::blobservice::BlobService;
use snix_glue::known_paths::KnownPaths;
use snix_store::pathinfoservice::{PathInfo, PathInfoService};

use crate::error::SnixError;
use crate::{SnixEngine, SnixOutput};

async fn read_store_path_bytes(
    store_path: &NixStorePath<String>,
    blob_service: &Arc<dyn BlobService>,
    path_info_service: &Arc<dyn PathInfoService>,
) -> std::io::Result<Vec<u8>> {
    let info = path_info_service
        .get(*store_path.digest())
        .await
        .map_err(std::io::Error::other)?
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("PathInfo not found: {}", store_path.to_absolute_path()),
            )
        })?;

    let Node::File { digest, .. } = info.node else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "expected store path to be a file node",
        ));
    };

    let mut reader = blob_service
        .open_read(&digest)
        .await?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "blob not found"))?;

    let mut bytes = Vec::new();
    tokio::io::copy(&mut reader, &mut bytes).await?;
    Ok(bytes)
}

async fn populate_known_paths(
    derivation: &Derivation,
    blob_service: &Arc<dyn BlobService>,
    path_info_service: &Arc<dyn PathInfoService>,
    known_paths: &mut KnownPaths,
) -> Result<(), SnixError> {
    for drv_path in derivation.input_derivations.keys() {
        if known_paths.get_drv_by_drvpath(drv_path).is_some() {
            continue;
        }

        // Read .drv bytes from store
        let bytes = read_store_path_bytes(drv_path, blob_service, path_info_service)
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "read_drv",
                source: Box::new(e),
            })?;

        // Parse derivation
        let input_drv =
            Derivation::from_aterm_bytes(&bytes).map_err(|e| SnixError::ConversionError {
                from: "ATerm",
                to: "Derivation",
                detail: format!("{:?}", e),
            })?;

        // Recurse for the input derivation's own input derivations
        Box::pin(populate_known_paths(
            &input_drv,
            blob_service,
            path_info_service,
            known_paths,
        ))
        .await?;

        // Add to known paths
        known_paths.add_derivation(drv_path.clone(), input_drv);
    }
    Ok(())
}

fn parse_exit_code(err_str: &str) -> Option<i32> {
    if let Some(pos) = err_str.find("exit status ") {
        let suffix = &err_str[pos + 12..];
        if let Some(end) = suffix.find(|c: char| !c.is_numeric()) {
            suffix[..end].parse().ok()
        } else {
            suffix.parse().ok()
        }
    } else if let Some(pos) = err_str.find("exit code ") {
        let suffix = &err_str[pos + 10..];
        if let Some(end) = suffix.find(|c: char| !c.is_numeric()) {
            suffix[..end].parse().ok()
        } else {
            suffix.parse().ok()
        }
    } else {
        None
    }
}

/// Helper function to perform derivation build.
pub async fn do_engine_build(
    engine: &SnixEngine,
    plan: &Derivation,
) -> Result<SnixOutput, SnixError> {
    let mut known_paths = KnownPaths::default();
    populate_known_paths(
        plan,
        &engine.blob_service,
        &engine.path_info_service,
        &mut known_paths,
    )
    .await?;

    let resolved_inputs: BTreeMap<NixStorePath<String>, Node> =
        snix_glue::builder::get_all_inputs(plan, &known_paths, |path| {
            let path_info_service = engine.path_info_service.clone();
            async move {
                path_info_service
                    .get(*path.digest())
                    .await
                    .map_err(std::io::Error::other)
            }
        })
        .try_collect()
        .await
        .map_err(|e| SnixError::StoreError {
            operation: "get_all_inputs",
            source: Box::new(e),
        })?;

    // Synthesize the build request
    let build_request =
        snix_glue::builder::derivation_into_build_request(plan.clone(), &resolved_inputs).map_err(
            |e| SnixError::ConversionError {
                from: "Derivation",
                to: "BuildRequest",
                detail: e.to_string(),
            },
        )?;

    // Assemble needle mapping table
    let mut output_paths: Vec<NixStorePath<String>> =
        Vec::with_capacity(build_request.outputs.len());
    let all_possible_refs: Vec<NixStorePath<String>> = build_request
        .outputs
        .iter()
        .map(|p| {
            let sp = NixStorePath::<String>::from_bytes(
                p.strip_prefix(&nix_compat::store_path::STORE_DIR[1..])
                    .expect("output doesn't have expected store_dir prefix")
                    .as_os_str()
                    .as_bytes(),
            )
            .expect("cannot parse output as StorePath");
            output_paths.push(sp.clone());
            sp
        })
        .chain(resolved_inputs.keys().cloned())
        .collect();

    // Trigger the build
    let build_result = engine
        .build_service
        .do_build(build_request)
        .await
        .map_err(|e| {
            let err_str = e.to_string();
            let exit_code = parse_exit_code(&err_str);
            SnixError::BuildFailed {
                plan_digest: engine.plan_digest(plan),
                exit_code,
                stderr: err_str,
            }
        })?;

    // Calculate deriver store path
    let name_bytes = plan
        .environment
        .get("name")
        .ok_or_else(|| SnixError::ConversionError {
            from: "Derivation",
            to: "drv_path",
            detail: "missing 'name' environment variable in derivation".to_string(),
        })?;
    let name_str = std::str::from_utf8(name_bytes).map_err(|e| SnixError::ConversionError {
        from: "name bytes",
        to: "str",
        detail: e.to_string(),
    })?;
    let drv_path =
        plan.calculate_derivation_path(name_str)
            .map_err(|e| SnixError::ConversionError {
                from: "Derivation",
                to: "drv_path",
                detail: e.to_string(),
            })?;

    let mut main_output = None;
    let mut ca = plan
        .fod_digest()
        .map(|fod_digest| CAHash::Nar(nix_compat::nixhash::NixHash::Sha256(fod_digest)));

    let main_out_path = plan
        .outputs
        .get("out")
        .or_else(|| plan.outputs.values().next())
        .and_then(|output| output.path.as_ref());

    for (output, output_path) in build_result.outputs.into_iter().zip(output_paths) {
        // Calculate NAR representation
        let (nar_size, nar_sha256) = engine
            .nar_calculation_service
            .calculate_nar(&output.node)
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "calculate_nar",
                source: e,
            })?;

        // Assemble path info
        let path_info = PathInfo {
            store_path: output_path.clone(),
            node: output.node.clone(),
            references: {
                let mut references = Vec::with_capacity(output.output_needles.len());
                for needle_idx in output.output_needles {
                    let ref_path = all_possible_refs
                        .get(needle_idx as usize)
                        .ok_or_else(|| SnixError::StoreError {
                            operation: "assemble_path_info",
                            source: Box::new(std::io::Error::other("invalid needle_idx")),
                        })?
                        .clone();
                    references.push(ref_path);
                }
                references.sort();
                references
            },
            nar_size,
            nar_sha256,
            signatures: vec![],
            deriver: Some(
                NixStorePath::from_name_and_digest_fixed(
                    drv_path.name().strip_suffix(".drv").ok_or_else(|| {
                        SnixError::ConversionError {
                            from: "drv_path",
                            to: "deriver",
                            detail: "missing .drv suffix".to_string(),
                        }
                    })?,
                    *drv_path.digest(),
                )
                .map_err(|e| SnixError::ConversionError {
                    from: "drv_path",
                    to: "deriver",
                    detail: e.to_string(),
                })?,
            ),
            ca: ca.take(),
        };

        // Persist PathInfo
        engine
            .path_info_service
            .put(path_info.clone())
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "put_path_info",
                source: e,
            })?;

        // If it matches the main output store path, capture it
        if Some(output_path.as_ref()) == main_out_path.map(|p| p.as_ref()) {
            main_output = Some(SnixOutput {
                path_info,
                node: output.node,
            });
        }
    }

    main_output.ok_or_else(|| SnixError::BuildFailed {
        plan_digest: engine.plan_digest(plan),
        exit_code: None,
        stderr: "build didn't produce the main output store path".to_string(),
    })
}

/// Helper function to perform lookup for cached build outputs.
pub async fn do_engine_lookup_cached(
    engine: &SnixEngine,
    plan: &Derivation,
) -> Result<Option<SnixOutput>, SnixError> {
    let main_out_path = plan
        .outputs
        .get("out")
        .or_else(|| plan.outputs.values().next())
        .and_then(|output| output.path.as_ref());

    if let Some(out_path) = main_out_path {
        let info = engine
            .path_info_service
            .get(*out_path.digest())
            .await
            .map_err(|e| SnixError::StoreError {
                operation: "lookup_cached",
                source: e,
            })?;
        if let Some(info) = info {
            return Ok(Some(SnixOutput {
                node: info.node.clone(),
                path_info: info,
            }));
        }
    }
    Ok(None)
}
