use axum::{Json, http::StatusCode, extract::{State, Query}};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{AppState, models::Deployment, error::{ApiError, ApiResult}};
use sqlx::{Row};

#[derive(Deserialize)]
pub struct CreateDeploymentRequest { pub app_name: String, pub artifact_url: String }

#[derive(Serialize)]
pub struct CreateDeploymentResponse { pub id: Uuid, pub status: &'static str }

#[tracing::instrument(level="info", skip(state, req), fields(app_name=%req.app_name))]
pub async fn create_deployment(State(state): State<AppState>, Json(req): Json<CreateDeploymentRequest>) -> ApiResult<(StatusCode, Json<CreateDeploymentResponse>)> {
    let pool = state.db.as_ref().ok_or_else(ApiError::service_unavailable)?;
    // Resolve application id
    let app_row = sqlx::query("SELECT id FROM applications WHERE name = $1")
        .bind(&req.app_name)
        .fetch_optional(pool).await.map_err(|e| ApiError::internal(format!("db error: {e}")))?;
    let app_id: uuid::Uuid = match app_row { Some(r) => r.get("id"), None => return Err(ApiError::not_found("application not found")) };
    let deployment: Deployment = sqlx::query_as::<_, Deployment>("INSERT INTO deployments (app_id, artifact_url, status) VALUES ($1, $2, $3) RETURNING id, app_id, artifact_url, status, created_at")
        .bind(app_id)
        .bind(&req.artifact_url)
        .bind("pending")
        .fetch_one(pool).await.map_err(|e| ApiError::internal(format!("insert failure: {e}")))?;
    tracing::info!(deployment_id=%deployment.id, "deployment created");
    Ok((StatusCode::CREATED, Json(CreateDeploymentResponse { id: deployment.id, status: "pending" })))
}

#[derive(Deserialize)]
pub struct DeploymentQuery { pub app_name: Option<String> }

#[derive(Serialize)]
pub struct DeploymentItem { pub id: Uuid, pub app_id: Uuid, pub artifact_url: String, pub status: String }

#[tracing::instrument(level="debug", skip(state, q), fields(filter_app=?q.app_name))]
pub async fn list_deployments(State(state): State<AppState>, Query(q): Query<DeploymentQuery>) -> ApiResult<Json<Vec<DeploymentItem>>> {
    let pool = state.db.as_ref().ok_or_else(ApiError::service_unavailable)?;
    let rows: Vec<Deployment> = if let Some(app_name) = q.app_name {
        let sql = "SELECT d.id, d.app_id, d.artifact_url, d.status, d.created_at FROM deployments d JOIN applications a ON a.id = d.app_id WHERE a.name = $1 ORDER BY d.created_at DESC";
        sqlx::query_as::<_, Deployment>(sql).bind(app_name).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    } else {
        let sql = "SELECT id, app_id, artifact_url, status, created_at FROM deployments ORDER BY created_at DESC";
        sqlx::query_as::<_, Deployment>(sql).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    };
    Ok(Json(rows.into_iter().map(|d| DeploymentItem { id: d.id, app_id: d.app_id, artifact_url: d.artifact_url, status: d.status }).collect()))
}
