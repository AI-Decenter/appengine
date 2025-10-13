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
async fn s3_multipart_flow() {
    if std::env::var("MINIO_TEST").ok().as_deref() != Some("1") { return; } // skip silently unless integration env present
    if std::env::var("AETHER_STORAGE_MODE").unwrap_or_default().to_lowercase() != "s3" { eprintln!("skipping: AETHER_STORAGE_MODE != s3"); return; }
    let pool = pool().await;
    let app = build_router(AppState { db: pool.clone() });
    sqlx::query("INSERT INTO applications (name) VALUES ($1) ON CONFLICT DO NOTHING").bind("mpapp").execute(&pool).await.ok();
    // Build a payload > part threshold (assume env part size default 8MiB -> we just produce 10MiB)
    let size: usize = 10 * 1024 * 1024;
    let data = vec![42u8; size];
    let mut h = Sha256::new(); h.update(&data); let digest = format!("{:x}", h.finalize());
    // init
    let init_body = serde_json::json!({"app_name":"mpapp","digest":digest}).to_string();
    let init_req = Request::builder().method("POST").uri("/artifacts/multipart/init").header("content-type","application/json").body(Body::from(init_body)).unwrap();
    let init_resp = app.clone().oneshot(init_req).await.unwrap();
    assert!(init_resp.status().is_success(), "init status {}", init_resp.status());
    let init_bytes = axum::body::to_bytes(init_resp.into_body(), 4096).await.unwrap();
    let init_json: serde_json::Value = serde_json::from_slice(&init_bytes).unwrap();
    let upload_id = init_json["upload_id"].as_str().unwrap();
    let storage_key = init_json["storage_key"].as_str().unwrap();
    assert!(upload_id.len()>3 && storage_key.contains(&digest));
    // slice into two parts (first 8MiB, second remainder)
    let part1 = &data[..8*1024*1024];
    let part2 = &data[8*1024*1024..];
    let mut parts_meta: Vec<serde_json::Value> = Vec::new();
    for (idx, slice) in [part1, part2].into_iter().enumerate() {
        let presign_body = serde_json::json!({"digest":digest, "upload_id":upload_id, "part_number": (idx as i32)+1}).to_string();
        let req = Request::builder().method("POST").uri("/artifacts/multipart/presign-part").header("content-type","application/json").body(Body::from(presign_body)).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_success(), "presign-part status {}", resp.status());
        let body_bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let url = v["url"].as_str().unwrap();
        let client = reqwest::Client::new();
        let mut put_req = client.put(url);
        if let Some(hdrs) = v["headers"].as_object() { for (k,val) in hdrs { if let Some(s)=val.as_str() { put_req = put_req.header(k, s); } } }
        let put_resp = put_req.body(slice.to_vec()).send().await.unwrap();
        assert!(put_resp.status().is_success(), "put status {}", put_resp.status());
        let etag = put_resp.headers().get("ETag").and_then(|h| h.to_str().ok()).unwrap_or("").trim_matches('"').to_string();
        assert!(!etag.is_empty());
        parts_meta.push(serde_json::json!({"part_number": (idx as i32)+1, "etag": etag}));
    }
    // complete
    let complete_body = serde_json::json!({"app_name":"mpapp","digest":digest,"upload_id":upload_id,"size_bytes": size as i64, "parts": parts_meta, "signature": null}).to_string();
    let comp_req = Request::builder().method("POST").uri("/artifacts/multipart/complete").header("content-type","application/json").body(Body::from(complete_body)).unwrap();
    let comp_resp = app.clone().oneshot(comp_req).await.unwrap();
    assert!(comp_resp.status().is_success(), "complete status {}", comp_resp.status());
}
