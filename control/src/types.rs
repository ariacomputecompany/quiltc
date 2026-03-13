use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadPolicyRequest {
    pub max_concurrency: u32,
    pub hard_quota: u32,
    pub soft_burst: u32,
    pub absolute_limit: u32,
    pub priority: u8,
    pub cooldown_seconds: u32,
    pub hysteresis_pct: f64,
    pub burst_cpu_cap: f64,
    pub burst_mem_mb: u32,
    pub burst_ttl_seconds: u32,
    pub target_container_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadSloRequest {
    pub p95_latency_ms: u32,
    pub max_cold_start_pct: f64,
    pub max_reject_pct: f64,
    pub rto_seconds: u32,
    pub max_cost_per_compute_unit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadObservation {
    pub tenant_id: String,
    pub workload_id: String,
    pub node_group: String,
    pub queue_depth: u32,
    pub cpu_pressure: f64,
    pub mem_pressure: f64,
    pub io_pressure: f64,
    pub cold_start_pct: f64,
    pub invoke_p95_ms: u32,
    pub reject_pct: f64,
    pub active_compute_units: u32,
    pub cost_per_compute_unit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGroupObservation {
    pub node_group: String,
    pub cpu_pressure: f64,
    pub mem_pressure: f64,
    pub io_pressure: f64,
    pub warm_ready: u32,
    pub warm_hit_rate: f64,
    pub capacity_units: u32,
    pub used_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationIngestRequest {
    pub workloads: Vec<WorkloadObservation>,
    pub node_groups: Vec<NodeGroupObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorIntent {
    pub tenant_id: String,
    pub workload_id: String,
    pub target_concurrency: u32,
    pub burst_cpu_cap: f64,
    pub burst_mem_mb: u32,
    pub burst_ttl_seconds: u32,
    pub pool_min_ready: u32,
    pub pool_target_ready: u32,
    pub pool_max_ready: u32,
    pub preferred_node_group: String,
    pub anti_affinity: bool,
    pub reason_code: String,
    pub effective_at: i64,
    pub ttl_seconds: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorAction {
    pub action_id: String,
    pub tenant_id: String,
    pub workload_id: String,
    pub action_type: String,
    pub payload_json: serde_json::Value,
    pub ttl_seconds: u32,
    pub rollback_action_json: Option<serde_json::Value>,
    pub parent_action_id: Option<String>,
    pub idempotency_key: String,
    pub decision_window_start: i64,
    pub status: String,
    pub reason_code: Option<String>,
    pub reason_message: Option<String>,
    pub effective_at: i64,
    pub outbound_requested_at: Option<i64>,
    pub runtime_operation_id: Option<String>,
    pub runtime_operation_type: Option<String>,
    pub terminal_status: Option<String>,
    pub terminal_at: Option<i64>,
    pub total_latency_ms: Option<i64>,
    pub attempt_count: u32,
    pub next_retry_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResultRequest {
    pub status: String,
    pub reason_code: Option<String>,
    pub reason_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionListResponse {
    pub actions: Vec<OrchestratorAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentListResponse {
    pub intents: Vec<OrchestratorIntent>,
}
