//! Store-service construction for the daemon.
//!
//! A single seam through which both the daemon process and the eval worker
//! reach the snix store composition layer, so a remote deployment that points
//! the address flags at `grpc+http://` endpoints stays viable.

use std::sync::Arc;

use clap::Parser;
use snix_castore::blobservice::BlobService;
use snix_castore::directoryservice::DirectoryService;
use snix_store::nar::NarCalculationService;
use snix_store::pathinfoservice::PathInfoService;

use crate::config::DaemonConfig;

/// Constructs the snix store services (blob, directory, path-info, and NAR
/// calculation) from the daemon's configured service addresses.
///
/// The address flags accept any scheme snix understands — `redb:`,
/// `objectstore+file:`, `memory:`, `grpc+http://`, … — so this is also the
/// path by which a daemon is pointed at remote store services.
///
/// # Errors
///
/// Returns an error if an address fails to parse or a service cannot be
/// constructed.
pub async fn construct_store_services(
    config: &DaemonConfig,
) -> Result<
    (
        Arc<dyn BlobService>,
        Arc<dyn DirectoryService>,
        Arc<dyn PathInfoService>,
        Box<dyn NarCalculationService>,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let urls = snix_store::utils::ServiceUrls::parse_from([
        "eosd",
        "--blob-service-addr",
        &config.blob_service_addr,
        "--directory-service-addr",
        &config.directory_service_addr,
        "--path-info-service-addr",
        &config.path_info_service_addr,
    ]);

    snix_store::utils::construct_services(urls)
        .await
        .map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "Failed to initialize Snix services: {}",
                e
            ))
        })
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::construct_store_services;
    use crate::config::DaemonConfig;

    /// Pins that the daemon's store-service construction accepts remote
    /// `grpc+http://` endpoints. gRPC channels connect lazily, so this needs no
    /// live server; it guards the remote-deployment config path against drift.
    #[tokio::test]
    async fn accepts_grpc_remote_store_addresses() {
        let config = DaemonConfig::parse_from([
            "eosd",
            "--blob-service-addr",
            "grpc+http://[::1]:8035",
            "--directory-service-addr",
            "grpc+http://[::1]:8035",
            "--path-info-service-addr",
            "grpc+http://[::1]:8035",
        ]);

        construct_store_services(&config)
            .await
            .expect("daemon must accept grpc+http store-service addresses");
    }
}
