#[derive(Deserialize, ToSchema)]
pub struct UpdateDeploymentRequest { pub digest: String }

/// Update deployment digest (rollout)
#[utoipa::path(patch, path = "/deployments/{id}", request_body = UpdateDeploymentRequest, params(("id" = Uuid, Path, description="Deployment ID")), responses((status=200, body=DeploymentStatusResponse), (status=404, body=ApiErrorBody), (status=400, body=ApiErrorBody), (status=500, body=ApiErrorBody)))]
#[tracing::instrument(level="info", skip(state, req))]
pub async fn update_deployment(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<Uuid>, Json(req): Json<UpdateDeploymentRequest>) -> ApiResult<Json<DeploymentStatusResponse>> {
    // Validate digest
    let digest = req.digest.trim();
    if digest.len()!=64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("digest must be 64 hex chars"));
    }
    let dep = services::deployments::update_deployment_digest(&state.db, id, digest).await.map_err(|e| {
        if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("deployment not found"); }
        ApiError::internal(format!("update error: {e}"))
    })?;
    Ok(Json(DeploymentStatusResponse { id: dep.id, status: dep.status, digest: dep.digest, failure_reason: dep.failure_reason, artifact_url: dep.artifact_url, last_transition_at: dep.last_transition_at, signature: dep.signature }))
}
use axum::{Json, http::StatusCode, extract::{State, Query}};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use crate::{AppState, models::Deployment, error::{ApiError, ApiResult, ApiErrorBody}, services};
use sqlx::Row;
// use sqlx::Row; // no longer needed after refactor

#[derive(Deserialize, ToSchema)]
pub struct CreateDeploymentRequest { pub app_name: String, pub artifact_url: String, pub signature: Option<String>, #[serde(default)] pub dev_hot: bool }

#[derive(Serialize, ToSchema)]
pub struct CreateDeploymentResponse { pub id: Uuid, pub status: &'static str }

fn extract_digest(s: &str) -> Option<String> {
    // Split on common URL delimiters and search for a 64-length hex segment (sha256 digest)
    for segment in s.split(['/', '?', '&']) {
        if segment.len() == 64 && segment.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(segment.to_lowercase());
        }
    }
    None
}

async fn resolve_digest(db: &sqlx::Pool<sqlx::Postgres>, artifact_url: &str) -> Option<String> {
    if let Some(d) = extract_digest(artifact_url) {
        if let Ok(Some(row)) = sqlx::query("SELECT digest FROM artifacts WHERE digest=$1 AND status='stored'").bind(&d).fetch_optional(db).await { let dg: String = row.get("digest"); return Some(dg); }
    }
    None
}

#[tracing::instrument(level="debug", skip(db, signature), fields(app=%app_name, has_signature=%signature.is_some()))]
async fn verify_signature_if_present(db: &sqlx::Pool<sqlx::Postgres>, app_name: &str, digest_opt: Option<&str>, signature: &Option<String>) -> Result<(), ApiError> {
    if signature.is_none() { return Ok(()); }
    let Some(digest) = digest_opt else { return Err(ApiError::bad_request("signature provided but digest unavailable for verification")); };
    let sig_hex = signature.as_ref().unwrap();
    if sig_hex.len() != 128 || !sig_hex.chars().all(|c| c.is_ascii_hexdigit()) { return Err(ApiError::bad_request("signature must be 128 hex chars (ed25519)")); }
    // Load active public keys for app
    let row = sqlx::query("SELECT id FROM applications WHERE name=$1").bind(app_name).fetch_optional(db).await.map_err(|e| ApiError::internal(format!("lookup app: {e}")))?;
    let Some(app_id_row) = row else { return Err(ApiError::not_found("application not found")); };
    let app_id: uuid::Uuid = app_id_row.get("id");
    let keys: Vec<(String,)> = sqlx::query_as("SELECT public_key_hex FROM public_keys WHERE app_id=$1 AND active=TRUE")
        .bind(app_id).fetch_all(db).await.map_err(|e| ApiError::internal(format!("load keys: {e}")))?;
    if keys.is_empty() { return Err(ApiError::bad_request("no active public keys for app to verify signature")); }
    // Perform ed25519 verification against digest bytes
    let sig_bytes = match hex::decode(sig_hex) { Ok(b) => b, Err(_) => return Err(ApiError::bad_request("invalid signature hex")) };
    use ed25519_dalek::{Signature, VerifyingKey, Verifier};
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| ApiError::bad_request("invalid signature format"))?;
    let mut verified = false;
    for (pk_hex,) in keys.iter() {
        if let Ok(pk_bytes) = hex::decode(pk_hex) {
            if pk_bytes.len()==32 {
                if let Ok(vk) = VerifyingKey::from_bytes(&pk_bytes.try_into().unwrap()) {
                    if vk.verify(digest.as_bytes(), &sig).is_ok() { verified = true; break; }
                }
            }
        }
    }
    if !verified { return Err(ApiError::bad_request("signature verification failed")); }
    Ok(())
}

/// Create deployment
#[utoipa::path(post, path = "/deployments", request_body = CreateDeploymentRequest, responses( (status=201, body=CreateDeploymentResponse), (status=404, body=ApiErrorBody, description="app not found"), (status=400, body=ApiErrorBody), (status=500, body=ApiErrorBody) ))]
#[tracing::instrument(level="info", skip(state, req), fields(app_name=%req.app_name))]
pub async fn create_deployment(State(state): State<AppState>, Json(req): Json<CreateDeploymentRequest>) -> ApiResult<(StatusCode, Json<CreateDeploymentResponse>)> {
    let resolved_digest = resolve_digest(&state.db, &req.artifact_url).await;
    verify_signature_if_present(&state.db, &req.app_name, resolved_digest.as_deref(), &req.signature).await?;
    let deployment: Deployment = services::deployments::create_deployment(&state.db, &req.app_name, &req.artifact_url, resolved_digest.as_deref(), req.signature.as_deref())
        .await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("application not found"); }
            ApiError::internal(format!("insert failure: {e}"))
        })?;
    tracing::info!(deployment_id=%deployment.id, "deployment created");
    // Fire-and-forget k8s apply using resolved digest (if any) with SHA256 verification.
    let app_name = req.app_name.clone();
    let artifact_url = req.artifact_url.clone();
    let digest_opt = resolved_digest.clone();
    let signature = req.signature.clone();
    let dev_hot = req.dev_hot;
    tokio::spawn(async move {
        let digest = digest_opt.as_deref().unwrap_or("");
        if let Err(e) = crate::k8s::apply_deployment(&app_name, digest, &artifact_url, "default", signature.as_deref(), dev_hot).await {
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
        sqlx::query_as::<_, Deployment>("SELECT d.id, d.app_id, d.artifact_url, d.status, d.created_at, d.digest, d.failure_reason, d.last_transition_at, d.signature FROM deployments d JOIN applications a ON a.id = d.app_id WHERE a.name = $1 ORDER BY d.created_at DESC LIMIT $2 OFFSET $3")
            .bind(app_name).bind(limit).bind(offset).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    } else {
        sqlx::query_as::<_, Deployment>("SELECT id, app_id, artifact_url, status, created_at, digest, failure_reason, last_transition_at, signature FROM deployments ORDER BY created_at DESC LIMIT $1 OFFSET $2")
            .bind(limit).bind(offset).fetch_all(pool).await.map_err(|e| ApiError::internal(format!("query error: {e}")))?
    };
    Ok(Json(rows.into_iter().map(|d| DeploymentItem { id: d.id, app_id: d.app_id, artifact_url: d.artifact_url, status: d.status }).collect()))
}

#[derive(Serialize, ToSchema)]
pub struct DeploymentStatusResponse {
    pub id: Uuid,
    pub status: String,
    pub digest: Option<String>,
    pub failure_reason: Option<String>,
    pub artifact_url: String,
    pub last_transition_at: chrono::DateTime<chrono::Utc>,
    pub signature: Option<String>,
}

#[utoipa::path(get, path="/deployments/{id}", params( ("id" = Uuid, Path, description="Deployment ID") ), responses( (status=200, body=DeploymentStatusResponse), (status=404, body=ApiErrorBody) ))]
#[tracing::instrument(level="debug", skip(state))]
pub async fn get_deployment(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<Uuid>) -> ApiResult<Json<DeploymentStatusResponse>> {
    let dep = services::deployments::get_deployment(&state.db, id).await.map_err(|e| {
        if matches!(e, sqlx::Error::RowNotFound) { return ApiError::not_found("deployment not found"); }
        ApiError::internal(format!("query error: {e}"))
    })?;
    Ok(Json(DeploymentStatusResponse {
        id: dep.id,
        status: dep.status,
        digest: dep.digest,
        failure_reason: dep.failure_reason,
        artifact_url: dep.artifact_url,
        last_transition_at: dep.last_transition_at,
        signature: dep.signature,
    }))
}
