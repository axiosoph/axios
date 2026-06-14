//! Simulation metrics: the machine-readable result and its human summary.
//!
//! All quantities are defined in IBC item 3:
//! - `makespan` — wall-clock time to complete every EP.
//! - `redundant_work` — CPU-time on plan nodes built more than once concurrently (`Σ
//!   d(v)·(builders(v) − 1)`).
//! - `ep_count` — number of entry points `|S|`.
//! - `mean_utilization` — `Σ worker busy time / (workers · makespan)`.
//! - `critical_path_accuracy` — predicted EP-DAG critical path / actual makespan (a perfect
//!   predictor with no contention scores `1.0`).
//! - `max_dispatch_wait` — the largest `dispatch_time − ready_time` over EPs (the fairness /
//!   starvation indicator; bounds the delay credit's effect).

use serde::Serialize;

/// Machine-readable simulation result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metrics {
    /// Wall-clock makespan (seconds).
    pub makespan: f64,
    /// Concurrent redundant work (CPU-seconds).
    pub redundant_work: f64,
    /// Number of entry points scheduled.
    pub ep_count: usize,
    /// Mean worker utilization in `[0, 1]`.
    pub mean_utilization: f64,
    /// Critical-path prediction accuracy in `[0, 1]`.
    pub critical_path_accuracy: f64,
    /// Maximum per-EP dispatch wait (seconds).
    pub max_dispatch_wait: f64,
    /// Objective value `makespan + λ · redundant_work` for the configured λ.
    pub objective: f64,
}

impl Metrics {
    /// Serialize to a single deterministic JSON line.
    pub fn to_json(&self) -> String {
        // `Metrics` contains only finite f64s and integers, so serialization
        // cannot fail; field order is fixed by the struct definition.
        serde_json::to_string(self).expect("metrics serialize")
    }

    /// A multi-line human-readable summary.
    pub fn human_summary(&self) -> String {
        format!(
            "makespan               {:.3}\n\
             redundant_work         {:.3}\n\
             ep_count               {}\n\
             mean_utilization       {:.3}\n\
             critical_path_accuracy {:.3}\n\
             max_dispatch_wait      {:.3}\n\
             objective              {:.3}",
            self.makespan,
            self.redundant_work,
            self.ep_count,
            self.mean_utilization,
            self.critical_path_accuracy,
            self.max_dispatch_wait,
            self.objective,
        )
    }
}
