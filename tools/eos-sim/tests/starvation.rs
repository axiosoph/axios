//! Starvation-freedom regression (IBC functional contract item 6, acceptance
//! criterion via constraint C-A). The empirical companion to the TLA+ proof in
//! `docs/models/tla/StarvationModel.tla`, whose γ=0 mutation likewise starves.
//!
//! Contention fixture (single worker = maximal contention): a hub of `K`
//! high-priority EPs `H_0..H_{K-1}`, each depending into a common final EP `F`
//! so every `H_k` has the **same** bounded `OCT = d(F)` (the bounded-spread
//! precondition of the ADR bounded-wait result). The `H_k` are *staggered* —
//! `H_k` arrives at `t = k·d(H)`, exactly as the previous one finishes — so each
//! freed worker meets a *fresh* high arrival (age 0), modelling the recurring
//! high-priority stream of the TLA model. One low-priority EP `L` (OCT 0) is
//! present from `t = 0`.
//!
//! - γ = 0: `L` never outranks a fresh `H`, so it waits behind the entire stream — its dispatch
//!   wait grows with `K` (unbounded).
//! - γ > 0: `L`'s delay credit `γ·age` rises until it overtakes a fresh `H` within ≈ `d(F)/γ` ticks
//!   — a bounded wait, independent of `K`.

use std::collections::BTreeMap;

use eos_sim::config::{HeuristicConfig, Variant};
use eos_sim::simulate_report;
use eos_sim::trace::{Trace, TraceEdge, TraceNode, WorkerSpec};

const D_HIGH: f64 = 2.0;
const D_HUB: f64 = 11.0;

fn node(id: &str, duration: f64, arrival: f64) -> TraceNode {
    TraceNode {
        id: id.to_string(),
        duration,
        peak_mem: None,
        is_atom: false,
        plan_name: None,
        confidence: None,
        arrival: Some(arrival),
    }
}

/// One low EP `L`, a hub `F`, and `k_high` staggered high EPs feeding `F`.
fn contention_trace(k_high: usize) -> Trace {
    let mut nodes = vec![node("L", D_HIGH, 0.0), node("F", D_HUB, 0.0)];
    let mut edges = Vec::new();
    for k in 0..k_high {
        let id = format!("H{k}");
        nodes.push(node(&id, D_HIGH, D_HIGH * k as f64));
        edges.push(TraceEdge {
            from: "F".to_string(),
            to: id,
        });
    }
    Trace {
        nodes,
        edges,
        workers: vec![WorkerSpec {
            id: "w0".to_string(),
            speed: 1.0,
            capacity: BTreeMap::new(),
            cached: Vec::new(),
        }],
        store_cached: Vec::new(),
    }
}

fn config(gamma: f64) -> HeuristicConfig {
    HeuristicConfig {
        variant: Variant::H1,
        theta_cost: 0.0, // promote every node to its own EP
        theta_critical: 1e9,
        theta_redundancy: 1e9,
        theta_scale: 0.0,
        cache_speedup: 0.0,
        beta: 0.0,
        gamma,
        delta: 0.0,
        ..HeuristicConfig::default()
    }
}

fn victim_wait(gamma: f64, k_high: usize) -> f64 {
    let report = simulate_report(&contention_trace(k_high), &config(gamma), 42).expect("runs");
    *report
        .ep_waits
        .get("L")
        .expect("L is eventually dispatched")
}

#[test]
fn gamma_zero_starves_low_priority_node() {
    // Without the delay credit, L's wait scales with the length of the
    // high-priority stream — unbounded as K grows.
    let short = victim_wait(0.0, 10);
    let long = victim_wait(0.0, 30);
    assert!(
        long > short,
        "γ=0: longer stream must delay L longer ({long} vs {short})"
    );
    assert!(
        long >= D_HIGH * 30.0 - D_HIGH,
        "γ=0: L waits behind the whole stream"
    );
}

#[test]
fn positive_gamma_bounds_the_wait_independent_of_stream_length() {
    // With the delay credit, L's wait is bounded by ≈ d(F)/γ regardless of K.
    let short = victim_wait(1.0, 10);
    let long = victim_wait(1.0, 30);
    assert_eq!(
        short, long,
        "γ>0: L's wait must not grow with stream length"
    );
    assert!(
        short <= D_HUB + D_HIGH,
        "γ>0: bounded by ≈ d(F)/γ plus one in-flight build"
    );
}

#[test]
fn gamma_strictly_helps_at_fixed_stream_length() {
    // Direct contrast at the same K: the credit shortens L's wait.
    assert!(victim_wait(1.0, 30) < victim_wait(0.0, 30));
}
