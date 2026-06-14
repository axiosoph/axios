//! The uncached plan DAG and its structural metrics.
//!
//! [`Graph`] is built from a [`Trace`] after the ingress **cache filter**
//! (ADR-0004 §2b): plans already in the global artifact store are removed, and
//! the remaining nodes form the *uncached sub-DAG* the coarsener operates on.
//!
//! Node indices are assigned in sorted-id order so that every downstream pass
//! (coarsening, PEFT, the event loop) iterates deterministically regardless of
//! trace authoring order.
//!
//! Metrics, all defined over the uncached sub-DAG with edges oriented
//! *dependent → dependency*:
//!
//! - `d(v)` — isolated build duration (the node weight).
//! - `critical_path(v)` — longest weighted dependency chain at or below `v` (`d(v) + max over
//!   deps`); the serial portion that constrains makespan.
//! - `subgraph_cost(v)` — total weight of `v` and its transitive dependency closure, each node
//!   counted once (shared deps are not double-counted).
//! - `fan_in(v)` — number of dependents (in-degree in `G∪`); high fan-in marks a convergence point
//!   shared by multiple entry-point scopes.

use std::collections::{BTreeMap, BTreeSet};

use crate::trace::{Trace, TraceError};

/// The uncached plan DAG with per-node attributes and adjacency.
#[derive(Debug, Clone)]
pub struct Graph {
    /// Node id by index (ascending id order).
    ids: Vec<String>,
    /// Isolated build duration `d(v)` by index.
    duration: Vec<f64>,
    /// Predicted peak memory (bytes) by index; `0` when unknown.
    peak_mem: Vec<u64>,
    /// Atom marker by index.
    is_atom: Vec<bool>,
    /// Prediction confidence in `[0, 1]` by index.
    confidence: Vec<f64>,
    /// Arrival time (system-entry) by index; `0.0` when present from the start.
    arrival: Vec<f64>,
    /// `deps[v]` = indices `v` depends on (ascending).
    deps: Vec<Vec<usize>>,
    /// `dependents[v]` = indices that depend on `v` (ascending).
    dependents: Vec<Vec<usize>>,
    /// id → index.
    index: BTreeMap<String, usize>,
}

impl Graph {
    /// Build the uncached sub-DAG from a trace, applying the cache filter and
    /// checking acyclicity.
    ///
    /// Nodes whose id is in `store_cached` are removed; edges incident to a
    /// removed node are dropped (a cached dependency is already satisfied; a
    /// cached dependent never needs scheduling).
    pub fn from_trace(trace: &Trace) -> Result<Self, TraceError> {
        let cached: BTreeSet<&str> = trace.store_cached.iter().map(String::as_str).collect();

        // Cache filter + deterministic indexing by sorted id.
        let mut kept: Vec<&crate::trace::TraceNode> = trace
            .nodes
            .iter()
            .filter(|n| !cached.contains(n.id.as_str()))
            .collect();
        kept.sort_by(|a, b| a.id.cmp(&b.id));

        let mut index = BTreeMap::new();
        let mut ids = Vec::with_capacity(kept.len());
        let mut duration = Vec::with_capacity(kept.len());
        let mut peak_mem = Vec::with_capacity(kept.len());
        let mut is_atom = Vec::with_capacity(kept.len());
        let mut confidence = Vec::with_capacity(kept.len());
        let mut arrival = Vec::with_capacity(kept.len());
        for (i, n) in kept.iter().enumerate() {
            index.insert(n.id.clone(), i);
            ids.push(n.id.clone());
            duration.push(n.duration);
            peak_mem.push(n.peak_mem.unwrap_or(0));
            is_atom.push(n.is_atom);
            confidence.push(n.confidence());
            arrival.push(n.arrival.unwrap_or(0.0).max(0.0));
        }

        let n = kept.len();
        let mut deps = vec![BTreeSet::new(); n];
        let mut dependents = vec![BTreeSet::new(); n];
        for e in &trace.edges {
            let (Some(&from), Some(&to)) = (index.get(&e.from), index.get(&e.to)) else {
                continue; // edge incident to a cached (removed) node
            };
            deps[from].insert(to);
            dependents[to].insert(from);
        }
        let deps: Vec<Vec<usize>> = deps.into_iter().map(|s| s.into_iter().collect()).collect();
        let dependents: Vec<Vec<usize>> = dependents
            .into_iter()
            .map(|s| s.into_iter().collect())
            .collect();

        let graph = Graph {
            ids,
            duration,
            peak_mem,
            is_atom,
            confidence,
            arrival,
            deps,
            dependents,
            index,
        };
        graph.topo_order().map_err(TraceError::Invalid)?;
        Ok(graph)
    }

    /// Number of uncached nodes.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the uncached sub-DAG is empty (all plans were cache hits).
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Node id at `v`.
    pub fn id(&self, v: usize) -> &str {
        &self.ids[v]
    }

    /// Index of node `id`, if present in the uncached sub-DAG.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// Isolated build duration `d(v)`.
    pub fn duration(&self, v: usize) -> f64 {
        self.duration[v]
    }

    /// Predicted peak memory of `v` in bytes (`0` when unknown).
    pub fn peak_mem(&self, v: usize) -> u64 {
        self.peak_mem[v]
    }

    /// Whether `v` is marked as an atom.
    pub fn is_atom(&self, v: usize) -> bool {
        self.is_atom[v]
    }

    /// Prediction confidence of `v` in `[0, 1]`.
    pub fn confidence(&self, v: usize) -> f64 {
        self.confidence[v]
    }

    /// System-entry (arrival) time of `v`; `0.0` when present from the start.
    pub fn arrival(&self, v: usize) -> f64 {
        self.arrival[v]
    }

    /// Dependencies of `v` (built before `v`).
    pub fn deps(&self, v: usize) -> &[usize] {
        &self.deps[v]
    }

    /// Dependents of `v` (built after `v`).
    pub fn dependents(&self, v: usize) -> &[usize] {
        &self.dependents[v]
    }

    /// In-degree `fan_in(v)` = number of dependents.
    pub fn fan_in(&self, v: usize) -> usize {
        self.dependents[v].len()
    }

    /// Roots of the uncached sub-DAG: nodes nothing depends on (the top-level
    /// requested plans). Always entry points. Ascending index order.
    pub fn roots(&self) -> Vec<usize> {
        (0..self.len())
            .filter(|&v| self.dependents[v].is_empty())
            .collect()
    }

    /// Topological order with every node's dependencies before itself. Returns
    /// the offending description if the graph contains a cycle.
    pub fn topo_order(&self) -> Result<Vec<usize>, String> {
        let n = self.len();
        // Count of unsatisfied dependencies per node.
        let mut indeg: Vec<usize> = self.deps.iter().map(Vec::len).collect();
        // Deterministic Kahn's algorithm: a sorted ready frontier.
        let mut ready: BTreeSet<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(&v) = ready.iter().next() {
            ready.remove(&v);
            order.push(v);
            for &dependent in &self.dependents[v] {
                indeg[dependent] -= 1;
                if indeg[dependent] == 0 {
                    ready.insert(dependent);
                }
            }
        }
        if order.len() != n {
            return Err(format!(
                "plan DAG has a cycle ({} of {} nodes ordered)",
                order.len(),
                n
            ));
        }
        Ok(order)
    }

    /// `critical_path(v)` for every node: the longest weighted dependency chain
    /// at or below `v`.
    pub fn critical_paths(&self) -> Vec<f64> {
        let mut cp = vec![0.0f64; self.len()];
        // Dependencies precede dependents, so this order fills deps first.
        for &v in &self.topo_order().expect("acyclic: checked at construction") {
            let mut best = 0.0f64;
            for &c in &self.deps[v] {
                if cp[c] > best {
                    best = cp[c];
                }
            }
            cp[v] = self.duration[v] + best;
        }
        cp
    }

    /// `subgraph_cost(v)` for every node: total weight of `v` and its
    /// transitive dependency closure, each node counted once.
    pub fn subgraph_costs(&self) -> Vec<f64> {
        (0..self.len()).map(|v| self.subgraph_cost(v)).collect()
    }

    /// `subgraph_cost(v)` for a single node.
    pub fn subgraph_cost(&self, v: usize) -> f64 {
        let mut seen = BTreeSet::new();
        let mut stack = vec![v];
        let mut total = 0.0;
        while let Some(u) = stack.pop() {
            if seen.insert(u) {
                total += self.duration[u];
                stack.extend_from_slice(&self.deps[u]);
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::Trace;

    fn graph(json: &str) -> Graph {
        Graph::from_trace(&Trace::from_json(json).expect("valid trace")).expect("acyclic")
    }

    const DIAMOND: &str = r#"{
        "nodes": [
            {"id": "top", "duration": 1.0},
            {"id": "a", "duration": 2.0},
            {"id": "b", "duration": 3.0},
            {"id": "leaf", "duration": 4.0}
        ],
        "edges": [
            {"from": "top", "to": "a"}, {"from": "top", "to": "b"},
            {"from": "a", "to": "leaf"}, {"from": "b", "to": "leaf"}
        ],
        "workers": [{"id": "w0"}]
    }"#;

    #[test]
    fn diamond_metrics_match_hand_computation() {
        let g = graph(DIAMOND);
        let (top, a, b, leaf) = (
            g.index_of("top").unwrap(),
            g.index_of("a").unwrap(),
            g.index_of("b").unwrap(),
            g.index_of("leaf").unwrap(),
        );
        let cp = g.critical_paths();
        // cp(leaf)=4, cp(a)=6, cp(b)=7, cp(top)=1+max(6,7)=8.
        assert_eq!(cp[leaf], 4.0);
        assert_eq!(cp[a], 6.0);
        assert_eq!(cp[b], 7.0);
        assert_eq!(cp[top], 8.0);
        // subgraph_cost(top)=1+2+3+4=10 (leaf counted once).
        assert_eq!(g.subgraph_cost(top), 10.0);
        assert_eq!(g.subgraph_cost(a), 6.0);
        assert_eq!(g.subgraph_cost(leaf), 4.0);
        // fan_in: leaf=2 (a,b), a=1, b=1, top=0.
        assert_eq!(g.fan_in(leaf), 2);
        assert_eq!(g.fan_in(a), 1);
        assert_eq!(g.fan_in(top), 0);
        assert_eq!(g.roots(), vec![top]);
    }

    #[test]
    fn cache_filter_removes_node_and_incident_edges() {
        let json = r#"{
            "nodes": [
                {"id": "top", "duration": 1.0},
                {"id": "a", "duration": 2.0},
                {"id": "b", "duration": 3.0},
                {"id": "leaf", "duration": 4.0}
            ],
            "edges": [
                {"from": "top", "to": "a"}, {"from": "top", "to": "b"},
                {"from": "a", "to": "leaf"}, {"from": "b", "to": "leaf"}
            ],
            "workers": [{"id": "w0"}],
            "store_cached": ["leaf"]
        }"#;
        let g = graph(json);
        assert_eq!(g.len(), 3);
        assert!(g.index_of("leaf").is_none());
        let top = g.index_of("top").unwrap();
        let a = g.index_of("a").unwrap();
        // leaf gone: cp(a)=2, cp(top)=1+max(2,3)=4; subgraph_cost(top)=1+2+3=6.
        assert_eq!(g.critical_paths()[a], 2.0);
        assert_eq!(g.critical_paths()[top], 4.0);
        assert_eq!(g.subgraph_cost(top), 6.0);
        assert_eq!(g.deps(a), &[] as &[usize]);
    }

    #[test]
    fn chain_metrics() {
        let json = r#"{
            "nodes": [
                {"id": "top", "duration": 1.0},
                {"id": "mid", "duration": 2.0},
                {"id": "leaf", "duration": 4.0}
            ],
            "edges": [{"from": "top", "to": "mid"}, {"from": "mid", "to": "leaf"}],
            "workers": [{"id": "w0"}]
        }"#;
        let g = graph(json);
        let top = g.index_of("top").unwrap();
        assert_eq!(g.critical_paths()[top], 7.0); // 1+2+4
        assert_eq!(g.subgraph_cost(top), 7.0);
        assert_eq!(g.roots(), vec![top]);
    }

    #[test]
    fn rejects_cycle() {
        let json = r#"{
            "nodes": [{"id": "a", "duration": 1.0}, {"id": "b", "duration": 1.0}],
            "edges": [{"from": "a", "to": "b"}, {"from": "b", "to": "a"}],
            "workers": [{"id": "w0"}]
        }"#;
        let trace = Trace::from_json(json).expect("parses");
        assert!(Graph::from_trace(&trace).is_err(), "cycle must be rejected");
    }
}
