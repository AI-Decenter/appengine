use axum::{extract::{State, Path}, Json};
use axum::http::{StatusCode, HeaderMap};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;
use sqlx::pool::PoolConnection;
use crate::{AppState, error::ApiError, telemetry::REGISTRY, models::Artifact};
// Import re-exported get_storage from crate root (avoids direct module path dependency)
use crate::get_storage;
use utoipa::ToSchema;
use std::{fs, path::PathBuf, io::Write, time::Instant};
use tracing::{info,error,warn, span, Level};
use sha2::{Sha256, Digest};
use sqlx::Row;

// Helper to obtain &mut PgConnection without inline &mut *conn pattern (avoids clippy explicit_auto_deref)
#[inline]
fn pg(conn: &mut PoolConnection<sqlx::Postgres>) -> &mut sqlx::PgConnection { &mut *conn }

// Metrics & concurrency primitives (module-level)
static ARTIFACT_UPLOAD_BYTES: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_upload_bytes_total", "Total uploaded artifact bytes (after write)").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static ARTIFACT_UPLOAD_DURATION: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
    let h = prometheus::Histogram::with_opts(prometheus::HistogramOpts::new("artifact_upload_duration_seconds", "Artifact upload+verify duration seconds")).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok(); h
});
static ARTIFACT_PUT_DURATION: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
    let h = prometheus::Histogram::with_opts(prometheus::HistogramOpts::new("artifact_put_duration_seconds", "Client reported raw PUT upload duration (seconds)")).unwrap();
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
static LEGACY_UPLOAD_REQUESTS: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_legacy_upload_requests_total", "Total legacy multipart /artifacts endpoint requests").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static ARTIFACT_EVENTS_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_events_total", "Total artifact events emitted").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static MULTIPART_INITS_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_multipart_inits_total", "Total multipart upload initiations").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static MULTIPART_PART_PRESIGNS_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_multipart_part_presigns_total", "Total multipart part presign requests").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static MULTIPART_COMPLETES_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_multipart_completes_total", "Total multipart completions").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
static MULTIPART_COMPLETE_FAILURES_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_multipart_complete_failures_total", "Total multipart completion failures").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok(); c
});
// Histogram for individual multipart part sizes (bytes) observed at complete time.
static MULTIPART_PART_SIZE_HIST: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
    let opts = prometheus::HistogramOpts::new("artifact_multipart_part_size_bytes", "Size distribution of multipart parts (bytes)")
        .buckets(vec![256_000.0, 512_000.0, 1_000_000.0, 2_000_000.0, 4_000_000.0, 8_000_000.0, 16_000_000.0, 32_000_000.0, 64_000_000.0]);
    let h = prometheus::Histogram::with_opts(opts).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok(); h
});
// Histogram for number of parts per multipart artifact.
static MULTIPART_PARTS_PER_ARTIFACT: once_cell::sync::Lazy<prometheus::Histogram> = once_cell::sync::Lazy::new(|| {
    let opts = prometheus::HistogramOpts::new("artifact_multipart_parts_per_artifact", "Distribution of multipart part counts per artifact")
        .buckets(vec![1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0]);
    let h = prometheus::Histogram::with_opts(opts).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok(); h
});
static QUOTA_EXCEEDED_TOTAL: once_cell::sync::Lazy<prometheus::IntCounter> = once_cell::sync::Lazy::new(|| {
    let c = prometheus::IntCounter::new("artifact_quota_exceeded_total", "Total quota enforcement rejections").unwrap();
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

#[utoipa::path(
    post,
    path="/artifacts/presign",
    request_body=PresignRequest,
    responses(
        (status=200, body=PresignResponse, description = "Presigned URL (or duplicate already stored)"),
        (status=400, body=crate::error::ApiErrorBody)
    ),
    tag="aether",
    summary="Presign single-part artifact upload",
    description="Phase 1 of two-phase upload. Creates a pending artifact row (idempotent by digest) and returns a presigned PUT URL. If the artifact already exists (status=stored) an empty method NONE response is returned."
)]
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
    // If application exists, link immediately so quota/retention count sees pending
    let app_id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM applications WHERE name=$1")
        .bind(&req.app_name)
        .fetch_optional(&state.db).await.ok().flatten();
    let _ = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status) VALUES ($1,$2,0,NULL,NULL,NULL,FALSE,$3,'pending') ON CONFLICT (digest) DO NOTHING")
        .bind(app_id)
        .bind(&req.digest)
        .bind(&presigned.storage_key)
        .execute(&state.db).await;
    (StatusCode::OK, Json(PresignResponse { upload_url: presigned.url, storage_key: presigned.storage_key, method: presigned.method, headers: presigned.headers })).into_response()
}

#[derive(serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct CompleteRequest { pub app_name: String, pub digest: String, pub size_bytes: i64, pub signature: Option<String>, pub idempotency_key: Option<String> }

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CompleteResponse { pub artifact_id: String, pub digest: String, pub duplicate: bool, pub verified: bool, pub storage_key: String, pub status: String, pub idempotency_key: Option<String> }

#[utoipa::path(
    post,
    path="/artifacts/complete",
    request_body=CompleteRequest,
    responses(
        (status=200, body=CompleteResponse, description="Artifact stored (or duplicate)"),
        (status=400, body=crate::error::ApiErrorBody),
        (status=403, body=crate::error::ApiErrorBody, description="Quota exceeded"),
        (status=409, body=crate::error::ApiErrorBody, description="Idempotency conflict")
    ),
    tag="aether",
    summary="Complete single-part artifact upload",
    description="Phase 2 of two-phase upload. Verifies remote object integrity (size & optional digest), enforces quotas & retention, and finalizes artifact metadata."
)]
pub async fn complete_artifact(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CompleteRequest>) -> impl IntoResponse {
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
    .fetch_optional(pg(&mut conn)).await.ok().flatten();

    // Check if artifact exists
    let existing = sqlx::query_as::<_, (Uuid,bool,Option<String>,String)>("SELECT id, verified, storage_key, status FROM artifacts WHERE digest=$1")
        .bind(&req.digest)
    .fetch_optional(pg(&mut conn)).await.ok().flatten();
    let require_presign = std::env::var("AETHER_REQUIRE_PRESIGN").ok().map(|v| v=="1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
    if require_presign && existing.is_none() {
        return ApiError::new(StatusCode::BAD_REQUEST, "presign_required", "presign step required before completion").into_response();
    }
    if let Some((id, verified_prev, sk, status)) = &existing {
        if status == "stored" {
            if let Some(dh) = headers.get("X-Aether-Upload-Duration").and_then(|v| v.to_str().ok()) { if let Ok(vf)=dh.parse::<f64>() { ARTIFACT_PUT_DURATION.observe(vf); } }
            return (StatusCode::OK, Json(CompleteResponse {
                artifact_id: id.to_string(),
                digest: req.digest.clone(),
                duplicate: true,
                verified: *verified_prev,
                storage_key: sk.clone().unwrap_or_default(),
                status: status.clone(),
                idempotency_key: req.idempotency_key.clone(),
            })).into_response();
        } else if status == "pending" {
            // Idempotency key conflict check: if provided and differs from existing row key (if any)
            if let Some(ref key) = req.idempotency_key {
                if let Ok(Some(Some(k))) = sqlx::query_scalar::<_, Option<String>>("SELECT idempotency_key FROM artifacts WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pg(&mut conn)).await {
                    if k!=*key { return ApiError::new(StatusCode::CONFLICT, "idempotency_conflict", "different operation for same digest").into_response(); }
                }
            }
            // Backfill app_id if missing to ensure quotas see previous stored artifacts
            if let Some(app_uuid)=app_id {
                let _ = sqlx::query("UPDATE artifacts SET app_id=$1 WHERE id=$2 AND app_id IS NULL")
                    .bind(app_uuid)
                    .bind(id)
                    .execute(pg(&mut conn)).await;
            }
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
            // Quota enforcement (if app scoped)
            if let Some(app_uuid) = app_id {
                if let Err(resp) = enforce_quota(&mut conn, app_uuid, req.size_bytes).await { return resp.into_response(); }
            }
            let upd = sqlx::query("UPDATE artifacts SET app_id=$1, size_bytes=$2, signature=$3, verified=$4, storage_key=$5, status='stored', completed_at=NOW(), idempotency_key=COALESCE(idempotency_key,$7) WHERE id=$6 RETURNING verified, idempotency_key")
                .bind(app_id)
                .bind(req.size_bytes)
                .bind(signature.as_ref())
                .bind(false) // pending completions are never verified
                .bind(&key)
                .bind(id)
                .bind(&req.idempotency_key)
                .fetch_one(pg(&mut conn)).await;
            match upd {
                Ok(r) => {
                    let final_verified: bool = r.get(0);
                    let idem: Option<String> = r.get(1);
                    insert_event(&mut conn, *id, "stored").await.ok();
                    retention_gc_if_needed(&mut conn, app_id).await.ok();
                    if let Some(dh) = headers.get("X-Aether-Upload-Duration").and_then(|v| v.to_str().ok()) { if let Ok(vf)=dh.parse::<f64>() { ARTIFACT_PUT_DURATION.observe(vf); } }
                    return (StatusCode::OK, Json(CompleteResponse {
                        artifact_id: id.to_string(),
                        digest: req.digest,
                        duplicate: false,
                        verified: final_verified,
                        storage_key: key,
                        status: "stored".into(),
                        idempotency_key: idem,
                    })).into_response();
                }
                Err(e) => {
                    if let sqlx::Error::Database(db_err) = &e { if db_err.constraint().unwrap_or("").contains("idempotency") { return ApiError::new(StatusCode::CONFLICT, "idempotency_conflict", "idempotency key already used").into_response(); } }
                    error!(?e, "update_artifact_failed"); COMPLETE_FAILURES.inc(); return ApiError::internal("db update").into_response(); }
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
                .fetch_optional(pg(&mut conn)).await {
                if let Ok(rows) = sqlx::query_scalar::<_, String>(
                    "SELECT public_key_hex FROM public_keys WHERE app_id=$1 AND active")
                    .bind(app_uuid)
                    .fetch_all(pg(&mut conn)).await {
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
    // Idempotency key uniqueness: if provided and maps to different digest -> conflict
            if let Some(ref key) = req.idempotency_key {
                if let Ok(Some(Some(d))) = sqlx::query_scalar::<_, Option<String>>("SELECT digest FROM artifacts WHERE idempotency_key=$1")
                    .bind(key)
                    .fetch_optional(pg(&mut conn)).await {
                    if d != req.digest { return ApiError::new(StatusCode::CONFLICT, "idempotency_conflict", "idempotency key already used").into_response(); }
                }
            }
    if let Some(app_uuid) = app_id { if let Err(resp)=enforce_quota(&mut conn, app_uuid, req.size_bytes).await { return resp.into_response(); } }
    let ins = sqlx::query("INSERT INTO artifacts (app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status, completed_at, idempotency_key) VALUES ($1,$2,$3,$4,NULL,NULL,$5,$6,'stored', NOW(), $7) RETURNING id, idempotency_key")
        .bind(app_id)
        .bind(&req.digest)
        .bind(req.size_bytes)
        .bind(signature.as_ref())
        .bind(verified) // $5 verified
        .bind(&key) // $6 storage_key
        .bind(&req.idempotency_key)
    .fetch_one(pg(&mut conn)).await;

    match ins {
        Ok(row) => {
            let id: Uuid = row.get(0);
            let idem: Option<String> = row.get(1);
            ARTIFACTS_TOTAL.inc();
            COMPLETE_DURATION.observe(start.elapsed().as_secs_f64());
            if let Some(dh) = headers.get("X-Aether-Upload-Duration").and_then(|v| v.to_str().ok()) { if let Ok(vf)=dh.parse::<f64>() { ARTIFACT_PUT_DURATION.observe(vf); } }
            insert_event(&mut conn, id, "stored").await.ok();
            retention_gc_if_needed(&mut conn, app_id).await.ok();
            (StatusCode::OK, Json(CompleteResponse {
                artifact_id: id.to_string(),
                digest: req.digest,
                duplicate: false,
                verified,
                storage_key: key,
                status: "stored".into(),
                idempotency_key: idem,
            })).into_response()
        }
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint().unwrap_or("") .contains("idempotency_key") { return ApiError::new(StatusCode::CONFLICT, "idempotency_conflict", "idempotency key already used").into_response(); }
            }
            error!(?e, "insert_artifact_failed"); COMPLETE_FAILURES.inc(); ApiError::internal("db insert").into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/artifacts",
    responses(
        (status = 200, description = "Legacy direct upload (deprecated) or duplicate", body = UploadResponse),
        (status = 400, description = "Missing or invalid digest", body = crate::error::ApiErrorBody)
    ),
    tag = "aether",
    summary = "Legacy direct multipart upload (deprecated)",
    description = "Legacy single-call multipart/form-data endpoint. Prefer the two-phase /artifacts/presign + /artifacts/complete (or multipart variants). Returns X-Aether-Deprecated header on success."
)]
pub async fn upload_artifact(State(state): State<AppState>, headers: HeaderMap, mut multipart: axum::extract::Multipart) -> impl IntoResponse {
    LEGACY_UPLOAD_REQUESTS.inc();
    tracing::warn!("legacy_upload_endpoint_deprecated");
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
    .fetch_one(pg(&mut conn)).await {
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
    .fetch_optional(pg(&mut conn)).await.ok().flatten();
    // Attempt signature verification using DB stored public keys (active)
    let mut verified = false;
    if let Some(sig_hex) = signature.as_ref() {
        if sig_hex.len() == 128 && sig_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let span_verify = span!(Level::DEBUG, "signature_verify", app=%app, digest=%computed);
            let _e = span_verify.enter();
            if let Ok(Some(app_uuid)) = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM applications WHERE name=$1")
                .bind(&app)
                .fetch_optional(pg(&mut conn)).await {
                if let Ok(rows) = sqlx::query_scalar::<_, String>("SELECT public_key_hex FROM public_keys WHERE app_id=$1 AND active")
                    .bind(app_uuid)
                    .fetch_all(pg(&mut conn)).await {
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
    .fetch_one(pg(&mut conn)).await;
    match rec {
        Ok(row)=> {
            let id: Uuid = row.get::<Uuid, _>(0);
            info!(app=%app, digest=%computed, artifact_id=%id, size_bytes=size, "artifact_uploaded");
            ARTIFACT_UPLOAD_BYTES.inc_by(size as u64);
            ARTIFACT_UPLOAD_DURATION.observe(start.elapsed().as_secs_f64());
            ARTIFACTS_TOTAL.inc();
            let mut resp = (StatusCode::OK, Json(serde_json::json!(UploadResponse { artifact_url: url, digest: computed, duplicate: false, app_linked: app_id.is_some(), verified })) ).into_response();
            resp.headers_mut().insert("X-Aether-Deprecated", axum::http::HeaderValue::from_static("true"));
            resp
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

#[utoipa::path(get, path="/artifacts/{digest}/meta", params(("digest"=String, description="Artifact digest")), responses((status=200, body=Artifact),(status=404)), tag="aether")]
pub async fn artifact_meta(State(state): State<AppState>, Path(digest): Path<String>) -> impl IntoResponse {
    if digest.len()!=64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) { return StatusCode::BAD_REQUEST.into_response(); }
    match sqlx::query_as::<_, Artifact>("SELECT id, app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status, created_at, completed_at, idempotency_key, multipart_upload_id, provenance_present, manifest_digest, sbom_manifest_digest, sbom_validated FROM artifacts WHERE digest=$1")
        .bind(&digest)
        .fetch_optional(&state.db).await {
        Ok(Some(a)) => Json(a).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => ApiError::internal("db").into_response(),
    }
}

/// Insert artifact event (best-effort)
async fn insert_event(conn: &mut PoolConnection<sqlx::Postgres>, artifact_id: Uuid, event_type: &str) -> anyhow::Result<()> {
    let _ = sqlx::query("INSERT INTO artifact_events (artifact_id, event_type) VALUES ($1,$2)")
        .bind(artifact_id)
        .bind(event_type)
        .execute(pg(conn)).await;
    ARTIFACT_EVENTS_TOTAL.inc();
    Ok(())
}

/// Enforce per-app quotas if configured
async fn enforce_quota(conn: &mut PoolConnection<sqlx::Postgres>, app_id: Uuid, incoming_size: i64) -> Result<(), ApiError> {
    let max_count = std::env::var("AETHER_MAX_ARTIFACTS_PER_APP").ok().and_then(|v| v.parse::<i64>().ok()).filter(|v| *v>0);
    let max_bytes = std::env::var("AETHER_MAX_TOTAL_BYTES_PER_APP").ok().and_then(|v| v.parse::<i64>().ok()).filter(|v| *v>0);
    if max_count.is_none() && max_bytes.is_none() { return Ok(()); }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE app_id=$1 AND status!='pending'")
        .bind(app_id).fetch_one(pg(conn)).await.unwrap_or(0);
    let used_bytes: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes),0) FROM artifacts WHERE app_id=$1 AND status!='pending'")
        .bind(app_id).fetch_one(pg(conn)).await.unwrap_or(0);
    if let Some(mc)=max_count { if count >= mc { QUOTA_EXCEEDED_TOTAL.inc(); return Err(ApiError::new(StatusCode::FORBIDDEN, "quota_exceeded", format!("artifact count quota {} reached", mc))); } }
    if let Some(mb)=max_bytes { if used_bytes + incoming_size > mb { QUOTA_EXCEEDED_TOTAL.inc(); return Err(ApiError::new(StatusCode::FORBIDDEN, "quota_exceeded", format!("size quota {} exceeded ({} + {})", mb, used_bytes, incoming_size))); } }
    Ok(())
}

/// Retention GC: keep only latest N per app if configured (order by created_at desc)
async fn retention_gc_if_needed(conn: &mut PoolConnection<sqlx::Postgres>, app_id: Option<Uuid>) -> anyhow::Result<()> {
    let Some(app) = app_id else { return Ok(()); };
    let retain = std::env::var("AETHER_RETAIN_LATEST_PER_APP").ok().and_then(|v| v.parse::<i64>().ok()).filter(|v| *v>0).unwrap_or(0);
    if retain == 0 { return Ok(()); }
    // Delete surplus (skip newest retain)
    let obsolete: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM artifacts WHERE app_id=$1 AND status='stored' ORDER BY created_at DESC, id DESC OFFSET $2")
        .bind(app)
        .bind(retain)
    .fetch_all(pg(conn)).await.unwrap_or_default();
    if !obsolete.is_empty() {
        for id in &obsolete { insert_event(conn, *id, "retention_delete").await.ok(); }
    let _ = sqlx::query("DELETE FROM artifacts WHERE id = ANY($1)").bind(&obsolete).execute(pg(conn)).await;
    }
    Ok(())
}

// ================= Multipart Upload Endpoints (S3 feature only for now) ==================

#[derive(serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct MultipartInitRequest { pub app_name: String, pub digest: String }
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MultipartInitResponse { pub upload_id: String, pub storage_key: String }

#[utoipa::path(
    post,
    path="/artifacts/multipart/init",
    request_body=MultipartInitRequest,
    responses(
        (status=200, body=MultipartInitResponse, description="Multipart upload initiated"),
        (status=400, body=crate::error::ApiErrorBody),
        (status=409, body=crate::error::ApiErrorBody, description="Already stored")
    ),
    tag="aether",
    summary="Initiate multipart artifact upload",
    description="Begins a multipart upload session and returns an upload_id used for part presigning."
)]
pub async fn multipart_init(State(state): State<AppState>, Json(req): Json<MultipartInitRequest>) -> impl IntoResponse {
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) { return ApiError::new(StatusCode::BAD_REQUEST, "invalid_digest", "digest must be 64 hex").into_response(); }
    let mut conn = match state.db.acquire().await { Ok(c)=>c, Err(_)=> return ApiError::internal("db").into_response() };
    let key = format!("artifacts/{}/{}/app.tar.gz", req.app_name, req.digest);
    // Ensure pending row exists (if already stored, shortcut)
    if let Ok(Some(status)) = sqlx::query_scalar::<_, String>("SELECT status FROM artifacts WHERE digest=$1")
    .bind(&req.digest).fetch_optional(pg(&mut conn)).await { if status=="stored" { return ApiError::new(StatusCode::CONFLICT, "already_stored", "artifact already stored").into_response(); } }
    let storage = get_storage().await;
    match storage.backend().init_multipart(&key, &req.digest).await {
        Ok(upload_id)=> {
            let _ = sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,signature,sbom_url,manifest_url,verified,storage_key,status,multipart_upload_id) VALUES (NULL,$1,0,NULL,NULL,NULL,FALSE,$2,'pending',$3) ON CONFLICT (digest) DO UPDATE SET multipart_upload_id=EXCLUDED.multipart_upload_id, storage_key=EXCLUDED.storage_key")
                .bind(&req.digest).bind(&key).bind(&upload_id).execute(pg(&mut conn)).await;
            MULTIPART_INITS_TOTAL.inc();
            (StatusCode::OK, Json(MultipartInitResponse { upload_id, storage_key: key })).into_response()
        }
        Err(_)=> ApiError::new(StatusCode::NOT_IMPLEMENTED, "multipart_unsupported", "multipart not supported by backend").into_response()
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct MultipartPresignPartRequest { pub digest: String, pub upload_id: String, pub part_number: i32 }
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MultipartPresignPartResponse { pub url: String, pub method: String, pub headers: std::collections::HashMap<String,String> }

#[utoipa::path(
    post,
    path="/artifacts/multipart/presign-part",
    request_body=MultipartPresignPartRequest,
    responses(
        (status=200, body=MultipartPresignPartResponse, description="Presigned URL for part"),
        (status=400, body=crate::error::ApiErrorBody)
    ),
    tag="aether",
    summary="Presign a multipart upload part",
    description="Returns a presigned PUT URL for a specific part number within an active multipart upload session."
)]
pub async fn multipart_presign_part(State(state): State<AppState>, Json(req): Json<MultipartPresignPartRequest>) -> impl IntoResponse {
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) { return ApiError::bad_request("invalid digest").into_response(); }
    if req.part_number <=0 { return ApiError::bad_request("part_number must be >0").into_response(); }
    let mut conn = match state.db.acquire().await { Ok(c)=>c, Err(_)=> return ApiError::internal("db").into_response() };
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>)>("SELECT status, storage_key, multipart_upload_id FROM artifacts WHERE digest=$1")
    .bind(&req.digest).fetch_optional(pg(&mut conn)).await.ok().flatten();
    let Some((status, sk_opt, upload_id_opt)) = row else { return ApiError::new(StatusCode::BAD_REQUEST, "unknown_digest", "digest not initialized").into_response(); };
    if status=="stored" { return ApiError::new(StatusCode::CONFLICT, "already_stored", "artifact already stored").into_response(); }
    let Some(storage_key) = sk_opt else { return ApiError::internal("missing storage_key").into_response(); };
    if upload_id_opt.as_deref()!=Some(&req.upload_id) { return ApiError::new(StatusCode::BAD_REQUEST, "upload_id_mismatch", "upload id mismatch").into_response(); }
    let storage = get_storage().await;
    match storage.backend().presign_multipart_part(&storage_key, &req.upload_id, req.part_number).await {
        Ok(p)=> { MULTIPART_PART_PRESIGNS_TOTAL.inc(); (StatusCode::OK, Json(MultipartPresignPartResponse { url: p.url, method: p.method, headers: p.headers })).into_response() },
        Err(_)=> ApiError::new(StatusCode::NOT_IMPLEMENTED, "multipart_unsupported", "multipart not supported by backend").into_response()
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct MultipartPartEtag { pub part_number: i32, pub etag: String }
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct MultipartCompleteRequest { pub app_name: String, pub digest: String, pub upload_id: String, pub size_bytes: i64, pub parts: Vec<MultipartPartEtag>, pub signature: Option<String>, pub idempotency_key: Option<String> }
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MultipartCompleteResponse { pub status: String, pub storage_key: String, pub digest: String }

#[utoipa::path(
    post,
    path="/artifacts/multipart/complete",
    request_body=MultipartCompleteRequest,
    responses(
        (status=200, body=MultipartCompleteResponse, description="Multipart upload completed"),
        (status=400, body=crate::error::ApiErrorBody),
        (status=403, body=crate::error::ApiErrorBody, description="Quota exceeded"),
        (status=409, body=crate::error::ApiErrorBody, description="Idempotency conflict"),
        (status=501, body=crate::error::ApiErrorBody, description="Backend does not support multipart")
    ),
    tag="aether",
    summary="Complete multipart artifact upload",
    description="Finalizes multipart upload (after all parts uploaded) and promotes the artifact to stored status with retention & quota enforcement."
)]
pub async fn multipart_complete(State(state): State<AppState>, Json(req): Json<MultipartCompleteRequest>) -> impl IntoResponse {
    if req.digest.len()!=64 || !req.digest.chars().all(|c| c.is_ascii_hexdigit()) { return ApiError::bad_request("invalid digest").into_response(); }
    if req.size_bytes < 0 { return ApiError::bad_request("size_bytes must be >=0").into_response(); }
    let mut conn = match state.db.acquire().await { Ok(c)=>c, Err(_)=> return ApiError::internal("db").into_response() };
    let row = sqlx::query_as::<_, (Uuid,String,Option<String>,Option<String>)>("SELECT id,status,storage_key,multipart_upload_id FROM artifacts WHERE digest=$1")
    .bind(&req.digest).fetch_optional(pg(&mut conn)).await.ok().flatten();
    let Some((id,status,sk_opt,upload_id_opt)) = row else { return ApiError::new(StatusCode::BAD_REQUEST, "unknown_digest", "digest not initialized").into_response(); };
    if status=="stored" { return (StatusCode::OK, Json(MultipartCompleteResponse { status: "stored".into(), storage_key: sk_opt.unwrap_or_default(), digest: req.digest })).into_response(); }
    let Some(storage_key) = sk_opt else { return ApiError::internal("missing storage_key").into_response(); };
    if upload_id_opt.as_deref()!=Some(&req.upload_id) { return ApiError::new(StatusCode::BAD_REQUEST, "upload_id_mismatch", "upload id mismatch").into_response(); }
    let storage = get_storage().await;
    if storage.backend().complete_multipart(&storage_key, &req.upload_id, req.parts.iter().map(|p| (p.part_number, p.etag.clone())).collect()).await.is_err() { MULTIPART_COMPLETE_FAILURES_TOTAL.inc(); return ApiError::new(StatusCode::NOT_IMPLEMENTED, "multipart_unsupported", "multipart not supported by backend").into_response(); }
    // Observe parts metrics (counts + (approx) size per part if size_bytes set). We approximate uniform part size except possibly last part.
    if !req.parts.is_empty() {
        MULTIPART_PARTS_PER_ARTIFACT.observe(req.parts.len() as f64);
        if req.size_bytes > 0 {
            let full_parts = req.parts.len();
            let avg_part = (req.size_bytes / full_parts as i64).max(1);
            for (idx, _p) in req.parts.iter().enumerate() {
                let size_est = if idx == full_parts - 1 { // last part may have remainder
                    let consumed = avg_part * (full_parts as i64 - 1);
                    (req.size_bytes - consumed).max(0)
                } else { avg_part };
                MULTIPART_PART_SIZE_HIST.observe(size_est as f64);
            }
        }
    }
    // finalize DB row similar to complete_artifact pending branch
    let app_id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM applications WHERE name=$1")
    .bind(&req.app_name).fetch_optional(pg(&mut conn)).await.ok().flatten();
    if let Some(app_uuid)=app_id { if let Err(resp)=enforce_quota(&mut conn, app_uuid, req.size_bytes).await { return resp.into_response(); } }
    let upd = sqlx::query("UPDATE artifacts SET app_id=$1,size_bytes=$2, signature=$3, verified=FALSE, status='stored', completed_at=NOW(), idempotency_key=COALESCE(idempotency_key,$5) WHERE id=$4 RETURNING id")
        .bind(app_id)
        .bind(req.size_bytes)
        .bind(req.signature.as_ref())
        .bind(id)
        .bind(&req.idempotency_key)
    .fetch_one(pg(&mut conn)).await;
    match upd { Ok(_)=> { MULTIPART_COMPLETES_TOTAL.inc(); insert_event(&mut conn, id, "stored").await.ok(); retention_gc_if_needed(&mut conn, app_id).await.ok(); (StatusCode::OK, Json(MultipartCompleteResponse { status: "stored".into(), storage_key, digest: req.digest })).into_response() }, Err(e)=> { MULTIPART_COMPLETE_FAILURES_TOTAL.inc(); error!(?e, "multipart_complete_update_failed"); ApiError::internal("db update").into_response() } }
}

#[utoipa::path(
    get,
    path = "/artifacts",
    responses( (status = 200, description = "List artifacts", body = [Artifact]) ),
    tag = "aether"
)]
pub async fn list_artifacts(State(state): State<AppState>) -> impl IntoResponse {
    // Select columns in the exact order of the Artifact struct definition.
    let rows = sqlx::query_as::<_, Artifact>("SELECT id, app_id, digest, size_bytes, signature, sbom_url, manifest_url, verified, storage_key, status, created_at, completed_at, idempotency_key, multipart_upload_id, provenance_present, manifest_digest, sbom_manifest_digest, sbom_validated FROM artifacts ORDER BY created_at DESC, id DESC LIMIT 200")
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
