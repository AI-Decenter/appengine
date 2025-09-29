use control_plane::{build_router, AppState};
use sqlx::Row;
use axum::{body::Body, http::{Request, StatusCode}};
use tower::util::ServiceExt;

#[tokio::test]
async fn mock_kube_deployment_status_transition() {
    // This test requires --features mock-kube
    let pool = control_plane::test_support::test_pool().await;
    sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
    sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
    sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("mockapp").execute(&pool).await.unwrap();
    let app = build_router(AppState { db: pool.clone() });
    let body = serde_json::json!({"app_name":"mockapp","artifact_url":"file://artifact"}).to_string();
    let req = Request::builder().method("POST").uri("/deployments")
        .header("content-type","application/json")
        .body(Body::from(body)).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    // PATCH to simulate rollout
    let dep_id: uuid::Uuid = {
        let row = sqlx::query("SELECT id FROM deployments WHERE app_id = (SELECT id FROM applications WHERE name = $1) LIMIT 1")
            .bind("mockapp").fetch_one(&pool).await.unwrap();
        row.get("id")
    };
    let patch_body = serde_json::json!({"digest":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}).to_string();
    let patch_req = Request::builder().method("PATCH").uri(&format!("/deployments/{}", dep_id))
        .header("content-type","application/json")
        .body(Body::from(patch_body)).unwrap();
    let patch_res = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(patch_res.status(), StatusCode::OK);
}
