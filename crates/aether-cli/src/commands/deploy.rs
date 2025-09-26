use anyhow::Result;use tracing::{info,warn};use std::path::Path;use sha2::{Sha256,Digest};use walkdir::WalkDir;use std::fs;use tar::Builder;use flate2::write::GzEncoder;use flate2::Compression;use glob::Pattern;

pub async fn handle(dry_run: bool) -> Result<()> {
    let root = Path::new(".");
    if !root.join("package.json").exists() { warn!("package.json not found – only NodeJS projects supported in this phase"); return Ok(()); }
    if dry_run { info!(event="deploy.dry_run", msg="Would package current project"); return Ok(()); }
    let mut hasher = Sha256::new();
    let ignore_patterns = load_ignore_patterns(root);
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let pbuf = entry.path().to_path_buf();
    if should_skip(&pbuf) { continue; }
    if matches_patterns(pbuf.as_path(), &ignore_patterns) { continue; }
    if pbuf.is_file() { if let Ok(bytes) = fs::read(&pbuf) { hasher.update(&bytes); paths.push(pbuf); } }
    }
    let digest = format!("{:x}", hasher.finalize());
    let out_file = format!("artifact-{digest}.tar.gz");
    let f = fs::File::create(&out_file)?; let enc = GzEncoder::new(f, Compression::default()); let mut tarb = Builder::new(enc);
    for p in paths { tarb.append_path_with_name(&p, p.strip_prefix(root).unwrap())?; }
    tarb.finish()?;
    info!(artifact=%out_file, sha256=%digest, "deploy.mock.packaged");
    Ok(())
}

fn should_skip(p:&std::path::Path)->bool { let name = p.file_name().and_then(|s| s.to_str()).unwrap_or(""); matches!(name, ".git"|"target"|"node_modules") }

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

fn matches_patterns(p:&Path, patterns:&[Pattern])->bool { matches_patterns_inner(p, patterns) }

fn matches_patterns_inner(p:&Path, patterns:&[Pattern])->bool { let rel:&Path = p.strip_prefix(".").unwrap_or(p); let s = rel.to_string_lossy(); patterns.iter().any(|pat| pat.matches(&s)) }
