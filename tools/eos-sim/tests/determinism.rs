//! Determinism regression: a fixed seed reproduces byte-identical metrics
//! output (IBC acceptance criterion 3, functional contract item 4).

use eos_sim::config::{HeuristicConfig, Variant};
use eos_sim::{Trace, simulate};

fn config() -> HeuristicConfig {
    // theta_cost=0 shatters the diamond into per-node EPs; the symmetric a/b
    // pair ties on priority, exercising the seed-driven tie-break.
    HeuristicConfig {
        variant: Variant::H1,
        theta_cost: 0.0,
        theta_critical: 1e9,
        theta_redundancy: 1e9,
        theta_scale: 0.0,
        ..HeuristicConfig::default()
    }
}

#[test]
fn same_seed_is_byte_identical() {
    let trace = Trace::load("fixtures/diamond.json").expect("fixture");
    let a = simulate(&trace, &config(), 42).expect("runs").to_json();
    let b = simulate(&trace, &config(), 42).expect("runs").to_json();
    assert_eq!(a, b, "same seed must produce byte-identical metrics");
}

#[test]
fn reload_then_resimulate_is_identical() {
    // Re-loading the trace from disk and re-running must not perturb output.
    let cfg = config();
    let first = simulate(&Trace::load("fixtures/diamond.json").unwrap(), &cfg, 7)
        .unwrap()
        .to_json();
    let second = simulate(&Trace::load("fixtures/diamond.json").unwrap(), &cfg, 7)
        .unwrap()
        .to_json();
    assert_eq!(first, second);
}
