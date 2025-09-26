//! Binary entrypoint. Core logic resides in lib for testability.
use control_plane::{init_db, build_router};
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
    let pool = match init_db(&database_url).await {
        Ok(p) => p,
        Err(e) => { error!(error=%e, "database init failed"); panic!("DB required for control plane MVP"); }
    };
    let app = build_router(Some(pool));
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!(%addr, "control-plane listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
