mod ipam;
mod route_manager;
mod service;

use anyhow::{Context, Result};
use clap::Parser;
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

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Quilt Runtime");

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

    // Create gRPC service
    let service = QuiltRuntimeService::new(ipam, route_manager);

    // Parse listen address
    let addr = args
        .grpc_addr
        .parse()
        .context("Invalid gRPC address")?;

    info!("Starting gRPC server on {}", addr);

    // Start gRPC server
    Server::builder()
        .add_service(QuiltRuntimeServer::new(service))
        .serve(addr)
        .await
        .context("gRPC server failed")?;

    Ok(())
}
