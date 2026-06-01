//! Platform-specific sandbox backend dispatch for Snix builds.

use std::sync::Arc;

use snix_build::buildservice::BuildService;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use snix_build::buildservice::DummyBuildService;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;

use crate::error::SnixError;

/// Configuration options for the local Snix sandbox.
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    /// Remote builder URI, if configured (e.g. "grpc+http://localhost:8080").
    pub remote_builder: Option<String>,
    /// Directory used for bundle state/mountpoints.
    pub workdir: std::path::PathBuf,
}

/// Check if a command is executable on the system path.
fn command_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Selects and constructs the appropriate [`BuildService`] for the host platform.
pub async fn select_sandbox(
    config: &SandboxConfig,
    blob_service: Arc<dyn BlobService>,
    directory_service: Arc<dyn DirectoryService>,
) -> Result<Arc<dyn BuildService>, SnixError> {
    if let Some(ref remote_uri) = config.remote_builder {
        let bs = snix_build::buildservice::from_addr(remote_uri, blob_service, directory_service)
            .await
            .map_err(|e| SnixError::SandboxError {
                platform: "remote",
                source: Box::new(e),
            })?;
        return Ok(Arc::from(bs));
    }

    #[cfg(target_os = "linux")]
    {
        let has_crun = command_exists("crun");
        let has_runc = command_exists("runc");
        let uri = if has_crun || has_runc {
            format!("oci:{}", config.workdir.display())
        } else if command_exists("bwrap") {
            format!("bwrap:{}", config.workdir.display())
        } else {
            return Err(SnixError::SandboxError {
                platform: "linux",
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No sandbox runtime found: install crun, runc, or bwrap",
                )),
            });
        };

        let bs = snix_build::buildservice::from_addr(&uri, blob_service, directory_service)
            .await
            .map_err(|e| SnixError::SandboxError {
                platform: "linux",
                source: Box::new(e),
            })?;
        Ok(Arc::from(bs))
    }

    #[cfg(target_os = "macos")]
    {
        Err(SnixError::SandboxError {
            platform: "macos",
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "macOS local sandboxing via birdcage is not fully implemented; use a remote \
                 builder",
            )),
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(Arc::new(DummyBuildService::default()))
    }
}
