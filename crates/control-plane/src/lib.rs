pub mod db;
pub mod handlers;
pub mod models;

use axum::{Router, routing::{get, post}};
use sqlx::{Pool, Postgres};
use handlers::{health::health, apps::{list_apps, app_logs}, deployments::create_deployment, readiness::readiness};

#[derive(Clone)]
pub struct AppState { pub db: Option<Pool<Postgres>> }

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readiness))
        .route("/deployments", post(create_deployment))
        .route("/apps", get(list_apps))
        .route("/apps/:app_name/logs", get(app_logs))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::{Request, StatusCode}, body::Body};
    use tower::util::ServiceExt;
    use serde_json::json;

    #[tokio::test]
    async fn health_ok() {
        let app = build_router(AppState { db: None });
        let res = app.oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, json!({"status":"ok"}));
    }

    #[tokio::test]
    async fn create_deployment_201() {
        let app = build_router(AppState { db: None });
        let req = Request::builder().method("POST").uri("/deployments")
            .header("content-type","application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn list_apps_empty() {
        let app = build_router(AppState { db: None });
        let res = app.oneshot(Request::builder().uri("/apps").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, json!([]));
    }

    #[tokio::test]
    async fn app_logs_empty() {
        let app = build_router(AppState { db: None });
        let res = app.oneshot(Request::builder().uri("/apps/demo/logs").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn readiness_ok() {
        let app = build_router(AppState { db: None });
        let res = app.oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_deployment_bad_json() {
        let app = build_router(AppState { db: None });
        let req = Request::builder().method("POST").uri("/deployments")
            .header("content-type","application/json")
            .body(Body::from("{invalid"))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        // Axum returns 400 for body deserialization errors
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
