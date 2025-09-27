use anyhow::Result;
use crate::errors::{CliError, CliErrorKind};

pub async fn handle() -> Result<()> {
    Err(CliError::new(CliErrorKind::Network("simulated network failure".into())).into())
}