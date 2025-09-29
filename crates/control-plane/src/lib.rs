pub mod db;
pub mod handlers;
pub mod models;
pub mod error;
pub mod services;
pub mod telemetry;
pub mod storage;

// Re-export storage accessor to provide a stable import path even if the module path resolution behaves differently in some build contexts.
pub use storage::get_storage;

use axum::{Router, routing::{get, post}};
use sqlx::{Pool, Postgres};
use handlers::{health::health, apps::{list_apps, app_logs, create_app, app_deployments, add_public_key}, deployments::{create_deployment, list_deployments}, readiness::readiness, uploads::{upload_artifact, list_artifacts, head_artifact, presign_artifact, complete_artifact, multipart_init, multipart_presign_part, multipart_complete}};
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
    handlers::uploads::upload_artifact,
    handlers::uploads::list_artifacts,
    handlers::uploads::presign_artifact,
    handlers::uploads::complete_artifact,
    handlers::uploads::artifact_meta,
    handlers::uploads::multipart_init,
    handlers::uploads::multipart_presign_part,
    handlers::uploads::multipart_complete,
    handlers::apps::add_public_key,
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
    let mut openapi = ApiDoc::openapi();
    // Inject security scheme manually (workaround for macro limitations)
    if let Ok(mut value) = serde_json::to_value(&openapi) {
        use serde_json::json;
        value["components"]["securitySchemes"]["bearer_auth"] = json!({"type":"http","scheme":"bearer"});
        value["security"] = json!([{"bearer_auth": []}]);
        if let Ok(spec) = serde_json::from_value(value.clone()) { openapi = spec; }
    }
    // Initialize artifacts_total gauge asynchronously
    let db_clone = state.db.clone();
    tokio::spawn(async move { crate::handlers::uploads::init_artifacts_total(&db_clone).await; });
    // Spawn pending artifact GC loop
    let db_gc = state.db.clone();
    tokio::spawn(async move {
        let ttl = std::env::var("AETHER_PENDING_TTL_SECS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(3600);
        let interval = std::env::var("AETHER_PENDING_GC_INTERVAL_SECS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60);
        loop {
            crate::handlers::uploads::run_pending_gc(&db_gc, ttl).await.ok();
            tokio::time::sleep(std::time::Duration::from_secs(interval.max(5))).await;
        }
    });
    Router::new()
        .route("/health", get(health))
    .route("/readyz", get(readiness))
    .route("/startupz", get(handlers::readiness::startupz))
        .route("/metrics", get(metrics_handler))
    .route("/deployments", post(create_deployment).get(list_deployments))
    .route("/artifacts", post(upload_artifact).get(list_artifacts))
    .route("/artifacts/presign", post(presign_artifact))
    .route("/artifacts/complete", post(complete_artifact))
    .route("/artifacts/multipart/init", post(multipart_init))
    .route("/artifacts/multipart/presign-part", post(multipart_presign_part))
    .route("/artifacts/multipart/complete", post(multipart_complete))
    .route("/artifacts/:digest", axum::routing::head(head_artifact))
    .route("/artifacts/:digest/meta", get(handlers::uploads::artifact_meta))
        .route("/apps", post(create_app))
        .route("/apps", get(list_apps))
        .route("/apps/:app_name/deployments", get(app_deployments))
        .route("/apps/:app_name/logs", get(app_logs))
        .route("/apps/:app_name/public-keys", post(add_public_key))
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
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, json!({"status":"ok"}));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_deployment_201() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
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
    #[serial_test::serial]
    async fn list_apps_empty() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
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
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/apps/demo/logs").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn readiness_ok() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_deployment_bad_json() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let app_router = build_router(AppState { db: pool });
        let req = Request::builder().method("POST").uri("/deployments")
            .header("content-type","application/json")
            .body(Body::from("{invalid"))
            .unwrap();
        let res = app_router.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn list_deployments_empty() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
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
    #[serial_test::serial]
    async fn app_deployments_flow() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
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
    #[serial_test::serial]
    async fn app_deployments_empty_when_no_deployments() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
        // Clean state
        sqlx::query("DELETE FROM deployments").execute(&pool).await.ok();
        sqlx::query("DELETE FROM applications").execute(&pool).await.ok();
        // Insert application only
        sqlx::query("INSERT INTO applications (name) VALUES ($1)").bind("emptyapp").execute(&pool).await.unwrap();
        let app_router = build_router(AppState { db: pool });
        let res = app_router.oneshot(Request::builder().uri("/apps/emptyapp/deployments").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "expected 200 for existing app with zero deployments");
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, serde_json::json!([]));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_app_conflict_error_json() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
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
