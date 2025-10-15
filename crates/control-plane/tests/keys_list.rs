use control_plane::{build_router, AppState};
use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;

#[tokio::test]
#[serial_test::serial]
async fn keys_endpoint_empty_ok() {
    let pool = control_plane::test_support::test_pool().await;
    let app = build_router(AppState { db: pool });
    let req = Request::builder().uri("/provenance/keys").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.as_array().unwrap().is_empty());
}
