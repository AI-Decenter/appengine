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
use serde::{Serialize,Deserialize};
use std::io::Read;
use tokio_util::io::ReaderStream;

#[derive(Debug, Clone, Copy)]
enum PackageManager { Npm, Yarn, Pnpm }

impl PackageManager {
    fn binary_name(self) -> &'static str { match self { PackageManager::Npm => "npm", PackageManager::Yarn => "yarn", PackageManager::Pnpm => "pnpm" } }
}

#[derive(Debug, Serialize)]
struct ManifestEntry { path: String, size: u64, sha256: String }

#[derive(Debug, Serialize)]
struct Manifest { files: Vec<ManifestEntry>, total_files: usize, total_size: u64 }

pub async fn handle(dry_run: bool, pack_only: bool, compression_level: u32, out: Option<String>, no_upload: bool, no_cache: bool) -> Result<()> {
    let root = Path::new(".");
    if !is_node_project(root) { return Err(CliError::new(CliErrorKind::Usage("not a NodeJS project (missing package.json)".into())).into()); }
    if dry_run { info!(event="deploy.dry_run", msg="Would run install + prune + package project"); return Ok(()); }

    // Only detect and use a package manager when we actually need to install/prune.
    if !pack_only {
        let pm = detect_package_manager(root)?; // choose manager
        install_dependencies(root, pm, no_cache)?;
        prune_dependencies(root, pm)?;
    } else {
        info!(event="deploy.pack_only", msg="Skipping dependency install (pack-only mode) and package manager detection");
    }

    let mut ignore_patterns = load_ignore_patterns(root);
    append_gitignore_patterns(root, &mut ignore_patterns);
    let (paths, digest, manifest) = collect_files_hash_and_manifest(root, &ignore_patterns)?;

    let artifact_name = match &out {
        Some(p) => {
            let candidate = PathBuf::from(p);
            if candidate.is_dir() {
                candidate.join(format!("app-{digest}.tar.gz"))
            } else if p.ends_with('/') {
                PathBuf::from(format!("{p}app-{digest}.tar.gz"))
            } else {
                candidate
            }
        }
        None => PathBuf::from(format!("app-{digest}.tar.gz")),
    };

    create_artifact(root, &paths, &artifact_name, compression_level)?;
    write_manifest(&artifact_name, &manifest)?;
    generate_sbom(root, &artifact_name, &manifest)?;
    let size = fs::metadata(&artifact_name).map(|m| m.len()).unwrap_or(0);
    println!("Artifact created: {} ({} bytes)", artifact_name.display(), size); // user-facing
    info!(event="deploy.artifact", artifact=%artifact_name.display(), size_bytes=size, sha256=%digest, files=%manifest.total_files);
    maybe_sign(&artifact_name, &digest)?;

    if !no_upload {
        if let Ok(base) = std::env::var("AETHER_API_BASE") {
            match real_upload(&artifact_name, root, &base).await {
                Ok(url)=> info!(event="deploy.upload", base=%base, artifact=%artifact_name.display(), status="ok", returned_url=%url),
                Err(e)=> info!(event="deploy.upload", base=%base, artifact=%artifact_name.display(), status="error", err=%e)
            }
        } else { info!(event="deploy.upload", status="skipped_missing_env"); }
    } else { info!(event="deploy.upload", status="disabled_by_flag"); }
    Ok(())
}

fn is_node_project(root: &Path) -> bool { root.join("package.json").exists() }

fn detect_package_manager(root:&Path) -> Result<PackageManager> {
    // priority: pnpm, yarn, npm (lockfiles)
    if root.join("pnpm-lock.yaml").exists() && which::which("pnpm").is_ok() { return Ok(PackageManager::Pnpm); }
    if root.join("yarn.lock").exists() && which::which("yarn").is_ok() { return Ok(PackageManager::Yarn); }
    if root.join("package-lock.json").exists() && which::which("npm").is_ok() { return Ok(PackageManager::Npm); }
    // fallback to npm
    if which::which("npm").is_ok() { return Ok(PackageManager::Npm); }
    Err(CliError::new(CliErrorKind::Runtime("no supported package manager found (need npm|yarn|pnpm)".into())).into())
}

fn install_dependencies(root:&Path, pm:PackageManager, no_cache: bool) -> Result<()> {
    let bin = which::which(pm.binary_name()).map_err(|_| CliError::new(CliErrorKind::Runtime(format!("{} not found in PATH", pm.binary_name()))))?;
    if !no_cache { restore_cache(root, pm); }
    match pm {
        PackageManager::Npm => {
            info!(event="deploy.install", pm="npm", cmd="npm install --production");
            let status = Command::new(&bin).current_dir(root).arg("install").arg("--production").status()
                .map_err(|e| CliError::with_source(CliErrorKind::Runtime("failed to spawn npm".into()), e))?;
            if !status.success() { return Err(CliError::new(CliErrorKind::Runtime(format!("npm install failed with status {status}"))).into()); }
        }
        PackageManager::Yarn => {
            info!(event="deploy.install", pm="yarn", cmd="yarn install --production");
            let status = Command::new(&bin).current_dir(root).arg("install").arg("--production").status()
                .map_err(|e| CliError::with_source(CliErrorKind::Runtime("failed to spawn yarn".into()), e))?;
            if !status.success() { return Err(CliError::new(CliErrorKind::Runtime(format!("yarn install failed with status {status}"))).into()); }
        }
        PackageManager::Pnpm => {
            info!(event="deploy.install", pm="pnpm", cmd="pnpm install --prod");
            let status = Command::new(&bin).current_dir(root).arg("install").arg("--prod").status()
                .map_err(|e| CliError::with_source(CliErrorKind::Runtime("failed to spawn pnpm".into()), e))?;
            if !status.success() { return Err(CliError::new(CliErrorKind::Runtime(format!("pnpm install failed with status {status}"))).into()); }
        }
    }
    if !no_cache { save_cache(root, pm); }
    Ok(())
}

fn prune_dependencies(root:&Path, pm:PackageManager) -> Result<()> {
    let bin = which::which(pm.binary_name()).map_err(|_| CliError::new(CliErrorKind::Runtime(format!("{} not found", pm.binary_name()))))?;
    let status = match pm {
        PackageManager::Npm => Command::new(bin).current_dir(root).arg("prune").arg("--production").status(),
        PackageManager::Yarn => return Ok(()), // yarn handles prod filtering by default
        PackageManager::Pnpm => return Ok(()), // pnpm similar
    }.map_err(|e| CliError::with_source(CliErrorKind::Runtime("failed to spawn prune".into()), e))?;
    if !status.success() { warn!(event="deploy.prune_failed", status=?status); }
    Ok(())
}

fn cache_key(root:&Path, pm:PackageManager) -> Option<String> {
    let lockfile = match pm { PackageManager::Npm => "package-lock.json", PackageManager::Yarn => "yarn.lock", PackageManager::Pnpm => "pnpm-lock.yaml" };
    let path = root.join(lockfile);
    let content = fs::read(&path).ok()?;
    let mut h = Sha256::new(); h.update(&content);
    if let Ok(node_v) = std::env::var("NODE_VERSION") { h.update(node_v.as_bytes()); }
    Some(format!("{:x}", h.finalize()))
}

fn cache_dir_for(root:&Path, pm:PackageManager) -> Option<PathBuf> {
    let key = cache_key(root, pm)?;
    let mut base = dirs::cache_dir()?; base.push("aether"); base.push("node_modules"); base.push(key); Some(base)
}

fn restore_cache(root:&Path, pm:PackageManager) {
    if root.join("node_modules").exists() { return; }
    if let Some(dir) = cache_dir_for(root, pm) { if dir.exists() { info!(event="deploy.cache.restore", path=%dir.display()); copy_dir(&dir, &root.join("node_modules")); } }
}

fn save_cache(root:&Path, pm:PackageManager) {
    let nm = root.join("node_modules");
    if !nm.exists() { return; }
    if let Some(dir) = cache_dir_for(root, pm) {
        if dir.exists() { return; }
        if let Some(parent) = dir.parent() {
            let _ = fs::create_dir_all(parent);
        }
        info!(event="deploy.cache.save", path=%dir.display());
        copy_dir(&nm, &dir);
    }
}

fn copy_dir(src:&Path, dst:&Path) {
    if let Err(e)= (|| -> Result<()> { if !dst.exists() { fs::create_dir_all(dst)?; }
        for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) { let p = entry.path(); if p.is_file() { let rel = p.strip_prefix(src).unwrap(); let target = dst.join(rel); if let Some(parent)=target.parent(){ fs::create_dir_all(parent)?; } fs::copy(p, target)?; } }
        Ok(()) })() { warn!(event="deploy.cache.copy_failed", err=%e); }
}

fn collect_files_hash_and_manifest(root:&Path, patterns:&[Pattern]) -> Result<(Vec<PathBuf>, String, Manifest)> {
    let mut global = Sha256::new();
    let mut files = Vec::new();
    let mut entries = Vec::new();
    let mut total_size = 0u64;
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if should_skip(path) { continue; }
        if matches_patterns(path, patterns) { continue; }
        if path.is_file() {
            match fs::File::open(path) {
                Ok(mut f)=> {
                    let mut buf = [0u8; 65536];
                    let mut file_hasher = Sha256::new();
                    let mut size = 0u64;
                    loop {
                        match f.read(&mut buf) { Ok(0)=> break, Ok(n)=> { file_hasher.update(&buf[..n]); global.update(&buf[..n]); size += n as u64; }, Err(e)=> { warn!(?path, err=%e, "skip_unreadable_file"); break; } }
                    }
                    let sha = format!("{:x}", file_hasher.finalize());
                    let rel = path.strip_prefix(root).unwrap_or(path); let rel_s = rel.to_string_lossy().to_string();
                    entries.push(ManifestEntry { path: rel_s, size, sha256: sha });
                    total_size += size;
                    files.push(path.to_path_buf());
                },
                Err(e)=> { warn!(?path, err=%e, "skip_open_file"); }
            }
        }
    }
    let digest = format!("{:x}", global.finalize());
    let manifest = Manifest { total_files: entries.len(), total_size, files: entries };
    Ok((files, digest, manifest))
}

fn create_artifact(root:&Path, files:&[PathBuf], artifact:&Path, compression_level:u32) -> Result<()> {
    let level = if (1..=9).contains(&compression_level) { Compression::new(compression_level) } else { Compression::default() };
    if let Some(parent)=artifact.parent() { if !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; } }
    let file = fs::File::create(artifact)?;
    let enc = GzEncoder::new(file, level);
    let mut builder = Builder::new(enc);
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(f.as_path());
        builder.append_path_with_name(f, rel)?;
    }
    let enc = builder.into_inner()?; // finish tar writing
    enc.finish()?; // finish gzip
    Ok(())
}

fn write_manifest(artifact:&Path, manifest:&Manifest) -> Result<()> {
    let manifest_path = artifact.with_file_name(format!("{}.manifest.json", artifact.file_name().and_then(|s| s.to_str()).unwrap_or("artifact.tar.gz")));
    let data = serde_json::to_vec_pretty(manifest)?;
    fs::write(&manifest_path, data)?;
    info!(event="deploy.manifest", path=%manifest_path.display(), files=manifest.total_files, total_size=manifest.total_size);
    Ok(())
}

fn should_skip(p:&Path)->bool { 
    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.starts_with("artifact-") || name.starts_with("app-") { return true; }
    matches!(name, ".git"|"target"|"node_modules"|".DS_Store")
}

fn load_ignore_patterns(root:&Path)->Vec<Pattern> { load_patterns_file(root.join(".aetherignore")) }

fn append_gitignore_patterns(root:&Path, out:&mut Vec<Pattern>) { for p in load_patterns_file(root.join(".gitignore")) { out.push(p); } }

fn load_patterns_file(path:PathBuf)->Vec<Pattern> {
    let mut out = Vec::new();
    if let Ok(content) = fs::read_to_string(&path) {
        for (idx,line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            match Pattern::new(line) { Ok(p)=> out.push(p), Err(e)=> warn!(file=%path.display(), line=idx+1, err=%e, pattern=%line, "invalid_ignore_pattern") }
        }
    }
    out
}

fn matches_patterns(p:&Path, patterns:&[Pattern])->bool { let rel:&Path = p.strip_prefix(".").unwrap_or(p); let s = rel.to_string_lossy(); patterns.iter().any(|pat| pat.matches(&s)) }

#[derive(Deserialize)]
struct PackageJson { name: Option<String>, version: Option<String>, #[serde(default)] dependencies: Option<serde_json::Map<String, serde_json::Value>> }

fn parse_package_json(root:&Path)->Option<PackageJson> {
    let content = fs::read_to_string(root.join("package.json")).ok()?;
    serde_json::from_str(&content).ok()
}

fn generate_sbom(root:&Path, artifact:&Path, manifest:&Manifest) -> Result<()> {
    let pkg = parse_package_json(root);
    #[derive(Serialize)] struct Dependency<'a> { name: &'a str, spec: String }
    #[derive(Serialize)] struct Sbom<'a> {
        schema: &'a str,
        package: Option<String>,
        version: Option<String>,
        total_files: usize,
        total_size: u64,
        manifest_digest: String,
        files: &'a [ManifestEntry],
        dependencies: Vec<Dependency<'a>>,
    }
    let mut deps = Vec::new();
    if let Some(map) = pkg.as_ref().and_then(|p| p.dependencies.as_ref()) {
        for (k,v) in map.iter() { if let Some(spec)=v.as_str() { deps.push(Dependency { name: k, spec: spec.to_string() }); } }
    }
    let mut h = Sha256::new();
    for f in &manifest.files { h.update(f.path.as_bytes()); h.update(f.sha256.as_bytes()); }
    let manifest_digest = format!("{:x}", h.finalize());
    let sbom = Sbom { schema: "aether-sbom-v1", package: pkg.as_ref().and_then(|p| p.name.clone()), version: pkg.as_ref().and_then(|p| p.version.clone()), total_files: manifest.total_files, total_size: manifest.total_size, manifest_digest, files: &manifest.files, dependencies: deps };
    let path = artifact.with_file_name(format!("{}.sbom.json", artifact.file_name().and_then(|s| s.to_str()).unwrap_or("artifact")));
    fs::write(&path, serde_json::to_vec_pretty(&sbom)?)?;
    info!(event="deploy.sbom", path=%path.display(), files=manifest.total_files);
    Ok(())
}

fn maybe_sign(artifact:&Path, digest:&str) -> Result<()> {
    if let Ok(hex_key) = std::env::var("AETHER_SIGNING_KEY") {
        use ed25519_dalek::{SigningKey,Signer};
        let bytes = hex::decode(hex_key.trim()).map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid signing key".into()), e))?;
        if bytes.len()!=32 { return Err(CliError::new(CliErrorKind::Runtime("signing key must be 32 bytes".into())).into()); }
        let sk = SigningKey::from_bytes(&bytes.try_into().unwrap());
        let sig = sk.sign(digest.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        let sig_path = artifact.with_file_name(format!("{}.sig", artifact.file_name().and_then(|s| s.to_str()).unwrap_or("artifact")));
        fs::write(&sig_path, sig_hex)?;
        info!(event="deploy.sign", path=%sig_path.display());
    } else {
        info!(event="deploy.sign", status="skipped_no_key");
    }
    Ok(())
}

async fn real_upload(artifact:&Path, root:&Path, base:&str) -> Result<String> {
    let pkg = parse_package_json(root);
    let app_name = pkg.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| "default-app".into());
    let client = reqwest::Client::new();
    let meta = fs::metadata(artifact)?; let len = meta.len();
    let file_name = artifact.file_name().unwrap().to_string_lossy().to_string();
    let part = if len <= 512 * 1024 { // small -> buffer
        let bytes = tokio::fs::read(artifact).await.map_err(|e| CliError::with_source(CliErrorKind::Io("read artifact".into()), e))?;
        reqwest::multipart::Part::bytes(bytes).file_name(file_name).mime_str("application/gzip").unwrap()
    } else {
        let file = tokio::fs::File::open(artifact).await.map_err(|e| CliError::with_source(CliErrorKind::Io("open artifact".into()), e))?;
        let stream = ReaderStream::new(file);
        reqwest::multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), len)
            .file_name(file_name)
            .mime_str("application/gzip").unwrap()
    };
    let form = reqwest::multipart::Form::new().text("app_name", app_name.clone()).part("artifact", part);
    let url = format!("{}/artifacts", base.trim_end_matches('/'));
    let resp = client.post(&url).multipart(form).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("upload request failed".into()), e))?;
    if !resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("upload failed status {}", resp.status()))).into()); }
    let v: serde_json::Value = resp.json().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid upload response".into()), e))?;
    let artifact_url = v.get("artifact_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let dep_body = serde_json::json!({"app_name": app_name, "artifact_url": artifact_url});
    let dep_url = format!("{}/deployments", base.trim_end_matches('/'));
    let _ = client.post(&dep_url).json(&dep_body).send().await; // ignore error
    Ok(artifact_url)
}

// Benchmark helper (not part of public CLI API) kept always available for benches
#[allow(dead_code)]
pub fn collect_for_bench(root:&Path, patterns:&[glob::Pattern]) -> (Vec<PathBuf>, String, usize) {
    let (p,d,m) = collect_files_hash_and_manifest(root, patterns).expect("collect ok");
    (p,d,m.total_files)
}
