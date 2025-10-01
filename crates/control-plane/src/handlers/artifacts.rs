use axum::{extract::{Path, State}, http::StatusCode, Json};
use crate::AppState;
use crate::error::{ApiError, ApiResult};
use axum::response::IntoResponse;
use std::path::PathBuf;
use tracing::info;
use serde::Deserialize;
use crate::models::Artifact;
use crate::telemetry::REGISTRY;
use prometheus::{IntCounter, IntCounterVec};

// Metrics for SBOM lifecycle
static SBOM_UPLOADS_TOTAL: once_cell::sync::Lazy<IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = IntCounter::new("sbom_uploads_total", "Total SBOM upload attempts").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static SBOM_UPLOAD_STATUS_TOTAL: once_cell::sync::Lazy<IntCounterVec> = once_cell::sync::Lazy::new(|| {
    let v = IntCounterVec::new(prometheus::Opts::new("sbom_upload_status_total", "SBOM upload outcomes"), &["status"]).unwrap();
    REGISTRY.register(Box::new(v.clone())).ok(); v
});

#[derive(Deserialize)]
pub struct SbomUploadQuery { #[allow(dead_code)] pub overwrite: Option<bool> }

/// Basic CycloneDX validation (minimal required fields). Returns whether it's CycloneDX.
fn validate_cyclonedx(doc: &serde_json::Value) -> Result<bool, String> {
    if let Some(fmt) = doc.get("bomFormat").and_then(|v| v.as_str()) {
        if fmt != "CycloneDX" { return Err("bomFormat must be CycloneDX".into()); }
    } else { return Err("missing bomFormat".into()); }
    if let Some(spec) = doc.get("specVersion").and_then(|v| v.as_str()) {
        if !spec.starts_with("1.") { return Err("unsupported specVersion".into()); }
    } else { return Err("missing specVersion".into()); }
    Ok(true)
}

pub async fn get_sbom(State(_state): State<AppState>, Path(digest): Path<String>) -> ApiResult<impl IntoResponse> {
    // SBOM expected at storage layout: /data/sbom/<digest>.sbom.json OR configurable base dir
    let dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    let filename = format!("{}.sbom.json", digest);
    let primary = PathBuf::from(&dir).join(&filename);
    if primary.exists() {
        let bytes = match tokio::fs::read(&primary).await { Ok(b)=>b, Err(e)=> return Err(ApiError::internal(format!("read sbom: {e}"))) };
        return Ok((StatusCode::OK, [ ("Content-Type","application/json") ], bytes));
    }
    Err(ApiError::not_found("sbom not found"))
}

/// Upload SBOM (CycloneDX JSON or legacy aether-sbom-v1). Overwrites existing by default.
/// Content-Type: application/vnd.cyclonedx+json OR application/json.
pub async fn upload_sbom(State(state): State<AppState>, Path(digest): Path<String>, body: axum::body::Bytes) -> ApiResult<impl IntoResponse> {
    SBOM_UPLOADS_TOTAL.inc();
    if digest.len()!=64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["bad_digest"]).inc(); return Err(ApiError::bad_request("digest must be 64 hex")); }
    // Ensure artifact exists
    let art = sqlx::query_as::<_, Artifact>("SELECT id, app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status, created_at, completed_at, idempotency_key, multipart_upload_id FROM artifacts WHERE digest=$1")
        .bind(&digest)
        .fetch_optional(&state.db).await.map_err(|e| ApiError::internal(format!("db: {e}")))?;
    let Some(_artifact) = art else { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["not_found"]).inc(); return Err(ApiError::not_found("artifact not found")); };
    // Parse JSON
    let json: serde_json::Value = serde_json::from_slice(&body).map_err(|e| { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["invalid_json"]).inc(); ApiError::bad_request(format!("invalid json: {e}")) })?;
    let is_cyclonedx = json.get("bomFormat").is_some();
    if is_cyclonedx {
        match validate_cyclonedx(&json) { Ok(_) => { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["cyclonedx_valid"]).inc(); }, Err(e) => { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["cyclonedx_invalid"]).inc(); return Err(ApiError::bad_request(format!("invalid CycloneDX: {e}"))); } }
    } else if json.get("schema").and_then(|v| v.as_str()) == Some("aether-sbom-v1") {
        SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["legacy_ok"]).inc();
    } else {
        SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["unsupported_format"]).inc();
        return Err(ApiError::bad_request("unsupported SBOM format (expect CycloneDX or aether-sbom-v1)"));
    }
    // Size guard
    if body.len() > 2 * 1024 * 1024 { // 2MB limit
        SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["too_large"]).inc();
        return Err(ApiError::bad_request("sbom too large (max 2MB)"));
    }
    let dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    if let Err(e) = tokio::fs::create_dir_all(&dir).await { return Err(ApiError::internal(format!("create sbom dir: {e}"))); }
    let filename = format!("{}.sbom.json", digest);
    let path = PathBuf::from(&dir).join(&filename);
    if let Err(e) = tokio::fs::write(&path, &body).await { return Err(ApiError::internal(format!("write sbom: {e}"))); }
    // Update DB (best-effort)
    let url = format!("/artifacts/{digest}/sbom");
    let _ = sqlx::query("UPDATE artifacts SET sbom_url=$1 WHERE digest=$2")
        .bind(&url)
        .bind(&digest)
        .execute(&state.db).await;
    info!(digest=%digest, len=body.len(), cyclonedx=is_cyclonedx, "sbom_uploaded");
    Ok((StatusCode::CREATED, Json(serde_json::json!({"status":"ok","cyclonedx":is_cyclonedx,"url":url}))))
}
