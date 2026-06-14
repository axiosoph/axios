//! PEFT placement: the Option-C duration model, the Optimistic Cost Table
//! (OCT), and EFT+OCT worker selection (ADR-0004 §2b/§3).
//!
//! - **Duration (Option C, §3)**: cache affinity and resource fit are folded into the per-worker
//!   predicted duration `d(e, w)` rather than evaluated as a separate placement score: `d(e, w) =
//!   base(e) · speed(w) · (1 − affinity·cache_speedup) · (1 + β·util)` where `base(e)` is the
//!   aggregate isolated cost of the EP's scope, `affinity` is the fraction of the scope already
//!   cached on `w`, and `util` is the EP's resource demand as a fraction of `w`'s capacity (poor
//!   fit = little headroom = larger `d`).
//! - **OCT (§2b)**: computed backward over the EP DAG, `OCT(e, w) = max_{succ} min_{w'} [OCT(e',
//!   w') + d(e', w')]`, exit EPs `0`.
//! - **Selection (§2b, spec `[eos-scheduler-placement]`)**: among feasible workers, minimise
//!   `EFT(e, w) + OCT(e, w)`, `EFT = avail(w) + d(e, w)`.

use std::collections::{BTreeMap, BTreeSet};

use crate::coarsen::Coarsening;
use crate::config::HeuristicConfig;
use crate::graph::Graph;
use crate::trace::Trace;

/// Resource dimension key used for the EP's single derivable demand.
const MEM_DIM: &str = "mem";

/// A runtime worker: identity, speed, capacity, and (mutable) local cache.
#[derive(Debug, Clone)]
pub struct Worker {
    /// Worker id (opaque).
    pub id: String,
    /// Duration multiplier.
    pub speed: f64,
    /// Reported capacity vector.
    pub capacity: BTreeMap<String, u64>,
    /// Node indices cached locally (grows as the worker completes EPs).
    pub cached: BTreeSet<usize>,
}

/// Build the worker pool from a trace, resolving cached ids to node indices and
/// ordering by id for determinism. Cached ids not present in the uncached
/// sub-DAG (already filtered / unknown) are ignored.
pub fn build_workers(trace: &Trace, graph: &Graph) -> Vec<Worker> {
    let mut workers: Vec<Worker> = trace
        .workers
        .iter()
        .map(|w| Worker {
            id: w.id.clone(),
            speed: w.speed,
            capacity: w.capacity.clone(),
            cached: w
                .cached
                .iter()
                .filter_map(|id| graph.index_of(id))
                .collect(),
        })
        .collect();
    workers.sort_by(|a, b| a.id.cmp(&b.id));
    workers
}

/// The EP's resource demand vector. Only the memory dimension is derivable from
/// trace data: the EP's peak memory is the max over its scope.
pub fn resource_requirement(graph: &Graph, scope: &[usize]) -> BTreeMap<String, u64> {
    let mem = scope.iter().map(|&v| graph.peak_mem(v)).max().unwrap_or(0);
    let mut req = BTreeMap::new();
    if mem > 0 {
        req.insert(MEM_DIM.to_string(), mem);
    }
    req
}

/// Whether `worker` can host an EP with the given resource requirement. A
/// dimension the worker does not declare is treated as unconstrained.
pub fn feasible(worker: &Worker, requirement: &BTreeMap<String, u64>) -> bool {
    requirement
        .iter()
        .all(|(dim, &req)| match worker.capacity.get(dim) {
            Some(&cap) => req <= cap,
            None => true,
        })
}

/// Option-C predicted duration `d(e, w)` for building `scope` on `worker`.
pub fn predicted_duration(
    graph: &Graph,
    cfg: &HeuristicConfig,
    worker: &Worker,
    scope: &[usize],
) -> f64 {
    if scope.is_empty() {
        return 0.0;
    }
    let base: f64 = scope.iter().map(|&v| graph.duration(v)).sum();
    let cached = scope.iter().filter(|v| worker.cached.contains(v)).count();
    let affinity = cached as f64 / scope.len() as f64;

    // Resource utilisation: demand as a fraction of declared capacity, averaged
    // over the demanded dimensions. Higher util = tighter fit = larger penalty.
    let req = resource_requirement(graph, scope);
    let mut util_sum = 0.0;
    let mut util_dims = 0u32;
    for (dim, &r) in &req {
        if let Some(&cap) = worker.capacity.get(dim)
            && cap > 0
        {
            util_sum += (r as f64 / cap as f64).clamp(0.0, 1.0);
            util_dims += 1;
        }
    }
    let util = if util_dims == 0 {
        0.0
    } else {
        util_sum / util_dims as f64
    };

    base * worker.speed * (1.0 - affinity * cfg.cache_speedup) * (1.0 + cfg.beta * util)
}

/// Optimistic Cost Table: per-EP, per-worker OCT values, keyed by entry index.
#[derive(Debug, Clone)]
pub struct OctTable {
    per_ep: BTreeMap<usize, Vec<f64>>,
    n_workers: usize,
}

impl OctTable {
    /// OCT row for an EP (one value per worker, worker index order).
    pub fn row(&self, entry: usize) -> &[f64] {
        &self.per_ep[&entry]
    }

    /// Average OCT across workers — the `avg_OCT(e)` priority term.
    pub fn avg(&self, entry: usize) -> f64 {
        if self.n_workers == 0 {
            return 0.0;
        }
        self.per_ep[&entry].iter().sum::<f64>() / self.n_workers as f64
    }
}

/// Compute the OCT table over the full EP DAG (ADR-0004 §2b). Durations use the
/// full EP scope (the optimistic, pre-cache-skip estimate).
pub fn compute_oct(
    graph: &Graph,
    cfg: &HeuristicConfig,
    coarsening: &Coarsening,
    workers: &[Worker],
) -> OctTable {
    let n_w = workers.len();
    // Per-worker d(e, w) for every EP, and EP-DAG successor adjacency.
    let mut dew: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    let mut succ: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for ep in &coarsening.eps {
        let row: Vec<f64> = workers
            .iter()
            .map(|w| predicted_duration(graph, cfg, w, &ep.scope))
            .collect();
        dew.insert(ep.entry, row);
        succ.entry(ep.entry).or_default();
        for &dep in &ep.deps {
            succ.entry(dep).or_default().push(ep.entry);
        }
    }

    // Backward pass: an EP's OCT needs its successors' OCT first. Process EP
    // entries in reverse topological order of the plan DAG (a dependency EP's
    // entry precedes its dependents', so reverse yields successors-first).
    let topo = graph.topo_order().expect("acyclic");
    let ep_entries: BTreeSet<usize> = coarsening.eps.iter().map(|e| e.entry).collect();
    let mut oct: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for &node in topo.iter().rev() {
        if !ep_entries.contains(&node) {
            continue;
        }
        // With τ = 0 (single cluster, ADR-0004 §2b) the recurrence is
        // independent of the host worker w_k, so OCT(e, ·) is one value
        // replicated across the worker row.
        let mut worst = 0.0f64;
        for &s in &succ[&node] {
            // min over workers of OCT(s, w') + d(s, w').
            let s_oct = &oct[&s];
            let s_dew = &dew[&s];
            let best = (0..n_w)
                .map(|w2| s_oct[w2] + s_dew[w2])
                .fold(f64::INFINITY, f64::min);
            if best.is_finite() && best > worst {
                worst = best;
            }
        }
        oct.insert(node, vec![worst; n_w]);
    }

    OctTable {
        per_ep: oct,
        n_workers: n_w,
    }
}

/// Select the worker minimising `EFT(e, w) + OCT(e, w)` among feasible workers,
/// where `EFT = avail(w) + d(e, w)`. `avail[w]` is the worker's earliest free
/// time. Returns the worker index, or `None` if no worker is feasible.
///
/// Ties are broken by the lowest `eft + oct`, then by worker index, keeping
/// selection deterministic without consulting the RNG (genuine ties between
/// distinct workers are resolved by the caller when needed).
pub fn select_worker(
    graph: &Graph,
    cfg: &HeuristicConfig,
    workers: &[Worker],
    scope: &[usize],
    oct_row: &[f64],
    avail: &[f64],
) -> Option<usize> {
    let requirement = resource_requirement(graph, scope);
    let mut best: Option<(f64, usize)> = None;
    for (w, worker) in workers.iter().enumerate() {
        if !feasible(worker, &requirement) {
            continue;
        }
        let d = predicted_duration(graph, cfg, worker, scope);
        let eft = avail[w] + d;
        let objective = eft + oct_row[w];
        match best {
            Some((b, _)) if objective >= b => {},
            _ => best = Some((objective, w)),
        }
    }
    best.map(|(_, w)| w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Variant;

    fn graph(json: &str) -> Graph {
        Graph::from_trace(&Trace::from_json(json).expect("valid")).expect("acyclic")
    }

    fn cfg() -> HeuristicConfig {
        HeuristicConfig {
            cache_speedup: 0.5,
            beta: 1.0,
            theta_scale: 0.0,
            ..HeuristicConfig::default()
        }
    }

    #[test]
    fn affinity_and_speed_shrink_duration() {
        let g = graph(r#"{"nodes":[{"id":"a","duration":10.0}],"workers":[{"id":"w0"}]}"#);
        let a = g.index_of("a").unwrap();
        let cold = Worker {
            id: "c".into(),
            speed: 1.0,
            capacity: BTreeMap::new(),
            cached: BTreeSet::new(),
        };
        let warm = Worker {
            id: "h".into(),
            speed: 1.0,
            capacity: BTreeMap::new(),
            cached: BTreeSet::from([a]),
        };
        let fast = Worker {
            id: "f".into(),
            speed: 0.5,
            capacity: BTreeMap::new(),
            cached: BTreeSet::new(),
        };
        assert_eq!(predicted_duration(&g, &cfg(), &cold, &[a]), 10.0);
        // Fully cached: (1 - 1*0.5) = 0.5 => 5.0.
        assert_eq!(predicted_duration(&g, &cfg(), &warm, &[a]), 5.0);
        // Speed 0.5 => 5.0.
        assert_eq!(predicted_duration(&g, &cfg(), &fast, &[a]), 5.0);
    }

    #[test]
    fn resource_penalty_and_feasibility() {
        let g = graph(
            r#"{"nodes":[{"id":"a","duration":10.0,"peak_mem":4000}],"workers":[{"id":"w0"}]}"#,
        );
        let a = g.index_of("a").unwrap();
        let big = Worker {
            id: "big".into(),
            speed: 1.0,
            capacity: BTreeMap::from([("mem".into(), 8000)]),
            cached: BTreeSet::new(),
        };
        let small = Worker {
            id: "small".into(),
            speed: 1.0,
            capacity: BTreeMap::from([("mem".into(), 2000)]),
            cached: BTreeSet::new(),
        };
        // util = 4000/8000 = 0.5; beta=1 => 10 * (1 + 0.5) = 15.
        assert_eq!(predicted_duration(&g, &cfg(), &big, &[a]), 15.0);
        let req = resource_requirement(&g, &[a]);
        assert!(feasible(&big, &req));
        assert!(!feasible(&small, &req), "4000 > 2000 capacity");
    }

    #[test]
    fn oct_is_downstream_cost() {
        // chain: top(d=3) depends on leaf(d=5). Promote leaf via theta_cost=4.
        let g = graph(
            r#"{
                "nodes":[{"id":"top","duration":3.0},{"id":"leaf","duration":5.0}],
                "edges":[{"from":"top","to":"leaf"}],
                "workers":[{"id":"w0"}]
            }"#,
        );
        let c = Coarsening::build(
            &g,
            &HeuristicConfig {
                variant: Variant::H1,
                theta_cost: 4.0,
                theta_critical: 1e9,
                theta_redundancy: 1e9,
                theta_scale: 0.0,
                ..cfg()
            },
        );
        let workers = build_workers(
            &Trace::from_json(r#"{"nodes":[],"workers":[{"id":"w0"}]}"#).unwrap(),
            &g,
        );
        let oct = compute_oct(&g, &cfg(), &c, &workers);
        let top = g.index_of("top").unwrap();
        let leaf = g.index_of("leaf").unwrap();
        // top is exit (nothing depends on it): OCT 0. leaf's successor is top:
        // OCT(leaf) = d(top) = 3 (single worker, no cache/penalty).
        assert_eq!(oct.avg(top), 0.0);
        assert_eq!(oct.avg(leaf), 3.0);
    }

    #[test]
    fn selection_prefers_warm_then_minimises_objective() {
        let g =
            graph(r#"{"nodes":[{"id":"a","duration":10.0}],"workers":[{"id":"w0"},{"id":"w1"}]}"#);
        let a = g.index_of("a").unwrap();
        let workers = vec![
            Worker {
                id: "cold".into(),
                speed: 1.0,
                capacity: BTreeMap::new(),
                cached: BTreeSet::new(),
            },
            Worker {
                id: "warm".into(),
                speed: 1.0,
                capacity: BTreeMap::new(),
                cached: BTreeSet::from([a]),
            },
        ];
        let oct_row = vec![0.0, 0.0];
        let avail = vec![0.0, 0.0];
        // warm worker (index 1) has d=5 < cold d=10 => selected.
        assert_eq!(
            select_worker(&g, &cfg(), &workers, &[a], &oct_row, &avail),
            Some(1)
        );
    }
}
