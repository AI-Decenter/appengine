use kube::Client;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let client = Client::try_default().await?;
    info!("operator starting (skeleton)");
    // Controller loop removed in MVP skeleton; future implementation will reconcile AetherApp CRs.
    let _ = client; // suppress unused warning
    Ok(())
}
