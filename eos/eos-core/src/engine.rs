//! Build engine trait and associated types.
//!
//! Provides the primary [`BuildEngine`] trait and execution plan types.

use atom_id::AtomId;
use trait_variant::make;

use crate::digest::Digest;
use crate::eval::EvalRequest;
use crate::store::StorePath;

/// A cryptographic snapshot reference to an atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomRef<D: Digest> {
    /// Content-addressed atom identifier.
    pub id: AtomId,
    /// Verified content digest of the atom snapshot.
    pub digest: D,
}

/// The build plan state of an atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildPlan<D: Digest, P> {
    /// The outputs of the plan already exist in the store.
    Cached(Vec<StorePath>),
    /// The plan is computed, but the outputs are missing and need building.
    NeedsBuild(P),
    /// The plan is not yet computed and needs evaluation.
    NeedsEvaluation(AtomRef<D>),
}

/// Trait defining the build and evaluation runtime engine.
#[make(Send)]
pub trait BuildEngine: Send + Sync + 'static {
    /// The digest algorithm used by this engine.
    type Digest: Digest;

    /// The engine-specific plan representation (e.g., a Nix derivation).
    type Plan: Clone + Send + Sync + 'static;

    /// The engine-specific output representation (e.g., store paths with metadata).
    type Output: Send + Sync + 'static;

    /// The error type returned by engine operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Evaluates an evaluation request to produce a build plan.
    async fn evaluate(&self, request: EvalRequest<Self::Digest>)
    -> Result<Self::Plan, Self::Error>;

    /// Executes a build plan to produce output artifacts.
    async fn build(&self, plan: &Self::Plan) -> Result<Self::Output, Self::Error>;

    /// Checks if a pre-built output exists for the given plan.
    async fn lookup_cached(&self, plan: &Self::Plan) -> Result<Option<Self::Output>, Self::Error>;

    /// Computes the content-addressed digest of a plan.
    fn plan_digest(&self, plan: &Self::Plan) -> Self::Digest;
}
