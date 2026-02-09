mod control_client;
mod overlay;
mod quilt_client;
mod types;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use control_client::ControlClient;
use overlay::VxlanManager;
use quilt_client::QuiltClient;
use types::PeerInfo;

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

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
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

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Quilt Mesh Agent");

    // Parse host IP
    let host_ip: Ipv4Addr = args
        .host_ip
        .parse()
        .context("Invalid host IP address")?;

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

    info!("Agent configuration: hostname={}, host_ip={}, cpu_cores={}, ram_mb={}",
          hostname, host_ip, cpu_cores, ram_mb);

    // Create control plane client
    let control_client = ControlClient::new(args.control_plane.clone())
        .context("Failed to create control plane client")?;

    // Register with control plane
    info!("Registering with control plane at {}", args.control_plane);
    let registration = control_client
        .register_node(hostname.clone(), host_ip.to_string(), Some(cpu_cores), Some(ram_mb))
        .await
        .context("Failed to register with control plane")?;

    let node_id = registration.node_id;
    let subnet = registration.subnet;

    info!("Successfully registered as node_id={}, assigned subnet={}", node_id, subnet);

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
    let mut quilt_client = QuiltClient::new("http://127.0.0.1:50051".to_string())
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

    // Spawn heartbeat loop
    let heartbeat_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = heartbeat_loop(heartbeat_state).await {
            error!("Heartbeat loop failed: {}", e);
        }
    });

    // Spawn peer sync loop
    let peer_sync_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = peer_sync_loop(peer_sync_state).await {
            error!("Peer sync loop failed: {}", e);
        }
    });

    info!("Agent initialized successfully - running background tasks");

    // Keep main thread alive
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

/// Heartbeat loop - send heartbeat every 10s
async fn heartbeat_loop(state: Arc<AgentState>) -> Result<()> {
    info!("Starting heartbeat loop (every 10s)");

    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        if let Err(e) = state.control_client.heartbeat(&state.node_id).await {
            warn!("Failed to send heartbeat: {}", e);
        }
    }
}

/// Peer sync loop - poll control plane for peer changes every 5s
async fn peer_sync_loop(state: Arc<AgentState>) -> Result<()> {
    info!("Starting peer sync loop (every 5s)");

    let mut known_peers: HashSet<String> = HashSet::new();

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

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

        let current_subnets: HashSet<String> = current_peers
            .iter()
            .map(|p| p.subnet.clone())
            .collect();

        // Find new peers (in current but not in known)
        for peer in &current_peers {
            if !known_peers.contains(&peer.subnet) {
                info!("New peer discovered: subnet={}, host_ip={}", peer.subnet, peer.host_ip);

                // Parse peer host IP
                if let Ok(peer_ip) = peer.host_ip.parse::<Ipv4Addr>() {
                    // Add to VXLAN FDB
                    let mut vxlan = state.vxlan_manager.write().await;
                    if let Err(e) = vxlan.add_peer(peer.subnet.clone(), peer_ip).await {
                        error!("Failed to add peer to VXLAN: {}", e);
                    }

                    // Inject route in Quilt
                    let mut quilt = state.quilt_client.write().await;
                    if let Err(e) = quilt.inject_route(peer.subnet.clone(), "vxlan100".to_string()).await {
                        error!("Failed to inject route in Quilt: {}", e);
                    }
                } else {
                    warn!("Invalid peer IP address: {}", peer.host_ip);
                }

                known_peers.insert(peer.subnet.clone());
            }
        }

        // Find removed peers (in known but not in current)
        let removed_subnets: Vec<String> = known_peers
            .difference(&current_subnets)
            .cloned()
            .collect();

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
