//! Dedicated OS thread evaluation bridge for the Snix evaluator.
//!
//! The Snix evaluator is `!Send` because it relies internally on `Rc` pointers
//! (e.g., `Rc<Closure>`). This module isolates the evaluator on a dedicated thread,
//! executing evaluation synchronously and returning only `Send` data (the final
//! `Derivation`) to the async runtime.

use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use bstr::ByteSlice;
use eos_core::digest::Blake3Digest;
use eos_core::eval::{ComposerConfig, EvalRequest, EvalTarget, ResolvedInput};
use nix_compat::derivation::Derivation;
use nix_compat::store_path::StorePath as NixStorePath;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
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

/// Helper function to extract filesystem path from service URI.
fn extract_path_from_addr(addr: &str) -> Option<std::path::PathBuf> {
    if let Some(pos) = addr.find(':') {
        let path_part = &addr[pos + 1..];
        let clean_path = path_part.split('?').next().unwrap_or(path_part);
        let path = std::path::PathBuf::from(clean_path);
        if path.is_absolute() {
            return Some(path);
        }
    }
    None
}

/// Serializable Data Transfer Object for `EvalTarget`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EvalTargetDto {
    File(std::path::PathBuf),
    Expression(String),
}

/// Serializable Data Transfer Object for `ResolvedInput`.
///
/// The digest is serialized as a lowercase hex string for JSON
/// readability and interoperability.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolvedInputDto {
    pub digest: String,
    pub store_path: String,
}

/// Serializable Data Transfer Object for `ComposerConfig`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComposerConfigDto {
    /// Serialized as the `<anchor_b64ut>::<label>` display form.
    pub atom_id: String,
    pub entry: String,
    pub version: String,
}

impl From<ComposerConfig> for ComposerConfigDto {
    fn from(config: ComposerConfig) -> Self {
        ComposerConfigDto {
            atom_id: config.atom_id.to_string(),
            entry: config.entry,
            version: config.version,
        }
    }
}

impl ComposerConfigDto {
    /// Converts back to a `ComposerConfig`, parsing the `AtomId`.
    pub fn into_config(self) -> Result<ComposerConfig, SnixError> {
        let atom_id =
            atom_id::AtomId::from_str(&self.atom_id).map_err(|e| SnixError::ConversionError {
                from: "ComposerConfigDto",
                to: "ComposerConfig",
                detail: format!("invalid AtomId '{}': {}", self.atom_id, e),
            })?;
        Ok(ComposerConfig {
            atom_id,
            entry: self.entry,
            version: self.version,
        })
    }
}

/// Serializable Data Transfer Object for `EvalRequest`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EvalRequestDto {
    pub expression: EvalTargetDto,
    pub inputs: HashMap<String, ResolvedInputDto>,
    pub composer: Option<ComposerConfigDto>,
    pub eval_args: Vec<(String, String)>,
}

impl From<EvalTarget> for EvalTargetDto {
    fn from(target: EvalTarget) -> Self {
        match target {
            EvalTarget::File(p) => EvalTargetDto::File(p),
            EvalTarget::Expression(s) => EvalTargetDto::Expression(s),
        }
    }
}

impl From<EvalTargetDto> for EvalTarget {
    fn from(dto: EvalTargetDto) -> Self {
        match dto {
            EvalTargetDto::File(p) => EvalTarget::File(p),
            EvalTargetDto::Expression(s) => EvalTarget::Expression(s),
        }
    }
}

impl From<ResolvedInput<Blake3Digest>> for ResolvedInputDto {
    fn from(input: ResolvedInput<Blake3Digest>) -> Self {
        let hex_digest: String = input.digest.0.iter().map(|b| format!("{b:02x}")).collect();
        ResolvedInputDto {
            digest: hex_digest,
            store_path: input.store_path.0,
        }
    }
}

impl From<ResolvedInputDto> for ResolvedInput<Blake3Digest> {
    fn from(dto: ResolvedInputDto) -> Self {
        let mut bytes = [0u8; 32];
        for (i, chunk) in dto.digest.as_bytes().chunks(2).enumerate() {
            if i < 32 {
                bytes[i] =
                    u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or("00"), 16).unwrap_or(0);
            }
        }
        ResolvedInput {
            digest: Blake3Digest(bytes),
            store_path: eos_core::store::StorePath(dto.store_path),
        }
    }
}

impl From<EvalRequest<Blake3Digest>> for EvalRequestDto {
    fn from(req: EvalRequest<Blake3Digest>) -> Self {
        let mut inputs = HashMap::new();
        for (k, v) in req.inputs {
            inputs.insert(k, v.into());
        }
        EvalRequestDto {
            expression: req.expression.into(),
            inputs,
            composer: req.composer.map(ComposerConfigDto::from),
            eval_args: req.eval_args,
        }
    }
}

impl EvalRequestDto {
    /// Converts this DTO into a standard `EvalRequest<Blake3Digest>`.
    pub fn into_request(self) -> Result<EvalRequest<Blake3Digest>, SnixError> {
        let mut inputs = HashMap::new();
        for (k, v) in self.inputs {
            inputs.insert(k, v.into());
        }
        let mut req = EvalRequest::new(self.expression.into());
        req.inputs = inputs;
        req.composer = self.composer.map(|c| c.into_config()).transpose()?;
        req.eval_args = self.eval_args;
        Ok(req)
    }
}

/// Computes a deterministic cache key for the given evaluation request.
pub fn compute_eval_cache_key(request: &EvalRequest<Blake3Digest>) -> [u8; 32] {
    let dto = EvalRequestDto::from(request.clone());

    // Deterministic sorting of inputs
    let mut inputs_sorted: Vec<(String, ResolvedInputDto)> = dto.inputs.into_iter().collect();
    inputs_sorted.sort_by(|a, b| a.0.cmp(&b.0));

    // Deterministic sorting of evaluation arguments
    let mut eval_args_sorted = dto.eval_args.clone();
    eval_args_sorted.sort_by(|a, b| a.0.cmp(&b.0));

    #[derive(serde::Serialize)]
    struct DeterministicKey<'a> {
        expression: &'a EvalTargetDto,
        inputs: &'a [(String, ResolvedInputDto)],
        composer: &'a Option<ComposerConfigDto>,
        eval_args: &'a [(String, String)],
    }

    let key_struct = DeterministicKey {
        expression: &dto.expression,
        inputs: &inputs_sorted,
        composer: &dto.composer,
        eval_args: &eval_args_sorted,
    };

    let bytes = serde_json::to_vec(&key_struct).unwrap_or_default();
    blake3::hash(&bytes).into()
}
/// Constructs the common CLI arguments passed to the `--eval-worker` subprocess.
fn build_worker_args(config: &crate::SandboxedEvalConfig) -> Vec<String> {
    vec![
        "--eval-worker".to_string(),
        "--blob-service-addr".to_string(),
        config.blob_service_addr.clone(),
        "--directory-service-addr".to_string(),
        config.directory_service_addr.clone(),
        "--path-info-service-addr".to_string(),
        config.path_info_service_addr.clone(),
    ]
}

/// Parses the worker subprocess output into a `Derivation`.
///
/// Returns an error if the process exited with a non-zero status or
/// if the stdout bytes are not valid ATerm.
fn parse_worker_output(
    output: std::process::Output,
    expression_debug: &str,
) -> Result<Derivation, SnixError> {
    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(SnixError::EvalFailed {
            expression: expression_debug.to_string(),
            source: Box::new(std::io::Error::other(format!(
                "sandboxed evaluation failed: {}",
                stderr_str
            ))),
        });
    }

    Derivation::from_aterm_bytes(&output.stdout).map_err(|e| SnixError::ConversionError {
        from: "ATerm",
        to: "Derivation",
        detail: format!("{:?}", e),
    })
}

/// Executes evaluation inside a platform-specific restricted sandbox.
///
/// Spawns the worker binary inside a platform-native container
/// (Bubblewrap on Linux, Birdcage on macOS), feeds the serialized
/// `EvalRequest` via stdin, and reads the ATerm derivation from stdout.
pub async fn evaluate_sandboxed(
    config: &crate::SandboxedEvalConfig,
    request: EvalRequest<Blake3Digest>,
) -> Result<Derivation, SnixError> {
    let dto = EvalRequestDto::from(request);
    let req_bytes = serde_json::to_vec(&dto).map_err(|e| SnixError::ConversionError {
        from: "EvalRequest",
        to: "JSON",
        detail: e.to_string(),
    })?;
    let expression_debug = format!("{:?}", dto.expression);

    #[cfg(target_os = "linux")]
    {
        let exe_path = config.resolve_worker_bin()?;

        let mut args = vec![
            "--unshare-uts".to_string(),
            "--unshare-ipc".to_string(),
            "--unshare-pid".to_string(),
            "--die-with-parent".to_string(),
            "--unshare-user".to_string(),
            "--uid".to_string(),
            "1000".to_string(),
            "--gid".to_string(),
            "100".to_string(),
            "--unshare-net".to_string(),
            "--tmpfs".to_string(),
            "/".to_string(),
            "--dev".to_string(),
            "/dev".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--tmpfs".to_string(),
            "/tmp".to_string(),
        ];

        // Bind minimal OS paths required for dynamic linking and execution.
        // Deliberately excludes /etc — host configuration files
        // (/etc/passwd, /etc/hostname, /etc/resolv.conf, /etc/localtime)
        // are impurity vectors prohibited by [no-unbounded-eval-io].
        for path in &["/usr", "/bin", "/lib"] {
            if std::path::Path::new(path).exists() {
                args.push("--ro-bind".to_string());
                args.push(path.to_string());
                args.push(path.to_string());
            }
        }
        if std::path::Path::new("/lib64").exists() {
            args.push("--ro-bind".to_string());
            args.push("/lib64".to_string());
            args.push("/lib64".to_string());
        }

        // Bind current executable
        args.push("--ro-bind".to_string());
        args.push(exe_path.to_string_lossy().into_owned());
        args.push(exe_path.to_string_lossy().into_owned());

        // Bind workspace directory
        args.push("--ro-bind".to_string());
        args.push(config.workspace_dir.to_string_lossy().into_owned());
        args.push(config.workspace_dir.to_string_lossy().into_owned());

        // Bind sandbox workdir
        if !config.sandbox_workdir.exists() {
            std::fs::create_dir_all(&config.sandbox_workdir).map_err(|e| {
                SnixError::SandboxError {
                    platform: "linux",
                    source: Box::new(e),
                }
            })?;
        }
        args.push("--bind".to_string());
        args.push(config.sandbox_workdir.to_string_lossy().into_owned());
        args.push(config.sandbox_workdir.to_string_lossy().into_owned());

        // Bind DB directories
        for addr in &[
            &config.blob_service_addr,
            &config.directory_service_addr,
            &config.path_info_service_addr,
        ] {
            if let Some(path) = extract_path_from_addr(addr) {
                let host_path = if path.extension().is_some() {
                    path.parent().unwrap_or(&path).to_path_buf()
                } else {
                    path.clone()
                };
                if !host_path.exists() {
                    let _ = std::fs::create_dir_all(&host_path);
                }
                args.push("--bind".to_string());
                args.push(host_path.to_string_lossy().into_owned());
                args.push(host_path.to_string_lossy().into_owned());
            }
        }

        // Bind expression parent directory if it's a file
        if let EvalTargetDto::File(ref p) = dto.expression
            && let Some(parent) = p.parent()
        {
            args.push("--ro-bind".to_string());
            args.push(parent.to_string_lossy().into_owned());
            args.push(parent.to_string_lossy().into_owned());
        }

        let worker_args = build_worker_args(config);
        let mut child = tokio::process::Command::new("bwrap")
            .args(&args)
            .arg("--")
            .arg(&exe_path)
            .args(&worker_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| SnixError::SandboxError {
                platform: "linux bwrap",
                source: Box::new(e),
            })?;

        // Write the serialized request to the worker's stdin, then
        // close the pipe explicitly so the worker sees EOF and begins
        // processing.
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&req_bytes)
                .await
                .map_err(|e| SnixError::SandboxError {
                    platform: "linux bwrap stdin",
                    source: Box::new(e),
                })?;
            stdin.flush().await.map_err(|e| SnixError::SandboxError {
                platform: "linux bwrap stdin flush",
                source: Box::new(e),
            })?;
            // Explicit drop signals EOF to the worker subprocess.
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| SnixError::SandboxError {
                platform: "linux bwrap wait",
                source: Box::new(e),
            })?;

        parse_worker_output(output, &expression_debug)
    }

    #[cfg(target_os = "macos")]
    {
        let exe_path = config.resolve_worker_bin()?;
        let workspace_dir = config.workspace_dir.clone();
        let blob_service_addr = config.blob_service_addr.clone();
        let directory_service_addr = config.directory_service_addr.clone();
        let path_info_service_addr = config.path_info_service_addr.clone();
        let worker_args = build_worker_args(config);

        tokio::task::spawn_blocking(move || -> Result<Derivation, SnixError> {
            use std::io::Write;
            use std::process::{Command, Stdio};

            use birdcage::{Birdcage, Exception, Sandbox};

            let mut sandbox = Birdcage::new();
            sandbox
                .add_exception(Exception::Environment("PATH".into()))
                .map_err(|e| SnixError::SandboxError {
                    platform: "macos birdcage",
                    source: Box::new(e),
                })?;
            sandbox
                .add_exception(Exception::ExecuteAndRead(exe_path.clone()))
                .map_err(|e| SnixError::SandboxError {
                    platform: "macos birdcage",
                    source: Box::new(e),
                })?;
            sandbox
                .add_exception(Exception::Read(workspace_dir))
                .map_err(|e| SnixError::SandboxError {
                    platform: "macos birdcage",
                    source: Box::new(e),
                })?;

            for addr in &[
                &blob_service_addr,
                &directory_service_addr,
                &path_info_service_addr,
            ] {
                if let Some(path) = extract_path_from_addr(addr) {
                    sandbox
                        .add_exception(Exception::WriteAndRead(path))
                        .map_err(|e| SnixError::SandboxError {
                            platform: "macos birdcage",
                            source: Box::new(e),
                        })?;
                }
            }

            let mut cmd = Command::new(&exe_path);
            cmd.args(&worker_args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = sandbox.spawn(cmd).map_err(|e| SnixError::SandboxError {
                platform: "macos birdcage spawn",
                source: Box::new(e),
            })?;

            // Write request then close stdin to signal EOF.
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(&req_bytes)
                    .map_err(|e| SnixError::SandboxError {
                        platform: "macos birdcage stdin",
                        source: Box::new(e),
                    })?;
                drop(stdin);
            }

            let output = child
                .wait_with_output()
                .map_err(|e| SnixError::SandboxError {
                    platform: "macos birdcage wait",
                    source: Box::new(e),
                })?;

            parse_worker_output(output, &expression_debug)
        })
        .await
        .map_err(|_| SnixError::EvalThreadPanic)?
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(SnixError::SandboxError {
            platform: "unsupported",
            source: Box::new(std::io::Error::other(
                "sandboxed evaluation not supported on this platform",
            )),
        })
    }
}
