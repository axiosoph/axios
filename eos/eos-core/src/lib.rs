//! Build engine trait definitions.
//!
//! Defines [`BuildEngine`] with plan/apply and associated types,
//! plus [`BuildPlan`] for cache-skipping decisions.

#![allow(async_fn_in_trait)]

pub mod bridge;
pub mod digest;
pub mod engine;
pub mod error;
pub mod eval;
pub mod executor;
pub mod index;
pub mod ingest;
pub mod job;
pub mod request;
pub mod store;

pub use bridge::AtomContentBridge;
pub use digest::{Blake3Digest, Digest, ParseBlake3DigestError};
pub use engine::{AtomRef, BuildEngine, BuildPlan};
pub use eval::{ComposerConfig, EvalRequest, EvalTarget, ResolvedInput};
pub use executor::{ActionId, ExecutionEngine, ExecutionPlan};
pub use index::{AtomIndex, AtomMeta, AtomQuery, VersionInfo};
pub use ingest::ContentIngestService;
pub use job::{ArtifactInfo, JobId, JobStatus, ProgressEvent};
pub use request::{
    AtomFetchDescriptor, AtomSetInfo, BuildRequest, ComposerSpec, FetchDescriptor,
    NixFetchDescriptor, NixGitFetchDescriptor, NixSrcFetchDescriptor, NixTarFetchDescriptor,
};
pub use store::{ArtifactStore, StorePath};
