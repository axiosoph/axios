//! `eos-sim` command-line entry point.
//!
//! Usage and the trace-file format are documented in `README.md`. The binary
//! emits two contract lines on a successful run, consumed by campaign node P10
//! / constraint C2: `Loaded <N> plans` and `Simulation completed`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use eos_sim::config::{HeuristicConfig, Seeding, Variant};
use eos_sim::{Trace, simulate};

/// Deterministic scheduling simulator for the Eos build scheduler.
#[derive(Debug, Parser)]
#[command(name = "eos-sim", version, about)]
struct Cli {
    /// Path to the input trace file (JSON).
    #[arg(long)]
    trace: PathBuf,
    /// Seed for deterministic tie-breaking. A fixed seed reproduces identical
    /// output.
    #[arg(long, default_value_t = 0)]
    seed: u64,

    /// Promotion variant: H0 (roots only) | H1 | H2 | H3 | H4 | H5 | H6.
    #[arg(long, default_value_t = Variant::H1)]
    variant: Variant,
    /// Initial-cover seeding: from-scratch | atom-seeded.
    #[arg(long, default_value_t = Seeding::FromScratch)]
    seeding: Seeding,

    /// Critical-path-cut threshold θ_critical.
    #[arg(long)]
    theta_critical: Option<f64>,
    /// Cost-gated convergence threshold θ_redundancy.
    #[arg(long)]
    theta_redundancy: Option<f64>,
    /// Troublesome-node threshold θ_cost.
    #[arg(long)]
    theta_cost: Option<f64>,
    /// Confidence-gating scale θ_scale.
    #[arg(long)]
    theta_scale: Option<f64>,
    /// H4 subgraph-cost threshold.
    #[arg(long)]
    theta_subgraph: Option<f64>,
    /// H4 fan-in threshold.
    #[arg(long)]
    theta_fanin: Option<usize>,
    /// Atom-absorption threshold θ_trivial.
    #[arg(long)]
    theta_trivial: Option<f64>,
    /// H5 relative-CP fraction threshold θ_rel_critical (range 0–1).
    #[arg(long)]
    theta_rel_critical: Option<f64>,

    /// Cache-affinity speedup factor.
    #[arg(long)]
    cache_speedup: Option<f64>,
    /// Resource-fit penalty weight β.
    #[arg(long)]
    beta: Option<f64>,

    /// Bounded dispatch window Δ (seconds).
    #[arg(long)]
    delta: Option<f64>,
    /// Delay-credit weight γ.
    #[arg(long)]
    gamma: Option<f64>,
    /// Redundant-work weight λ in the objective.
    #[arg(long)]
    lambda: Option<f64>,

    /// Emit the metrics JSON line instead of the human summary.
    #[arg(long)]
    json: bool,
}

impl Cli {
    /// Build the heuristic config, overriding defaults with provided flags.
    fn config(&self) -> HeuristicConfig {
        let mut c = HeuristicConfig {
            variant: self.variant,
            seeding: self.seeding,
            ..Default::default()
        };
        macro_rules! set {
            ($field:ident) => {
                if let Some(v) = self.$field {
                    c.$field = v;
                }
            };
        }
        set!(theta_critical);
        set!(theta_redundancy);
        set!(theta_cost);
        set!(theta_scale);
        set!(theta_subgraph);
        set!(theta_fanin);
        set!(theta_trivial);
        set!(theta_rel_critical);
        set!(cache_speedup);
        set!(beta);
        set!(delta);
        set!(gamma);
        set!(lambda);
        c
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let trace = Trace::load(&cli.trace)?;
    // Contract line 1 (node P10 / C2): N = plan node count.
    println!("Loaded {} plans", trace.nodes.len());

    let metrics = simulate(&trace, &cli.config(), cli.seed)?;
    if cli.json {
        println!("{}", metrics.to_json());
    } else {
        println!("{}", metrics.human_summary());
    }

    // Contract line 2 (node P10 / C2).
    println!("Simulation completed");
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        },
    }
}
