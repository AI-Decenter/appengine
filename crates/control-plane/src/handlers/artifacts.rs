use axum::{extract::{Path, State}, http::{StatusCode, HeaderMap, HeaderValue}, Json};
use crate::AppState;
use crate::error::{ApiError, ApiResult};
use axum::response::IntoResponse;
use std::path::PathBuf;
use tracing::info;
use serde::Deserialize;
use crate::models::Artifact;
use crate::telemetry::REGISTRY;
use prometheus::{IntCounter, IntCounterVec};
use sha2::{Sha256, Digest};

// Metrics for SBOM lifecycle
static SBOM_UPLOADS_TOTAL: once_cell::sync::Lazy<IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = IntCounter::new("sbom_uploads_total", "Total SBOM upload attempts").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static SBOM_UPLOAD_STATUS_TOTAL: once_cell::sync::Lazy<IntCounterVec> = once_cell::sync::Lazy::new(|| {
    let v = IntCounterVec::new(prometheus::Opts::new("sbom_upload_status_total", "SBOM upload outcomes"), &["status"]).unwrap();
    REGISTRY.register(Box::new(v.clone())).ok(); v
});
static SBOM_VALIDATION_TOTAL: once_cell::sync::Lazy<IntCounterVec> = once_cell::sync::Lazy::new(|| {
    let v = IntCounterVec::new(prometheus::Opts::new("sbom_validation_total", "SBOM validation outcomes"), &["result"]).unwrap();
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
    // Basic JSON schema subset validation
    // If FULL schema validation enabled via env, load embedded extended schema (minimal augmentation w/ dependencies block)
    let full = std::env::var("AETHER_CYCLONEDX_FULL_SCHEMA").ok().as_deref()==Some("1");
    let schema_json = if full { serde_json::json!({
        "$schema":"http://json-schema.org/draft-07/schema#",
        "type": "object",
        "required": ["bomFormat","specVersion","components"],
        "properties": {
            "bomFormat": {"const":"CycloneDX"},
            "specVersion": {"type":"string","pattern":"^1\\.5"},
            "serialNumber": {"type":"string"},
            "components": {"type":"array","items": {"type":"object","required":["type","name"],"properties":{"type":{"type":"string"},"name":{"type":"string"},"version":{"type":"string"},"hashes":{"type":"array","items":{"type":"object","required":["alg","content"],"properties":{"alg":{"type":"string"},"content":{"type":"string"}}}}}}},
            "dependencies": {"type":"array","items":{"type":"object","required":["ref"],"properties":{"ref":{"type":"string"},"dependsOn":{"type":"array","items":{"type":"string"}}}}}
        }
    }) } else { serde_json::json!({
        "type": "object",
        "required": ["bomFormat","specVersion","components"],
        "properties": {
            "bomFormat": {"const":"CycloneDX"},
            "specVersion": {"type":"string"},
            "components": {"type":"array","items": {"type":"object","required":["type","name"],"properties":{"type":{"type":"string"},"name":{"type":"string"}}}}
        }
    }) };
    if let Ok(compiled) = jsonschema::JSONSchema::compile(&schema_json) {
        if let Err(errors) = compiled.validate(doc) {
            let first = errors.into_iter().next().map(|e| e.to_string()).unwrap_or_else(|| "schema validation failed".into());
            return Err(first);
        }
    }
    Ok(true)
}

pub async fn get_sbom(State(_state): State<AppState>, Path(digest): Path<String>, headers_in: HeaderMap) -> ApiResult<impl IntoResponse> {
    // SBOM expected at storage layout: /data/sbom/<digest>.sbom.json OR configurable base dir
    let dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    let filename = format!("{}.sbom.json", digest);
    let primary = PathBuf::from(&dir).join(&filename);
    if primary.exists() {
        let bytes = match tokio::fs::read(&primary).await { Ok(b)=>b, Err(e)=> return Err(ApiError::internal(format!("read sbom: {e}"))) };
        let mut hasher = Sha256::new(); hasher.update(&bytes); let etag_val = format!("\"{:x}\"", hasher.finalize());
        if let Some(if_none) = headers_in.get("if-none-match").and_then(|v| v.to_str().ok()) {
            if if_none == etag_val { return Ok((StatusCode::NOT_MODIFIED, HeaderMap::new(), Vec::new())); }
        }
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("ETag", HeaderValue::from_str(&etag_val).unwrap_or(HeaderValue::from_static("invalid")));
        headers.insert("Cache-Control", HeaderValue::from_static("public, immutable, max-age=31536000"));
        return Ok((StatusCode::OK, headers, bytes));
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
        match validate_cyclonedx(&json) { Ok(_) => { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["cyclonedx_valid"]).inc(); SBOM_VALIDATION_TOTAL.with_label_values(&["ok"]).inc(); }, Err(e) => { SBOM_UPLOAD_STATUS_TOTAL.with_label_values(&["cyclonedx_invalid"]).inc(); SBOM_VALIDATION_TOTAL.with_label_values(&["fail"]).inc(); return Err(ApiError::bad_request(format!("invalid CycloneDX: {e}"))); } }
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
