//! Entry-point coarsening: the bottom-up promotion pass (ADR-0004 §2a).
//!
//! The coarsener selects a set of **promoted** nodes — the entry points (EPs) —
//! and assigns every uncached node to one or more EP scopes. Each EP covers the
//! nodes reachable downward from its entry without crossing another promoted
//! node; a node reachable from two EPs lands in both scopes, which is exactly
//! the *concurrent redundant work* the objective penalises.
//!
//! Promotion criteria are the variant axis (H1–H4); the initial cover is the
//! seeding axis (`from-scratch` vs `atom-seeded`). The two are treated as
//! orthogonal: seeding contributes extra entry points (non-trivial atom
//! boundaries) and the selected variant's criteria then refine *all* nodes.
//!
//! Note on the atom-seeded refinement: ADR-0004 §2a step (4) phrases
//! refinement as "run the **H1** promotion criteria within an atom". This
//! simulator generalises refinement to the *selected* variant so the seeding
//! axis stays cleanly orthogonal to the variant axis (a well-defined
//! H{1..4} × {scratch, atom} cross-product); for `--variant H1` the two
//! readings coincide. This is a documented, plastic concretisation
//! (campaign finding, see report).

use std::collections::BTreeSet;

use crate::config::{HeuristicConfig, Variant};
use crate::graph::Graph;

/// One coarsened entry point: a record in the scheduling table `T` (spec
/// `EpRecord`). The entry node index doubles as the stable EP id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ep {
    /// Entry node index; the stable EP id.
    pub entry: usize,
    /// `G∪` node indices covered by this EP (ascending, includes `entry`).
    pub scope: Vec<usize>,
    /// EP-level dependency pointers `E_S`: entry indices of the EPs this EP
    /// depends on (ascending).
    pub deps: Vec<usize>,
}

/// The result of a coarsening pass: the EP set `S`, ordered by entry index.
#[derive(Debug, Clone)]
pub struct Coarsening {
    /// Entry points, ascending by `entry`.
    pub eps: Vec<Ep>,
}

/// Precomputed promotion-criterion inputs.
struct Metrics {
    /// `critical_path(v)`.
    cp: Vec<f64>,
    /// Minimum confidence along `v`'s critical chain (weakest-link, ADR §2a).
    cp_conf: Vec<f64>,
    /// `subgraph_cost(v)`.
    subgraph: Vec<f64>,
}

impl Metrics {
    fn compute(g: &Graph) -> Self {
        let cp = g.critical_paths();
        let subgraph = g.subgraph_costs();
        let order = g.topo_order().expect("acyclic: checked at construction");
        let mut cp_conf = vec![0.0f64; g.len()];
        for &v in &order {
            // Pick the dependency achieving the longest chain (lowest index on
            // ties, for determinism); the weakest-link confidence propagates.
            let mut best_cp = f64::NEG_INFINITY;
            let mut best_conf = f64::INFINITY;
            for &c in g.deps(v) {
                if cp[c] > best_cp {
                    best_cp = cp[c];
                    best_conf = cp_conf[c];
                }
            }
            cp_conf[v] = if g.deps(v).is_empty() {
                g.confidence(v)
            } else {
                g.confidence(v).min(best_conf)
            };
        }
        Metrics {
            cp,
            cp_conf,
            subgraph,
        }
    }
}

impl Coarsening {
    /// Run the coarsening pass for `config` over `graph`.
    pub fn build(graph: &Graph, config: &HeuristicConfig) -> Self {
        let m = Metrics::compute(graph);
        let promoted = select_promoted(graph, config, &m);
        let eps = build_eps(graph, &promoted);
        Coarsening { eps }
    }

    /// Entry node indices of the promoted EPs (ascending). Convenience for
    /// tests and divergence comparisons.
    pub fn entries(&self) -> Vec<usize> {
        self.eps.iter().map(|e| e.entry).collect()
    }
}

/// Whether node `v` fires the variant's promotion criteria.
fn fires(g: &Graph, cfg: &HeuristicConfig, m: &Metrics, v: usize) -> bool {
    let d = g.duration(v);
    let fan_in = g.fan_in(v) as f64;
    let conf = g.confidence(v);
    let convergence = (fan_in - 1.0) * d;
    match cfg.variant {
        Variant::H1 => {
            m.cp[v] > cfg.theta_eff(cfg.theta_critical, m.cp_conf[v])
                || convergence > cfg.theta_eff(cfg.theta_redundancy, conf)
                || d > cfg.theta_eff(cfg.theta_cost, conf)
        },
        Variant::H2 => {
            let score = cfg.w_critical * m.cp[v] + cfg.w_redundancy * convergence + cfg.w_cost * d;
            score > cfg.theta_eff(cfg.theta_combined, conf)
        },
        Variant::H3 => {
            m.cp[v] > cfg.theta_eff(cfg.theta_critical, m.cp_conf[v])
                || d > cfg.theta_eff(cfg.theta_cost, conf)
        },
        Variant::H4 => {
            d > cfg.theta_eff(cfg.theta_eff_cost, conf)
                || g.fan_in(v) > cfg.theta_fanin
                || m.subgraph[v] > cfg.theta_eff(cfg.theta_subgraph, conf)
        },
    }
}

/// Select the promoted node set: roots (always), variant promotions, plus
/// non-trivial atom seeds under atom-seeding.
fn select_promoted(g: &Graph, cfg: &HeuristicConfig, m: &Metrics) -> BTreeSet<usize> {
    let mut promoted: BTreeSet<usize> = g.roots().into_iter().collect();
    for v in 0..g.len() {
        if fires(g, cfg, m, v) {
            promoted.insert(v);
        }
        if cfg.seeding == crate::config::Seeding::AtomSeeded
            && g.is_atom(v)
            && m.subgraph[v] >= cfg.theta_trivial
        {
            promoted.insert(v);
        }
    }
    promoted
}

/// Build EP records: scope by downward flood halting at promoted nodes, with
/// EP-level dependency pointers recorded at the boundaries.
fn build_eps(g: &Graph, promoted: &BTreeSet<usize>) -> Vec<Ep> {
    let mut eps = Vec::with_capacity(promoted.len());
    for &p in promoted {
        let mut scope = BTreeSet::new();
        scope.insert(p);
        let mut dep_eps = BTreeSet::new();
        let mut visited = BTreeSet::new();
        visited.insert(p);
        let mut stack: Vec<usize> = g.deps(p).to_vec();
        while let Some(u) = stack.pop() {
            if !visited.insert(u) {
                continue;
            }
            if promoted.contains(&u) {
                // Boundary: u is its own EP; p's EP depends on it. Do not
                // recurse — u's scope covers everything below it.
                dep_eps.insert(u);
            } else {
                scope.insert(u);
                stack.extend_from_slice(g.deps(u));
            }
        }
        eps.push(Ep {
            entry: p,
            scope: scope.into_iter().collect(),
            deps: dep_eps.into_iter().collect(),
        });
    }
    eps.sort_by_key(|e| e.entry);
    eps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Seeding;
    use crate::trace::Trace;

    fn graph(json: &str) -> Graph {
        Graph::from_trace(&Trace::from_json(json).expect("valid")).expect("acyclic")
    }

    /// Thresholds so high that only roots are promoted, then relaxed per test.
    fn inert() -> HeuristicConfig {
        HeuristicConfig {
            theta_critical: 1e9,
            theta_redundancy: 1e9,
            theta_cost: 1e9,
            theta_scale: 0.0,
            theta_eff_cost: 1e9,
            theta_fanin: usize::MAX,
            theta_subgraph: 1e9,
            theta_combined: 1e9,
            ..HeuristicConfig::default()
        }
    }

    const CHAIN: &str = r#"{
        "nodes": [
            {"id": "top", "duration": 1.0},
            {"id": "mid", "duration": 2.0},
            {"id": "leaf", "duration": 4.0}
        ],
        "edges": [{"from": "top", "to": "mid"}, {"from": "mid", "to": "leaf"}],
        "workers": [{"id": "w0"}]
    }"#;

    #[test]
    fn only_roots_promoted_when_inert() {
        let g = graph(CHAIN);
        let c = Coarsening::build(&g, &inert());
        // Root `top` is the sole EP; its scope is the whole chain.
        assert_eq!(c.eps.len(), 1);
        assert_eq!(c.eps[0].entry, g.index_of("top").unwrap());
        assert_eq!(c.eps[0].scope.len(), 3);
        assert!(c.eps[0].deps.is_empty());
    }

    #[test]
    fn critical_path_criterion_promotes_chain_head() {
        let g = graph(CHAIN);
        // cp(leaf)=4, cp(mid)=6, cp(top)=7. Threshold 5 promotes mid (and top
        // root), not leaf.
        let cfg = HeuristicConfig {
            theta_critical: 5.0,
            variant: Variant::H1,
            ..inert()
        };
        let c = Coarsening::build(&g, &cfg);
        let entries = c.entries();
        assert!(entries.contains(&g.index_of("top").unwrap()));
        assert!(entries.contains(&g.index_of("mid").unwrap()));
        assert!(!entries.contains(&g.index_of("leaf").unwrap()));
        // top's EP now depends on mid's EP; mid covers {mid, leaf}.
        let top_ep = c
            .eps
            .iter()
            .find(|e| e.entry == g.index_of("top").unwrap())
            .unwrap();
        assert_eq!(top_ep.deps, vec![g.index_of("mid").unwrap()]);
    }

    #[test]
    fn troublesome_node_criterion_promotes_on_duration() {
        let g = graph(CHAIN);
        // d(leaf)=4 is the only duration above 3.
        let cfg = HeuristicConfig {
            theta_cost: 3.0,
            variant: Variant::H1,
            ..inert()
        };
        let c = Coarsening::build(&g, &cfg);
        assert!(c.entries().contains(&g.index_of("leaf").unwrap()));
    }

    #[test]
    fn convergence_criterion_promotes_high_fan_in() {
        // Two parents share one expensive dependency: (fan_in-1)*d = 1*10 = 10.
        let json = r#"{
            "nodes": [
                {"id": "p1", "duration": 1.0},
                {"id": "p2", "duration": 1.0},
                {"id": "shared", "duration": 10.0}
            ],
            "edges": [{"from": "p1", "to": "shared"}, {"from": "p2", "to": "shared"}],
            "workers": [{"id": "w0"}]
        }"#;
        let g = graph(json);
        let cfg = HeuristicConfig {
            theta_redundancy: 5.0,
            variant: Variant::H1,
            ..inert()
        };
        let c = Coarsening::build(&g, &cfg);
        assert!(c.entries().contains(&g.index_of("shared").unwrap()));
        // Without the convergence gate, `shared` is in both parent scopes.
        let bare = Coarsening::build(&g, &inert());
        let in_two = bare
            .eps
            .iter()
            .filter(|e| e.scope.contains(&g.index_of("shared").unwrap()))
            .count();
        assert_eq!(
            in_two, 2,
            "shared dep duplicated across both parent EP scopes"
        );
    }

    #[test]
    fn confidence_gating_raises_threshold_under_low_confidence() {
        // cp(top) over a 2-node chain = 1 + 9 = 10. With scale=1: at conf=1.0
        // theta_eff = 10/2 = 5 (fires); at conf=0.0 theta_eff = 10 (does not).
        let mk = |conf: f64| {
            format!(
                r#"{{
                    "nodes": [
                        {{"id": "top", "duration": 1.0, "confidence": {conf}}},
                        {{"id": "dep", "duration": 9.0, "confidence": {conf}}}
                    ],
                    "edges": [{{"from": "top", "to": "dep"}}],
                    "workers": [{{"id": "w0"}}]
                }}"#
            )
        };
        let cfg = HeuristicConfig {
            theta_critical: 9.5,
            theta_scale: 1.0,
            variant: Variant::H1,
            ..inert()
        };
        // `dep` has cp = 9 < 9.5 ungated; the confidence gate must move `dep`.
        let g_hi = graph(&mk(1.0));
        let g_lo = graph(&mk(0.0));
        let dep_hi = g_hi.index_of("dep").unwrap();
        let dep_lo = g_lo.index_of("dep").unwrap();
        // High confidence lowers theta_eff to 9/(1+1)=4.75 < 9 => dep promoted.
        assert!(Coarsening::build(&g_hi, &cfg).entries().contains(&dep_hi));
        // Low confidence keeps theta_eff at 9.5 > 9 => dep not promoted.
        assert!(!Coarsening::build(&g_lo, &cfg).entries().contains(&dep_lo));
    }

    #[test]
    fn h2_combined_score_promotes_when_no_single_h1_criterion_fires() {
        // A node where no individual H1 criterion reaches threshold, but the
        // combined weighted sum does: each signal contributes partially.
        // Chain: top(d=1) -> mid(d=3) -> leaf(d=9).
        // For `mid`: cp(mid) = 3+9 = 12; fan_in(mid) = 1 so convergence = 0; d = 3.
        // With w_critical=1, w_redundancy=1, w_cost=1:
        //   score = 12 + 0 + 3 = 15.
        // theta_combined=14 (fired), theta_critical=20 (not fired alone), theta_cost=10 (not fired alone).
        // Confidence gating off (theta_scale=0).
        let json = r#"{
            "nodes": [
                {"id": "top",  "duration": 1.0},
                {"id": "mid",  "duration": 3.0},
                {"id": "leaf", "duration": 9.0}
            ],
            "edges": [
                {"from": "top", "to": "mid"},
                {"from": "mid", "to": "leaf"}
            ],
            "workers": [{"id": "w0"}]
        }"#;
        let g = graph(json);
        let cfg = HeuristicConfig {
            variant: Variant::H2,
            theta_scale: 0.0,
            theta_critical: 20.0,   // cp(mid)=12 < 20 → H1 criterion alone wouldn't fire
            theta_cost: 10.0,       // d(mid)=3 < 10 → troublesome alone wouldn't fire
            theta_redundancy: 1e9,  // inert
            theta_combined: 14.0,   // score=15 > 14 → combined fires
            w_critical: 1.0,
            w_redundancy: 1.0,
            w_cost: 1.0,
            ..HeuristicConfig::default()
        };
        let entries = Coarsening::build(&g, &cfg).entries();
        assert!(
            entries.contains(&g.index_of("mid").unwrap()),
            "H2 must promote `mid` when combined score exceeds theta_combined"
        );
        // Verify H1 alone would NOT have promoted it (sanity cross-check).
        let h1_cfg = HeuristicConfig { variant: Variant::H1, ..cfg.clone() };
        assert!(
            !Coarsening::build(&g, &h1_cfg).entries().contains(&g.index_of("mid").unwrap()),
            "H1 must not promote `mid` when no individual criterion fires"
        );
    }

    #[test]
    fn h3_promotes_on_critical_path_and_cost_but_not_on_convergence_alone() {
        // H3 = critical-path + troublesome-node only; no fan-in/convergence term.
        // Two roots (p1, p2) share an expensive dependency `shared` (fan_in=2).
        // convergence = (2-1)*d(shared) = 10 → would fire H1's cost-gated convergence.
        // cp(shared) = 10, d(shared) = 10, theta_critical = 20, theta_cost = 15.
        // H3 should NOT promote `shared` (neither cp nor d criterion fires alone).
        // A separate lone node `heavy` with d=20 DOES fire the troublesome-node criterion.
        let json = r#"{
            "nodes": [
                {"id": "p1",     "duration": 1.0},
                {"id": "p2",     "duration": 1.0},
                {"id": "shared", "duration": 10.0},
                {"id": "heavy",  "duration": 20.0}
            ],
            "edges": [
                {"from": "p1", "to": "shared"},
                {"from": "p2", "to": "shared"}
            ],
            "workers": [{"id": "w0"}]
        }"#;
        let g = graph(json);
        let cfg = HeuristicConfig {
            variant: Variant::H3,
            theta_scale: 0.0,
            theta_critical: 20.0, // cp(shared)=10 < 20 → no fire
            theta_cost: 15.0,     // d(shared)=10 < 15 → no fire; d(heavy)=20 > 15 → fires
            theta_redundancy: 5.0, // inert for H3 (no convergence term)
            ..HeuristicConfig::default()
        };
        let entries = Coarsening::build(&g, &cfg).entries();
        // `shared` must NOT be promoted by H3 (high fan-in alone does not trigger H3).
        assert!(
            !entries.contains(&g.index_of("shared").unwrap()),
            "H3 must not promote a high-fan-in node when neither cp nor cost criterion fires"
        );
        // `heavy` MUST be promoted (troublesome-node criterion: d=20 > theta_cost=15).
        assert!(
            entries.contains(&g.index_of("heavy").unwrap()),
            "H3 must promote `heavy` via the troublesome-node criterion"
        );
        // Contrast: H1 WOULD promote `shared` via convergence (fan_in=2, d=10, (2-1)*10=10 > 5).
        let h1_cfg = HeuristicConfig { variant: Variant::H1, ..cfg.clone() };
        assert!(
            Coarsening::build(&g, &h1_cfg).entries().contains(&g.index_of("shared").unwrap()),
            "H1 must promote `shared` via cost-gated convergence (contrast with H3)"
        );
    }

    #[test]
    fn atom_seeding_seeds_nontrivial_and_absorbs_trivial() {
        // `big` atom (subgraph 1+5=6) seeded; `small` atom (subgraph 1) absorbed.
        let json = r#"{
            "nodes": [
                {"id": "root", "duration": 1.0},
                {"id": "big", "duration": 1.0, "is_atom": true},
                {"id": "bigdep", "duration": 5.0},
                {"id": "small", "duration": 1.0, "is_atom": true}
            ],
            "edges": [
                {"from": "root", "to": "big"},
                {"from": "root", "to": "small"},
                {"from": "big", "to": "bigdep"}
            ],
            "workers": [{"id": "w0"}]
        }"#;
        let g = graph(json);
        let cfg = HeuristicConfig {
            seeding: Seeding::AtomSeeded,
            theta_trivial: 3.0,
            variant: Variant::H1,
            ..inert()
        };
        let entries = Coarsening::build(&g, &cfg).entries();
        assert!(
            entries.contains(&g.index_of("big").unwrap()),
            "non-trivial atom seeded"
        );
        assert!(
            !entries.contains(&g.index_of("small").unwrap()),
            "trivial atom absorbed"
        );
        // From-scratch with inert thresholds would seed neither atom.
        let scratch = HeuristicConfig {
            seeding: Seeding::FromScratch,
            ..cfg.clone()
        };
        assert!(
            !Coarsening::build(&g, &scratch)
                .entries()
                .contains(&g.index_of("big").unwrap())
        );
    }
}
