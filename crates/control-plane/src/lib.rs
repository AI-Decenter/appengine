pub mod db;
pub mod handlers;
pub mod models;
pub mod error;

use axum::{Router, routing::{get, post}};
use sqlx::{Pool, Postgres};
use handlers::{health::health, apps::{list_apps, app_logs, create_app, app_deployments}, deployments::{create_deployment, list_deployments}, readiness::readiness};

#[derive(Clone)]
pub struct AppState { pub db: Option<Pool<Postgres>> }

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readiness))
    .route("/deployments", post(create_deployment).get(list_deployments))
        .route("/apps", post(create_app))
        .route("/apps", get(list_apps))
    .route("/apps/:app_name/deployments", get(app_deployments))
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
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping create_deployment_201 (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("app1").execute(&pool).await.unwrap();
        let app_router = build_router(AppState { db: Some(pool) });
        let body = serde_json::json!({"app_name":"app1","artifact_url":"file://artifact"}).to_string();
        let req = Request::builder().method("POST").uri("/deployments")
            .header("content-type","application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app_router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn list_apps_empty() {
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping list_apps_empty (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        let app_router = build_router(AppState { db: Some(pool) });
        let res = app_router.oneshot(Request::builder().uri("/apps").body(Body::empty()).unwrap()).await.unwrap();
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
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping create_deployment_bad_json (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        let app_router = build_router(AppState { db: Some(pool) });
        let req = Request::builder().method("POST").uri("/deployments")
            .header("content-type","application/json")
            .body(Body::from("{invalid"))
            .unwrap();
        let res = app_router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_deployments_empty() {
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping list_deployments_empty (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        let app_router = build_router(AppState { db: Some(pool) });
        let res = app_router.oneshot(Request::builder().uri("/deployments").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, json!([]));
    }

    #[tokio::test]
    async fn app_deployments_flow() {
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping app_deployments_flow (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("appx").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO deployments (app_id, artifact_url, status) SELECT id, $1, 'pending' FROM applications WHERE name = $2")
            .bind("file://a").bind("appx").execute(&pool).await.unwrap();
        let app_router = build_router(AppState { db: Some(pool) });
        let res = app_router.oneshot(Request::builder().uri("/apps/appx/deployments").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
    }
}
