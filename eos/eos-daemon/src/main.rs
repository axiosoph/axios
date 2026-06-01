//! Eos daemon binary.

mod config;
mod scheduler;
mod server;

use std::sync::Arc;

use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use clap::Parser;
use eos::index::LockFileIndex;
use eos_proto::eos_capnp;
use eos_snix::{SandboxConfig, SnixEngine, select_sandbox};
use tokio::net::UnixListener;
use tokio::signal;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::config::DaemonConfig;
use crate::scheduler::Scheduler;
use crate::server::EosDaemonImpl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // 2. Parse configuration
    let config = Arc::new(DaemonConfig::parse());
    let socket_path = config
        .resolve_socket_path()
        .map_err(std::io::Error::other)?;

    info!("Starting Eos daemon");
    info!("Socket path: {:?}", socket_path);
    info!("Blob service: {}", config.blob_service_addr);
    info!("Directory service: {}", config.directory_service_addr);
    info!("Path info service: {}", config.path_info_service_addr);

    // 3. Initialize Snix services
    let urls = snix_store::utils::ServiceUrls::parse_from([
        "eosd",
        "--blob-service-addr",
        &config.blob_service_addr,
        "--directory-service-addr",
        &config.directory_service_addr,
        "--path-info-service-addr",
        &config.path_info_service_addr,
    ]);

    let (blob_service, directory_service, path_info_service, nar_calculation_service) =
        snix_store::utils::construct_services(urls)
            .await
            .map_err(|e| {
                std::io::Error::other(format!("Failed to initialize Snix services: {}", e))
            })?;

    let sandbox_config = SandboxConfig {
        remote_builder: None,
        workdir: config.sandbox_workdir.clone(),
    };

    let build_service = select_sandbox(
        &sandbox_config,
        blob_service.clone(),
        directory_service.clone(),
    )
    .await
    .map_err(|e| std::io::Error::other(format!("Failed to initialize sandbox: {}", e)))?;

    let engine = Arc::new(SnixEngine::new(
        blob_service,
        directory_service,
        path_info_service,
        nar_calculation_service.into(),
        build_service,
    ));

    // 4. Initialize Scheduler and Index
    let index = Arc::new(LockFileIndex::new());
    let scheduler = Arc::new(Scheduler::new(config.clone(), engine, index.clone()));

    // Ensure parent directory of socket exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Clean up existing socket file if any
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    // 5. Bind Unix socket
    let listener = UnixListener::bind(&socket_path)?;

    // 6. Start RPC event loop on a LocalSet since Cap'n Proto futures are !Send
    let local = LocalSet::new();

    let scheduler_clone = scheduler.clone();
    let index_clone = index.clone();
    let socket_path_clone = socket_path.clone();

    // Run loop within local set context
    local
        .run_until(async move {
            tokio::task::spawn_local(async move {
                info!("Eos daemon listening on Unix domain socket");
                loop {
                    match listener.accept().await {
                        Ok((stream, _)) => {
                            let (reader, writer) = stream.into_split();
                            let compat_reader = reader.compat();
                            let compat_writer = writer.compat_write();

                            let network = twoparty::VatNetwork::new(
                                compat_reader,
                                compat_writer,
                                rpc_twoparty_capnp::Side::Server,
                                Default::default(),
                            );

                            let daemon_server =
                                EosDaemonImpl::new(scheduler_clone.clone(), index_clone.clone());
                            let daemon_client: eos_capnp::eos_daemon::Client =
                                capnp_rpc::new_client(daemon_server);
                            let rpc_system =
                                RpcSystem::new(Box::new(network), Some(daemon_client.client));

                            tokio::task::spawn_local(rpc_system);
                        },
                        Err(e) => {
                            error!("Accept error: {}", e);
                        },
                    }
                }
            });

            // Handle signals for graceful shutdown [daemon-shutdown]
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
                .expect("failed to listen for SIGINT");
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to listen for SIGTERM");

            tokio::select! {
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down");
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down");
                }
            }

            // Clean up socket file
            if socket_path_clone.exists() {
                let _ = std::fs::remove_file(&socket_path_clone);
            }
        })
        .await;

    Ok(())
}
