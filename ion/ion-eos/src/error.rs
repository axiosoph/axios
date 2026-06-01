//! Client error types.

use thiserror::Error;

/// Errors returned by the Eos client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClientError {
    /// Failed to connect to the daemon.
    #[error("connection to eos daemon failed at {socket_path}: {source}")]
    ConnectionFailed {
        /// Socket path.
        socket_path: String,
        /// Source error.
        #[source]
        source: std::io::Error,
    },

    /// Protocol or serialization error.
    #[error("protocol error: {detail}")]
    ProtocolError {
        /// Error details.
        detail: String,
    },

    /// Build failed error.
    #[error("build failed: {status}")]
    BuildFailed {
        /// Status of the failed build.
        status: String,
    },

    /// Call timed out.
    #[error("operation timed out")]
    Timeout,
}

impl From<capnp::Error> for ClientError {
    fn from(err: capnp::Error) -> Self {
        ClientError::ProtocolError {
            detail: err.to_string(),
        }
    }
}
