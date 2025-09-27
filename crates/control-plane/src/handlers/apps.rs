use axum::{Json, extract::{Path, State, Query}};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use crate::{AppState, models::Application, error::{ApiError, ApiResult, ApiErrorBody}, services};
use axum::http::StatusCode;

#[derive(Deserialize, ToSchema)]
pub struct CreateAppReq { pub name: String }

#[derive(Serialize, ToSchema)]
pub struct CreateAppResp { pub id: uuid::Uuid, pub name: String }

/// Create application
#[utoipa::path(post, path = "/apps", request_body = CreateAppReq, responses( (status = 201, body = CreateAppResp), (status=409, body=ApiErrorBody, description="duplicate"), (status=400, body=ApiErrorBody), (status=500, body=ApiErrorBody) ))]
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

#[derive(Deserialize, ToSchema)]
pub struct AppsListQuery { pub limit: Option<i64>, pub offset: Option<i64> }

/// List applications (paginated)
#[utoipa::path(get, path = "/apps", params( ("limit" = Option<i64>, Query, description="Max items (default 100, max 1000)"), ("offset" = Option<i64>, Query, description="Offset for pagination")), responses( (status = 200, body = [ListAppItem]), (status=500, body=ApiErrorBody) ))]
#[tracing::instrument(level="debug", skip(state, q), fields(limit=?q.limit, offset=?q.offset))]
pub async fn list_apps(State(state): State<AppState>, Query(q): Query<AppsListQuery>) -> ApiResult<Json<Vec<ListAppItem>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows: Vec<Application> = sqlx::query_as::<_, Application>("SELECT id, name, created_at, updated_at FROM applications ORDER BY created_at DESC LIMIT $1 OFFSET $2")
        .bind(limit).bind(offset)
        .fetch_all(&state.db).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?;
    Ok(Json(rows.into_iter().map(|a| ListAppItem { id: a.id, name: a.name }).collect()))
}

/// Application logs (placeholder)
#[utoipa::path(get, path = "/apps/{app_name}/logs", params( ("app_name" = String, Path, description = "Application name") ), responses( (status=200, description="OK") ))]
pub async fn app_logs(Path(_app_name): Path<String>) -> (StatusCode, String) { (StatusCode::OK, String::new()) }

#[derive(serde::Serialize, ToSchema)]
pub struct AppDeploymentItem { pub id: uuid::Uuid, pub artifact_url: String, pub status: String }

#[derive(Deserialize, ToSchema)]
pub struct AppDeploymentsQuery { pub limit: Option<i64>, pub offset: Option<i64> }

/// List deployments for an application (paginated)
#[utoipa::path(get, path = "/apps/{app_name}/deployments", params( ("app_name" = String, Path, description = "Application name"), ("limit" = Option<i64>, Query, description="Max items (default 100, max 1000)"), ("offset" = Option<i64>, Query, description="Offset") ), responses( (status=200, body = [AppDeploymentItem]), (status=404, body=ApiErrorBody), (status=500, body=ApiErrorBody) ))]
#[tracing::instrument(level="debug", skip(state, app_name, q), fields(app_name=%app_name, limit=?q.limit, offset=?q.offset))]
pub async fn app_deployments(State(state): State<AppState>, Path(app_name): Path<String>, Query(q): Query<AppDeploymentsQuery>) -> ApiResult<Json<Vec<AppDeploymentItem>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows = services::deployments::list_for_app(&state.db, &app_name, limit, offset)
        .await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("application not found"); }
            ApiError::internal(format!("query error: {e}"))
        })?;
    Ok(Json(rows.into_iter().map(|d| AppDeploymentItem { id: d.id, artifact_url: d.artifact_url, status: d.status }).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct AddPublicKeyReq { pub public_key_hex: String }

#[derive(Serialize, ToSchema)]
pub struct AddPublicKeyResp { pub app_id: uuid::Uuid, pub public_key_hex: String, pub active: bool }

/// Add (or upsert activate) a public key for an application used to verify artifact signatures.
#[utoipa::path(post, path = "/apps/{app_name}/public-keys", request_body = AddPublicKeyReq, params(("app_name"=String, Path, description="Application name")), responses( (status=201, body=AddPublicKeyResp), (status=400, body=ApiErrorBody), (status=404, body=ApiErrorBody), (status=409, body=ApiErrorBody) ))]
#[tracing::instrument(level="info", skip(state, body), fields(app_name=%app_name))]
pub async fn add_public_key(State(state): State<AppState>, Path(app_name): Path<String>, Json(body): Json<AddPublicKeyReq>) -> ApiResult<(StatusCode, Json<AddPublicKeyResp>)> {
    if body.public_key_hex.len() != 64 || !body.public_key_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("public_key_hex must be 64 hex chars"));
    }
    let mut tx = state.db.begin().await.map_err(|e| ApiError::internal(format!("tx begin: {e}")))?;
    let app: Option<Application> = sqlx::query_as::<_, Application>("SELECT id, name, created_at, updated_at FROM applications WHERE name=$1")
        .bind(&app_name).fetch_optional(&mut *tx).await.map_err(|e| ApiError::internal(format!("query app: {e}")))?;
    let Some(app) = app else { return Err(ApiError::not_found("application not found")); };
    // Insert ignore conflict
    let _res = sqlx::query("INSERT INTO public_keys (app_id, public_key_hex, active) VALUES ($1,$2,TRUE) ON CONFLICT (app_id, public_key_hex) DO UPDATE SET active=EXCLUDED.active RETURNING app_id")
        .bind(app.id)
        .bind(&body.public_key_hex)
        .fetch_one(&mut *tx).await.map_err(|e| ApiError::internal(format!("insert key: {e}")))?;
    tx.commit().await.map_err(|e| ApiError::internal(format!("commit: {e}")))?;
    tracing::info!(app_id=%app.id, "public_key_added");
    Ok((StatusCode::CREATED, Json(AddPublicKeyResp { app_id: app.id, public_key_hex: body.public_key_hex, active: true })))
}
