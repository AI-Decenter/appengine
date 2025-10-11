//! Binary entrypoint for the Control Plane service.
use control_plane::{db::init_db, build_router, AppState};
#[cfg(feature = "dev-hot-ingest")]
use control_plane::dev_hot_ingest::spawn_dev_hot_log_ingestion;
use tracing::info;
use std::net::SocketAddr;
use axum::{http::{Request, HeaderValue}, middleware::{self, Next}, response::Response, body::Body};
use tower_http::{limit::RequestBodyLimitLayer, cors::CorsLayer};
use control_plane::telemetry::{HTTP_REQUESTS, HTTP_REQUEST_DURATION, normalize_path, DB_POOL_IDLE, DB_POOL_IN_USE, DB_POOL_SIZE};
use std::{time::Duration, collections::HashMap, net::IpAddr, sync::{Arc, Mutex}};
use uuid::Uuid;

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
    // Spawn dev-hot ingestion (optional via env AETHER_DEV_HOT_INGEST=1)
    #[cfg(feature = "dev-hot-ingest")]
    if let Err(e) = spawn_dev_hot_log_ingestion().await { tracing::warn!(error=%e, "failed to spawn dev-hot ingestion"); }
    let rate_limit_enabled = std::env::var("AETHER_RATE_LIMIT").unwrap_or_default() == "1";
    // Support multiple tokens via CSV env AETHER_API_TOKENS; keep backward compat with single AETHER_API_TOKEN
    let auth_tokens: Vec<String> = if let Ok(list) = std::env::var("AETHER_API_TOKENS") {
        list.split(',').filter_map(|s| { let t = s.trim(); if t.is_empty() { None } else { Some(t.to_string()) } }).collect()
    } else if let Ok(single) = std::env::var("AETHER_API_TOKEN") { vec![single] } else { Vec::new() };
    let rate_state: Arc<Mutex<HashMap<IpAddr,(u32,std::time::Instant)>>> = Arc::new(Mutex::new(HashMap::new()));
    let app = build_router(state.clone());
    async fn track_metrics(mut req: Request<Body>, next: Next) -> Response {
        let method = req.method().clone();
        let raw_path = req.uri().path().to_string();
        let path_label = normalize_path(&raw_path);
    let req_id = Uuid::new_v4();
        req.extensions_mut().insert(req_id);
        let start = std::time::Instant::now();
        let mut resp = next.run(req).await;
        let status = resp.status().as_u16().to_string();
        let outcome = if let Ok(code) = status.parse::<u16>() { code < 400 } else { false };
        HTTP_REQUESTS.with_label_values(&[method.as_str(), path_label.as_str(), status.as_str(), if outcome {"success"} else {"error"}]).inc();
        let elapsed = start.elapsed().as_secs_f64();
        HTTP_REQUEST_DURATION.with_label_values(&[method.as_str(), path_label.as_str()]).observe(elapsed);
    resp.headers_mut().insert("x-request-id", HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap());
        resp
    }
    // Auth + Rate limit + Pool gauges
    let auth_tokens_clone = auth_tokens.clone();
    let state_clone = state.clone();
    let rate_state_clone = rate_state.clone();
    let auth_and_limit = move |req: Request<Body>, next: Next| {
    let auth_tokens = auth_tokens_clone.clone();
        let state_for_pool = state_clone.clone();
        let rate_state = rate_state_clone.clone();
        async move {
            let path = req.uri().path();
            let exempt = matches!(path, "/health"|"/readyz"|"/startupz"|"/metrics"|"/openapi.json"|"/swagger");
            if !exempt && rate_limit_enabled {
                if let Some(remote) = req.extensions().get::<std::net::SocketAddr>() { // may not be set; skip if absent
                    let ip = remote.ip();
                    let mut guard = rate_state.lock().unwrap();
                    let entry = guard.entry(ip).or_insert((0, std::time::Instant::now() + Duration::from_secs(60)));
                    if std::time::Instant::now() > entry.1 { *entry = (0, std::time::Instant::now() + Duration::from_secs(60)); }
                    if entry.0 >= 60 {
                        tracing::warn!(client_ip=%ip, "rate_limit.429");
                        return Response::builder().status(429).body(Body::from("rate_limit")).unwrap();
                    }
                    entry.0 += 1;
                }
            }
            if !exempt && !auth_tokens.is_empty() {
                let provided = req.headers().get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
                let valid = auth_tokens.iter().any(|tok| provided == format!("Bearer {tok}"));
                if !valid {
                    static UNAUTH_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                    let n = UNAUTH_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n % 10 == 0 { tracing::warn!("auth.unauthorized.legacy_path"); }
                    return Response::builder().status(401).body(Body::from("unauthorized")).unwrap();
                }
            }
            let pool = &state_for_pool.db;
            let size = pool.size() as i64;
            let idle = pool.num_idle() as i64;
            DB_POOL_SIZE.set(size);
            DB_POOL_IDLE.set(idle);
            DB_POOL_IN_USE.set(size - idle);
            next.run(req).await
        }
    };
    const MAX_BODY_BYTES: usize = 1024 * 1024; // 1MB
    let app = app
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(auth_and_limit))
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
