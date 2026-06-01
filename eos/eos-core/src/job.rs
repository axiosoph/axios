//! Job management types.
//!
//! Defines job status, progress events, and artifact metadata.

use std::fmt;
use std::time::SystemTime;
use crate::digest::Digest;
use crate::store::StorePath;

/// Opaque unique identifier for a job, wrapping a plan digest.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId<D: Digest>(pub D);

impl<D: Digest> fmt::Display for JobId<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<D: Digest> fmt::Debug for JobId<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JobId({:?})", self.0)
    }
}

/// The execution status of a build job.
#[derive(Clone, Debug, PartialEq)]
pub enum JobStatus<D: Digest> {
    /// Waiting in the scheduler queue.
    Queued,
    /// Expression is being evaluated.
    Evaluating {
        /// Diagnostic message or status details.
        message: String,
    },
    /// Build execution is in progress.
    Building {
        /// Current build phase name.
        phase: String,
        /// Optional build progress percentage (0.0 to 1.0).
        progress: Option<f32>,
    },
    /// Job completed successfully.
    Completed {
        /// Metadata of the produced artifacts.
        outputs: Vec<ArtifactInfo<D>>,
    },
    /// Job failed to execute.
    Failed {
        /// Diagnostic error description.
        error: String,
        /// Process exit code if applicable.
        exit_code: Option<i32>,
    },
    /// Job was explicitly cancelled.
    Cancelled,
}

/// Metadata describing a built artifact in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactInfo<D: Digest> {
    /// Content-addressed digest of the artifact.
    pub digest: D,
    /// Opaque store path where the artifact is located.
    pub store_path: StorePath,
    /// Total byte size of the artifact.
    pub size: u64,
    /// Transitive runtime store path references.
    pub references: Vec<StorePath>,
    /// Optional digest of the plan that produced this artifact.
    pub deriver: Option<D>,
}

/// An event representing build progress.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressEvent<D: Digest> {
    /// Identifier of the job producing the progress.
    pub job_id: JobId<D>,
    /// Time when the event was generated.
    pub timestamp: SystemTime,
    /// Current job status.
    pub status: JobStatus<D>,
    /// Structured or raw log output line.
    pub log_line: Option<String>,
}
