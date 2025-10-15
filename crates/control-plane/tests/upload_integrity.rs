use axum::{body::Body, http::{Request, StatusCode}, middleware};
use control_plane::{build_router, AppState};
use tower::util::ServiceExt; // for oneshot
use sha2::{Sha256, Digest};
use ed25519_dalek::{SigningKey, Signature, Signer};
use once_cell::sync::OnceCell;
use sqlx::Connection; // for PgConnection::connect
use tokio::sync::OnceCell as AsyncOnceCell;

// Perform schema validation once per test process using a direct connection to avoid
// consuming connections from the shared pool and triggering PoolTimedOut.
static SCHEMA_OK: AsyncOnceCell<()> = AsyncOnceCell::const_new();

fn init_tracing() {
    static INIT: OnceCell<()> = OnceCell::new();
    let _ = INIT.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_target(false)
            .try_init();
    });
}


// Prefer a per-test pool for this suite to avoid cross-binary pool contention.
async fn pool() -> sqlx::Pool<sqlx::Postgres> {
    init_tracing();
    // Use a dedicated pool for this test binary
    std::env::set_var("AETHER_TEST_SHARED_POOL", "0");
    // Give this suite a slightly larger pool to absorb sequential requests
    if std::env::var("AETHER_TEST_MAX_CONNS").is_err() {
        std::env::set_var("AETHER_TEST_MAX_CONNS", "24");
    }
    control_plane::test_support::test_pool().await
}

async fn ensure_schema_once(pool: &sqlx::Pool<sqlx::Postgres>) {
    if SCHEMA_OK.get().is_some() { return; }
    SCHEMA_OK.get_or_init(|| async {
        // Prefer direct PgConnection via DATABASE_URL to avoid using pool connections.
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if !url.is_empty() {
                if let Ok(mut conn) = sqlx::postgres::PgConnection::connect(&url).await {
                    let required = ["applications", "artifacts", "public_keys", "deployments"];
                    for table in required {
                        let exists: bool = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name=$1)"
                        ).bind(table).fetch_one(&mut conn).await.expect("schema table exists check");
                        assert!(exists, "required table '{}' missing (run migrations)", table);
                    }
                    let cols: Vec<String> = sqlx::query_scalar(
                        "SELECT column_name FROM information_schema.columns WHERE table_name='artifacts' ORDER BY ordinal_position"
                    ).fetch_all(&mut conn).await.expect("fetch artifacts columns");
                    for e in ["id","app_id","digest","size_bytes","signature","sbom_url","manifest_url","verified","storage_key","status","created_at"] {
                        assert!(cols.contains(&e.to_string()), "artifacts column '{}' missing", e);
                    }
                    return;
                }
            }
        }
        // Fallback path: use pool to acquire a single connection for checks
        let mut conn = pool.acquire().await.expect("db acquire for schema checks (fallback)");
        let required = ["applications", "artifacts", "public_keys", "deployments"];
        for table in required {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name=$1)"
            ).bind(table).fetch_one(&mut *conn).await.expect("schema table exists check");
            assert!(exists, "required table '{}' missing (run migrations)", table);
        }
        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns WHERE table_name='artifacts' ORDER BY ordinal_position"
        ).fetch_all(&mut *conn).await.expect("fetch artifacts columns");
        for e in ["id","app_id","digest","size_bytes","signature","sbom_url","manifest_url","verified","storage_key","status","created_at"] {
            assert!(cols.contains(&e.to_string()), "artifacts column '{}' missing", e);
        }
    }).await;
}

fn multipart_body(fields: Vec<(&str, &str)>, file: Option<(&str, Vec<u8>)>) -> (Vec<u8>, String) {
    let boundary = format!("----BOUNDARYTEST{}", uuid::Uuid::new_v4());
    let mut body: Vec<u8> = Vec::new();
    for (name, val) in fields {
        body.extend(format!("--{}\r\n", boundary).as_bytes());
        body.extend(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", name, val).as_bytes());
    }
    if let Some((name, bytes)) = file {
        body.extend(format!("--{}\r\n", boundary).as_bytes());
        body.extend(format!("Content-Disposition: form-data; name=\"{}\"; filename=\"f.tar.gz\"\r\nContent-Type: application/gzip\r\n\r\n", name).as_bytes());
        body.extend(&bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{}--\r\n", boundary).as_bytes());
    (body, boundary)
}

#[tokio::test]
#[serial_test::serial]
async fn upload_missing_digest() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    let app = build_router(AppState { db: pool });
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", b"data".to_vec())));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial_test::serial]
async fn upload_digest_mismatch() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    let app = build_router(AppState { db: pool });
    let data = b"abcdef".to_vec();
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    // compute wrong digest
    let mut h = Sha256::new(); h.update(b"zzz"); let wrong = format!("{:x}", h.finalize());
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", wrong)
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial_test::serial]
async fn upload_ok_and_duplicate() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    // Clean artifacts
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    let app = build_router(AppState { db: pool });
    let data = b"hello-world-data".to_vec();
    let mut h = Sha256::new(); h.update(&data); let dgst = format!("{:x}", h.finalize());
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dgst.clone())
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["digest"].as_str().unwrap(), dgst);
    assert_eq!(v["duplicate"], false);
    // Duplicate
    let (artifact_bytes2, boundary2) = multipart_body(vec![("app_name","demo")], Some(("artifact", data.clone())));
    let req2 = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary2))
        .header("x-aether-artifact-digest", dgst.clone())
        .body(Body::from(artifact_bytes2)).unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024).await.unwrap();
    let v2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert!(v2["duplicate"].as_bool().unwrap());
}

#[tokio::test]
#[serial_test::serial]
async fn upload_with_verification_true() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("verifapp").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    // Generate key
    let sk = SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key();
    let pk_hex = hex::encode(pk.to_bytes());
    // Register key
    let body = serde_json::json!({"public_key_hex": pk_hex}).to_string();
    let req_reg = Request::builder().method("POST").uri("/apps/verifapp/public-keys")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp_reg = app.clone().oneshot(req_reg).await.unwrap();
    assert_eq!(resp_reg.status(), StatusCode::CREATED);
    // Prepare artifact
    let data = b"verifiable-bytes".to_vec();
    let mut h = Sha256::new(); h.update(&data); let dgst = format!("{:x}", h.finalize());
    let sig: Signature = sk.sign(dgst.as_bytes());
    let sig_hex = hex::encode(sig.to_bytes());
    let (artifact_bytes, boundary) = multipart_body(vec![("app_name","verifapp")], Some(("artifact", data)));
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dgst.clone())
        .header("x-aether-signature", sig_hex)
        .body(Body::from(artifact_bytes)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(v["verified"], true);
    // HEAD existence
    let head_req = Request::builder().method("HEAD").uri(format!("/artifacts/{}", dgst)).body(Body::empty()).unwrap();
    let head_resp = app.clone().oneshot(head_req).await.unwrap();
    if head_resp.status()!=StatusCode::OK {
        let st: Option<String> = sqlx::query_scalar("SELECT status FROM artifacts WHERE digest=$1").bind(&dgst).fetch_optional(&pool).await.unwrap();
        panic!("expected HEAD 200, got {:?}, db status={:?}", head_resp.status(), st);
    }
}

#[tokio::test]
#[serial_test::serial]
async fn presign_complete_idempotent() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("presignapp").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    // Presign
    let body = serde_json::json!({"app_name":"presignapp","digest":digest}).to_string();
    let req = Request::builder().method("POST").uri("/artifacts/presign")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let presign_body = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&presign_body).unwrap();
    assert_eq!(v["method"], "PUT");
    let storage_key = v["storage_key"].as_str().unwrap().to_string();
    assert!(storage_key.contains(&digest));
    // Complete first time
    let complete_req = serde_json::json!({"app_name":"presignapp","digest":digest,"size_bytes":1234,"signature":null}).to_string();
    let comp = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(complete_req)).unwrap();
    let comp_resp = app.clone().oneshot(comp).await.unwrap();
    assert_eq!(comp_resp.status(), StatusCode::OK);
    let comp_body = axum::body::to_bytes(comp_resp.into_body(), 2048).await.unwrap();
    let cv: serde_json::Value = serde_json::from_slice(&comp_body).unwrap();
    assert_eq!(cv["duplicate"], false);
    assert_eq!(cv["status"], "stored");
    // Complete second time (idempotent)
    let comp2_req = serde_json::json!({"app_name":"presignapp","digest":digest,"size_bytes":1234,"signature":null}).to_string();
    let comp2 = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(comp2_req)).unwrap();
    let comp2_resp = app.clone().oneshot(comp2).await.unwrap();
    assert_eq!(comp2_resp.status(), StatusCode::OK);
    let comp2_body = axum::body::to_bytes(comp2_resp.into_body(), 2048).await.unwrap();
    let cv2: serde_json::Value = serde_json::from_slice(&comp2_body).unwrap();
    assert_eq!(cv2["duplicate"], true);
}

#[tokio::test]
#[serial_test::serial]
async fn upload_unauthorized() {
    // Preserve old value
    let prev = std::env::var("AETHER_API_TOKENS").ok();
    std::env::set_var("AETHER_API_TOKENS", "tok1,tok2");
    let pool = pool().await; ensure_schema_once(&pool).await;
    // Build secured router (replicating auth logic from main)
    let tokens: Vec<String> = std::env::var("AETHER_API_TOKENS").unwrap().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let secured = build_router(AppState { db: pool })
        .layer(middleware::from_fn(move |req: Request<Body>, next: axum::middleware::Next| {
            let tokens = tokens.clone();
            async move {
                let path = req.uri().path();
                let exempt = matches!(path, "/health"|"/readyz"|"/startupz"|"/metrics"|"/openapi.json"|"/swagger");
                if !exempt && !tokens.is_empty() {
                    let provided = req.headers().get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
                    let ok = tokens.iter().any(|t| provided == format!("Bearer {t}"));
                    if !ok { return axum::response::Response::builder().status(401).body(Body::from("unauthorized")).unwrap(); }
                }
                next.run(req).await
            }
        }));
    let (body_bytes, boundary) = multipart_body(vec![("app_name","demo")], Some(("artifact", b"abc".to_vec())));
    let mut h = Sha256::new(); h.update(b"abc"); let dg = format!("{:x}", h.finalize());
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={}", boundary))
        .header("x-aether-artifact-digest", dg)
        .body(Body::from(body_bytes)).unwrap();
    let resp = secured.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    // restore env
    if let Some(v)=prev { std::env::set_var("AETHER_API_TOKENS", v); } else { std::env::remove_var("AETHER_API_TOKENS"); }
}

#[tokio::test]
#[serial_test::serial]
async fn presign_creates_pending_and_head_not_found_until_complete() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("pendingapp").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    let digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    // Presign
    let body = serde_json::json!({"app_name":"pendingapp","digest":digest}).to_string();
    let req = Request::builder().method("POST").uri("/artifacts/presign")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // HEAD should be 404 (not stored yet)
    let head_req = Request::builder().method("HEAD").uri(format!("/artifacts/{}", digest)).body(Body::empty()).unwrap();
    let head_resp = app.clone().oneshot(head_req).await.unwrap();
    assert_eq!(head_resp.status(), StatusCode::NOT_FOUND);
    // Row status should be pending
    {
        let mut conn = pool.acquire().await.expect("db acquire for test verify");
        let status: String = sqlx::query_scalar("SELECT status FROM artifacts WHERE digest=$1").bind(&digest).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(status, "pending");
    } // drop conn before complete to free pool slot
    // Complete
    let comp_body = serde_json::json!({"app_name":"pendingapp","digest":digest,"size_bytes":42,"signature":null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(comp_body)).unwrap();
    let comp_resp = app.clone().oneshot(comp_req).await.unwrap();
    assert_eq!(comp_resp.status(), StatusCode::OK);
    let mut conn = pool.acquire().await.expect("db acquire for test verify 2");
    let new_status: String = sqlx::query_scalar("SELECT status FROM artifacts WHERE digest=$1").bind(&digest).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(new_status, "stored");
    // HEAD now OK
    let head2 = Request::builder().method("HEAD").uri(format!("/artifacts/{}", digest)).body(Body::empty()).unwrap();
    let head2_resp = app.clone().oneshot(head2).await.unwrap();
    assert_eq!(head2_resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial]
async fn complete_requires_presign_when_flag_enabled() {
    // Enable enforcement
    let prev = std::env::var("AETHER_REQUIRE_PRESIGN").ok();
    std::env::set_var("AETHER_REQUIRE_PRESIGN", "1");
    let pool = pool().await; ensure_schema_once(&pool).await;
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    let app = build_router(AppState { db: pool.clone() });
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    // Attempt complete without presign
    let body = serde_json::json!({"app_name":"enforceapp","digest":digest,"size_bytes":10,"signature":null}).to_string();
    let req = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["code"], "presign_required");
    // Now presign then complete
    let presign_body = serde_json::json!({"app_name":"enforceapp","digest":digest}).to_string();
    let preq = Request::builder().method("POST").uri("/artifacts/presign")
        .header("content-type","application/json")
        .body(Body::from(presign_body)).unwrap();
    let presign_resp = app.clone().oneshot(preq).await.unwrap();
    assert_eq!(presign_resp.status(), StatusCode::OK);
    let comp_body = serde_json::json!({"app_name":"enforceapp","digest":digest,"size_bytes":10,"signature":null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(comp_body)).unwrap();
    let comp_resp = app.clone().oneshot(comp_req).await.unwrap();
    assert_eq!(comp_resp.status(), StatusCode::OK);
    // restore env
    if let Some(v)=prev { std::env::set_var("AETHER_REQUIRE_PRESIGN", v); } else { std::env::remove_var("AETHER_REQUIRE_PRESIGN"); }
}

#[tokio::test]
#[serial_test::serial]
async fn pending_gc_deletes_old_rows() {
    let pool = pool().await; ensure_schema_once(&pool).await;
    sqlx::query("DELETE FROM artifacts").execute(&pool).await.ok();
    // Insert artificially old pending row
    let digest = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let key = "artifacts/test/app.tar.gz";
    let mut conn = pool.acquire().await.expect("db acquire for gc test");
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,signature,sbom_url,manifest_url,verified,storage_key,status,created_at) VALUES (NULL,$1,0,NULL,NULL,NULL,FALSE,$2,'pending', NOW() - INTERVAL '7200 seconds')")
        .bind(digest)
        .bind(key)
        .execute(&mut *conn).await.unwrap();
    // Force adjust created_at in case default timing interferes
    sqlx::query("UPDATE artifacts SET created_at = NOW() - INTERVAL '7200 seconds' WHERE digest=$1")
        .bind(digest)
        .execute(&mut *conn).await.ok();
    let deleted = control_plane::handlers::uploads::run_pending_gc(&pool, 3600).await.unwrap();
    assert_eq!(deleted, 1, "expected one pending row to be deleted");
    let remain: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE status='pending'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(remain, 0);
}
