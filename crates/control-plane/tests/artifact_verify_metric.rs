use control_plane::{build_router, AppState};
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt; // oneshot
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

// This test ensures a failing signature verification path increments the
// artifact_verify_failure_total{app="..",reason="verify_failed"} counter.
// It crafts an application with one active public key, an existing stored
// artifact (so digest resolution succeeds), then sends a deployment create
// request with an invalid signature (128 hex chars that won't verify).
#[tokio::test]
#[serial_test::serial]
async fn artifact_verify_failure_metric_increments() {
    let pool = control_plane::test_support::test_pool().await;
    // Clean relevant tables for isolation
    for tbl in ["deployments","artifacts","public_keys","applications"].iter() {
        let _ = sqlx::query(&format!("DELETE FROM {}", tbl)).execute(&pool).await;
    }
    // Seed application
    sqlx::query("INSERT INTO applications (name) VALUES ($1)")
        .bind("sigfail")
        .execute(&pool).await.unwrap();
    // Generate and register a random ed25519 public key (active)
    let sk = SigningKey::generate(&mut OsRng);
    let pk_hex = hex::encode(sk.verifying_key().to_bytes());
    sqlx::query("INSERT INTO public_keys (app_id, public_key_hex, active) SELECT id,$1,TRUE FROM applications WHERE name=$2")
        .bind(&pk_hex)
        .bind("sigfail")
        .execute(&pool).await.unwrap();
    // Insert stored artifact row so digest resolution works
    let digest = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"; // 64 hex
    sqlx::query("INSERT INTO artifacts (app_id,digest,size_bytes,status) SELECT id,$1,0,'stored' FROM applications WHERE name=$2")
        .bind(digest)
        .bind("sigfail")
        .execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    // Craft invalid signature (128 hex chars) that will not verify under the key
    let invalid_sig = "aa".repeat(64); // 128 'a' hex chars => 64 bytes 0xaa
    let body = serde_json::json!({
        "app_name":"sigfail",
        "artifact_url": format!("file://{digest}"),
        "signature": invalid_sig
    }).to_string();
    let req = Request::builder().method("POST").uri("/deployments")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "expected 400 on signature verify failure");
    // Fetch metrics and assert counter incremented for our label set
    let metrics_req = Request::builder().method("GET").uri("/metrics")
        .body(Body::empty()).unwrap();
    let metrics_resp = app.clone().oneshot(metrics_req).await.unwrap();
    assert_eq!(metrics_resp.status(), StatusCode::OK, "metrics endpoint should be 200");
    let body_bytes = axum::body::to_bytes(metrics_resp.into_body(), 64 * 1024).await.unwrap();
    let metrics_text = String::from_utf8(body_bytes.to_vec()).unwrap();
    // Find line containing metric (Prometheus exposition: name{labels} value)
    let line = metrics_text.lines().find(|l| l.contains("artifact_verify_failure_total") && l.contains("app=\"sigfail\"") && l.contains("reason=\"verify_failed\""))
        .expect("artifact_verify_failure_total line with expected labels missing");
    // Parse numeric value (last whitespace-separated token)
    let val: f64 = line.split_whitespace().last().unwrap().parse().unwrap();
    assert!(val >= 1.0, "expected counter >= 1, got {val} (line: {line})");
}
