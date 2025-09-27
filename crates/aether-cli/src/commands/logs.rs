use anyhow::Result;use tracing::info;use std::time::{Duration, SystemTime};
pub async fn handle(app: Option<String>) -> Result<()> { let appn = app.unwrap_or_else(|| "sample-app".into()); let now = SystemTime::now(); for i in 0..5 { info!(event="logs.line", app=%appn, line=i, ts=?now); } tokio::time::sleep(Duration::from_millis(10)).await; Ok(()) }
