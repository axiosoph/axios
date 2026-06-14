//! The discrete-event scheduling engine.
//!
//! A single-threaded simulation of the event-driven PEFT dispatch protocol
//! (ADR-0004 §2b): ready EPs are ranked by `avg_OCT + γ·age` (OCT downstream
//! impact plus the delay-credit fairness term), placed on the worker minimising
//! `EFT + OCT` within the bounded dispatch window Δ, and advanced in wall-clock
//! time as builds complete or new EPs arrive. On each completion the store is
//! populated, overlapping mutable EPs are cache-skipped, and dependents cascade
//! to ready.
//!
//! Concurrency model: one EP per worker at a time plus a capacity-feasibility
//! gate (the identical-machines, one-task-per-machine model the Lean Graham
//! bound is stated over; see the worker sketch ledger). Re-coarsening and
//! transient failure are out of scope — the simulator schedules one static
//! request to completion.

use std::collections::BTreeSet;

use crate::coarsen::Coarsening;
use crate::config::HeuristicConfig;
use crate::graph::Graph;
use crate::metrics::Metrics;
use crate::peft::{self, OctTable, Worker};
use crate::rng::SplitMix64;
use crate::trace::{Trace, TraceError};

const EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Ready,
    Dispatched,
    Complete,
}

#[derive(Debug, Clone)]
struct EpRun {
    entry: usize,
    scope: Vec<usize>,
    deps: Vec<usize>,
    status: Status,
    /// System-entry time of the EP's entry node (ADR `RequestArrival`).
    arrival: f64,
    ready_time: f64,
    dispatch_time: f64,
    finish_time: f64,
    worker: usize,
    /// Scope minus the store at dispatch time (the nodes this EP actually built).
    effective: Vec<usize>,
}

/// The simulation engine over a coarsened request.
struct Engine<'a> {
    graph: &'a Graph,
    cfg: &'a HeuristicConfig,
    workers: Vec<Worker>,
    oct: OctTable,
    seed: u64,
    eps: Vec<EpRun>,
    /// EP entry id → index into `eps`.
    by_entry: std::collections::BTreeMap<usize, usize>,
    /// Plan nodes whose outputs are in the global artifact store.
    store: BTreeSet<usize>,
    /// `free[w]` — worker `w` is idle.
    free: Vec<bool>,
    /// Concurrent builders per plan node (for redundant-work accounting).
    builders: Vec<usize>,
    /// Accumulated busy time per worker (for utilization).
    busy_time: Vec<f64>,
    /// Time each busy worker becomes free (for the Δ dispatch hold).
    busy_until: Vec<f64>,
    time: f64,
}

impl<'a> Engine<'a> {
    fn new(
        graph: &'a Graph,
        cfg: &'a HeuristicConfig,
        coarsening: &Coarsening,
        workers: Vec<Worker>,
        seed: u64,
    ) -> Self {
        let oct = peft::compute_oct(graph, cfg, coarsening, &workers);
        let mut eps = Vec::with_capacity(coarsening.eps.len());
        let mut by_entry = std::collections::BTreeMap::new();
        for ep in &coarsening.eps {
            by_entry.insert(ep.entry, eps.len());
            let arrival = graph.arrival(ep.entry);
            // Ready immediately only if it has no dependencies and has arrived.
            let status = if ep.deps.is_empty() && arrival <= 0.0 {
                Status::Ready
            } else {
                Status::Pending
            };
            eps.push(EpRun {
                entry: ep.entry,
                scope: ep.scope.clone(),
                deps: ep.deps.clone(),
                status,
                arrival,
                ready_time: 0.0,
                dispatch_time: 0.0,
                finish_time: 0.0,
                worker: 0,
                effective: Vec::new(),
            });
        }
        let n_w = workers.len();
        Engine {
            graph,
            cfg,
            workers,
            oct,
            seed,
            eps,
            by_entry,
            store: BTreeSet::new(),
            free: vec![true; n_w],
            builders: vec![0usize; graph.len()],
            busy_time: vec![0.0; n_w],
            busy_until: vec![0.0; n_w],
            time: 0.0,
        }
    }

    /// Effective priority of a ready EP: `avg_OCT(e) + γ · age(e)` (ADR-0004
    /// §2b, P12). The delay credit `γ · age` makes a waiting EP's priority rise
    /// without bound, so it eventually overtakes any stream of higher-priority
    /// arrivals — starvation-freedom by construction. At γ = 0 it degenerates
    /// to pure OCT and admits starvation (the model-checked counterexample).
    fn priority(&self, ep: &EpRun) -> f64 {
        let age = (self.time - ep.ready_time).max(0.0);
        self.oct.avg(ep.entry) + self.cfg.gamma * age
    }

    /// Dispatch as many ready EPs as possible at the current time.
    fn dispatch_phase(&mut self) {
        // Ready EPs in descending priority; ties broken by a seed-derived key.
        let mut ready: Vec<usize> = (0..self.eps.len())
            .filter(|&i| self.eps[i].status == Status::Ready)
            .collect();
        ready.sort_by(|&a, &b| {
            let (pa, pb) = (self.priority(&self.eps[a]), self.priority(&self.eps[b]));
            pb.partial_cmp(&pa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ka = SplitMix64::key_for(self.seed, self.eps[a].entry as u64);
                    let kb = SplitMix64::key_for(self.seed, self.eps[b].entry as u64);
                    ka.cmp(&kb)
                })
                .then_with(|| self.eps[a].entry.cmp(&self.eps[b].entry))
        });

        for i in ready {
            let effective: Vec<usize> = self.eps[i]
                .scope
                .iter()
                .copied()
                .filter(|v| !self.store.contains(v))
                .collect();
            if effective.is_empty() {
                // Everything already in the store: cache-skip rather than build.
                self.complete_skip(i);
                continue;
            }
            let avail: Vec<f64> = (0..self.workers.len())
                .map(|w| {
                    if self.free[w] {
                        self.time
                    } else {
                        f64::INFINITY
                    }
                })
                .collect();
            let oct_row = self.oct.row(self.eps[i].entry).to_vec();
            let Some(w) = peft::select_worker(
                self.graph,
                self.cfg,
                &self.workers,
                &effective,
                &oct_row,
                &avail,
            ) else {
                continue; // no feasible worker at all
            };
            if !self.free[w] {
                continue; // all feasible workers busy
            }
            let d = peft::predicted_duration(self.graph, self.cfg, &self.workers[w], &effective);
            // Bounded dispatch window Δ (P9'): a high-confidence EP may be held
            // up to Δ to wait for a higher-affinity worker about to free, unless
            // the delay credit overrides the hold (age ≥ Δ ⇒ release rather than
            // starve).
            if self.should_hold(i, &effective, self.time + d) {
                continue;
            }
            self.free[w] = false;
            self.busy_until[w] = self.time + d;
            for &v in &effective {
                self.builders[v] += 1;
            }
            self.busy_time[w] += d;
            let ep = &mut self.eps[i];
            ep.status = Status::Dispatched;
            ep.dispatch_time = self.time;
            ep.finish_time = self.time + d;
            ep.worker = w;
            ep.effective = effective;
        }
    }

    /// Whether to hold ready EP `i` within the bounded window Δ rather than
    /// dispatch it to the chosen free worker now (whose earliest finish time is
    /// `eft_free`). The hold fires only when prediction confidence is high, the
    /// window is not exhausted (`age < Δ` — the delay-credit override releases
    /// the EP at the boundary rather than letting it starve), and some busy but
    /// feasible worker freeing within Δ would finish the EP strictly sooner.
    ///
    /// (OCT is worker-independent at τ = 0, so comparing earliest finish times
    /// is equivalent to comparing the full `EFT + OCT` objective.)
    fn should_hold(&self, i: usize, effective: &[usize], eft_free: f64) -> bool {
        if self.cfg.delta <= 0.0 {
            return false;
        }
        let conf = self.graph.confidence(self.eps[i].entry);
        if conf < self.cfg.confidence_threshold {
            return false;
        }
        let age = self.time - self.eps[i].ready_time;
        if age >= self.cfg.delta {
            return false; // window exhausted: delay-credit override, dispatch now
        }
        let requirement = peft::resource_requirement(self.graph, effective);
        for w in 0..self.workers.len() {
            if self.free[w] || !peft::feasible(&self.workers[w], &requirement) {
                continue;
            }
            if self.busy_until[w] <= self.time + self.cfg.delta {
                let d = peft::predicted_duration(self.graph, self.cfg, &self.workers[w], effective);
                if self.busy_until[w] + d + EPS < eft_free {
                    return true;
                }
            }
        }
        false
    }

    /// Mark a mutable EP complete without building (cache-skip).
    fn complete_skip(&mut self, i: usize) {
        let ep = &mut self.eps[i];
        ep.status = Status::Complete;
        ep.finish_time = self.time;
        ep.effective = Vec::new();
    }

    /// Earliest finish time among dispatched EPs.
    fn next_completion(&self) -> Option<f64> {
        self.eps
            .iter()
            .filter(|e| e.status == Status::Dispatched)
            .map(|e| e.finish_time)
            .fold(None, |acc, t| match acc {
                Some(m) if m <= t => Some(m),
                _ => Some(t),
            })
    }

    /// Complete all dispatched EPs finishing at `self.time`, freeing their
    /// workers and publishing outputs to the store and local cache.
    fn completion_phase(&mut self) {
        for i in 0..self.eps.len() {
            if self.eps[i].status == Status::Dispatched
                && self.eps[i].finish_time <= self.time + EPS
            {
                let w = self.eps[i].worker;
                self.free[w] = true;
                let effective = self.eps[i].effective.clone();
                for v in effective {
                    self.store.insert(v);
                    self.workers[w].cached.insert(v);
                }
                self.eps[i].status = Status::Complete;
            }
        }
    }

    /// Promote mutable EPs to a fixpoint: a fully-cached scope is cache-skipped;
    /// an EP whose dependencies are complete becomes ready once it has also
    /// arrived (`self.time ≥ arrival`). `ready_time` is stamped when the EP
    /// actually becomes ready, so the delay-credit age is measured from then.
    fn refresh(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.eps.len() {
                if matches!(self.eps[i].status, Status::Pending | Status::Ready)
                    && self.eps[i].scope.iter().all(|v| self.store.contains(v))
                {
                    self.complete_skip(i);
                    changed = true;
                    continue;
                }
                if self.eps[i].status == Status::Pending
                    && self.deps_complete(i)
                    && self.time + EPS >= self.eps[i].arrival
                {
                    self.eps[i].status = Status::Ready;
                    self.eps[i].ready_time = self.time;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Earliest future arrival of a dependency-satisfied EP still waiting to
    /// enter the system — an event the loop must wake for even when no build is
    /// in flight.
    fn next_arrival(&self) -> Option<f64> {
        self.eps
            .iter()
            .filter(|e| e.status == Status::Pending && e.arrival > self.time + EPS)
            .filter(|e| {
                e.deps.iter().all(|&dep| {
                    self.by_entry
                        .get(&dep)
                        .is_some_and(|&j| self.eps[j].status == Status::Complete)
                })
            })
            .map(|e| e.arrival)
            .fold(None, |acc, t| match acc {
                Some(m) if m <= t => Some(m),
                _ => Some(t),
            })
    }

    fn deps_complete(&self, i: usize) -> bool {
        self.eps[i].deps.iter().all(|&dep| {
            self.by_entry
                .get(&dep)
                .is_some_and(|&j| self.eps[j].status == Status::Complete)
        })
    }

    fn all_terminal(&self) -> bool {
        self.eps.iter().all(|e| e.status == Status::Complete)
    }

    fn run(&mut self) -> Result<(), TraceError> {
        loop {
            self.refresh();
            self.dispatch_phase();
            if self.all_terminal() {
                break;
            }
            // Advance to the next event: a build completing or an EP arriving.
            let next = match (self.next_completion(), self.next_arrival()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            let Some(tn) = next else {
                return Err(TraceError::Invalid(
                    "scheduling deadlock: ready EPs but no feasible worker (check capacities)"
                        .into(),
                ));
            };
            // Guard against a non-advancing loop (e.g. a permanent Δ-hold).
            self.time = tn.max(self.time);
            self.completion_phase();
        }
        Ok(())
    }

    fn metrics(&self) -> Metrics {
        let makespan = self
            .eps
            .iter()
            .map(|e| e.finish_time)
            .fold(0.0f64, f64::max);
        let redundant_work: f64 = (0..self.graph.len())
            .map(|v| self.builders[v].saturating_sub(1) as f64 * self.graph.duration(v))
            .sum();
        let total_busy: f64 = self.busy_time.iter().sum();
        let mean_utilization = if makespan > 0.0 && !self.workers.is_empty() {
            total_busy / (self.workers.len() as f64 * makespan)
        } else {
            0.0
        };
        let predicted_cp = self.predicted_critical_path();
        let critical_path_accuracy = if makespan > 0.0 {
            (predicted_cp / makespan).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let max_dispatch_wait = self
            .eps
            .iter()
            .filter(|e| e.status == Status::Complete && !e.effective.is_empty())
            .map(|e| (e.dispatch_time - e.ready_time).max(0.0))
            .fold(0.0f64, f64::max);
        Metrics {
            makespan,
            redundant_work,
            ep_count: self.eps.len(),
            mean_utilization,
            critical_path_accuracy,
            max_dispatch_wait,
            objective: makespan + self.cfg.lambda * redundant_work,
        }
    }

    /// Build the detailed report: metrics plus per-EP dispatch waits and worker
    /// assignments (keyed by entry-node id), for tests that probe a specific
    /// EP's fate — e.g. the starvation victim.
    fn report(&self) -> SimReport {
        let mut ep_waits = std::collections::BTreeMap::new();
        let mut ep_workers = std::collections::BTreeMap::new();
        for e in &self.eps {
            if e.status == Status::Complete && !e.effective.is_empty() {
                let id = self.graph.id(e.entry).to_string();
                ep_waits.insert(id.clone(), (e.dispatch_time - e.ready_time).max(0.0));
                ep_workers.insert(id, self.workers[e.worker].id.clone());
            }
        }
        SimReport {
            metrics: self.metrics(),
            ep_waits,
            ep_workers,
        }
    }

    /// Longest path through the EP DAG by best-worker full-scope duration — the
    /// predicted makespan lower bound.
    fn predicted_critical_path(&self) -> f64 {
        // Memoise over EPs in plan-topo order (dependencies before dependents).
        let topo = self.graph.topo_order().expect("acyclic");
        let mut cp: std::collections::BTreeMap<usize, f64> = std::collections::BTreeMap::new();
        for &node in &topo {
            let Some(&i) = self.by_entry.get(&node) else {
                continue;
            };
            let d_min = self
                .workers
                .iter()
                .map(|w| peft::predicted_duration(self.graph, self.cfg, w, &self.eps[i].scope))
                .fold(f64::INFINITY, f64::min);
            let d_min = if d_min.is_finite() { d_min } else { 0.0 };
            let below = self.eps[i]
                .deps
                .iter()
                .map(|d| cp.get(d).copied().unwrap_or(0.0))
                .fold(0.0, f64::max);
            cp.insert(node, d_min + below);
        }
        cp.values().copied().fold(0.0, f64::max)
    }
}

/// A detailed simulation result: the metrics plus per-EP dispatch waits and
/// worker assignments, keyed by the EP's entry-node id.
#[derive(Debug, Clone)]
pub struct SimReport {
    /// Aggregate metrics.
    pub metrics: Metrics,
    /// Dispatch wait (`dispatch_time − ready_time`) per dispatched EP.
    pub ep_waits: std::collections::BTreeMap<String, f64>,
    /// Assigned worker id per dispatched EP.
    pub ep_workers: std::collections::BTreeMap<String, String>,
}

/// Run a full simulation and return the detailed report: cache-filter, coarsen,
/// build workers, then schedule the request to completion.
pub fn simulate_report(
    trace: &Trace,
    cfg: &HeuristicConfig,
    seed: u64,
) -> Result<SimReport, TraceError> {
    let graph = Graph::from_trace(trace)?;
    let coarsening = Coarsening::build(&graph, cfg);
    let workers = peft::build_workers(trace, &graph);
    if workers.is_empty() {
        return Err(TraceError::Invalid("no workers to schedule on".into()));
    }
    let mut engine = Engine::new(&graph, cfg, &coarsening, workers, seed);
    engine.run()?;
    Ok(engine.report())
}

/// Run a full simulation, returning just the aggregate metrics.
pub fn simulate(trace: &Trace, cfg: &HeuristicConfig, seed: u64) -> Result<Metrics, TraceError> {
    Ok(simulate_report(trace, cfg, seed)?.metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Variant;

    /// Config with the duration model neutralised (no cache/fit adjustment) so
    /// makespans equal raw durations and are hand-verifiable.
    fn clean(theta_cost: f64) -> HeuristicConfig {
        HeuristicConfig {
            variant: Variant::H1,
            theta_cost,
            theta_critical: 1e9,
            theta_redundancy: 1e9,
            theta_scale: 0.0,
            cache_speedup: 0.0,
            beta: 0.0,
            gamma: 0.0,
            delta: 0.0,
            lambda: 1.0,
            ..HeuristicConfig::default()
        }
    }

    fn run(json: &str, cfg: &HeuristicConfig) -> Metrics {
        let trace = Trace::from_json(json).expect("valid");
        simulate(&trace, cfg, 42).expect("runs")
    }

    #[test]
    fn diamond_reaches_optimal_makespan() {
        let trace = Trace::load("fixtures/diamond.json").expect("fixture");
        // theta_cost=0 promotes every node to its own EP.
        let m = simulate(&trace, &clean(0.0), 42).expect("runs");
        // leaf(2) then a(5)||b(5) then top(1) => 8 on two workers.
        assert_eq!(m.makespan, 8.0);
        assert_eq!(m.ep_count, 4);
        assert_eq!(m.redundant_work, 0.0);
        assert_eq!(m.max_dispatch_wait, 0.0);
        // total busy = 2+5+5+1 = 13; 2 workers * 8 = 16.
        assert!((m.mean_utilization - 13.0 / 16.0).abs() < 1e-9);
        // predicted cp = 1+5+2 = 8 == makespan.
        assert!((m.critical_path_accuracy - 1.0).abs() < 1e-9);
    }

    #[test]
    fn concurrent_shared_dep_is_redundant_work() {
        // Two roots share an unpromoted dep; two workers build both EPs at once,
        // so the shared dep is built twice concurrently.
        let json = r#"{
            "nodes": [
                {"id": "p1", "duration": 1.0},
                {"id": "p2", "duration": 1.0},
                {"id": "shared", "duration": 4.0}
            ],
            "edges": [{"from": "p1", "to": "shared"}, {"from": "p2", "to": "shared"}],
            "workers": [{"id": "w0"}, {"id": "w1"}]
        }"#;
        let m = run(json, &clean(1e9)); // inert: nothing promoted but roots
        assert_eq!(m.ep_count, 2);
        assert_eq!(m.redundant_work, 4.0); // shared (d=4) built by both EPs
        assert_eq!(m.makespan, 5.0); // each EP builds {root, shared} = 1+4
    }

    #[test]
    fn sequential_shared_dep_is_cache_skipped() {
        // Same DAG, one worker: the second EP cache-skips the shared dep.
        let json = r#"{
            "nodes": [
                {"id": "p1", "duration": 1.0},
                {"id": "p2", "duration": 1.0},
                {"id": "shared", "duration": 4.0}
            ],
            "edges": [{"from": "p1", "to": "shared"}, {"from": "p2", "to": "shared"}],
            "workers": [{"id": "w0"}]
        }"#;
        let m = run(json, &clean(1e9));
        assert_eq!(m.redundant_work, 0.0); // shared built once, reused
        // first EP {root,shared}=5, second EP {root}=1 (shared skipped) => 6.
        assert_eq!(m.makespan, 6.0);
    }

    #[test]
    fn fully_cached_request_is_empty() {
        let json = r#"{
            "nodes": [{"id": "a", "duration": 5.0}],
            "workers": [{"id": "w0"}],
            "store_cached": ["a"]
        }"#;
        let m = run(json, &clean(0.0));
        assert_eq!(m.ep_count, 0);
        assert_eq!(m.makespan, 0.0);
    }

    #[test]
    fn bounded_window_holds_for_a_warmer_worker() {
        // F (OCT 5 via payload P) is dispatched first to w0; X (cached on w0)
        // then chooses between cold w1 (free now, d=10) and warm w0 (busy until
        // 3, d=2). With Δ=0 X must dispatch immediately to w1; with Δ≥3 and high
        // confidence it is held for w0.
        let json = r#"{
            "nodes": [
                {"id": "X", "duration": 10.0, "confidence": 1.0},
                {"id": "F", "duration": 3.0},
                {"id": "P", "duration": 5.0}
            ],
            "edges": [{"from": "P", "to": "F"}],
            "workers": [{"id": "w0", "cached": ["X"]}, {"id": "w1"}]
        }"#;
        let cfg = |delta: f64| HeuristicConfig {
            variant: Variant::H1,
            theta_cost: 0.0,
            theta_critical: 1e9,
            theta_redundancy: 1e9,
            theta_scale: 0.0,
            cache_speedup: 0.8,
            beta: 0.0,
            // A small delay credit lets the older X outrank the freshly-ready P
            // for the warm worker at t=3 (otherwise they tie and the seed
            // decides); it does not affect the t=0 hold decision.
            gamma: 0.1,
            delta,
            confidence_threshold: 0.7,
            lambda: 1.0,
            ..HeuristicConfig::default()
        };
        let trace = Trace::from_json(json).expect("valid");

        let immediate = simulate_report(&trace, &cfg(0.0), 42).expect("runs");
        assert_eq!(
            immediate.ep_workers["X"], "w1",
            "Δ=0 dispatches X to the cold free worker"
        );
        assert_eq!(immediate.metrics.makespan, 10.0);

        let held = simulate_report(&trace, &cfg(5.0), 42).expect("runs");
        assert_eq!(
            held.ep_workers["X"], "w0",
            "Δ=5 holds X for the warm worker"
        );
        assert_eq!(held.metrics.makespan, 8.0); // X on warm finishes at 5; P at 8
    }
}
