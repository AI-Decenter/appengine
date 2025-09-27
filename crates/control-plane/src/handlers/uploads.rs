use axum::{extract::State, Json};
use axum::http::{StatusCode, HeaderMap};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, error::ApiError, telemetry::REGISTRY, models::Artifact};
use utoipa::{ToSchema, IntoParams};
use std::{fs, path::PathBuf, io::Write, time::Instant};
use tracing::{info,error,warn};
use sha2::{Sha256, Digest};
use sqlx::Row;

#[derive(Deserialize)]
pub struct UploadForm { pub app_name: String }

#[utoipa::path(
    post,
    path = "/artifacts",
    responses(
        (status = 200, description = "Artifact uploaded or duplicate", body = UploadResponse),
        (status = 400, description = "Missing or invalid digest", body = crate::error::ApiErrorBody),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "aether"
)]
pub async fn upload_artifact(State(state): State<AppState>, headers: HeaderMap, mut multipart: axum::extract::Multipart) -> impl IntoResponse {
    static ARTIFACT_UPLOAD_BYTES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
        let c = prometheus::IntCounter::new("artifact_upload_bytes_total", "Total uploaded artifact bytes (after write)").unwrap();
        REGISTRY.register(Box::new(c.clone())).ok(); c
    });
    static ARTIFACT_UPLOAD_DURATION: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
        let h = prometheus::Histogram::with_opts(prometheus::HistogramOpts::new("artifact_upload_duration_seconds", "Artifact upload+verify duration seconds")).unwrap();
        REGISTRY.register(Box::new(h.clone())).ok(); h
    });
    static ARTIFACT_ACTIVE_GAUGE: once_cell::sync::Lazy<prometheus::IntGauge> = once_cell::sync::Lazy::new(|| {
        let g = prometheus::IntGauge::new("artifact_uploads_in_progress", "Concurrent artifact uploads in progress").unwrap();
        REGISTRY.register(Box::new(g.clone())).ok(); g
    });
    let start = Instant::now();
    ARTIFACT_ACTIVE_GAUGE.inc();
    // Validate headers
    let Some(digest_header) = headers.get("x-aether-artifact-digest").and_then(|v| v.to_str().ok()) else {
        return ApiError::new(StatusCode::BAD_REQUEST, "missing_digest", "X-Aether-Artifact-Digest required").into_response();
    };
    if digest_header.len() != 64 || !digest_header.chars().all(|c| c.is_ascii_hexdigit()) {
        return ApiError::new(StatusCode::BAD_REQUEST, "invalid_digest", "digest must be 64 hex chars").into_response();
    }
    let signature = headers.get("x-aether-signature").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    let mut app_name: Option<String> = None;
    // We'll stream artifact into temp file and recompute digest
    let dir = std::env::var("ARTIFACT_STORE_DIR").unwrap_or_else(|_| "./data/artifacts".into());
    if let Err(e) = fs::create_dir_all(&dir) { error!(?e, "create_store_dir_failed"); return ApiError::internal("store dir").into_response(); }
    let tmp_id = Uuid::new_v4();
    let tmp_path = PathBuf::from(&dir).join(format!("upload-{tmp_id}.part"));
    let mut hasher = Sha256::new();
    let mut file_written = false;
    let mut f = match fs::File::create(&tmp_path) { Ok(f)=>f, Err(e)=> { error!(?e, "create_tmp_failed"); return ApiError::internal("tmp create").into_response(); } };

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().map(|s| s.to_string());
        match name.as_deref() {
            Some("app_name") => { if let Ok(val) = field.text().await { app_name = Some(val); } }
            Some("artifact") => {
                // stream
                let mut stream = field;
                while let Ok(Some(chunk)) = stream.chunk().await {
                    hasher.update(&chunk);
                    if let Err(e) = f.write_all(&chunk) { error!(?e, "write_chunk_failed"); return ApiError::internal("write").into_response(); }
                }
                file_written = true;
            }
            _ => {}
        }
    }
    let Some(app) = app_name else { let _ = fs::remove_file(&tmp_path); return ApiError::bad_request("missing app_name").into_response(); };
    if !file_written { let _ = fs::remove_file(&tmp_path); return ApiError::bad_request("missing artifact file").into_response(); }
    let computed = format!("{:x}", hasher.finalize());
    if computed != digest_header {
        let _ = fs::remove_file(&tmp_path);
        return ApiError::new(StatusCode::BAD_REQUEST, "digest_mismatch", "artifact digest mismatch").into_response();
    }

    // Idempotency: if digest exists in DB, reuse path / do not rewrite
    let mut conn = match state.db.acquire().await { Ok(c)=>c, Err(e)=> { error!(?e, "acquire_conn"); return ApiError::internal("db").into_response(); } };
    if let Ok(existing) = sqlx::query_scalar::<_, String>("SELECT id::text FROM artifacts WHERE digest = $1").bind(&computed).fetch_one(&mut *conn).await {
        info!(app=%app, digest=%computed, artifact_id=%existing, "artifact_duplicate_digest");
        let url = format!("file://{dir}/{}.tar.gz", existing);
        let _ = fs::remove_file(&tmp_path);
        return (StatusCode::OK, Json(serde_json::json!({"artifact_url": url, "digest": computed, "duplicate": true}))).into_response();
    }

    // Promote temp -> final
    let file_id = Uuid::new_v4();
    let final_path = PathBuf::from(&dir).join(format!("{file_id}.tar.gz"));
    if let Err(e)=fs::rename(&tmp_path, &final_path) { warn!(?e, "rename_failed_fallback_copy"); if let Err(e2)=fs::copy(&tmp_path, &final_path) { error!(?e2, "copy_failed"); return ApiError::internal("persist").into_response(); } let _ = fs::remove_file(&tmp_path); }
    let meta = fs::metadata(&final_path).ok();
    let size = meta.map(|m| m.len() as i64).unwrap_or(0);
    let url = format!("file://{}", final_path.display());
    // Resolve app_id if app exists
    let app_id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM applications WHERE name = $1")
        .bind(&app)
        .fetch_optional(&mut *conn).await.ok().flatten();
    // Insert artifact row
    let rec = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified) VALUES ($1,$2,$3,$4,NULL,NULL,FALSE) RETURNING id")
        .bind(app_id)
        .bind(&computed)
        .bind(size)
        .bind(signature.as_ref())
        .fetch_one(&mut *conn).await;
    match rec {
        Ok(row)=> {
            let id: Uuid = row.get::<Uuid, _>(0);
            info!(app=%app, digest=%computed, artifact_id=%id, size_bytes=size, "artifact_uploaded");
            ARTIFACT_UPLOAD_BYTES.inc_by(size as u64);
            ARTIFACT_UPLOAD_DURATION.observe(start.elapsed().as_secs_f64());
            (StatusCode::OK, Json(serde_json::json!(UploadResponse { artifact_url: url, digest: computed, duplicate: false, app_linked: app_id.is_some() })) ).into_response()
        }
        Err(e)=> { error!(?e, "db_insert_artifact_failed"); ApiError::internal("db insert").into_response() }
    }
}

impl DropGuard {
    fn new() -> Self { Self }
}
struct DropGuard;
impl Drop for DropGuard { fn drop(&mut self) { if let Some(g) = std::thread::panicking().then(|| ()).is_none().then_some(()) { let _ = g; } } }

#[derive(serde::Serialize, ToSchema)]
pub struct UploadResponse { pub artifact_url: String, pub digest: String, pub duplicate: bool, pub app_linked: bool }

#[utoipa::path(
    get,
    path = "/artifacts",
    responses( (status = 200, description = "List artifacts", body = [Artifact]) ),
    tag = "aether"
)]
pub async fn list_artifacts(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, Artifact>("SELECT id, app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, created_at FROM artifacts ORDER BY created_at DESC LIMIT 200")
        .fetch_all(&state.db).await
        .unwrap_or_default();
    Json(rows)
}
