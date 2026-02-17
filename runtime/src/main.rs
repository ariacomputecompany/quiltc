mod ipam;
mod route_manager;
mod service;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use ipam::IpamManager;
use route_manager::RouteManager;
use service::quilt::quilt_runtime_server::QuiltRuntimeServer;
use service::QuiltRuntimeService;

#[derive(Parser, Debug)]
#[command(name = "quilt-runtime")]
#[command(about = "Quilt container runtime with gRPC API", long_about = None)]
struct Args {
    /// gRPC server listen address
    #[arg(long, default_value = "127.0.0.1:50051")]
    grpc_addr: String,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// TLS certificate file (PEM)
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// TLS private key file (PEM)
    #[arg(long)]
    tls_key: Option<PathBuf>,

    /// CA certificate for client verification (enables mTLS)
    #[arg(long)]
    tls_ca: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Quilt Runtime");

    #[cfg(feature = "dev-stubs")]
    tracing::warn!(
        "Running in dev-stub mode — route management ops are no-ops. NOT for production."
    );

    // Create IPAM manager
    let ipam = Arc::new(IpamManager::new());
    info!("IPAM manager initialized");

    // Create route manager
    let route_manager = Arc::new(
        RouteManager::new()
            .await
            .context("Failed to create route manager")?,
    );
    info!("Route manager initialized");

    // Keep a reference for cleanup after shutdown
    let route_manager_cleanup = route_manager.clone();

    // Create gRPC service
    let service = QuiltRuntimeService::new(ipam, route_manager);

    // Parse listen address
    let addr = args.grpc_addr.parse().context("Invalid gRPC address")?;

    // Build server with optional TLS
    let mut server = Server::builder();

    if let (Some(cert_path), Some(key_path)) = (&args.tls_cert, &args.tls_key) {
        info!("Starting gRPC server on {} (TLS enabled)", addr);

        let cert = std::fs::read(cert_path)
            .with_context(|| format!("Failed to read TLS cert: {:?}", cert_path))?;
        let key = std::fs::read(key_path)
            .with_context(|| format!("Failed to read TLS key: {:?}", key_path))?;
        let server_identity = tonic::transport::Identity::from_pem(cert, key);

        let mut tls_config = tonic::transport::ServerTlsConfig::new().identity(server_identity);

        if let Some(ca_path) = &args.tls_ca {
            let ca = std::fs::read(ca_path)
                .with_context(|| format!("Failed to read CA cert: {:?}", ca_path))?;
            let ca_cert = tonic::transport::Certificate::from_pem(ca);
            tls_config = tls_config.client_ca_root(ca_cert);
        }

        server = server
            .tls_config(tls_config)
            .context("Failed to configure TLS")?;
    } else {
        info!("Starting gRPC server on {} (no TLS)", addr);
    }

    // Start gRPC server with graceful shutdown
    server
        .add_service(QuiltRuntimeServer::new(service))
        .serve_with_shutdown(addr, shutdown_signal())
        .await
        .context("gRPC server failed")?;

    // Clean up routes after server stops
    info!("Cleaning up routes...");
    route_manager_cleanup.cleanup_all_routes().await;

    info!("Runtime shutdown complete");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, draining gRPC connections...");
}
