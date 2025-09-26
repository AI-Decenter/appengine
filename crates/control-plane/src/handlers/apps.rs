use axum::{Json, extract::{Path, State}};
use serde::{Serialize, Deserialize};
use crate::{AppState, models::Application, error::{ApiError, ApiResult}};
use sqlx::Row;
use axum::http::StatusCode;

#[derive(Deserialize)]
pub struct CreateAppReq { pub name: String }

#[derive(Serialize)]
pub struct CreateAppResp { pub id: uuid::Uuid, pub name: String }

pub async fn create_app(State(state): State<AppState>, Json(body): Json<CreateAppReq>) -> ApiResult<(StatusCode, Json<CreateAppResp>)> {
    let pool = state.db.as_ref().ok_or_else(ApiError::service_unavailable)?;
    let rec: Application = sqlx::query_as::<_, Application>("INSERT INTO applications (name) VALUES ($1) RETURNING id, name, created_at, updated_at")
        .bind(&body.name)
        .fetch_one(pool).await.map_err(|e| {
            if let Some(db_code) = e.as_database_error().and_then(|d| d.code()) { if db_code == "23505" { return ApiError::conflict("application name exists"); } }
            ApiError::internal(format!("insert error: {e}"))
        })?;
    Ok((StatusCode::CREATED, Json(CreateAppResp { id: rec.id, name: rec.name })))
}

#[derive(Serialize)]
pub struct ListAppItem { pub id: uuid::Uuid, pub name: String }

pub async fn list_apps(State(state): State<AppState>) -> ApiResult<Json<Vec<ListAppItem>>> {
    let pool = state.db.as_ref().ok_or_else(ApiError::service_unavailable)?;
    let rows: Vec<Application> = sqlx::query_as::<_, Application>("SELECT id, name, created_at, updated_at FROM applications ORDER BY created_at DESC")
        .fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?;
    Ok(Json(rows.into_iter().map(|a| ListAppItem { id: a.id, name: a.name }).collect()))
}

pub async fn app_logs(Path(_app_name): Path<String>) -> (StatusCode, String) { (StatusCode::OK, String::new()) }

use crate::models::Deployment;
#[derive(serde::Serialize)]
pub struct AppDeploymentItem { pub id: uuid::Uuid, pub artifact_url: String, pub status: String }

pub async fn app_deployments(State(state): State<AppState>, Path(app_name): Path<String>) -> ApiResult<Json<Vec<AppDeploymentItem>>> {
    let pool = state.db.as_ref().ok_or_else(ApiError::service_unavailable)?;
    // resolve app id
    let app_row = sqlx::query("SELECT id FROM applications WHERE name = $1").bind(&app_name).fetch_optional(pool).await
        .map_err(|e| ApiError::internal(format!("lookup error: {e}")))?;
    let app_id: uuid::Uuid = match app_row { Some(r) => r.get("id"), None => return Err(ApiError::not_found("application not found")) };
    let rows: Vec<Deployment> = sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at FROM deployments WHERE app_id = $1 ORDER BY created_at DESC")
        .bind(app_id)
        .fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?;
    Ok(Json(rows.into_iter().map(|d| AppDeploymentItem { id: d.id, artifact_url: d.artifact_url, status: d.status }).collect()))
}
