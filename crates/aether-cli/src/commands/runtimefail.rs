use anyhow::Result;
use crate::errors::{CliError, CliErrorKind};

pub async fn handle() -> Result<()> {
    Err(CliError::new(CliErrorKind::Runtime("general runtime failure - this is a test simulation".into())).into())
}