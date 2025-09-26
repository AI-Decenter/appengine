use std::{fs, path::PathBuf};
use anyhow::{Result};
use clap::{Parser, Subcommand};
use tracing::{info, error};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "aether", version, about = "AetherEngine CLI (MVP)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Increase verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate and store a token
    Login { #[arg(long)] token: Option<String> },
    /// Package and deploy current application
    Deploy { #[arg(long, default_value = ".")] path: PathBuf },
    /// Stream logs for an application
    Logs { app: String },
    /// List applications
    List {},
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity { 0 => "info", 1 => "debug", _ => "trace" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli.command {
        Commands::Login { token } => cmd_login(token).await?,
        Commands::Deploy { path } => cmd_deploy(path).await?,
        Commands::Logs { app } => cmd_logs(app).await?,
        Commands::List {} => cmd_list().await?,
    }
    Ok(())
}

async fn cmd_login(token: Option<String>) -> Result<()> {
    let token = token.unwrap_or_else(|| "dev-local-token".to_string());
    let dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let config_path = dir.join("aether");
    fs::create_dir_all(&config_path)?;
    let file = config_path.join("credentials.json");
    let payload = serde_json::json!({"token": token});
    fs::write(&file, serde_json::to_vec_pretty(&payload)?)?;
    info!(path=%file.display(), "Stored credentials");
    Ok(())
}

async fn cmd_deploy(path: PathBuf) -> Result<()> {
    if !path.join("package.json").exists() {
        error!("No package.json detected; only Node.js projects supported in MVP");
        return Ok(());
    }
    info!(app_path=%path.display(), "Packaging (placeholder)");
    Ok(())
}

async fn cmd_logs(app: String) -> Result<()> { info!(%app, "(placeholder) streaming logs"); Ok(()) }
async fn cmd_list() -> Result<()> { info!("(placeholder) listing applications"); Ok(()) }
