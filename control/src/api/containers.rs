use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

use crate::{
    api::nodes::AppState,
    db::execute_async,
    services::{container_registry, node_registry},
    types::{CreateContainerRequest, CreateContainerResponse, Container, ListContainersResponse},
};
use std::sync::Arc;

/// POST /api/containers - Create a new container
/// The control plane picks a node via scheduler, creates the record, returns node_id
/// The caller is responsible for sending the actual creation request to the agent
pub async fn create_container(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateContainerRequest>,
) -> Result<Json<CreateContainerResponse>, (StatusCode, String)> {
    let namespace = req.namespace.unwrap_or_else(|| "default".to_string());

    info!(
        "Creating container: namespace={}, name={}, image={}",
        namespace, req.name, req.image
    );

    // List all "up" nodes
    let db = state.db.clone();
    let nodes = execute_async(&db, move |conn| node_registry::list_nodes(conn))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let up_nodes: Vec<_> = nodes.into_iter().filter(|n| n.status == "up").collect();

    if up_nodes.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "No available nodes".to_string(),
        ));
    }

    // Pick a node using the scheduler
    let selected_node = state
        .scheduler
        .pick_node(&up_nodes)
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Scheduling failed".to_string()))?;

    let node_id = selected_node.node_id.clone();

    info!("Scheduled container on node: {}", node_id);

    // Create container record in database
    let db = state.db.clone();
    let name = req.name.clone();
    let image = req.image.clone();
    let node_id_clone = node_id.clone();

    let container_id = execute_async(&db, move |conn| {
        container_registry::create_container(conn, &node_id_clone, &name, &namespace, &image)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Container record created: container_id={}", container_id);

    Ok(Json(CreateContainerResponse {
        container_id,
        node_id,
        ip_address: None, // IP will be assigned by agent
    }))
}

/// GET /api/containers/:id - Get container by ID
pub async fn get_container(
    State(state): State<Arc<AppState>>,
    Path(container_id): Path<String>,
) -> Result<Json<Container>, (StatusCode, String)> {
    let db = state.db.clone();

    let container =
        execute_async(&db, move |conn| container_registry::get_container(conn, &container_id))
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(container))
}

/// GET /api/containers - List all containers
pub async fn list_containers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListContainersResponse>, (StatusCode, String)> {
    let db = state.db.clone();

    let containers = execute_async(&db, move |conn| container_registry::list_containers(conn))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ListContainersResponse { containers }))
}

/// DELETE /api/containers/:id - Delete a container
pub async fn delete_container(
    State(state): State<Arc<AppState>>,
    Path(container_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    info!("Deleting container: {}", container_id);

    let db = state.db.clone();

    execute_async(&db, move |conn| {
        container_registry::delete_container(conn, &container_id)
    })
    .await
    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/containers/:id/ip - Update container IP (called by agent after creation)
pub async fn update_container_ip(
    State(state): State<Arc<AppState>>,
    Path(container_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let ip_address = req
        .get("ip_address")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing ip_address".to_string()))?;

    info!("Updating container IP: container_id={}, ip={}", container_id, ip_address);

    let db = state.db.clone();
    let ip = ip_address.to_string();

    execute_async(&db, move |conn| {
        container_registry::update_container_ip(conn, &container_id, &ip)?;
        container_registry::update_container_status(conn, &container_id, "running")
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}
