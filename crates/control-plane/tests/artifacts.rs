use control_plane::{build_router, AppState};
use reqwest::Url;
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use serde_json::json;

async fn setup() -> control_plane::AppState {
    // Allow tests to run without externally provided DATABASE_URL by falling back to a conventional local instance.
    // Default credentials match common local dev setups: postgres:postgres@localhost:5432
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/aether_test".into());
    ensure_database(&url).await;
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.expect("connect test db");
    sqlx::migrate!().run(&pool).await.expect("run migrations");
    // Clean tables (order matters if FKs appear later)
    let _ = sqlx::query("DELETE FROM artifacts").execute(&pool).await;
    let _ = sqlx::query("DELETE FROM applications").execute(&pool).await;
    AppState { db: pool }
}

/// Ensure the target database exists; if missing, create it using a temporary admin connection to the 'postgres' db.
async fn ensure_database(url: &str) {
    // Parse database name from URL path component.
    let parsed = match Url::parse(url) { Ok(u)=>u, Err(_)=> return }; // If parse fails, bail; subsequent connect will error clearly.
    let db_name = parsed.path().trim_start_matches('/').to_string();
    if db_name.is_empty() { return; }
    let mut admin = parsed.clone();
    admin.set_path("/postgres");
    if let Ok(admin_pool) = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(admin.as_str()).await {
        // Check existence
        let exists: Option<String> = sqlx::query_scalar("SELECT datname FROM pg_database WHERE datname = $1")
            .bind(&db_name)
            .fetch_optional(&admin_pool)
            .await
            .ok()
            .flatten();
        if exists.is_none() {
            // Unsafe identifier interpolation avoided by simple character whitelist check.
            if db_name.chars().all(|c| c.is_ascii_alphanumeric() || c=='_' ) {
                let create_sql = format!("CREATE DATABASE {}", db_name);
                let _ = sqlx::query(&create_sql).execute(&admin_pool).await; // ignore race / errors
            }
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn artifact_meta_flow() {
    let state = setup().await; let app = build_router(state.clone());
    // 404 first
    let digest = format!("{:064x}", 1);
    let res = app.clone().oneshot(Request::builder().method("GET").uri(format!("/artifacts/{}/meta", digest)).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    // presign then complete
    let presign_body = json!({"app_name":"metaapp","digest": digest}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign_body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let complete_body = json!({"app_name":"metaapp","digest": digest, "size_bytes":0, "signature": null}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(complete_body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let res = app.clone().oneshot(Request::builder().method("GET").uri(format!("/artifacts/{}/meta", digest)).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial]
async fn idempotency_conflict() {
    let state = setup().await; let app = build_router(state.clone());
    let digest1 = format!("{:064x}", 2); let digest2 = format!("{:064x}", 3);
    // first complete with key
    let presign1 = json!({"app_name":"idapp","digest": digest1}).to_string();
    app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign1)).unwrap()).await.unwrap();
    let comp1 = json!({"app_name":"idapp","digest": digest1, "size_bytes":0, "signature":null, "idempotency_key":"idem-test"}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(comp1)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // second different digest same key -> conflict
    let presign2 = json!({"app_name":"idapp","digest": digest2}).to_string();
    app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign2)).unwrap()).await.unwrap();
    let comp2 = json!({"app_name":"idapp","digest": digest2, "size_bytes":0, "signature":null, "idempotency_key":"idem-test"}).to_string();
    let res = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(comp2)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial_test::serial]
async fn quota_exceeded() {
    std::env::set_var("AETHER_MAX_ARTIFACTS_PER_APP", "1");
    let state = setup().await; let app = build_router(state.clone());
    // insert application
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("qapp").execute(&state.db).await.unwrap();
    let d1 = format!("{:064x}", 4); let d2 = format!("{:064x}", 5);
    // Presign and complete first artifact
    let presign1 = json!({"app_name":"qapp","digest": d1}).to_string();
    app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign1)).unwrap()).await.unwrap();
    let comp1 = json!({"app_name":"qapp","digest": d1, "size_bytes":0, "signature":null}).to_string();
    let r1 = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(comp1)).unwrap()).await.unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    // (debug removed)
    // Presign second after first is stored so count=1 triggers quota
    let presign2 = json!({"app_name":"qapp","digest": d2}).to_string();
    app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign2)).unwrap()).await.unwrap();
    let comp2 = json!({"app_name":"qapp","digest": d2, "size_bytes":0, "signature":null}).to_string();
    let r2 = app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(comp2)).unwrap()).await.unwrap();
    assert_eq!(StatusCode::FORBIDDEN, r2.status());
    std::env::remove_var("AETHER_MAX_ARTIFACTS_PER_APP");
}

#[tokio::test]
#[serial_test::serial]
async fn retention_keeps_latest_only() {
    std::env::set_var("AETHER_RETAIN_LATEST_PER_APP", "1");
    let state = setup().await; let app = build_router(state.clone());
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("rapp").execute(&state.db).await.unwrap();
    let d1 = format!("{:064x}", 10); let d2 = format!("{:064x}", 11);
    for d in [d1.clone(), d2.clone()] { let presign = json!({"app_name":"rapp","digest": d}).to_string(); app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/presign").header("content-type","application/json").body(Body::from(presign)).unwrap()).await.unwrap(); let comp = json!({"app_name":"rapp","digest": d, "size_bytes":0, "signature":null}).to_string(); app.clone().oneshot(Request::builder().method("POST").uri("/artifacts/complete").header("content-type","application/json").body(Body::from(comp)).unwrap()).await.unwrap(); }
    let list = app.clone().oneshot(Request::builder().method("GET").uri("/artifacts").body(Body::empty()).unwrap()).await.unwrap();
    let body = axum::body::to_bytes(list.into_body(), 1024 * 1024).await.unwrap();
    let arr: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // only second digest should remain for rapp
    let mut seen_latest = false; let mut old_present = false;
    if let Some(a) = arr.as_array() {
        for item in a {
            if item.get("digest").and_then(|v| v.as_str()) == Some(&d2) {
                seen_latest = true;
            }
            if item.get("digest").and_then(|v| v.as_str()) == Some(&d1) {
                old_present = true;
            }
        }
    }
    assert!(seen_latest); assert!(!old_present, "old artifact should have been GC'd");
    std::env::remove_var("AETHER_RETAIN_LATEST_PER_APP");
}

#[tokio::test]
#[serial_test::serial]
async fn legacy_upload_deprecation_header() {
    let state = setup().await; let app = build_router(state.clone());
    // Minimal valid legacy multipart upload
    let boundary = "XBOUNDARY";
    let data = b"hello";
    use sha2::{Sha256, Digest}; let mut h=Sha256::new(); h.update(data); let digest = format!("{:x}", h.finalize());
    let body_str = format!("--{b}\r\nContent-Disposition: form-data; name=\"app_name\"\r\n\r\nlegacyapp\r\n--{b}\r\nContent-Disposition: form-data; name=\"artifact\"; filename=\"a.tgz\"\r\nContent-Type: application/gzip\r\n\r\nhello\r\n--{b}--\r\n", b=boundary);
    let req = Request::builder().method("POST").uri("/artifacts")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .header("X-Aether-Artifact-Digest", digest)
        .body(Body::from(body_str))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("X-Aether-Deprecated").is_some(), "missing deprecation header");
}