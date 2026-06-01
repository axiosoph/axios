//! Error types for the Snix backend bridge.

use eos_core::digest::Blake3Digest;
use thiserror::Error;

/// Error type returned by the Snix backend.
#[derive(Debug, Error)]
pub enum SnixError {
    /// Nix evaluation failed.
    #[error("evaluation failed for `{expression}`: {source}")]
    EvalFailed {
        /// The Nix expression that failed to evaluate.
        expression: String,
        /// The underlying evaluation error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Nix build execution failed.
    #[error("build failed for plan {plan_digest}: exit code {exit_code:?}")]
    BuildFailed {
        /// The BLAKE3 digest of the build plan.
        plan_digest: Blake3Digest,
        /// The process exit code, if any.
        exit_code: Option<i32>,
        /// Stderr output of the build process.
        stderr: String,
    },

    /// Storage operation failed.
    #[error("store operation `{operation}` failed: {source}")]
    StoreError {
        /// The store operation that failed (e.g. "has", "get_info", "import", "list").
        operation: &'static str,
        /// The underlying storage error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Sandbox execution environment error.
    #[error("sandbox error on {platform}: {source}")]
    SandboxError {
        /// The host platform / sandbox backend name.
        platform: &'static str,
        /// The underlying sandbox setup or run error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Conversion between eos-core and Snix data types failed.
    #[error("type conversion {from} → {to}: {detail}")]
    ConversionError {
        /// The source type name.
        from: &'static str,
        /// The target type name.
        to: &'static str,
        /// Detailed information about the conversion failure.
        detail: String,
    },

    /// The evaluation thread panicked or was aborted.
    #[error("eval thread panicked")]
    EvalThreadPanic,
}
