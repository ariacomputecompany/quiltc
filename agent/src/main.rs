mod control_client;
mod overlay;
mod quilt_client;
mod types;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use control_client::ControlClient;
use overlay::VxlanManager;
use quilt_client::QuiltClient;
use types::{PeerInfo, TlsConfig};

#[derive(Parser, Debug)]
#[command(name = "quilt-mesh-agent")]
#[command(about = "Quilt Mesh agent", long_about = None)]
struct Args {
    /// Control plane URL
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    control_plane: String,

    /// This node's host IP address
    #[arg(long)]
    host_ip: String,

    /// Hostname (defaults to system hostname)
    #[arg(long)]
    hostname: Option<String>,

    /// Quilt runtime gRPC endpoint
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    quilt_runtime: String,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// CA certificate for verifying servers (PEM)
    #[arg(long)]
    tls_ca: Option<PathBuf>,

    /// Client certificate for mTLS (PEM)
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Client private key for mTLS (PEM)
    #[arg(long)]
    tls_key: Option<PathBuf>,
}

struct AgentState {
    node_id: String,
    subnet: String,
    host_ip: Ipv4Addr,
    control_client: ControlClient,
    vxlan_manager: Arc<RwLock<VxlanManager>>,
    quilt_client: Arc<RwLock<QuiltClient>>,
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

    info!("Starting Quilt Mesh Agent");

    #[cfg(feature = "dev-stubs")]
    warn!("Running in dev-stub mode — VXLAN/network ops are no-ops. NOT for production.");

    // Parse host IP
    let host_ip: Ipv4Addr = args.host_ip.parse().context("Invalid host IP address")?;

    // Get hostname
    let hostname = if let Some(h) = args.hostname {
        h
    } else {
        hostname::get()
            .context("Failed to get system hostname")?
            .to_string_lossy()
            .to_string()
    };

    // Get CPU cores and RAM
    let cpu_cores = num_cpus::get() as u32;
    let ram_mb = get_total_memory_mb();

    info!(
        "Agent configuration: hostname={}, host_ip={}, cpu_cores={}, ram_mb={}",
        hostname, host_ip, cpu_cores, ram_mb
    );

    // Build TLS config if CA cert provided
    let tls_config = args.tls_ca.map(|ca| TlsConfig {
        ca_cert: ca,
        client_cert: args.tls_cert,
        client_key: args.tls_key,
    });

    // Create control plane client
    let control_client = ControlClient::new(args.control_plane.clone(), tls_config.as_ref())
        .context("Failed to create control plane client")?;

    // Register with control plane
    info!("Registering with control plane at {}", args.control_plane);
    let registration = control_client
        .register_node(
            hostname.clone(),
            host_ip.to_string(),
            Some(cpu_cores),
            Some(ram_mb),
        )
        .await
        .context("Failed to register with control plane")?;

    let node_id = registration.node_id;
    let subnet = registration.subnet;

    info!(
        "Successfully registered as node_id={}, assigned subnet={}",
        node_id, subnet
    );

    // Create VXLAN manager
    let vxlan_manager = VxlanManager::new(host_ip)
        .await
        .context("Failed to create VXLAN manager")?;

    // Set up VXLAN interface
    vxlan_manager
        .setup_vxlan()
        .await
        .context("Failed to set up VXLAN interface")?;

    let vxlan_manager = Arc::new(RwLock::new(vxlan_manager));

    // Create Quilt client
    let quilt_endpoint = if tls_config.is_some() && !args.quilt_runtime.starts_with("https://") {
        args.quilt_runtime.replace("http://", "https://")
    } else {
        args.quilt_runtime
    };
    let mut quilt_client = QuiltClient::new(quilt_endpoint, tls_config.as_ref())
        .await
        .context("Failed to create Quilt client")?;

    // Configure Quilt's subnet allocation
    info!("Configuring Quilt to use subnet: {}", subnet);
    quilt_client
        .configure_node_subnet(subnet.clone())
        .await
        .context("Failed to configure node subnet")?;

    let quilt_client = Arc::new(RwLock::new(quilt_client));

    // Create agent state
    let state = Arc::new(AgentState {
        node_id: node_id.clone(),
        subnet: subnet.clone(),
        host_ip,
        control_client,
        vxlan_manager,
        quilt_client,
    });

    // Create cancellation token for background loops
    let cancel = CancellationToken::new();

    // Spawn heartbeat loop
    let heartbeat_state = state.clone();
    let heartbeat_cancel = cancel.clone();
    let heartbeat_handle =
        tokio::spawn(async move { heartbeat_loop(heartbeat_state, heartbeat_cancel).await });

    // Spawn peer sync loop
    let peer_sync_state = state.clone();
    let peer_sync_cancel = cancel.clone();
    let peer_sync_handle =
        tokio::spawn(async move { peer_sync_loop(peer_sync_state, peer_sync_cancel).await });

    info!("Agent initialized successfully - running background tasks");

    // Wait for shutdown signal
    shutdown_signal().await;

    // Cancel background loops
    cancel.cancel();

    // Wait for loops to finish (with timeout)
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        let _ = heartbeat_handle.await;
        let _ = peer_sync_handle.await;
    })
    .await;

    // Perform cleanup
    if let Err(e) = graceful_shutdown(&state).await {
        error!("Error during shutdown cleanup: {}", e);
    }

    info!("Agent shutdown complete");

    Ok(())
}

/// Graceful shutdown: deregister, remove routes, clean FDB entries
async fn graceful_shutdown(state: &AgentState) -> Result<()> {
    // 1. Deregister from control plane
    info!("Deregistering from control plane...");
    if let Err(e) = state.control_client.deregister(&state.node_id).await {
        warn!("Failed to deregister (control plane may be down): {}", e);
    }

    // 2. Remove all routes via Quilt runtime
    info!("Removing injected routes...");
    let peer_subnets: Vec<String> = {
        let vxlan = state.vxlan_manager.read().await;
        vxlan.peers().keys().cloned().collect()
    };

    {
        let mut quilt = state.quilt_client.write().await;
        for subnet in &peer_subnets {
            if let Err(e) = quilt.remove_route(subnet.clone()).await {
                warn!("Failed to remove route for {}: {}", subnet, e);
            }
        }
    }

    // 3. Clean up VXLAN FDB entries
    info!("Cleaning up VXLAN FDB entries...");
    {
        let mut vxlan = state.vxlan_manager.write().await;
        for subnet in &peer_subnets {
            if let Err(e) = vxlan.remove_peer(subnet).await {
                warn!("Failed to remove VXLAN peer {}: {}", subnet, e);
            }
        }
    }

    info!("Graceful shutdown cleanup complete");
    Ok(())
}

/// Heartbeat loop - send heartbeat every 10s, cancellable
async fn heartbeat_loop(state: Arc<AgentState>, cancel: CancellationToken) -> Result<()> {
    info!("Starting heartbeat loop (every 10s)");

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => {},
            _ = cancel.cancelled() => {
                info!("Heartbeat loop cancelled");
                return Ok(());
            }
        }

        if let Err(e) = state.control_client.heartbeat(&state.node_id).await {
            warn!("Failed to send heartbeat: {}", e);
        }
    }
}

/// Peer sync loop - poll control plane for peer changes every 5s, cancellable
async fn peer_sync_loop(state: Arc<AgentState>, cancel: CancellationToken) -> Result<()> {
    info!("Starting peer sync loop (every 5s)");

    let mut known_peers: HashSet<String> = HashSet::new();

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {},
            _ = cancel.cancelled() => {
                info!("Peer sync loop cancelled");
                return Ok(());
            }
        }

        // List all nodes from control plane
        let nodes_response = match state.control_client.list_nodes().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to list nodes: {}", e);
                continue;
            }
        };

        // Extract peers (exclude self, only include "up" nodes)
        let current_peers: Vec<PeerInfo> = nodes_response
            .nodes
            .into_iter()
            .filter(|n| n.node_id != state.node_id && n.status == "up")
            .map(|n| PeerInfo {
                node_id: n.node_id,
                host_ip: n.host_ip,
                subnet: n.subnet,
            })
            .collect();

        let current_subnets: HashSet<String> =
            current_peers.iter().map(|p| p.subnet.clone()).collect();

        // Find new peers (in current but not in known)
        for peer in &current_peers {
            if !known_peers.contains(&peer.subnet) {
                info!(
                    "New peer discovered: subnet={}, host_ip={}",
                    peer.subnet, peer.host_ip
                );

                // Parse peer host IP
                if let Ok(peer_ip) = peer.host_ip.parse::<Ipv4Addr>() {
                    // Add to VXLAN FDB
                    let mut vxlan = state.vxlan_manager.write().await;
                    if let Err(e) = vxlan.add_peer(peer.subnet.clone(), peer_ip).await {
                        error!("Failed to add peer to VXLAN: {}", e);
                    }

                    // Inject route in Quilt
                    let mut quilt = state.quilt_client.write().await;
                    if let Err(e) = quilt
                        .inject_route(peer.subnet.clone(), "vxlan100".to_string())
                        .await
                    {
                        error!("Failed to inject route in Quilt: {}", e);
                    }
                } else {
                    warn!("Invalid peer IP address: {}", peer.host_ip);
                }

                known_peers.insert(peer.subnet.clone());
            }
        }

        // Find removed peers (in known but not in current)
        let removed_subnets: Vec<String> =
            known_peers.difference(&current_subnets).cloned().collect();

        for subnet in removed_subnets {
            info!("Peer removed: subnet={}", subnet);

            // Remove from VXLAN FDB
            let mut vxlan = state.vxlan_manager.write().await;
            if let Err(e) = vxlan.remove_peer(&subnet).await {
                error!("Failed to remove peer from VXLAN: {}", e);
            }

            // Remove route from Quilt
            let mut quilt = state.quilt_client.write().await;
            if let Err(e) = quilt.remove_route(subnet.clone()).await {
                error!("Failed to remove route from Quilt: {}", e);
            }

            known_peers.remove(&subnet);
        }
    }
}

/// Get total system memory in MB (best effort)
fn get_total_memory_mb() -> u64 {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.total_memory() / 1024 / 1024 // Convert bytes to MB
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

    info!("Shutdown signal received, beginning graceful shutdown...");
}
