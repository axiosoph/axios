//! Atom discovery index.
//!
//! Defines the [`AtomIndex`] trait and structured discovery metadata and queries.

use atom_id::AtomId;
use trait_variant::make;

/// Information about a specific version of an atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    /// Semantic version of the atom.
    pub version: String,
    /// Pinned Git revision (commit hash).
    pub rev: String,
    /// Git mirror/anchor of the atom-set containing this version.
    pub set: String,
}

/// Metadata about a discovered atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomMeta {
    /// Content-addressed identifier of the atom.
    pub id: AtomId,
    /// Human-readable label of the atom.
    pub label: String,
    /// Registered versions of this atom.
    pub versions: Vec<VersionInfo>,
    /// Anchor hashes of the sets containing this atom.
    pub sets: Vec<String>,
}

/// A query for searching the atom discovery index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomQuery {
    /// Pattern (glob or substring) to match against labels.
    pub label_pattern: String,
    /// Optional filter to restrict search to a specific atom-set anchor.
    pub set_filter: Option<String>,
    /// Maximum number of results to return.
    pub limit: u32,
}

/// Trait representing a queryable store of atom knowledge.
///
/// Every eos instance tracks the atoms it has processed, exposing this index
/// for interactive dependency searches and resolution checks.
#[make(Send)]
pub trait AtomIndex: Send + Sync + 'static {
    /// The error type returned by index operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Looks up metadata about a specific atom.
    async fn resolve(&self, id: &AtomId) -> Result<Option<AtomMeta>, Self::Error>;

    /// Performs a fast check to see if the index contains the atom.
    async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error>;

    /// Searches for atoms matching a structured query.
    async fn search(&self, query: &AtomQuery) -> Result<Vec<AtomMeta>, Self::Error>;

    /// Ingests new atom metadata into the index.
    async fn ingest(&self, meta: AtomMeta) -> Result<(), Self::Error>;
}
