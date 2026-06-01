//! Evaluation request and targets.
//!
//! Defines types used when requesting an expression or composition evaluation.

use std::collections::HashMap;
use std::path::PathBuf;
use atom_id::AtomId;
use crate::digest::Digest;
use crate::store::StorePath;

/// The target of an evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalTarget {
    /// Evaluate a file path.
    File(PathBuf),
    /// Evaluate a string expression.
    Expression(String),
}

/// A resolved input to the evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedInput<D: Digest> {
    /// Content-addressed digest of this input.
    pub digest: D,
    /// Store path where the input is materialized.
    pub store_path: StorePath,
}

/// Configuration for a composer atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerConfig {
    /// Atom providing composition logic.
    pub atom_id: AtomId,
    /// Evaluation entrypoint within the atom.
    pub entry: String,
    /// Composer atom version.
    pub version: String,
}

/// A request to evaluate an expression or file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EvalRequest<D: Digest> {
    /// What target to evaluate.
    pub expression: EvalTarget,
    /// Pre-resolved inputs (atoms, nix sources).
    pub inputs: HashMap<String, ResolvedInput<D>>,
    /// Optional composer configuration.
    pub composer: Option<ComposerConfig>,
    /// Evaluation arguments.
    pub eval_args: Vec<(String, String)>,
}

impl<D: Digest> EvalRequest<D> {
    /// Creates a new `EvalRequest` with the specified expression target.
    pub fn new(expression: EvalTarget) -> Self {
        Self {
            expression,
            inputs: HashMap::new(),
            composer: None,
            eval_args: Vec::new(),
        }
    }
}
