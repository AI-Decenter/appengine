use axum::{Json, http::StatusCode, extract::{State, Query}};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use crate::{AppState, models::Deployment, error::{ApiError, ApiResult, ApiErrorBody}, services};
use sqlx::Row;
// use sqlx::Row; // no longer needed after refactor

#[derive(Deserialize, ToSchema)]
pub struct CreateDeploymentRequest { pub app_name: String, pub artifact_url: String }

#[derive(Serialize, ToSchema)]
pub struct CreateDeploymentResponse { pub id: Uuid, pub status: &'static str }

fn extract_digest(s: &str) -> Option<String> {
    for segment in s.split(|c| c=='/' || c=='?' || c=='&') { if segment.len()==64 && segment.chars().all(|c| c.is_ascii_hexdigit()) { return Some(segment.to_lowercase()); } }
    None
}

async fn resolve_digest(db: &sqlx::Pool<sqlx::Postgres>, artifact_url: &str) -> Option<String> {
    if let Some(d) = extract_digest(artifact_url) {
        if let Ok(Some(row)) = sqlx::query("SELECT digest FROM artifacts WHERE digest=$1 AND status='stored'").bind(&d).fetch_optional(db).await { let dg: String = row.get("digest"); return Some(dg); }
    }
    None
}

/// Create deployment
#[utoipa::path(post, path = "/deployments", request_body = CreateDeploymentRequest, responses( (status=201, body=CreateDeploymentResponse), (status=404, body=ApiErrorBody, description="app not found"), (status=400, body=ApiErrorBody), (status=500, body=ApiErrorBody) ))]
#[tracing::instrument(level="info", skip(state, req), fields(app_name=%req.app_name))]
pub async fn create_deployment(State(state): State<AppState>, Json(req): Json<CreateDeploymentRequest>) -> ApiResult<(StatusCode, Json<CreateDeploymentResponse>)> {
    let resolved_digest = resolve_digest(&state.db, &req.artifact_url).await;
    let deployment: Deployment = services::deployments::create_deployment(&state.db, &req.app_name, &req.artifact_url, resolved_digest.as_deref())
        .await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("application not found"); }
            ApiError::internal(format!("insert failure: {e}"))
        })?;
    tracing::info!(deployment_id=%deployment.id, "deployment created");
    // Fire-and-forget k8s apply using resolved digest (if any) with SHA256 verification.
    let app_name = req.app_name.clone();
    let artifact_url = req.artifact_url.clone();
    let digest_opt = resolved_digest.clone();
    tokio::spawn(async move {
        let digest = digest_opt.as_deref().unwrap_or("");
        if let Err(e) = crate::k8s::apply_deployment(&app_name, digest, &artifact_url, "default").await {
            tracing::error!(error=%e, app=%app_name, "k8s apply failed");
        } else {
            tracing::info!(app=%app_name, "k8s apply scheduled");
        }
    });
    Ok((StatusCode::CREATED, Json(CreateDeploymentResponse { id: deployment.id, status: "pending" })))
}

#[derive(Deserialize, ToSchema)]
pub struct DeploymentQuery { pub app_name: Option<String>, pub limit: Option<i64>, pub offset: Option<i64> }

#[derive(Serialize, ToSchema)]
pub struct DeploymentItem { pub id: Uuid, pub app_id: Uuid, pub artifact_url: String, pub status: String }

/// List deployments (optionally filter by app_name, paginated)
#[utoipa::path(get, path = "/deployments", params( ("app_name" = Option<String>, Query, description = "Filter by application name"), ("limit" = Option<i64>, Query, description="Max items (default 100, max 1000)"), ("offset" = Option<i64>, Query, description="Offset") ), responses( (status=200, body=[DeploymentItem]), (status=500, body=ApiErrorBody) ))]
#[tracing::instrument(level="debug", skip(state, q), fields(filter_app=?q.app_name, limit=?q.limit, offset=?q.offset))]
pub async fn list_deployments(State(state): State<AppState>, Query(q): Query<DeploymentQuery>) -> ApiResult<Json<Vec<DeploymentItem>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let pool = &state.db;
    let rows: Vec<Deployment> = if let Some(app_name) = q.app_name {
        sqlx::query_as::<_, Deployment>("SELECT d.id, d.app_id, d.artifact_url, d.status, d.created_at, d.digest, d.failure_reason FROM deployments d JOIN applications a ON a.id = d.app_id WHERE a.name = $1 ORDER BY d.created_at DESC LIMIT $2 OFFSET $3")
            .bind(app_name).bind(limit).bind(offset).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    } else {
        sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at, digest, failure_reason FROM deployments ORDER BY created_at DESC LIMIT $1 OFFSET $2")
            .bind(limit).bind(offset).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    };
    Ok(Json(rows.into_iter().map(|d| DeploymentItem { id: d.id, app_id: d.app_id, artifact_url: d.artifact_url, status: d.status }).collect()))
}

#[derive(Serialize, ToSchema)]
pub struct DeploymentStatusResponse { pub id: Uuid, pub status: String, pub digest: Option<String>, pub failure_reason: Option<String>, pub artifact_url: String }

#[utoipa::path(get, path="/deployments/{id}", params( ("id" = Uuid, Path, description="Deployment ID") ), responses( (status=200, body=DeploymentStatusResponse), (status=404, body=ApiErrorBody) ))]
#[tracing::instrument(level="debug", skip(state))]
pub async fn get_deployment(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<Uuid>) -> ApiResult<Json<DeploymentStatusResponse>> {
    let dep = services::deployments::get_deployment(&state.db, id).await.map_err(|e| {
        if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("deployment not found"); }
        ApiError::internal(format!("query error: {e}"))
    })?;
    Ok(Json(DeploymentStatusResponse { id: dep.id, status: dep.status, digest: dep.digest, failure_reason: dep.failure_reason, artifact_url: dep.artifact_url }))
}
