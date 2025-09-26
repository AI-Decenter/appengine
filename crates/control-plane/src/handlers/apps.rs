use axum::{Json, extract::{Path, State}};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use crate::{AppState, models::Application, error::{ApiError, ApiResult}, services};
use sqlx::Row;
use axum::http::StatusCode;

#[derive(Deserialize, ToSchema)]
pub struct CreateAppReq { pub name: String }

#[derive(Serialize, ToSchema)]
pub struct CreateAppResp { pub id: uuid::Uuid, pub name: String }

/// Create application
#[utoipa::path(post, path = "/apps", request_body = CreateAppReq, responses( (status = 201, body = CreateAppResp), (status=409, description="duplicate") ))]
#[tracing::instrument(level="info", skip(state, body), fields(app_name=%body.name))]
pub async fn create_app(State(state): State<AppState>, Json(body): Json<CreateAppReq>) -> ApiResult<(StatusCode, Json<CreateAppResp>)> {
    let rec: Application = services::apps::create_app(&state.db, &body.name).await.map_err(|e| {
        if let Some(db_code) = e.as_database_error().and_then(|d| d.code()) { if db_code == "23505" { return ApiError::conflict("application name exists"); } }
        ApiError::internal(format!("insert error: {e}"))
    })?;
    tracing::info!(app_id=%rec.id, "application created");
    Ok((StatusCode::CREATED, Json(CreateAppResp { id: rec.id, name: rec.name })))
}

#[derive(Serialize, ToSchema)]
pub struct ListAppItem { pub id: uuid::Uuid, pub name: String }

/// List applications
#[utoipa::path(get, path = "/apps", responses( (status = 200, body = [ListAppItem]) ))]
#[tracing::instrument(level="debug", skip(state))]
pub async fn list_apps(State(state): State<AppState>) -> ApiResult<Json<Vec<ListAppItem>>> {
    let rows: Vec<Application> = services::apps::list_apps(&state.db).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?;
    Ok(Json(rows.into_iter().map(|a| ListAppItem { id: a.id, name: a.name }).collect()))
}

/// Application logs (placeholder)
#[utoipa::path(get, path = "/apps/{app_name}/logs", params( ("app_name" = String, Path, description = "Application name") ), responses( (status=200, description="OK") ))]
pub async fn app_logs(Path(_app_name): Path<String>) -> (StatusCode, String) { (StatusCode::OK, String::new()) }

use crate::models::Deployment;
#[derive(serde::Serialize, ToSchema)]
pub struct AppDeploymentItem { pub id: uuid::Uuid, pub artifact_url: String, pub status: String }

/// List deployments for an application
#[utoipa::path(get, path = "/apps/{app_name}/deployments", params( ("app_name" = String, Path, description = "Application name") ), responses( (status=200, body = [AppDeploymentItem]) ))]
#[tracing::instrument(level="debug", skip(state, app_name), fields(app_name=%app_name))]
pub async fn app_deployments(State(state): State<AppState>, Path(app_name): Path<String>) -> ApiResult<Json<Vec<AppDeploymentItem>>> {
    let pool = &state.db;
    // resolve app id
    let app_row = sqlx::query("SELECT id FROM applications WHERE name = $1").bind(&app_name).fetch_optional(pool).await
        .map_err(|e| ApiError::internal(format!("lookup error: {e}")))?;
    let app_id: uuid::Uuid = match app_row { Some(r) => r.get("id"), None => return Err(ApiError::not_found("application not found")) };
    let rows: Vec<Deployment> = sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at FROM deployments WHERE app_id = $1 ORDER BY created_at DESC")
        .bind(app_id)
        .fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?;
    Ok(Json(rows.into_iter().map(|d| AppDeploymentItem { id: d.id, artifact_url: d.artifact_url, status: d.status }).collect()))
}
