#![cfg(feature = "s3")]
use axum::{body::Body, http::Request};
use control_plane::{build_router, AppState, db::init_db};
use tower::util::ServiceExt;
use sha2::{Sha256, Digest};

async fn pool() -> sqlx::Pool<sqlx::Postgres> {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests");
    let pool = init_db(&url).await.expect("db init");
    sqlx::migrate!().run(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
#[serial_test::serial]
async fn s3_presign_complete_with_remote_hash() {
    if std::env::var("MINIO_TEST").ok().as_deref() != Some("1") { return; } // skip silently
    if std::env::var("AETHER_STORAGE_MODE").unwrap_or_default().to_lowercase() != "s3" { eprintln!("skipping: AETHER_STORAGE_MODE != s3"); return; }
    // Enable remote hash verification for small object (data length < threshold)
    std::env::set_var("AETHER_VERIFY_REMOTE_HASH", "true");
    std::env::set_var("AETHER_REMOTE_HASH_MAX_BYTES", "1048576"); // 1MB
    assert_eq!(std::env::var("AETHER_STORAGE_MODE").unwrap_or_default().to_lowercase(), "s3");
    let pool = pool().await;
    let app = build_router(AppState { db: pool.clone() });
    sqlx::query("INSERT INTO applications (name) VALUES ($1) ON CONFLICT DO NOTHING").bind("s3app-hash").execute(&pool).await.ok();
    let data = b"remote-hash-test".to_vec();
    let mut h = Sha256::new(); h.update(&data); let digest = format!("{:x}", h.finalize());
    // Presign
    let body = serde_json::json!({"app_name":"s3app-hash","digest":digest}).to_string();
    let req = Request::builder().method("POST").uri("/artifacts/presign")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "presign status {}", resp.status());
    let body_bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let upload_url = v["upload_url"].as_str().unwrap();
    // PUT upload
    let client = reqwest::Client::new();
    let mut put_req = client.put(upload_url);
    if let Some(hdrs) = v["headers"].as_object() { for (k,val) in hdrs { if let Some(s)=val.as_str() { put_req = put_req.header(k, s); } } }
    let put_resp = put_req.body(data.clone()).send().await.unwrap();
    assert!(put_resp.status().is_success(), "put status {}", put_resp.status());
    // Complete (should trigger remote hash verification)
    let comp_body = serde_json::json!({"app_name":"s3app-hash","digest":digest,"size_bytes":data.len() as i64,"signature":null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/complete")
        .header("content-type","application/json")
        .body(Body::from(comp_body)).unwrap();
    let comp_resp = app.clone().oneshot(comp_req).await.unwrap();
    assert!(comp_resp.status().is_success(), "complete status {}", comp_resp.status());
}