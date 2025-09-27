use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use crate::commands::LogFormat;

pub fn init_logging(level: &str, format: &LogFormat) -> Result<()> {
    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let base = fmt::layer().with_target(false).with_timer(fmt::time::uptime());
    match format {
        LogFormat::Json => tracing_subscriber::registry().with(env).with(base.json()).init(),
        _ => tracing_subscriber::registry().with(env).with(base.compact()).init(),
    }
    Ok(())
}
