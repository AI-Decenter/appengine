pub mod db;
pub mod handlers;
pub mod models;
pub mod error;
pub mod services;
pub mod telemetry;
pub mod storage;
pub mod test_support;
pub mod k8s; // Kubernetes integration (Issue 04)
pub mod k8s_watch;
#[cfg(feature = "dev-hot-ingest")]
pub mod dev_hot_ingest; // New module for hot ingest development (feature-gated)
pub mod provenance; // Register provenance module usage
pub mod backfill; // backfill job utilities (legacy SBOM/provenance generation)
pub mod auth; // Auth & RBAC (Issue 10)

// Re-export storage accessor to provide a stable import path even if the module path resolution behaves differently in some build contexts.
pub use storage::get_storage;

use axum::{Router, routing::{get, post}};
use sqlx::{Pool, Postgres};
use handlers::{health::health, apps::{list_apps, app_logs, create_app, app_deployments, add_public_key}, deployments::{create_deployment, list_deployments, get_deployment}, readiness::readiness, uploads::{upload_artifact, list_artifacts, head_artifact, presign_artifact, complete_artifact, multipart_init, multipart_presign_part, multipart_complete}};
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
        handlers::deployments::get_deployment,
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
    // Background tasks (can be disabled in tests via AETHER_DISABLE_BACKGROUND=1)
    if std::env::var("AETHER_DISABLE_BACKGROUND").ok().as_deref() != Some("1") {
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
        // Failed deployment GC loop
        let db_dep_gc = state.db.clone();
        tokio::spawn(async move {
            let ttl = std::env::var("AETHER_DEPLOYMENT_FAILED_TTL_SECS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(3600);
            let interval = std::env::var("AETHER_DEPLOYMENT_FAILED_GC_INTERVAL_SECS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(300);
            loop {
                if let Ok(deleted) = crate::services::deployments::run_failed_deployments_gc(&db_dep_gc, ttl).await { if deleted > 0 { tracing::info!(deleted, "failed_deployments_gc_deleted"); } }
                tokio::time::sleep(std::time::Duration::from_secs(interval.max(30))).await;
            }
        });
    } else {
        tracing::info!("background_tasks_disabled");
    }
    // Watch-based controller for deployment status (can be disabled for tests via env)
    if std::env::var("AETHER_DISABLE_WATCH").ok().as_deref() != Some("1") {
        let db_status = state.db.clone();
        tokio::spawn(async move {
            crate::k8s_watch::run_deployment_status_watcher(db_status).await;
        });
    }
    // Coverage metrics updater (also gated by AETHER_DISABLE_BACKGROUND to reduce DB churn in tests)
    if std::env::var("AETHER_DISABLE_BACKGROUND").ok().as_deref() != Some("1") {
        let db_metrics = state.db.clone();
        tokio::spawn(async move {
            use crate::telemetry::{ARTIFACTS_WITH_SBOM, ARTIFACTS_SIGNED, ARTIFACTS_WITH_PROVENANCE};
            loop {
                // counts
                let sbom: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE sbom_url IS NOT NULL").fetch_one(&db_metrics).await.unwrap_or(0);
                let signed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE signature IS NOT NULL").fetch_one(&db_metrics).await.unwrap_or(0);
                let prov: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE provenance_present=TRUE").fetch_one(&db_metrics).await.unwrap_or(0);
                ARTIFACTS_WITH_SBOM.set(sbom as i64);
                ARTIFACTS_SIGNED.set(signed as i64);
                ARTIFACTS_WITH_PROVENANCE.set(prov as i64);
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }
    // Build OpenAPI once with injected security scheme
    static OPENAPI_DOC: once_cell::sync::Lazy<utoipa::openapi::OpenApi> = once_cell::sync::Lazy::new(|| {
        let base = ApiDoc::openapi();
        if let Ok(mut value) = serde_json::to_value(&base) {
            use serde_json::json;
            value["components"]["securitySchemes"]["bearer_auth"] = json!({"type":"http","scheme":"bearer"});
            value["security"] = json!([{"bearer_auth": []}]);
            if let Ok(spec) = serde_json::from_value(value) { return spec; }
        }
        base
    });
    let openapi = OPENAPI_DOC.clone();
    // Middleware: add X-Trace-Id propagation & request id generation
    use axum::{extract::Request, middleware::Next};
    use axum::http::HeaderValue;
    async fn trace_layer(mut req: Request, next: Next) -> Result<axum::response::Response, axum::response::Response> {
        let headers = req.headers();
        let trace_id = headers.get("X-Trace-Id").and_then(|v| v.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let request_id = headers.get("X-Request-Id").and_then(|v| v.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        // Store in extensions
        req.extensions_mut().insert(trace_id.clone()); // store trace id as String
        req.extensions_mut().insert(request_id.clone());
        let method = req.method().clone();
        let path_raw = req.uri().path().to_string();
        let norm_path = crate::telemetry::normalize_path(&path_raw);
        let start = std::time::Instant::now();
        let span = tracing::info_span!(
            "http.req",
            %method,
            path=%norm_path,
            raw_path=%path_raw,
            %trace_id,
            %request_id,
            user_role = tracing::field::Empty,
            user_name = tracing::field::Empty,
            auth_result = tracing::field::Empty
        );
        let _enter = span.enter();
        let mut resp = next.run(req).await;
        let status = resp.status().as_u16();
        let outcome = if (200..400).contains(&status) { "success" } else { "error" };
        crate::telemetry::HTTP_REQUESTS.with_label_values(&[method.as_str(), &norm_path, &status.to_string(), outcome]).inc();
        crate::telemetry::HTTP_REQUEST_DURATION.with_label_values(&[method.as_str(), &norm_path]).observe(start.elapsed().as_secs_f64());
        // Propagate request id headers
        if let Ok(h) = HeaderValue::from_str(&trace_id) { resp.headers_mut().insert("X-Trace-Id", h); }
        if let Ok(h) = HeaderValue::from_str(&request_id) { resp.headers_mut().insert("X-Request-Id", h); }
        tracing::info!(status, took_ms=%start.elapsed().as_millis(), outcome, "request.complete");
        Ok(resp)
    }
    let trace_layer_mw = axum::middleware::from_fn(trace_layer);

    // Optional auth and RBAC layers (activate only when AETHER_AUTH_REQUIRED=1)
    let auth_store = std::sync::Arc::new(crate::auth::AuthStore::from_env());
    let auth_store_for_auth = auth_store.clone();
    let auth_layer = axum::middleware::from_fn_with_state(auth_store.clone(), move |req, next| {
        let store = auth_store_for_auth.clone();
        crate::auth::auth_middleware(req, next, store)
    });
    let auth_store_for_admin = auth_store.clone();
    let admin_guard = axum::middleware::from_fn_with_state(auth_store.clone(), move |req, next| {
        let store = auth_store_for_admin.clone();
        crate::auth::require_role(req, next, store, crate::auth::Role::Admin)
    });

    // Public endpoints
    let public = Router::new()
        .route("/health", get(health))
        .route("/readyz", get(readiness))
        .route("/startupz", get(handlers::readiness::startupz))
        .route("/metrics", get(metrics_handler));

    // Read endpoints (auth-only)
    let reads = Router::new()
        .route("/deployments", get(list_deployments))
        .route("/deployments/:id", get(get_deployment))
        .route("/artifacts", get(list_artifacts))
        .route("/artifacts/:digest", axum::routing::head(head_artifact))
        .route("/artifacts/:digest/meta", get(handlers::uploads::artifact_meta))
        .route("/artifacts/:digest/sbom", get(handlers::artifacts::get_sbom))
        .route("/artifacts/:digest/manifest", get(handlers::artifacts::get_manifest))
        .route("/provenance", get(handlers::provenance::list_provenance))
        .route("/artifacts/:digest/sbom", axum::routing::post(handlers::artifacts::upload_sbom))
        .route("/artifacts/:digest/manifest", axum::routing::post(handlers::artifacts::upload_manifest))
        .route("/provenance/:digest", get(handlers::provenance::get_provenance))
        .route("/provenance/:digest/attestation", get(handlers::provenance::get_attestation))
        .route("/provenance/keys", get(handlers::keys::list_keys))
        .route("/apps", get(list_apps))
        .route("/apps/:app_name/deployments", get(app_deployments))
        .route("/apps/:app_name/logs", get(app_logs))
        .layer(auth_layer.clone());

    // Write endpoints (auth + admin)
    let writes = Router::new()
        .route("/deployments", post(create_deployment))
        .route("/deployments/:id", axum::routing::patch(handlers::deployments::update_deployment))
        .route("/artifacts", post(upload_artifact))
        .route("/artifacts/presign", post(presign_artifact))
        .route("/artifacts/complete", post(complete_artifact))
        .route("/artifacts/multipart/init", post(multipart_init))
        .route("/artifacts/multipart/presign-part", post(multipart_presign_part))
        .route("/artifacts/multipart/complete", post(multipart_complete))
        .route("/apps", post(create_app))
        .route("/apps/:app_name/public-keys", post(add_public_key))
        .layer(admin_guard.clone())
        .layer(auth_layer.clone());

    Router::new()
        .merge(public)
        .merge(reads)
        .merge(writes)
        .route("/openapi.json", get(move || async move { axum::Json(openapi.clone()) }))
        .route("/swagger", get(swagger_ui))
        .layer(trace_layer_mw)
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
    let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/apps/demo/logs").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn app_logs_mock_json_default() {
        std::env::set_var("AETHER_MOCK_LOGS","1");
        let pool = crate::test_support::test_pool().await;
        let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/apps/app1/logs?tail_lines=3").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("application/x-ndjson"));
        let body = axum::body::to_bytes(res.into_body(), 10_000).await.unwrap();
        let s = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["app"], "app1");
        assert_eq!(v["pod"], "pod-a");
    }

    #[tokio::test]
    async fn app_logs_mock_text_format() {
        std::env::set_var("AETHER_MOCK_LOGS","1");
        let pool = crate::test_support::test_pool().await;
        let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/apps/app1/logs?tail_lines=2&format=text").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/plain"));
        let body = axum::body::to_bytes(res.into_body(), 10_000).await.unwrap();
        let s = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("pod-a"));
    }

    #[tokio::test]
    async fn app_logs_mock_multi_pod() {
        std::env::set_var("AETHER_MOCK_LOGS","1");
        std::env::set_var("AETHER_MOCK_LOGS_MULTI","1");
        let pool = crate::test_support::test_pool().await;
        let app = build_router(AppState { db: pool });
        let res = app.oneshot(Request::builder().uri("/apps/app2/logs?tail_lines=1").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 10_000).await.unwrap();
        let s = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // follow=false with tail=1 stops after first line across multi-pod loop (deterministic)
        assert_eq!(lines.len(), 1);
    }

    #[tokio::test]
    async fn readiness_ok() {
    let pool = crate::test_support::test_pool().await;
    let app = build_router(AppState { db: pool });
        // Retry loop to mitigate transient connection establishment races under CI
        let mut attempts = 0;
        loop {
            let res = app.clone().oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap()).await.unwrap();
            if res.status()==StatusCode::OK { break; }
            attempts += 1;
            if attempts > 5 { panic!("readiness did not reach 200 after retries (last={})", res.status()); }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_deployment_bad_json() {
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
    let pool = crate::test_support::test_pool().await;
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
