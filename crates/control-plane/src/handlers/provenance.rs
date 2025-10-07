use axum::{extract::{Path, State}, http::{StatusCode, HeaderMap, HeaderValue}, Json};
use crate::{AppState, error::{ApiError, ApiResult}};
use std::path::PathBuf;
use serde::Serialize;
use sha2::{Sha256, Digest};
use std::io::Write;

#[derive(Serialize)]
pub struct ProvenanceEntry { pub digest: String, pub app: Option<String>, pub sbom: bool, pub attestation: bool }

pub async fn list_provenance(State(state): State<AppState>) -> ApiResult<Json<Vec<ProvenanceEntry>>> {
    // Join artifacts with applications to recover app name
    let rows = sqlx::query("SELECT a.digest, apps.name as app_name, a.sbom_url FROM artifacts a LEFT JOIN applications apps ON apps.id = a.app_id WHERE a.provenance_present=TRUE ORDER BY a.created_at DESC LIMIT 500")
        .fetch_all(&state.db).await.map_err(|e| ApiError::internal(format!("db: {e}")))?;
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    let mut out = Vec::new();
    use sqlx::Row;
    for r in rows { let digest: String = r.get("digest"); let app: Option<String> = r.get("app_name"); let sbom_url: Option<String> = r.get("sbom_url"); let sbom = sbom_url.is_some(); let attestation = if let Some(ref appn) = app { PathBuf::from(&dir).join(format!("{appn}-{digest}.prov2.dsse.json")) } else { PathBuf::from(&dir).join(format!("{digest}.prov2.dsse.json")) }.exists(); out.push(ProvenanceEntry { digest, app, sbom, attestation }); }
    Ok(Json(out))
}

pub async fn get_provenance(State(_state): State<AppState>, Path(digest): Path<String>, headers_in: HeaderMap) -> ApiResult<(StatusCode, HeaderMap, Vec<u8>)> {
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    // app name unknown -> search first match
    let path_glob = format!("{}/*-{}.prov2.json", dir, digest);
    let mut found: Option<PathBuf> = None;
    if let Ok(entries) = glob::glob(&path_glob) { if let Some(e) = entries.flatten().next() { found = Some(e); } }
    let Some(p) = found else { return Err(ApiError::not_found("provenance not found")); };
    let bytes = std::fs::read(&p).map_err(|e| ApiError::internal(format!("read: {e}")))?;
    let mut hasher = Sha256::new(); hasher.update(&bytes); let etag = format!("\"{:x}\"", hasher.finalize());
    if let Some(if_none) = headers_in.get("if-none-match").and_then(|v| v.to_str().ok()) { if if_none == etag { return Ok((StatusCode::NOT_MODIFIED, HeaderMap::new(), Vec::new())); } }
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers.insert("ETag", HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("invalid")));
    let accept_enc = headers_in.get("accept-encoding").and_then(|v| v.to_str().ok()).unwrap_or("");
    if accept_enc.contains("gzip") {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        if enc.write_all(&bytes).is_ok() { if let Ok(comp)=enc.finish() { headers.insert("Content-Encoding", HeaderValue::from_static("gzip")); return Ok((StatusCode::OK, headers, comp)); } }
    }
    Ok((StatusCode::OK, headers, bytes))
}

pub async fn get_attestation(State(_state): State<AppState>, Path(digest): Path<String>, headers_in: HeaderMap) -> ApiResult<(StatusCode, HeaderMap, Vec<u8>)> {
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    let path_glob = format!("{}/*-{}.prov2.dsse.json", dir, digest);
    let mut found: Option<PathBuf> = None;
    if let Ok(entries) = glob::glob(&path_glob) { if let Some(e) = entries.flatten().next() { found = Some(e); } }
    let Some(p) = found else { return Err(ApiError::not_found("attestation not found")); };
    let bytes = std::fs::read(&p).map_err(|e| ApiError::internal(format!("read: {e}")))?;
    let mut hasher = Sha256::new(); hasher.update(&bytes); let etag = format!("\"{:x}\"", hasher.finalize());
    if let Some(if_none) = headers_in.get("if-none-match").and_then(|v| v.to_str().ok()) { if if_none == etag { return Ok((StatusCode::NOT_MODIFIED, HeaderMap::new(), Vec::new())); } }
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers.insert("ETag", HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("invalid")));
    let accept_enc = headers_in.get("accept-encoding").and_then(|v| v.to_str().ok()).unwrap_or("");
    if accept_enc.contains("gzip") {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        if enc.write_all(&bytes).is_ok() { if let Ok(comp)=enc.finish() { headers.insert("Content-Encoding", HeaderValue::from_static("gzip")); return Ok((StatusCode::OK, headers, comp)); } }
    }
    Ok((StatusCode::OK, headers, bytes))
}