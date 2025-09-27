use axum::{extract::{State, Path}, Json};
use axum::http::{StatusCode, HeaderMap};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, error::ApiError, telemetry::REGISTRY, models::Artifact};
use utoipa::ToSchema;
use std::{fs, path::PathBuf, io::Write, time::Instant};
use tracing::{info,error,warn, span, Level};
use sha2::{Sha256, Digest};
use sqlx::Row;

// Metrics & concurrency primitives (module-level)
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
static ARTIFACTS_TOTAL: once_cell::sync::Lazy<prometheus::IntGauge> = once_cell::sync::Lazy::new(|| {
    let g = prometheus::IntGauge::new("artifacts_total", "Total number of stored artifacts").unwrap();
    REGISTRY.register(Box::new(g.clone())).ok(); g
});
static UPLOAD_SEMAPHORE: once_cell::sync::Lazy<tokio::sync::Semaphore> = once_cell::sync::Lazy::new(|| {
    let max = std::env::var("AETHER_MAX_CONCURRENT_UPLOADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|v| *v>0).unwrap_or(32);
    tokio::sync::Semaphore::new(max)
});

#[derive(Deserialize)]
pub struct UploadForm { pub app_name: String }

#[derive(serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct PresignRequest { pub app_name: String, pub digest: String }

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PresignResponse { pub upload_url: String, pub storage_key: String, pub method: String, pub headers: std::collections::HashMap<String,String> }

#[utoipa::path(post, path="/artifacts/presign", request_body=PresignRequest, responses((status=200, body=PresignResponse),(status=400, body=crate::error::ApiErrorBody)) , tag="aether")]
pub async fn presign_artifact(State(state): State<AppState>, Json(req): Json<PresignRequest>) -> impl IntoResponse {
    if req.app_name.trim().is_empty() { return ApiError::bad_request("app_name required").into_response(); }
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) { return ApiError::new(StatusCode::BAD_REQUEST, "invalid_digest", "digest must be 64 hex").into_response(); }
    // Check existing artifact row
    if let Ok(Some((_id,status, sk))) = sqlx::query_as::<_, (String,String,Option<String>)>(
        "SELECT id::text, status, storage_key FROM artifacts WHERE digest=$1")
        .bind(&req.digest)
        .fetch_optional(&state.db).await {
        if status == "stored" {
            let headers = std::collections::HashMap::new();
            return (StatusCode::OK, Json(PresignResponse { upload_url: String::new(), storage_key: sk.unwrap_or_default(), method: "NONE".into(), headers })).into_response();
        } else {
            let bucket = std::env::var("AETHER_ARTIFACT_BUCKET").unwrap_or_else(|_| "artifacts".into());
            let key = sk.unwrap_or_else(|| format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest));
            let base_url = std::env::var("AETHER_S3_BASE_URL").unwrap_or_else(|_| "http://minio.local:9000".into());
            let upload_url = format!("{}/{}/{}", base_url.trim_end_matches('/'), bucket, key);
            let mut headers = std::collections::HashMap::new(); headers.insert("x-amz-acl".into(), "private".into());
            return (StatusCode::OK, Json(PresignResponse { upload_url, storage_key: key, method: "PUT".into(), headers })).into_response();
        }
    }
    // Create new pending record
    let bucket = std::env::var("AETHER_ARTIFACT_BUCKET").unwrap_or_else(|_| "artifacts".into());
    let key = format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest);
    let base_url = std::env::var("AETHER_S3_BASE_URL").unwrap_or_else(|_| "http://minio.local:9000".into());
    let upload_url = format!("{}/{}/{}", base_url.trim_end_matches('/'), bucket, key);
    let mut headers = std::collections::HashMap::new(); headers.insert("x-amz-acl".into(), "private".into());
    let _ = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status) VALUES (NULL,$1,0,NULL,NULL,NULL,FALSE,$2,'pending') ON CONFLICT (digest) DO NOTHING")
        .bind(&req.digest)
        .bind(&key)
        .execute(&state.db).await;
    (StatusCode::OK, Json(PresignResponse { upload_url, storage_key: key, method: "PUT".into(), headers })).into_response()
}

#[derive(serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct CompleteRequest { pub app_name: String, pub digest: String, pub size_bytes: i64, pub signature: Option<String> }

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CompleteResponse { pub artifact_id: String, pub digest: String, pub duplicate: bool, pub verified: bool, pub storage_key: String, pub status: String }

#[utoipa::path(post, path="/artifacts/complete", request_body=CompleteRequest, responses((status=200, body=CompleteResponse),(status=400, body=crate::error::ApiErrorBody)), tag="aether")]
pub async fn complete_artifact(State(state): State<AppState>, Json(req): Json<CompleteRequest>) -> impl IntoResponse {
    // Basic validation
    if req.app_name.trim().is_empty() { return ApiError::bad_request("app_name required").into_response(); }
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return ApiError::new(StatusCode::BAD_REQUEST, "invalid_digest", "digest must be 64 hex").into_response();
    }
    if req.size_bytes < 0 { return ApiError::bad_request("size_bytes must be >= 0").into_response(); }

    let signature = req.signature.clone();
    let key = format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest);

    // DB connection
    let mut conn = match state.db.acquire().await { Ok(c)=>c, Err(e)=> { error!(?e, "acquire_conn"); return ApiError::internal("db").into_response(); } };

    // Resolve application id (optional link)
    let app_id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM applications WHERE name=$1")
        .bind(&req.app_name)
        .fetch_optional(&mut *conn).await.ok().flatten();

    // Check if artifact exists
    let existing = sqlx::query_as::<_, (Uuid,bool,Option<String>,String)>("SELECT id, verified, storage_key, status FROM artifacts WHERE digest=$1")
        .bind(&req.digest)
        .fetch_optional(&mut *conn).await.ok().flatten();
    if let Some((id, verified_prev, sk, status)) = &existing {
        if status == "stored" {
            return (StatusCode::OK, Json(CompleteResponse {
                artifact_id: id.to_string(),
                digest: req.digest.clone(),
                duplicate: true,
                verified: *verified_prev,
                storage_key: sk.clone().unwrap_or_default(),
                status: status.clone(),
            })).into_response();
        } else if status == "pending" {
            // Update pending record with final size and optional signature
            let upd = sqlx::query("UPDATE artifacts SET app_id=$1, size_bytes=$2, signature=$3, verified=$4, storage_key=$5, status='stored' WHERE id=$6 RETURNING verified")
                .bind(app_id)
                .bind(req.size_bytes)
                .bind(signature.as_ref())
                .bind(false) // pending completions are never verified
                .bind(&key)
                .bind(id)
                .fetch_one(&mut *conn).await;
            match upd {
                Ok(r) => {
                    let final_verified: bool = r.get(0);
                    return (StatusCode::OK, Json(CompleteResponse {
                        artifact_id: id.to_string(),
                        digest: req.digest,
                        duplicate: false,
                        verified: final_verified,
                        storage_key: key,
                        status: "stored".into(),
                    })).into_response();
                }
                Err(e) => { error!(?e, "update_artifact_failed"); return ApiError::internal("db update").into_response(); }
            }
        }
    }

    // Signature verification (optional)
    let mut verified = false;
    if let Some(sig_hex) = signature.as_ref() {
        if sig_hex.len()==128 && sig_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(Some(app_uuid)) = sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT id FROM applications WHERE name=$1")
                .bind(&req.app_name)
                .fetch_optional(&mut *conn).await {
                if let Ok(rows) = sqlx::query_scalar::<_, String>(
                    "SELECT public_key_hex FROM public_keys WHERE app_id=$1 AND active")
                    .bind(app_uuid)
                    .fetch_all(&mut *conn).await {
                    for pk_hex in rows {
                        if let (Ok(sig_bytes), Ok(pk_bytes)) = (hex::decode(sig_hex), hex::decode(&pk_hex)) {
                            if pk_bytes.len()==32 && sig_bytes.len()==64 {
                                use ed25519_dalek::{Verifier, Signature, VerifyingKey};
                                if let (Ok(vk), Ok(sig)) = (
                                    VerifyingKey::from_bytes(pk_bytes.as_slice().try_into().unwrap()),
                                    Signature::from_slice(&sig_bytes)
                                ) {
                                    if vk.verify(req.digest.as_bytes(), &sig).is_ok() { verified = true; break; }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Insert metadata (status: stored)
    let ins = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status) VALUES ($1,$2,$3,$4,NULL,NULL,$5,$6,'stored') RETURNING id")
        .bind(app_id)
        .bind(&req.digest)
        .bind(req.size_bytes)
        .bind(signature.as_ref())
        .bind(verified) // $5 verified
        .bind(&key) // $6 storage_key
        .fetch_one(&mut *conn).await;

    match ins {
        Ok(row) => {
            let id: Uuid = row.get(0);
            ARTIFACTS_TOTAL.inc();
            (StatusCode::OK, Json(CompleteResponse {
                artifact_id: id.to_string(),
                digest: req.digest,
                duplicate: false,
                verified,
                storage_key: key,
                status: "stored".into(),
            })).into_response()
        }
        Err(e) => { error!(?e, "insert_artifact_failed"); ApiError::internal("db insert").into_response() }
    }
}

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
    let start = Instant::now();
    let _permit = match UPLOAD_SEMAPHORE.acquire().await { Ok(p)=>p, Err(_)=> { return ApiError::internal("semaphore closed").into_response(); } };
    struct InProgress;
    impl Drop for InProgress { fn drop(&mut self) { ARTIFACT_ACTIVE_GAUGE.dec(); } }
    ARTIFACT_ACTIVE_GAUGE.inc();
    let _g = InProgress;
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
    if let Ok(row) = sqlx::query("SELECT id, app_id, verified FROM artifacts WHERE digest = $1")
        .bind(&computed)
        .fetch_one(&mut *conn).await {
        let existing_id: Uuid = row.get(0);
        let existing_app_id: Option<Uuid> = row.get(1);
        let existing_verified: bool = row.get(2);
        info!(app=%app, digest=%computed, artifact_id=%existing_id, "artifact_duplicate_digest");
        let dup_url = format!("file://{dir}/{}.tar.gz", existing_id);
        let _ = fs::remove_file(&tmp_path);
        return (StatusCode::OK, Json(serde_json::json!(UploadResponse { artifact_url: dup_url, digest: computed, duplicate: true, app_linked: existing_app_id.is_some(), verified: existing_verified })) ).into_response();
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
    // Attempt signature verification using DB stored public keys (active)
    let mut verified = false;
    if let Some(sig_hex) = signature.as_ref() {
        if sig_hex.len() == 128 && sig_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let span_verify = span!(Level::DEBUG, "signature_verify", app=%app, digest=%computed);
            let _e = span_verify.enter();
            if let Ok(Some(app_uuid)) = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM applications WHERE name=$1")
                .bind(&app)
                .fetch_optional(&mut *conn).await {
                if let Ok(rows) = sqlx::query_scalar::<_, String>("SELECT public_key_hex FROM public_keys WHERE app_id=$1 AND active")
                    .bind(app_uuid)
                    .fetch_all(&mut *conn).await {
                    for pk_hex in rows {
                        if let (Ok(sig_bytes), Ok(pk_bytes)) = (hex::decode(sig_hex), hex::decode(&pk_hex)) {
                            if pk_bytes.len()==32 && sig_bytes.len()==64 {
                                use ed25519_dalek::{Verifier, Signature, VerifyingKey};
                                if let (Ok(vk), Ok(sig)) = (VerifyingKey::from_bytes(pk_bytes.as_slice().try_into().unwrap()), Signature::from_slice(&sig_bytes)) {
                                    if vk.verify(computed.as_bytes(), &sig).is_ok() { verified = true; info!("signature_verified"); break; }
                                }
                            }
                        }
                    }
                }
            }
            if !verified { warn!("signature_unverified"); }
        }
    }
    // Insert artifact row
    let rec = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified) VALUES ($1,$2,$3,$4,NULL,NULL,$5) RETURNING id")
        .bind(app_id)
        .bind(&computed)
        .bind(size)
        .bind(signature.as_ref())
        .bind(verified)
        .fetch_one(&mut *conn).await;
    match rec {
        Ok(row)=> {
            let id: Uuid = row.get::<Uuid, _>(0);
            info!(app=%app, digest=%computed, artifact_id=%id, size_bytes=size, "artifact_uploaded");
            ARTIFACT_UPLOAD_BYTES.inc_by(size as u64);
            ARTIFACT_UPLOAD_DURATION.observe(start.elapsed().as_secs_f64());
            ARTIFACTS_TOTAL.inc();
            (StatusCode::OK, Json(serde_json::json!(UploadResponse { artifact_url: url, digest: computed, duplicate: false, app_linked: app_id.is_some(), verified })) ).into_response()
        }
        Err(e)=> { error!(?e, "db_insert_artifact_failed"); ApiError::internal("db insert").into_response() }
    }
}

/// Initialize artifacts_total gauge at startup (call from router build)
pub async fn init_artifacts_total(db: &sqlx::Pool<sqlx::Postgres>) {
    static INIT: once_cell::sync::OnceCell<()> = once_cell::sync::OnceCell::new();
    if INIT.get().is_some() { return; }
    if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM artifacts").fetch_one(db).await { 
        let g = once_cell::sync::Lazy::force(&ARTIFACTS_TOTAL);
    g.set(count);
        INIT.set(()).ok();
    }
}

#[derive(serde::Serialize, ToSchema)]
pub struct UploadResponse { pub artifact_url: String, pub digest: String, pub duplicate: bool, pub app_linked: bool, pub verified: bool }

/// HEAD existence check for artifact by digest
pub async fn head_artifact(State(state): State<AppState>, Path(digest): Path<String>) -> impl IntoResponse {
    if digest.len()!=64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) { return StatusCode::BAD_REQUEST; }
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM artifacts WHERE digest=$1 AND status='stored'")
        .bind(&digest)
        .fetch_optional(&state.db).await.ok().flatten().is_some();
    if exists { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

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
