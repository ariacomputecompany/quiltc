use serde::{Deserialize, Serialize};

// ============================================================================
// Node Types
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
// Container Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerRequest {
    pub namespace: Option<String>,
    pub name: String,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub env: Option<Vec<EnvVar>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerResponse {
    pub container_id: String,
    pub node_id: String,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub container_id: String,
    pub node_id: String,
    pub name: String,
    pub namespace: String,
    pub image: String,
    pub ip_address: Option<String>,
    pub created_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListContainersResponse {
    pub containers: Vec<Container>,
}

// ============================================================================
// Health Check
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}
