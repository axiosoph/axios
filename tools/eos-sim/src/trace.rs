//! Trace file format and loading.
//!
//! A *trace* is the simulator's sole input: a plan DAG with duration ground
//! truth, a worker-pool spec, and per-node scheduling hints. It is the data
//! contract node P10 (heuristic evaluation) and node P9 (trace corpus) target,
//! so it is deliberately a plain, serde-friendly JSON document with no
//! dependency on any `eos` runtime crate. See `README.md` for the prose
//! specification.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Errors surfaced while loading or validating a trace.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// The trace file could not be read from disk.
    #[error("failed to read trace file `{path}`: {source}")]
    Io {
        /// Path the simulator attempted to read.
        path: String,
        /// Underlying I/O failure.
        source: std::io::Error,
    },
    /// The trace file was not well-formed JSON for the trace schema.
    #[error("failed to parse trace JSON: {0}")]
    Parse(#[from] serde_json::Error),
    /// The trace parsed but violates a structural invariant.
    #[error("invalid trace: {0}")]
    Invalid(String),
}

/// A single plan node: one `BuildEngine::Plan` with duration ground truth.
///
/// `id` is an opaque digest string (the simulator never interprets it). The
/// remaining fields are the prediction oracle the scheduler would normally
/// recover from the profile store (ADR-0004 §1); in simulation they are given
/// directly as ground truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceNode {
    /// Opaque plan digest. Unique within a trace.
    pub id: String,
    /// Isolated build duration in seconds (the `d(v)` of ADR-0004 §2a).
    pub duration: f64,
    /// Predicted peak memory in bytes, if known. Feeds capacity feasibility.
    #[serde(default)]
    pub peak_mem: Option<u64>,
    /// Synthetic atom marker consumed by the atom-seeded coarsening variant
    /// (ADR-0004 §2a). In the nixpkgs corpus these mark top-level `pkgs`-set
    /// attributes used as a proxy for atom boundaries.
    #[serde(default)]
    pub is_atom: bool,
    /// Human-readable, version-stable profile key (the spec's `plan_name`).
    /// Unused by the engine today; retained for corpus fidelity.
    #[serde(default)]
    pub plan_name: Option<String>,
    /// Per-node prediction confidence in `[0, 1]`. Drives confidence gating of
    /// the coarsening thresholds and the dispatch window. Defaults to
    /// [`Self::DEFAULT_CONFIDENCE`] when absent.
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Simulated time at which this plan enters the system (models the ADR
    /// `RequestArrival` staggering). An EP whose entry node arrives at `t`
    /// cannot become ready before `t`. Defaults to `0.0` (present from the
    /// start), so it is inert unless a trace stages arrivals.
    #[serde(default)]
    pub arrival: Option<f64>,
}

impl TraceNode {
    /// Confidence assumed for a node that does not declare one.
    pub const DEFAULT_CONFIDENCE: f64 = 0.5;

    /// Effective prediction confidence, clamped to `[0, 1]`.
    pub fn confidence(&self) -> f64 {
        self.confidence
            .unwrap_or(Self::DEFAULT_CONFIDENCE)
            .clamp(0.0, 1.0)
    }
}

/// A dependency edge: `from` depends on `to` (so `to` must build first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEdge {
    /// The dependent plan (built after `to`).
    pub from: String,
    /// The dependency plan (built before `from`).
    pub to: String,
}

fn default_speed() -> f64 {
    1.0
}

/// A worker in the build pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerSpec {
    /// Cryptographic worker identity (opaque here). Unique within a trace.
    pub id: String,
    /// Duration multiplier: `1.0` is nominal, `< 1.0` is faster hardware,
    /// `> 1.0` slower. Multiplies the Option-C predicted duration.
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// Reported capacity vector (abstract; e.g. `{ "mem": 8_000_000_000 }`).
    /// An EP is feasible on this worker only if its requirement is dominated
    /// dimension-wise (spec `[eos-scheduler-concurrency-limits]`).
    #[serde(default)]
    pub capacity: BTreeMap<String, u64>,
    /// Plan ids already cached locally on this worker at `t = 0`. Feeds the
    /// affinity term of the Option-C duration model (ADR-0004 §3).
    #[serde(default)]
    pub cached: Vec<String>,
}

/// The complete simulator input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// Plan nodes. Order is irrelevant; identity is by `id`.
    pub nodes: Vec<TraceNode>,
    /// Dependency edges between nodes.
    #[serde(default)]
    pub edges: Vec<TraceEdge>,
    /// The worker pool.
    pub workers: Vec<WorkerSpec>,
    /// Plans whose outputs are already in the global artifact store. These are
    /// removed by the ingress cache filter (ADR-0004 §2b) before coarsening.
    #[serde(default)]
    pub store_cached: Vec<String>,
}

impl Trace {
    /// Load and validate a trace from a JSON file on disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, TraceError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| TraceError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let trace: Trace = serde_json::from_slice(&bytes)?;
        trace.validate()?;
        Ok(trace)
    }

    /// Parse and validate a trace from an in-memory JSON string.
    pub fn from_json(json: &str) -> Result<Self, TraceError> {
        let trace: Trace = serde_json::from_str(json)?;
        trace.validate()?;
        Ok(trace)
    }

    /// Reject structurally malformed traces: duplicate node or worker ids,
    /// edges referencing unknown nodes, an empty worker pool, or non-finite /
    /// negative durations. (Acyclicity is checked when the graph is built.)
    pub fn validate(&self) -> Result<(), TraceError> {
        if self.workers.is_empty() {
            return Err(TraceError::Invalid("trace has no workers".into()));
        }
        let mut node_ids = std::collections::BTreeSet::new();
        for n in &self.nodes {
            if !node_ids.insert(n.id.as_str()) {
                return Err(TraceError::Invalid(format!("duplicate node id `{}`", n.id)));
            }
            if !n.duration.is_finite() || n.duration < 0.0 {
                return Err(TraceError::Invalid(format!(
                    "node `{}` has invalid duration {}",
                    n.id, n.duration
                )));
            }
        }
        let mut worker_ids = std::collections::BTreeSet::new();
        for w in &self.workers {
            if !worker_ids.insert(w.id.as_str()) {
                return Err(TraceError::Invalid(format!(
                    "duplicate worker id `{}`",
                    w.id
                )));
            }
        }
        for e in &self.edges {
            if !node_ids.contains(e.from.as_str()) {
                return Err(TraceError::Invalid(format!(
                    "edge from unknown node `{}`",
                    e.from
                )));
            }
            if !node_ids.contains(e.to.as_str()) {
                return Err(TraceError::Invalid(format!(
                    "edge to unknown node `{}`",
                    e.to
                )));
            }
            if e.from == e.to {
                return Err(TraceError::Invalid(format!(
                    "self-edge on node `{}`",
                    e.from
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAMOND: &str = r#"{
        "nodes": [
            {"id": "top", "duration": 1.0},
            {"id": "a", "duration": 2.0},
            {"id": "b", "duration": 3.0},
            {"id": "leaf", "duration": 4.0}
        ],
        "edges": [
            {"from": "top", "to": "a"},
            {"from": "top", "to": "b"},
            {"from": "a", "to": "leaf"},
            {"from": "b", "to": "leaf"}
        ],
        "workers": [{"id": "w0"}, {"id": "w1"}]
    }"#;

    #[test]
    fn parses_diamond() {
        let t = Trace::from_json(DIAMOND).expect("valid");
        assert_eq!(t.nodes.len(), 4);
        assert_eq!(t.edges.len(), 4);
        assert_eq!(t.workers.len(), 2);
        // Defaults applied.
        assert_eq!(t.workers[0].speed, 1.0);
        assert!(!t.nodes[0].is_atom);
        assert_eq!(t.nodes[0].confidence(), TraceNode::DEFAULT_CONFIDENCE);
    }

    #[test]
    fn round_trips_through_json() {
        let t = Trace::from_json(DIAMOND).expect("valid");
        let serialized = serde_json::to_string(&t).expect("serialize");
        let back = Trace::from_json(&serialized).expect("reparse");
        assert_eq!(t, back);
    }

    #[test]
    fn rejects_dangling_edge() {
        let json = r#"{
            "nodes": [{"id": "a", "duration": 1.0}],
            "edges": [{"from": "a", "to": "ghost"}],
            "workers": [{"id": "w0"}]
        }"#;
        let err = Trace::from_json(json).unwrap_err();
        assert!(matches!(err, TraceError::Invalid(_)), "got {err:?}");
    }

    #[test]
    fn rejects_duplicate_node() {
        let json = r#"{
            "nodes": [{"id": "a", "duration": 1.0}, {"id": "a", "duration": 2.0}],
            "workers": [{"id": "w0"}]
        }"#;
        assert!(matches!(
            Trace::from_json(json),
            Err(TraceError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_empty_worker_pool() {
        let json = r#"{"nodes": [], "workers": []}"#;
        assert!(matches!(
            Trace::from_json(json),
            Err(TraceError::Invalid(_))
        ));
    }
}
