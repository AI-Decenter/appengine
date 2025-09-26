use axum::{routing::{get, post}, Router, Json};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use tracing::{info, error};
use sqlx::{Pool, Postgres};
use once_cell::sync::Lazy;

static APP_STATE: Lazy<AppState> = Lazy::new(|| AppState { start_time: std::time::Instant::now() });

#[derive(Clone)]
pub struct AppCtx { pub db: Pool<Postgres> }

struct AppState { start_time: std::time::Instant }

#[derive(Serialize, Debug, PartialEq)]
pub struct Health { pub status: &'static str, pub uptime_seconds: u64 }

#[derive(Deserialize, Serialize, Debug)]
pub struct DeploymentRequest { pub app: String, pub artifact_digest: String }

#[derive(Serialize, Debug)]
pub struct DeploymentResponse { pub deployment_id: uuid::Uuid, pub status: &'static str }

pub async fn health() -> Json<Health> {
    Json(Health { status: "ok", uptime_seconds: APP_STATE.start_time.elapsed().as_secs() })
}

pub async fn deploy(Json(req): Json<DeploymentRequest>) -> Json<DeploymentResponse> {
    let id = uuid::Uuid::new_v4();
    info!(deployment_id=%id, app=%req.app, digest=%req.artifact_digest, "received deployment request (placeholder)");
    Json(DeploymentResponse { deployment_id: id, status: "accepted" })
}

pub async fn readiness() -> &'static str { "ready" }

pub async fn init_db(database_url: &str) -> anyhow::Result<Pool<Postgres>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await?;
    if let Err(e) = sqlx::migrate!().run(&pool).await { error!(error=%e, "migration failure"); } else { info!("migrations applied (if any)"); }
    Ok(pool)
}

pub fn build_router(pool: Option<Pool<Postgres>>) -> Router {
    // Always create router with explicit unit state then optionally layer real state; this keeps
    // the return type consistent (Router with concrete state) and avoids generic mismatch.
    match pool {
        Some(db) => Router::new()
            .route("/healthz", get(health))
            .route("/readyz", get(readiness))
            .route("/deployments", post(deploy))
            .with_state(AppCtx { db }),
        None => Router::new()
            .route("/healthz", get(health))
            .route("/readyz", get(readiness))
            .route("/deployments", post(deploy)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for oneshot

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let app = build_router(None);
        let res = app
            .oneshot(Request::builder().uri("/healthz").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // Simpler: decode via axum::body::to_bytes (available through axum) or skip body parse.
        let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["status"], "ok");
    }
}
