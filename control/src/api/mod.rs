pub mod containers;
pub mod nodes;

use axum::{
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;

use crate::types::HealthResponse;
use nodes::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health))
        // Node management
        .route("/api/nodes/register", post(nodes::register_node))
        .route("/api/nodes/:id/heartbeat", post(nodes::heartbeat))
        .route("/api/nodes", get(nodes::list_nodes))
        // Container management
        .route("/api/containers", post(containers::create_container))
        .route("/api/containers", get(containers::list_containers))
        .route("/api/containers/:id", get(containers::get_container))
        .route("/api/containers/:id", delete(containers::delete_container))
        .route("/api/containers/:id/ip", patch(containers::update_container_ip))
        .with_state(state)
}

/// GET /health - Health check endpoint
async fn health() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
        }),
    )
}
