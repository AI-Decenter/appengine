use axum::{extract::{State, Path}, Json};
use axum::http::{StatusCode, HeaderMap};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, error::ApiError, telemetry::REGISTRY, models::Artifact};
// Import re-exported get_storage from crate root (avoids direct module path dependency)
use crate::get_storage;
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
static PRESIGN_REQUESTS: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_presign_requests_total", "Total presign requests").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static COMPLETE_DURATION: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
    let h = prometheus::Histogram::with_opts(prometheus::HistogramOpts::new("artifact_complete_duration_seconds", "Duration of complete endpoint processing")).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok(); h
});
static PRESIGN_FAILURES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_presign_failures_total", "Total presign failures").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static COMPLETE_FAILURES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_complete_failures_total", "Total complete failures").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static SIZE_EXCEEDED_FAILURES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_size_exceeded_total", "Total artifacts rejected for exceeding max size").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static PENDING_GC_RUNS: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_pending_gc_runs_total", "Pending artifact GC runs").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static PENDING_GC_DELETED: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_pending_gc_deleted_total", "Pending artifacts deleted by GC").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static DIGEST_MISMATCHES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_digest_mismatch_total", "Total remote digest mismatches (metadata or hash)").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
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
    PRESIGN_REQUESTS.inc();
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
            let key = sk.unwrap_or_else(|| format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest));
            // Generate presigned URL via storage backend
            let storage = get_storage().await;
            let expire_secs = std::env::var("AETHER_PRESIGN_EXPIRE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(900);
            match storage.backend().presign_artifact_put(&key, &req.digest, std::time::Duration::from_secs(expire_secs)).await {
                Ok(p) => return (StatusCode::OK, Json(PresignResponse { upload_url: p.url, storage_key: p.storage_key, method: p.method, headers: p.headers })).into_response(),
                Err(e) => { error!(?e, "presign_backend_error"); PRESIGN_FAILURES.inc(); return ApiError::internal("presign backend").into_response(); }
            }
        }
    }
    // Create new pending record
    let key = format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest);
    let storage = get_storage().await;
    let expire_secs = std::env::var("AETHER_PRESIGN_EXPIRE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(900);
    let presigned = match storage.backend().presign_artifact_put(&key, &req.digest, std::time::Duration::from_secs(expire_secs)).await {
        Ok(p)=> p,
        Err(e)=> { error!(?e, "presign_backend_error"); PRESIGN_FAILURES.inc(); return ApiError::internal("presign backend").into_response(); }
    };
    let _ = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status) VALUES (NULL,$1,0,NULL,NULL,NULL,FALSE,$2,'pending') ON CONFLICT (digest) DO NOTHING")
        .bind(&req.digest)
        .bind(&presigned.storage_key)
        .execute(&state.db).await;
    (StatusCode::OK, Json(PresignResponse { upload_url: presigned.url, storage_key: presigned.storage_key, method: presigned.method, headers: presigned.headers })).into_response()
}

#[derive(serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct CompleteRequest { pub app_name: String, pub digest: String, pub size_bytes: i64, pub signature: Option<String> }

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CompleteResponse { pub artifact_id: String, pub digest: String, pub duplicate: bool, pub verified: bool, pub storage_key: String, pub status: String }

#[utoipa::path(post, path="/artifacts/complete", request_body=CompleteRequest, responses((status=200, body=CompleteResponse),(status=400, body=crate::error::ApiErrorBody)), tag="aether")]
pub async fn complete_artifact(State(state): State<AppState>, Json(req): Json<CompleteRequest>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    // Basic validation
    if req.app_name.trim().is_empty() { return ApiError::bad_request("app_name required").into_response(); }
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return ApiError::new(StatusCode::BAD_REQUEST, "invalid_digest", "digest must be 64 hex").into_response();
    }
    if req.size_bytes < 0 { return ApiError::bad_request("size_bytes must be >= 0").into_response(); }
    if let Ok(max_str) = std::env::var("AETHER_MAX_ARTIFACT_SIZE_BYTES") { if let Ok(max) = max_str.parse::<i64>() { if max>0 && req.size_bytes > max { SIZE_EXCEEDED_FAILURES.inc(); return ApiError::new(StatusCode::BAD_REQUEST, "size_exceeded", format!("reported size {} exceeds max {}", req.size_bytes, max)).into_response(); } } }

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
    let require_presign = std::env::var("AETHER_REQUIRE_PRESIGN").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if require_presign && existing.is_none() {
        return ApiError::new(StatusCode::BAD_REQUEST, "presign_required", "presign step required before completion").into_response();
    }
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
            // Optional remote size verification if backend can provide it
            let _verified_remote = true; // placeholder for future remote verification logic extension
            if let Some(storage_key) = sk.as_ref() {
                if std::env::var("AETHER_VERIFY_REMOTE_SIZE").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(true) {
                    let storage = get_storage().await;
                    if let Ok(Some(actual)) = storage.backend().head_size(storage_key).await {
                        if actual != req.size_bytes { return ApiError::new(StatusCode::BAD_REQUEST, "size_mismatch", format!("remote object size {} != reported {}", actual, req.size_bytes)).into_response(); }
                    }
                }
                if std::env::var("AETHER_VERIFY_REMOTE_DIGEST").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(true) {
                    let storage = get_storage().await;
                    if let Ok(Some(meta)) = storage.backend().head_metadata(storage_key).await {
                        if let Some(remote) = meta.get("sha256") { if remote != &req.digest { DIGEST_MISMATCHES.inc(); return ApiError::new(StatusCode::BAD_REQUEST, "digest_mismatch_remote", format!("remote metadata sha256 {} != provided {}", remote, req.digest)).into_response(); } }
                    }
                }
                // Optional remote hash verification (small objects only)
                if std::env::var("AETHER_VERIFY_REMOTE_HASH").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) {
                    let max_bytes = std::env::var("AETHER_REMOTE_HASH_MAX_BYTES").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(8_000_000); // 8MB default
                    if max_bytes > 0 {
                        let storage = get_storage().await;
                        if let Ok(Some(remote_hash)) = storage.backend().remote_sha256(storage_key, max_bytes).await {
                            if remote_hash != req.digest { DIGEST_MISMATCHES.inc(); return ApiError::new(StatusCode::BAD_REQUEST, "digest_mismatch_remote_hash", format!("remote hash {} != provided {}", remote_hash, req.digest)).into_response(); }
                        }
                    }
                }
            }
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
                Err(e) => { error!(?e, "update_artifact_failed"); COMPLETE_FAILURES.inc(); return ApiError::internal("db update").into_response(); }
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
            COMPLETE_DURATION.observe(start.elapsed().as_secs_f64());
            (StatusCode::OK, Json(CompleteResponse {
                artifact_id: id.to_string(),
                digest: req.digest,
                duplicate: false,
                verified,
                storage_key: key,
                status: "stored".into(),
            })).into_response()
        }
        Err(e) => { error!(?e, "insert_artifact_failed"); COMPLETE_FAILURES.inc(); ApiError::internal("db insert").into_response() }
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
    if let Ok(max_str) = std::env::var("AETHER_MAX_ARTIFACT_SIZE_BYTES") { if let Ok(max)=max_str.parse::<i64>() { if max>0 && size > max { SIZE_EXCEEDED_FAILURES.inc(); let _ = fs::remove_file(&final_path); return ApiError::new(StatusCode::BAD_REQUEST, "size_exceeded", format!("artifact size {} exceeds max {}", size, max)).into_response(); } } }
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
    let rec = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, status) VALUES ($1,$2,$3,$4,NULL,NULL,$5,'stored') RETURNING id")
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
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1::BIGINT FROM artifacts WHERE digest=$1 AND status='stored'")
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
    // Select columns in the exact order of the Artifact struct definition.
    let rows = sqlx::query_as::<_, Artifact>("SELECT id, app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status, created_at FROM artifacts ORDER BY created_at DESC LIMIT 200")
        .fetch_all(&state.db).await
        .unwrap_or_default();
    Json(rows)
}

/// GC utility: run once deleting stale pending artifacts older than ttl_secs.
#[allow(dead_code)]
pub async fn run_pending_gc(db: &sqlx::Pool<sqlx::Postgres>, ttl_secs: i64) -> anyhow::Result<u64> {
    use chrono::{Utc, Duration as ChronoDuration};
    let cutoff = Utc::now() - ChronoDuration::seconds(ttl_secs.max(0));
    // Use CTE for portability (DELETE not valid directly in FROM subquery across all Pg versions)
    let deleted_res = sqlx::query_scalar::<_, i64>("WITH del AS (DELETE FROM artifacts WHERE status='pending' AND created_at < $1 RETURNING 1) SELECT COUNT(*) FROM del")
        .bind(cutoff)
        .fetch_one(db).await;
    let deleted = match deleted_res { Ok(v)=>v, Err(e)=> { warn!(?e, "pending_gc_delete_failed"); 0 } };
    PENDING_GC_RUNS.inc();
    if deleted > 0 { PENDING_GC_DELETED.inc_by(deleted as u64); }
    Ok(deleted as u64)
}
