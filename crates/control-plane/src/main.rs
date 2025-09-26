//! Binary entrypoint for the Control Plane service.
use control_plane::{db::init_db, build_router, AppState};
use tracing::{info, error};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:postgres@localhost:5432/aether_dev".to_string());
    // Try DB init; if fails we still start (but endpoints needing DB would return 500 in future). For MVP we allow panic
    let db_pool = match init_db(&database_url).await { Ok(p)=>Some(p), Err(e)=> { error!(error=%e, "database init failed"); None } };
    let app = build_router(AppState { db: db_pool });
    let addr: SocketAddr = "0.0.0.0:3000".parse()?;
    info!(%addr, "control-plane listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
