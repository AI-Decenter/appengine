use axum::{http::{Request, StatusCode}, body::Body};
use tower::util::ServiceExt;
use sqlx::PgPool;

#[tokio::test]
async fn cors_rejects_disallowed_origin() {
    std::env::set_var("AETHER_DISABLE_BACKGROUND", "1");
    std::env::set_var("AETHER_DISABLE_WATCH", "1");
    std::env::set_var("AETHER_DISABLE_K8S", "1");
    // Lazy pool to avoid real DB connections
    let pool: PgPool = PgPool::connect_lazy("postgres://aether:postgres@localhost:5432/none").expect("lazy pool");
    let app = control_plane::build_router(control_plane::AppState { db: pool });
    let req = Request::builder()
        .uri("/health")
        .header("Origin", "https://evil.com")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // Should not include Access-Control-Allow-Origin for disallowed origin
    assert!(!res.headers().contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn auth_returns_401_for_missing_token() {
    std::env::set_var("AETHER_AUTH_REQUIRED", "1");
    std::env::remove_var("AETHER_API_TOKENS");
    std::env::set_var("AETHER_DISABLE_BACKGROUND", "1");
    std::env::set_var("AETHER_DISABLE_WATCH", "1");
    std::env::set_var("AETHER_DISABLE_K8S", "1");
    let pool: PgPool = PgPool::connect_lazy("postgres://aether:postgres@localhost:5432/none").expect("lazy pool");
    let app = control_plane::build_router(control_plane::AppState { db: pool });
    let req = Request::builder()
        .uri("/apps")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED); // 401
}

#[tokio::test]
async fn auth_returns_403_for_invalid_scope() {
    // Enable auth with a reader token and require admin for write endpoints
    std::env::set_var("AETHER_AUTH_REQUIRED", "1");
    std::env::set_var("AETHER_API_TOKENS", "t_reader:reader:bob");
    std::env::set_var("AETHER_DISABLE_BACKGROUND", "1");
    std::env::set_var("AETHER_DISABLE_WATCH", "1");
    std::env::set_var("AETHER_DISABLE_K8S", "1");
    let pool: PgPool = PgPool::connect_lazy("postgres://aether:postgres@localhost:5432/none").expect("lazy pool");
    let app = control_plane::build_router(control_plane::AppState { db: pool });
    let req = Request::builder()
        .method("POST")
        .uri("/apps")
        .header("Authorization", "Bearer t_reader:reader:bob")
        .header("content-type","application/json")
        .body(Body::from("{\"name\":\"x\"}"))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN); // 403
}
