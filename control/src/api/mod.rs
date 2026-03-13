pub mod orchestrator;

use axum::{
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use std::sync::Arc;

use crate::db::DbPool;
use crate::types::HealthResponse;

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route(
            "/v1/orchestrator/tenants/:tenant_id/workloads/:workload_id/policy",
            put(orchestrator::upsert_workload_policy),
        )
        .route(
            "/v1/orchestrator/tenants/:tenant_id/workloads/:workload_id/slo",
            put(orchestrator::upsert_workload_slo),
        )
        .route(
            "/v1/orchestrator/observations",
            post(orchestrator::ingest_observations),
        )
        .route("/v1/orchestrator/intents", get(orchestrator::list_intents))
        .route("/v1/orchestrator/actions", get(orchestrator::list_actions))
        .route(
            "/v1/orchestrator/actions/:action_id/result",
            post(orchestrator::update_action_result),
        )
        .route(
            "/v1/orchestrator/loops/fast:run",
            post(orchestrator::trigger_fast_loop),
        )
        .route(
            "/v1/orchestrator/loops/slow:run",
            post(orchestrator::trigger_slow_loop),
        )
        .with_state(state)
}

async fn health() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
        }),
    )
}
