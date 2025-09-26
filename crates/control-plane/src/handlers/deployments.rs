use axum::{Json, http::StatusCode, extract::State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{AppState, models::Deployment};
use sqlx::{Row};

#[derive(Deserialize)]
pub struct CreateDeploymentRequest { pub app_name: String, pub artifact_url: String }

#[derive(Serialize)]
pub struct CreateDeploymentResponse { pub id: Uuid, pub status: &'static str }

pub async fn create_deployment(State(state): State<AppState>, Json(req): Json<CreateDeploymentRequest>) -> Result<(StatusCode, Json<CreateDeploymentResponse>), StatusCode> {
    let pool = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    // Resolve application id
    let app_row = sqlx::query("SELECT id FROM applications WHERE name = $1")
        .bind(&req.app_name)
        .fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let app_id: uuid::Uuid = match app_row { Some(r) => r.get("id"), None => return Err(StatusCode::NOT_FOUND) };
    let deployment: Deployment = sqlx::query_as::<_, Deployment>("INSERT INTO deployments (app_id, artifact_url, status) VALUES ($1, $2, $3) RETURNING id, app_id, artifact_url, status, created_at")
        .bind(app_id)
        .bind(&req.artifact_url)
        .bind("pending")
        .fetch_one(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(CreateDeploymentResponse { id: deployment.id, status: "pending" })))
}
