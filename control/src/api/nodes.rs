use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

use crate::{
    db::{execute_async, DbPool},
    services::{node_registry, SimpleIPAM, SimpleScheduler},
    types::{ListNodesResponse, RegisterNodeRequest, RegisterNodeResponse},
};
use std::sync::Arc;

pub struct AppState {
    pub db: DbPool,
    pub ipam: Arc<SimpleIPAM>,
    pub scheduler: Arc<SimpleScheduler>,
}

/// POST /api/nodes/register - Register a new node
pub async fn register_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<Json<RegisterNodeResponse>, (StatusCode, String)> {
    info!(
        "Registering node: hostname={}, host_ip={}",
        req.hostname, req.host_ip
    );

    // Allocate subnet
    let subnet = state
        .ipam
        .allocate_subnet()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Register node in database
    let db = state.db.clone();
    let hostname = req.hostname.clone();
    let host_ip = req.host_ip.clone();
    let subnet_clone = subnet.clone();
    let cpu_cores = req.cpu_cores;
    let ram_mb = req.ram_mb;

    let node_id = execute_async(&db, move |conn| {
        node_registry::register_node(
            conn,
            &hostname,
            &host_ip,
            &subnet_clone,
            cpu_cores,
            ram_mb,
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Node registered: node_id={}, subnet={}", node_id, subnet);

    Ok(Json(RegisterNodeResponse { node_id, subnet }))
}

/// POST /api/nodes/:id/heartbeat - Update node heartbeat
pub async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let db = state.db.clone();

    execute_async(&db, move |conn| node_registry::update_heartbeat(conn, &node_id))
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(StatusCode::OK)
}

/// GET /api/nodes - List all nodes
pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListNodesResponse>, (StatusCode, String)> {
    let db = state.db.clone();

    let nodes = execute_async(&db, move |conn| node_registry::list_nodes(conn))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ListNodesResponse { nodes }))
}
