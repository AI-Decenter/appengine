pub mod db;
pub mod handlers;
pub mod models;
pub mod error;
pub mod services;
pub mod telemetry;

use axum::{Router, routing::{get, post}};
use sqlx::{Pool, Postgres};
use handlers::{health::health, apps::{list_apps, app_logs, create_app, app_deployments}, deployments::{create_deployment, list_deployments}, readiness::readiness};
use utoipa::OpenApi;
use crate::telemetry::metrics_handler;
use axum::response::Html;

#[derive(Clone)]
pub struct AppState { pub db: Pool<Postgres> }

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health::health,
    handlers::readiness::readiness,
    handlers::readiness::startupz,
        handlers::apps::create_app,
        handlers::apps::list_apps,
        handlers::apps::app_deployments,
        handlers::deployments::create_deployment,
        handlers::deployments::list_deployments,
    ),
    components(schemas(error::ApiErrorBody)),
    tags( (name = "aether", description = "Aether Control Plane API") )
)]
pub struct ApiDoc;

async fn swagger_ui() -> Html<String> {
    let html = r#"<!DOCTYPE html>
<html lang=\"en\">
<head><meta charset=\"UTF-8\"/><title>Aether API Docs</title>
<link rel=\"stylesheet\" href=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui.css\" />
</head>
<body>
<div id=\"swagger-ui\"></div>
<script src=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js\"></script>
<script>
window.onload = () => { SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' }); };
</script>
</body></html>"#;
    Html(html.to_string())
}

pub fn build_router(state: AppState) -> Router {
    let openapi = ApiDoc::openapi();
    Router::new()
        .route("/health", get(health))
    .route("/readyz", get(readiness))
    .route("/startupz", get(handlers::readiness::startupz))
        .route("/metrics", get(metrics_handler))
        .route("/deployments", post(create_deployment).get(list_deployments))
        .route("/apps", post(create_app))
        .route("/apps", get(list_apps))
        .route("/apps/:app_name/deployments", get(app_deployments))
        .route("/apps/:app_name/logs", get(app_logs))
        .route("/openapi.json", get(|| async move { axum::Json(openapi.clone()) }))
        .route("/swagger", get(swagger_ui))
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
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap_or_default()).await.ok();
    if pool.is_none() { eprintln!("skipping health_ok (no db)" ); return; }
    let app = build_router(AppState { db: pool.unwrap() });
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
        let app_router = build_router(AppState { db: pool });
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
    let app_router = build_router(AppState { db: pool });
        let res = app_router.oneshot(Request::builder().uri("/apps").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, json!([]));
    }

    #[tokio::test]
    async fn app_logs_empty() {
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap_or_default()).await.ok();
    if pool.is_none() { eprintln!("skipping app_logs_empty (no db)" ); return; }
    let app = build_router(AppState { db: pool.unwrap() });
        let res = app.oneshot(Request::builder().uri("/apps/demo/logs").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn readiness_ok() {
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap_or_default()).await.ok();
    if pool.is_none() { eprintln!("skipping readiness_ok (no db)" ); return; }
    let app = build_router(AppState { db: pool.unwrap() });
        let res = app.oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_deployment_bad_json() {
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping create_deployment_bad_json (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
    let app_router = build_router(AppState { db: pool });
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
    let app_router = build_router(AppState { db: pool });
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
    let app_router = build_router(AppState { db: pool });
        let res = app_router.oneshot(Request::builder().uri("/apps/appx/deployments").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_app_conflict_error_json() {
        if std::env::var("DATABASE_URL").is_err() { eprintln!("skipping create_app_conflict_error_json (no DATABASE_URL)" ); return; }
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("dupe").execute(&pool).await.unwrap();
    let app_router = build_router(AppState { db: pool });
        let body = serde_json::json!({"name":"dupe"}).to_string();
        let req = Request::builder().method("POST").uri("/apps")
            .header("content-type","application/json")
            .body(Body::from(body)).unwrap();
        let res = app_router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["code"], "conflict");
    }

    #[test]
    fn normalize_path_property() {
        use crate::telemetry::normalize_path;
        // UUID and numeric collapsed
        assert_eq!(normalize_path("/deployments/123"), "/deployments/:id");
        assert_eq!(normalize_path("/deployments/550e8400-e29b-41d4-a716-446655440000"), "/deployments/:id");
        assert_eq!(normalize_path("/apps/myapp/deployments"), "/apps/:app_name/deployments");
    }
}
