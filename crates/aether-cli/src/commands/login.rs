use anyhow::Result;
use std::{fs, os::unix::fs::PermissionsExt};
use tracing::info;
use crate::config::{session_file_path, cache_dir};

pub async fn handle(username: Option<String>) -> Result<()> {
    let user = username.unwrap_or_else(whoami::username);
    let session_path = session_file_path();
    std::fs::create_dir_all(cache_dir())?;
    let payload = serde_json::json!({"token":"dev-mock-token","user":user});
    fs::write(&session_path, serde_json::to_vec_pretty(&payload)?)?;
    #[cfg(unix)] {
        if std::env::var("AETHER_TEST_PERMISSIVE").ok().as_deref()==Some("1") {
            let mut perms = fs::metadata(&session_path)?.permissions();
            perms.set_mode(0o666); fs::set_permissions(&session_path, perms)?;
        }
        else {
            // Enforce restrictive permissions (rw-------) for session token by default.
            let mut perms = fs::metadata(&session_path)?.permissions();
            perms.set_mode(0o600); fs::set_permissions(&session_path, perms)?;
        }
        let meta = fs::metadata(&session_path)?;
        if meta.permissions().mode() & 0o077 != 0 { eprintln!("warning: session file permissions too open: {:o}", meta.permissions().mode() & 0o777); }
    }
    info!(event="login.stored", path=%session_path.display());
    Ok(())
}
