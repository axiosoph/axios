//! Regression test: variant H1 and the H4 baseline produce *different* EP sets
//! on a DAG built to separate the critical-path criterion from the
//! subgraph-cost criterion.
//!
//! The fixture `fixtures/divergence.json` (documented inline there) has a
//! serial branch with high critical path but low subgraph cost, and a wide
//! branch with low critical path but high subgraph cost. With matched
//! thresholds H1 promotes the serial head and H4 promotes the wide head, so
//! the coarsenings — and therefore the schedules — diverge. This is the
//! empirical evidence that the variant choice is load-bearing (campaign F14).

use std::collections::BTreeSet;

use eos_sim::config::{HeuristicConfig, Variant};
use eos_sim::{Coarsening, Graph, Trace};

fn diverging_config(variant: Variant) -> HeuristicConfig {
    HeuristicConfig {
        variant,
        theta_scale: 0.0, // exact arithmetic: no confidence gating
        theta_critical: 6.0,
        theta_subgraph: 10.0,
        // All other criteria held inert so only cp (H1) vs subgraph (H4) speak.
        theta_redundancy: 1e9,
        theta_cost: 1e9,
        theta_eff_cost: 1e9,
        theta_fanin: usize::MAX,
        theta_combined: 1e9,
        ..HeuristicConfig::default()
    }
}

fn promoted_ids(coarsening: &Coarsening, graph: &Graph) -> BTreeSet<String> {
    coarsening
        .entries()
        .iter()
        .map(|&v| graph.id(v).to_string())
        .collect()
}

#[test]
fn h1_and_h4_diverge_on_serial_vs_wide_dag() {
    let trace = Trace::load("fixtures/divergence.json").expect("fixture loads");
    let graph = Graph::from_trace(&trace).expect("acyclic");

    let h1 = promoted_ids(
        &Coarsening::build(&graph, &diverging_config(Variant::H1)),
        &graph,
    );
    let h4 = promoted_ids(
        &Coarsening::build(&graph, &diverging_config(Variant::H4)),
        &graph,
    );

    // Both keep the root; they disagree on which subtree head is promoted.
    assert!(h1.contains("top") && h4.contains("top"));
    assert!(
        h1.contains("s_head"),
        "H1 promotes the serial (high critical-path) head"
    );
    assert!(
        !h1.contains("w_head"),
        "H1 ignores the wide (low critical-path) head"
    );
    assert!(
        h4.contains("w_head"),
        "H4 promotes the wide (high subgraph-cost) head"
    );
    assert!(
        !h4.contains("s_head"),
        "H4 ignores the serial (low subgraph-cost) head"
    );

    assert_ne!(h1, h4, "the variant choice must change the entry-point set");
}
