mod config;
mod logging;
mod errors;
mod commands;
mod util;

use anyhow::Result;
use clap::Parser;
use commands::{Cli, Commands};
use logging::init_logging;
use tracing::{info_span, info};
use config::EffectiveConfig;
use std::process;
use std::time::Instant;
use errors::CliError;
use crate::util::time::fmt_duration;

#[tokio::main]
async fn main() -> Result<()> {
    let start = Instant::now();
    let cli = Cli::parse();
    init_logging(&cli.log_level, &cli.log_format)?;
    let cfg = match EffectiveConfig::load() { Ok(c)=>c, Err(e)=>{ let code = classify_exit_code(&e); info!(took_ms=%start.elapsed().as_millis(), event="cli.finished", exit_code=code); if code!=0 { process::exit(code);} return Ok(());} };
    let exit_code = match dispatch(cli, cfg).await { Ok(())=>0, Err(e)=> classify_exit_code(&e) };
    info!(took_ms=%start.elapsed().as_millis(), event="cli.finished", exit_code=exit_code);
    if exit_code != 0 { process::exit(exit_code); }
    Ok(())
}

async fn dispatch(cli: Cli, _cfg: EffectiveConfig) -> Result<()> {
    use std::time::Instant;
    let start = Instant::now();
    let result = match cli.command {
        Commands::Login { username } => { let _span = info_span!("cmd.login").entered(); commands::login::handle(username).await }
    Commands::Deploy { dry_run, pack_only, compression_level, out, no_upload, no_cache, no_sbom, legacy_sbom, cyclonedx, format, legacy_upload, dev_hot } => { let _span = info_span!("cmd.deploy", dry_run, pack_only, compression_level, out=?out, no_upload, no_cache, no_sbom, legacy_sbom, cyclonedx, format=?format, legacy_upload, dev_hot); commands::deploy::handle(commands::deploy::DeployOptions { dry_run, pack_only, compression_level, out, no_upload, no_cache, no_sbom, legacy_sbom, cyclonedx, format, use_legacy_upload: legacy_upload, dev_hot }).await }
        Commands::Logs { app } => { let _span = info_span!("cmd.logs"); commands::logs::handle(app).await }
        Commands::List {} => { let _span = info_span!("cmd.list"); commands::list::handle().await }
        Commands::Completions { shell } => { let _span = info_span!("cmd.completions"); commands::completions::handle(shell) }
        Commands::Netfail {} => { let _span = info_span!("cmd.netfail"); commands::netfail::handle().await }
        Commands::Iofail {} => { let _span = info_span!("cmd.iofail"); commands::iofail::handle().await }
        Commands::Usagefail {} => { let _span = info_span!("cmd.usagefail"); commands::usagefail::handle().await }
        Commands::Runtimefail {} => { let _span = info_span!("cmd.runtimefail"); commands::runtimefail::handle().await }
        Commands::Dev { hot, interval } => { let _span = info_span!("cmd.dev", hot, interval); commands::dev::handle(hot, interval).await }
    };
    let took_d = start.elapsed();
    let took_ms = took_d.as_millis();
    let human = fmt_duration(took_d);
    match &result { Ok(_) => info!(event="cmd.finished", took_ms=%took_ms, took_human=%human), Err(e)=> { eprintln!("error: {e}"); info!(event="cmd.failed", took_ms=%took_ms, took_human=%human); } }
    result
}

fn classify_exit_code(e: &anyhow::Error) -> i32 {
    use std::error::Error;
    let mut cur: &dyn Error = e.as_ref();
    loop {
        if let Some(cli) = cur.downcast_ref::<CliError>() { tracing::debug!(?cli, code=cli.kind.code(), "classified_cli_error"); return cli.kind.code(); }
        if let Some(ioe) = cur.downcast_ref::<std::io::Error>() { eprintln!("io error: {ioe}"); return 30; }
        if let Some(src) = cur.source() { cur = src; } else { break; }
    }
    eprintln!("runtime error: {e}");
    20
}
