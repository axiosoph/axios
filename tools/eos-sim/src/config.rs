//! Heuristic configuration: the promotion variant, the seeding axis, and all
//! thresholds / weights the engine consumes.
//!
//! The committed specs are the source of truth. Promotion variants H1–H4 and
//! the `from-scratch` vs `atom-seeded` seeding axis are ADR-0004 §2a; the
//! confidence-gating rule, the Option-C duration parameters, and the dispatch
//! parameters (Δ window, γ delay credit, λ redundancy weight) are §2b/§3.

use std::fmt;
use std::str::FromStr;

/// Entry-point promotion variant (ADR-0004 §2a). A node is promoted to a
/// standalone entry point when the variant's criteria fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Priority-ordered promotion (leading): critical-path cut, cost-gated
    /// convergence, troublesome node. Any criterion firing promotes.
    H1,
    /// Combined weighted score: `w_c·cp + w_r·(fan_in−1)·d + w_d·d > θ`.
    H2,
    /// Redundancy-aware critical path: critical-path and troublesome only, no
    /// explicit fan-in term.
    H3,
    /// ADR-0004 original baseline: `d > θ_cost ∨ fan_in > θ_fanin ∨
    /// subgraph_cost > θ_subgraph`.
    H4,
    /// Relative-CP gate: `cp[v] / cp_max > θ_rel_critical ∨ d > θ_cost`.
    /// Normalises by estimated makespan so the threshold is graph-scale
    /// independent; avoids the over-promotion H3 exhibits on dense graphs
    /// when absolute CP values rise uniformly.
    H5,
    /// Strict-AND intersection: `cp[v] > θ_critical ∧ d > θ_cost`.
    /// More conservative than H1/H3 (OR); promotes only nodes that are
    /// both on a long critical path AND individually expensive, preventing
    /// cheap CP-adjacent nodes from fragmenting the EP set.
    H6,
}

impl FromStr for Variant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "H1" => Ok(Variant::H1),
            "H2" => Ok(Variant::H2),
            "H3" => Ok(Variant::H3),
            "H4" => Ok(Variant::H4),
            "H5" => Ok(Variant::H5),
            "H6" => Ok(Variant::H6),
            other => Err(format!(
                "unknown variant `{other}` (expected H1|H2|H3|H4|H5|H6)"
            )),
        }
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Variant::H1 => "H1",
            Variant::H2 => "H2",
            Variant::H3 => "H3",
            Variant::H4 => "H4",
            Variant::H5 => "H5",
            Variant::H6 => "H6",
        };
        f.write_str(s)
    }
}

/// Initial entry-point cover seeding (ADR-0004 §2a, orthogonal to [`Variant`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seeding {
    /// Seed entry points from individual plan nodes (roots + promotions).
    FromScratch,
    /// Seed non-trivial atom boundaries as entry points, then refine with the
    /// variant's promotion criteria.
    AtomSeeded,
}

impl FromStr for Seeding {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('_', "-").as_str() {
            "from-scratch" | "scratch" => Ok(Seeding::FromScratch),
            "atom-seeded" | "atom" => Ok(Seeding::AtomSeeded),
            other => Err(format!(
                "unknown seeding `{other}` (expected from-scratch|atom-seeded)"
            )),
        }
    }
}

impl fmt::Display for Seeding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Seeding::FromScratch => "from-scratch",
            Seeding::AtomSeeded => "atom-seeded",
        };
        f.write_str(s)
    }
}

/// All knobs governing coarsening, placement, and dispatch.
#[derive(Debug, Clone)]
pub struct HeuristicConfig {
    /// Promotion variant.
    pub variant: Variant,
    /// Initial-cover seeding axis.
    pub seeding: Seeding,

    // --- H1 / H3 thresholds (ADR-0004 §2a) ---
    /// Critical-path-cut threshold θ_critical (PRIMARY, parallelism).
    pub theta_critical: f64,
    /// Cost-gated convergence threshold θ_redundancy (SECONDARY).
    pub theta_redundancy: f64,
    /// Troublesome-node threshold θ_cost (TERTIARY, resource isolation).
    pub theta_cost: f64,
    /// Confidence-gating scale θ_scale: `θ_eff = θ / (1 + conf·θ_scale)`.
    /// `0` disables gating (effective threshold equals the bare threshold).
    pub theta_scale: f64,

    // --- H5 relative-CP threshold ---
    /// Relative-CP fraction threshold for H5: promote when
    /// `cp[v] / cp_max > θ_rel_critical`.  Range [0, 1]; 0.3 promotes the
    /// top 70% of the CP distribution (all nodes whose chain is longer than
    /// 30% of the estimated makespan).
    pub theta_rel_critical: f64,

    // --- H2 combined score ---
    /// Weight on critical path.
    pub w_critical: f64,
    /// Weight on the cost-gated convergence term.
    pub w_redundancy: f64,
    /// Weight on isolated duration.
    pub w_cost: f64,
    /// Combined-score threshold θ_combined.
    pub theta_combined: f64,

    // --- H4 baseline thresholds ---
    /// Predicted-cost threshold (baseline `d(v)` gate).
    pub theta_eff_cost: f64,
    /// Bare fan-in threshold (integer compare, ungated).
    pub theta_fanin: usize,
    /// Subgraph-cost threshold.
    pub theta_subgraph: f64,

    // --- atom seeding ---
    /// Atoms with `subgraph_cost < θ_trivial` are absorbed rather than seeded.
    pub theta_trivial: f64,

    // --- Option-C duration model (ADR-0004 §3) ---
    /// Cache-affinity speedup factor: a fully-cached EP builds in
    /// `(1 − cache_speedup)` of its base cost on that worker.
    pub cache_speedup: f64,
    /// Resource-fit penalty weight β: poor fit inflates `d(e,w)` by up to `β`.
    pub beta: f64,

    // --- dispatch (ADR-0004 §2b) ---
    /// Bounded dispatch window Δ in seconds (P9′). `0` is strict immediate
    /// dispatch.
    pub delta: f64,
    /// Delay-credit weight γ in the priority `avg_OCT + γ·age` (P12). `0`
    /// disables the fairness term and admits starvation.
    pub gamma: f64,
    /// Redundant-work weight λ in the reported objective `makespan + λ·redundant`.
    pub lambda: f64,
    /// A ready EP may be held within Δ only when its confidence is at least
    /// this; below it Δ collapses to 0 (confidence-gated window, P9′).
    pub confidence_threshold: f64,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        HeuristicConfig {
            variant: Variant::H1,
            seeding: Seeding::FromScratch,
            theta_critical: 30.0,
            theta_redundancy: 20.0,
            theta_cost: 60.0,
            theta_scale: 1.0,
            theta_rel_critical: 0.3,
            w_critical: 1.0,
            w_redundancy: 1.0,
            w_cost: 1.0,
            theta_combined: 60.0,
            theta_eff_cost: 60.0,
            theta_fanin: 2,
            theta_subgraph: 120.0,
            theta_trivial: 10.0,
            cache_speedup: 0.5,
            beta: 0.5,
            delta: 0.0,
            gamma: 0.0,
            lambda: 1.0,
            confidence_threshold: 0.7,
        }
    }
}

impl HeuristicConfig {
    /// Confidence-gated effective threshold `θ_eff = θ / (1 + conf·θ_scale)`
    /// (ADR-0004 §2a). Low confidence raises the bar (conservative
    /// coarsening); high confidence lowers it (finer promotion).
    pub fn theta_eff(&self, theta: f64, confidence: f64) -> f64 {
        theta / (1.0 + confidence * self.theta_scale)
    }
}
