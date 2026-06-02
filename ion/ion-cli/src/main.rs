//! The ion CLI — reference frontend for the Atom publishing stack.
//!
//! Handles dependency resolution, build engine dispatch, and
//! dev workspace management.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ion")]
#[command(about = "ion CLI — reference frontend for the Atom publishing stack")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build resolved atoms using Eos
    Build {
        /// Optional path to the socket of the Eos daemon
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
    },
}

fn resolve_socket_path(explicit: Option<std::path::PathBuf>) -> Result<std::path::PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Ok(path_str) = std::env::var("EOS_SOCKET")
        && !path_str.is_empty()
    {
        return Ok(std::path::PathBuf::from(path_str));
    }
    if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR")
        && !xdg_runtime.is_empty()
    {
        return Ok(std::path::PathBuf::from(xdg_runtime)
            .join("eos")
            .join("eos.sock"));
    }
    Err(
        "Could not resolve Eos socket path: neither --socket, $EOS_SOCKET, nor $XDG_RUNTIME_DIR \
         was set"
            .to_string(),
    )
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { socket } => {
            let lock_path = std::path::Path::new("atom.lock");
            if !lock_path.exists() {
                eprintln!("Error: atom.lock not found in the current directory.");
                std::process::exit(1);
            }
            let lock_content = tokio::fs::read_to_string(lock_path)
                .await
                .expect("failed to read atom.lock file");

            let socket_path = match resolve_socket_path(socket) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                },
            };

            let local = tokio::task::LocalSet::new();
            local
                .run_until(async move {
                    let client = match ion_eos::EosClient::connect(&socket_path).await {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!(
                                "Error: Failed to connect to Eos daemon at {:?}: {}",
                                socket_path, e
                            );
                            std::process::exit(1);
                        },
                    };

                    println!("Connected to Eos daemon. Submitting build...");
                    let handle = match client.submit_build(&lock_content).await {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("Error: Build submission failed: {}", e);
                            std::process::exit(1);
                        },
                    };

                    println!("Build submitted. Job ID: {}", handle.job_id());

                    let mut stream = match handle.attach_progress().await {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Error: Failed to attach progress stream: {}", e);
                            std::process::exit(1);
                        },
                    };

                    while let Some(event) = stream.next().await {
                        use eos_core::job::JobStatus;
                        match event.status {
                            JobStatus::Queued => {
                                println!("[Queued] Waiting in scheduler queue...");
                            },
                            JobStatus::Evaluating { message } => {
                                println!("[Evaluating] {}", message);
                            },
                            JobStatus::Building { phase, progress } => {
                                if let Some(p) = progress {
                                    println!("[Building] {} ({:.1}%)", phase, p * 100.0);
                                } else {
                                    println!("[Building] {}", phase);
                                }
                            },
                            JobStatus::Completed { outputs } => {
                                println!("Build completed successfully!");
                                for out in outputs {
                                    println!("  Output: {}", out.store_path.0);
                                }
                                break;
                            },
                            JobStatus::Failed { error, exit_code } => {
                                eprintln!("Build failed: {}", error);
                                if let Some(ec) = exit_code {
                                    eprintln!("Exit code: {}", ec);
                                }
                                std::process::exit(1);
                            },
                            JobStatus::Cancelled => {
                                eprintln!("Build was cancelled.");
                                std::process::exit(1);
                            },
                        }
                    }
                })
                .await;
        },
    }
}
