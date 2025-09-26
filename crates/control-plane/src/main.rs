//! Binary entrypoint for the Control Plane service.
use control_plane::{db::init_db, build_router, AppState};
use tracing::info;
use std::net::SocketAddr;
use axum::{http::Request, middleware::{self, Next}, response::Response, body::Body};
use tower_http::limit::RequestBodyLimitLayer;
use control_plane::telemetry::{HTTP_REQUESTS, HTTP_REQUEST_DURATION, normalize_path};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:postgres@localhost:5432/aether_dev".to_string());
    let db_pool = init_db(&database_url).await.expect("database must be available");
    let state = AppState { db: db_pool };
    let app = build_router(state);
    async fn track_metrics(req: Request<Body>, next: Next) -> Response {
        let method = req.method().clone();
        let raw_path = req.uri().path().to_string();
        let path_label = normalize_path(&raw_path);
        let start = std::time::Instant::now();
        let resp = next.run(req).await;
        let status = resp.status().as_u16().to_string();
        HTTP_REQUESTS.with_label_values(&[method.as_str(), path_label.as_str(), status.as_str()]).inc();
        let elapsed = start.elapsed().as_secs_f64();
        HTTP_REQUEST_DURATION.with_label_values(&[method.as_str(), path_label.as_str()]).observe(elapsed);
        resp
    }
    const MAX_BODY_BYTES: usize = 1024 * 1024; // 1MB
    let app = app
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(middleware::from_fn(track_metrics));
    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    info!(%addr, "control-plane listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let shutdown = async {
        tokio::signal::ctrl_c().await.expect("install ctrl_c");
    info!(target: "shutdown.signal", "received Ctrl+C");
        tokio::time::sleep(Duration::from_millis(200)).await; // graceful drain window
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
