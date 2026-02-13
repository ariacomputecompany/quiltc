use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Node Registration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    pub hostname: String,
    pub host_ip: String,
    pub cpu_cores: Option<u32>,
    pub ram_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeResponse {
    pub node_id: String,
    pub subnet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub node_id: String,
    pub hostname: String,
    pub host_ip: String,
    pub subnet: String,
    pub cpu_cores: Option<u32>,
    pub ram_mb: Option<u64>,
    pub status: String,
    pub registered_at: i64,
    pub last_heartbeat: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<Node>,
}

// ============================================================================
// Peer Info (for overlay management)
// ============================================================================

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub node_id: String,
    pub host_ip: String,
    pub subnet: String,
}

// ============================================================================
// TLS Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub ca_cert: PathBuf,
    pub client_cert: Option<PathBuf>,
    pub client_key: Option<PathBuf>,
}
