//! `eos-sim` command-line entry point.
//!
//! Usage and the trace-file format are documented in `README.md`. The binary
//! emits two contract lines on a successful run, consumed by campaign node P10
//! / constraint C2: `Loaded <N> plans` and `Simulation completed`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use eos_sim::Trace;

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
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let trace = Trace::load(&cli.trace)?;
    // Contract line 1 (node P10 / C2): N = plan node count.
    println!("Loaded {} plans", trace.nodes.len());
    let _ = cli.seed;
    // The engine is wired in a later change; until then the load path is the
    // exercised surface.
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
