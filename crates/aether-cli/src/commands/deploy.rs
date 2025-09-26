use anyhow::Result;
use tracing::{info,warn};
use std::path::{Path, PathBuf};
use sha2::{Sha256,Digest};
use walkdir::WalkDir;
use std::fs;
use tar::Builder;
use flate2::write::GzEncoder;
use flate2::Compression;
use glob::Pattern;
use std::process::Command;
use crate::errors::{CliError, CliErrorKind};

pub async fn handle(dry_run: bool, pack_only: bool) -> Result<()> {
    let root = Path::new(".");
    if !is_node_project(root) { return Err(CliError::new(CliErrorKind::Usage("not a NodeJS project (missing package.json)".into())).into()); }
    if dry_run { info!(event="deploy.dry_run", msg="Would run npm install --production and package project"); return Ok(()); }

    if !pack_only { maybe_run_npm_install(root)?; }
    else { info!(event="deploy.pack_only", msg="Skipping npm install (pack-only mode)"); }

    let ignore_patterns = load_ignore_patterns(root);
    let (paths, digest) = collect_files_and_hash(root, &ignore_patterns)?;
    let artifact = format!("app-{digest}.tar.gz");
    create_artifact(root, &paths, &artifact)?;
    let size = fs::metadata(&artifact).map(|m| m.len()).unwrap_or(0);
    println!("Artifact created: {artifact} ({} bytes)", size); // user-facing
    info!(event="deploy.artifact", artifact=%artifact, size_bytes=size, sha256=%digest);
    Ok(())
}

fn is_node_project(root: &Path) -> bool { root.join("package.json").exists() }

fn maybe_run_npm_install(root:&Path) -> Result<()> {
    let npm = which::which("npm").map_err(|_| CliError::new(CliErrorKind::Runtime("npm command not found in PATH".into())))?;
    info!(event="deploy.npm_install", cmd="npm install --production");
    let status = Command::new(npm)
        .current_dir(root)
        .arg("install")
        .arg("--production")
        .status()
        .map_err(|e| CliError::with_source(CliErrorKind::Runtime("failed to spawn npm".into()), e))?;
    if !status.success() { return Err(CliError::new(CliErrorKind::Runtime(format!("npm install failed with status {status}"))).into()); }
    Ok(())
}

fn collect_files_and_hash(root:&Path, patterns:&[Pattern]) -> Result<(Vec<PathBuf>, String)> {
    let mut hasher = Sha256::new();
    let mut out = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if should_skip(path) { continue; }
        if matches_patterns(path, patterns) { continue; }
        if path.is_file() {
            match fs::read(path) {
                Ok(bytes)=> { hasher.update(&bytes); out.push(path.to_path_buf()); },
                Err(e)=> { warn!(?path, err=%e, "skip_unreadable_file"); }
            }
        }
    }
    let digest = format!("{:x}", hasher.finalize());
    Ok((out, digest))
}

fn create_artifact(root:&Path, files:&[PathBuf], artifact:&str) -> Result<()> {
    let file = fs::File::create(artifact)?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(enc);
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(f.as_path());
        builder.append_path_with_name(f, rel)?;
    }
    builder.finish()?;
    Ok(())
}

fn should_skip(p:&Path)->bool { 
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.starts_with("artifact-") || name.starts_with("app-") { return true; }
    matches!(name, ".git"|"target"|"node_modules"|".DS_Store")
}

fn load_ignore_patterns(root:&Path)->Vec<Pattern> {
    let mut out = Vec::new();
    let f = root.join(".aetherignore");
    if let Ok(content) = fs::read_to_string(f) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Ok(p)=Pattern::new(line) { out.push(p); }
        }
    }
    out
}

fn matches_patterns(p:&Path, patterns:&[Pattern])->bool { let rel:&Path = p.strip_prefix(".").unwrap_or(p); let s = rel.to_string_lossy(); patterns.iter().any(|pat| pat.matches(&s)) }
