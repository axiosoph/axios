//! Structured build request contract types.
//!
//! Replaces raw lock file TOML at the layer boundary between ion and eos.

use std::collections::HashMap;

use atom_id::AtomId;

use crate::digest::Digest;

/// A structured build request — the pre-fetch input to the orchestrator.
/// Replaces raw lock file content at the layer boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRequest<D: Digest> {
    /// Plan digest for deduplication (BLAKE3 of lock content).
    pub plan_digest: D,
    /// Atom-set declarations (anchor → mirrors, tag).
    /// Used by the AtomSource composite for registry mirror resolution.
    pub sets: HashMap<String, AtomSetInfo>,
    /// All dependency descriptors (pre-fetch).
    pub deps: Vec<FetchDescriptor>,
    /// Composer configuration.
    pub composer: ComposerSpec,
    /// Evaluation arguments (passed verbatim to evaluator).
    pub eval_args: Vec<(String, String)>,
}

/// Information about an atom-set — provided so the AtomSource
/// composite can locate registry mirrors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomSetInfo {
    /// Human-readable tag.
    pub tag: String,
    /// Mirror URLs ("::" = local).
    pub mirrors: Vec<String>,
}

/// A pre-fetch dependency descriptor — what eos needs to locate
/// and verify a dependency before evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchDescriptor {
    /// Atom dep — resolved via AtomSource. Eos does NOT fetch these
    /// from URLs; it calls AtomSource::resolve(). The atom protocol
    /// handles fetching, verification, and ingestion.
    Atom(AtomFetchDescriptor),
    /// Non-atom deps — eos fetches these directly from URLs.
    Nix(NixFetchDescriptor),
    NixGit(NixGitFetchDescriptor),
    NixTar(NixTarFetchDescriptor),
    NixSrc(NixSrcFetchDescriptor),
}

/// Fetch descriptor for an atom dependency.
/// Unlike non-atom deps, atom deps are resolved via AtomSource,
/// not fetched from URLs by eos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomFetchDescriptor {
    pub id: AtomId,
    pub label: String,
    pub version: String,
    /// Anchor hash referencing AtomSetInfo (for mirror resolution).
    pub set: String,
    /// Pinned revision (None for local unrevisioned).
    pub rev: Option<String>,
    /// Transitive requirement edges.
    pub requires: Vec<AtomId>,
    /// Whether this is a direct dependency.
    pub direct: bool,
}

/// A plain Nix file fetch descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixFetchDescriptor {
    pub name: String,
    pub url: String,
    pub hash: String,
    pub owner: Option<AtomId>,
}

/// A Nix Git repository fetch descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixGitFetchDescriptor {
    pub name: String,
    pub url: String,
    pub rev: String,
    pub version: Option<String>,
    pub owner: Option<AtomId>,
}

/// A Nix Tarball fetch descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixTarFetchDescriptor {
    pub name: String,
    pub url: String,
    pub hash: String,
    pub owner: Option<AtomId>,
}

/// A Nix Source File fetch descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixSrcFetchDescriptor {
    pub name: String,
    pub url: String,
    pub hash: String,
    pub owner: Option<AtomId>,
}

/// How to compose/evaluate the root atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerSpec {
    /// Use a published composer atom.
    Atom {
        id: AtomId,
        entry: Option<String>,
        args: HashMap<String, String>,
    },
    /// Use a trivial inline Nix expression.
    NixTrivial {
        expression: String,
        args: HashMap<String, String>,
    },
    /// Static configuration (no evaluation).
    Static,
}
