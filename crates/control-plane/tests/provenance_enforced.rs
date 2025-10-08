use control_plane::{build_router, AppState};
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;

// Minimal test: when provenance enforcement enabled but artifact digest unresolved (no artifact), deployment still created (enforcement only applies when digest known)
#[tokio::test]
#[serial_test::serial]
async fn deployment_without_artifact_digest_does_not_block() {
    std::env::set_var("AETHER_REQUIRE_PROVENANCE", "1");
    let pool = control_plane::test_support::test_pool().await;
    // insert application
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("app-prov").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool });
    let body = serde_json::json!({"app_name":"app-prov","artifact_url":"file://no-digest-here"}).to_string();
    let req = Request::builder().method("POST").uri("/deployments").header("content-type","application/json").body(Body::from(body)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}
