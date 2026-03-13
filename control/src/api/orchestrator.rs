use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::api::AppState;
use crate::services::orchestrator;
use crate::types::{
    ActionListResponse, ActionResultRequest, IntentListResponse, ObservationIngestRequest,
    WorkloadPolicyRequest, WorkloadSloRequest,
};

pub async fn upsert_workload_policy(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, workload_id)): Path<(String, String)>,
    Json(req): Json<WorkloadPolicyRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::upsert_workload_policy(&state.db, &tenant_id, &workload_id, req)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn upsert_workload_slo(
    State(state): State<Arc<AppState>>,
    Path((tenant_id, workload_id)): Path<(String, String)>,
    Json(req): Json<WorkloadSloRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::upsert_workload_slo(&state.db, &tenant_id, &workload_id, req)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn ingest_observations(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ObservationIngestRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::ingest_observations(&state.db, req)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn list_intents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<IntentListResponse>, (StatusCode, String)> {
    let intents = orchestrator::list_intents(&state.db)
        .await
        .map_err(internal_error)?;
    Ok(Json(IntentListResponse { intents }))
}

#[derive(Debug, Deserialize)]
pub struct ActionListQuery {
    pub tenant_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

pub async fn list_actions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActionListQuery>,
) -> Result<Json<ActionListResponse>, (StatusCode, String)> {
    let actions = orchestrator::list_actions(
        &state.db,
        query.tenant_id.as_deref(),
        query.status.as_deref(),
        query.limit.unwrap_or(100).min(1000),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(ActionListResponse { actions }))
}

pub async fn update_action_result(
    State(state): State<Arc<AppState>>,
    Path(action_id): Path<String>,
    Json(req): Json<ActionResultRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::update_action_result(
        &state.db,
        &action_id,
        &req.status,
        req.reason_code.as_deref(),
        req.reason_message.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(StatusCode::OK)
}

pub async fn trigger_fast_loop(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::run_fast_loop(&state.db)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn trigger_slow_loop(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    orchestrator::run_slow_loop(&state.db)
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::ACCEPTED)
}

fn internal_error(err: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
