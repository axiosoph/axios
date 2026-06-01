//! Daemon configuration.

use std::path::PathBuf;
use clap::Parser;

/// Configuration settings for the Eos daemon.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Eos build daemon", long_about = None)]
pub struct DaemonConfig {
    /// Socket path to bind to for incoming UDS connections.
    /// If not specified, resolved via $EOS_SOCKET, then $XDG_RUNTIME_DIR/eos/eos.sock.
    #[arg(long, env = "EOS_SOCKET")]
    pub socket_path: Option<PathBuf>,

    /// Address for the blob service.
    #[arg(long, env = "BLOB_SERVICE_ADDR", default_value = "objectstore+file:/var/lib/snix-castore/blobs")]
    pub blob_service_addr: String,

    /// Address for the directory service.
    #[arg(long, env = "DIRECTORY_SERVICE_ADDR", default_value = "redb:/var/lib/snix-castore/directories.redb")]
    pub directory_service_addr: String,

    /// Address for the path info service.
    #[arg(long, env = "PATH_INFO_SERVICE_ADDR", default_value = "redb:/var/lib/snix-store/pathinfo.redb")]
    pub path_info_service_addr: String,

    /// Maximum number of concurrent builds to execute locally.
    #[arg(long, env = "MAX_CONCURRENCY", default_value_t = 4)]
    pub max_concurrency: usize,

    /// Working directory for build sandboxes.
    #[arg(long, env = "SANDBOX_WORKDIR", default_value = "/tmp/eos-sandbox")]
    pub sandbox_workdir: PathBuf,

    /// Path to the local workspace git repository for resolving local "::" mirrors.
    #[arg(long, env = "EOS_WORKSPACE_DIR", default_value = "/var/home/nrd/git/github.com/axiosoph/axios")]
    pub workspace_dir: PathBuf,

    /// Directory where lock files are stored.
    #[arg(long, env = "EOS_LOCKS_DIR", default_value = "/tmp/eos-locks")]
    pub locks_dir: PathBuf,
}

impl DaemonConfig {
    /// Resolves the socket path according to spec precedence.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket path cannot be resolved from arguments or environments.
    pub fn resolve_socket_path(&self) -> Result<PathBuf, String> {
        if let Some(ref path) = self.socket_path {
            return Ok(path.clone());
        }
        if let Ok(path_str) = std::env::var("EOS_SOCKET") {
            if !path_str.is_empty() {
                return Ok(PathBuf::from(path_str));
            }
        }
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            if !xdg_runtime.is_empty() {
                return Ok(PathBuf::from(xdg_runtime).join("eos").join("eos.sock"));
            }
        }
        Err("Could not resolve socket path: neither --socket-path, $EOS_SOCKET, nor $XDG_RUNTIME_DIR was set".to_string())
    }

    /// Resolves the locks directory path.
    #[must_use]
    pub fn resolve_locks_dir(&self) -> PathBuf {
        if let Ok(locks_env) = std::env::var("EOS_LOCKS_DIR") {
            if !locks_env.is_empty() {
                return PathBuf::from(locks_env);
            }
        }
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            if !xdg_runtime.is_empty() {
                return PathBuf::from(xdg_runtime).join("eos").join("locks");
            }
        }
        self.locks_dir.clone()
    }
}
