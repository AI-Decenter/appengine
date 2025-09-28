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
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use atty;

#[derive(Debug, Clone, Copy)]
enum PackageManager { Npm, Yarn, Pnpm }

impl PackageManager {
    fn binary_name(self) -> &'static str { match self { PackageManager::Npm => "npm", PackageManager::Yarn => "yarn", PackageManager::Pnpm => "pnpm" } }
}

#[derive(Debug, Serialize)]
struct ManifestEntry { path: String, size: u64, sha256: String }

#[derive(Debug, Serialize)]
struct Manifest { files: Vec<ManifestEntry>, total_files: usize, total_size: u64 }

#[derive(Debug)]
pub struct DeployOptions {
    pub dry_run: bool,
    pub pack_only: bool,
    pub compression_level: u32,
    pub out: Option<String>,
    pub no_upload: bool,
    pub no_cache: bool,
    pub no_sbom: bool,
    pub format: Option<String>,
    pub use_legacy_upload: bool,
}

pub async fn handle(opts: DeployOptions) -> Result<()> {
    let DeployOptions { dry_run, pack_only, compression_level, out, no_upload, no_cache, no_sbom, format, use_legacy_upload } = opts;
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
    if !no_sbom { generate_sbom(root, &artifact_name, &manifest)?; } else { info!(event="deploy.sbom", status="skipped_no_sbom_flag"); }
    let size = fs::metadata(&artifact_name).map(|m| m.len()).unwrap_or(0);
    let digest_clone = digest.clone();
    let sig_path = artifact_name.with_file_name(format!("{}.sig", artifact_name.file_name().and_then(|s| s.to_str()).unwrap_or("artifact")));
    let sbom_path = artifact_name.with_file_name(format!("{}.sbom.json", artifact_name.file_name().and_then(|s| s.to_str()).unwrap_or("artifact")));
    let manifest_path = artifact_name.with_file_name(format!("{}.manifest.json", artifact_name.file_name().and_then(|s| s.to_str()).unwrap_or("artifact.tar.gz")));
    if format.as_deref()==Some("json") {
        #[derive(Serialize)] struct Out<'a> { artifact: &'a str, digest: &'a str, size_bytes: u64, manifest: String, sbom: Option<String>, signature: Option<String> }
        let o = Out { artifact: &artifact_name.to_string_lossy(), digest: &digest_clone, size_bytes: size, manifest: manifest_path.to_string_lossy().to_string(), sbom: if no_sbom { None } else { Some(sbom_path.to_string_lossy().to_string()) }, signature: sig_path.exists().then(|| sig_path.to_string_lossy().to_string()) };
        println!("{}", serde_json::to_string_pretty(&o)?);
    } else {
        println!("Artifact created: {} ({} bytes)", artifact_name.display(), size); // user-facing
    }
    info!(event="deploy.artifact", artifact=%artifact_name.display(), size_bytes=size, sha256=%digest, files=%manifest.total_files);
    maybe_sign(&artifact_name, &digest)?;

    if !no_upload {
        if let Ok(base) = std::env::var("AETHER_API_BASE") {
            let upload_res = if use_legacy_upload { legacy_upload(&artifact_name, root, &base, &digest, sig_path.exists().then(|| sig_path.clone())).await } else { two_phase_upload(&artifact_name, root, &base, &digest, sig_path.exists().then(|| sig_path.clone())).await };
            match upload_res {
                Ok(url)=> info!(event="deploy.upload", mode= if use_legacy_upload {"legacy"} else {"two_phase"}, base=%base, artifact=%artifact_name.display(), status="ok", returned_url=%url),
                Err(e)=> { return Err(e); }
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

async fn legacy_upload(artifact:&Path, root:&Path, base:&str, digest:&str, sig: Option<PathBuf>) -> Result<String> {
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
    let mut req = client.post(&url).multipart(form).header("X-Aether-Artifact-Digest", digest);
    if let Some(sig_path) = sig {
        if let Ok(content) = fs::read_to_string(&sig_path) { req = req.header("X-Aether-Signature", content.trim()); }
    }
    let resp = req.send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("upload request failed".into()), e))?;
    if !resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("upload failed status {}", resp.status()))).into()); }
    let v: serde_json::Value = resp.json().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid upload response".into()), e))?;
    let artifact_url = v.get("artifact_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let dep_body = serde_json::json!({"app_name": app_name, "artifact_url": artifact_url});
    let dep_url = format!("{}/deployments", base.trim_end_matches('/'));
    let _ = client.post(&dep_url).json(&dep_body).send().await; // ignore error
    Ok(artifact_url)
}

// real_upload removed: migration complete; use two_phase_upload unless --legacy-upload provided.

async fn two_phase_upload(artifact:&Path, root:&Path, base:&str, digest:&str, sig: Option<PathBuf>) -> Result<String> {
    let pkg = parse_package_json(root);
    let app_name = pkg.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| "default-app".into());
    let client = reqwest::Client::new();
    let presign_url = format!("{}/artifacts/presign", base.trim_end_matches('/'));
    let presign_body = serde_json::json!({"app_name": app_name, "digest": digest});
    // Decide between single PUT and multipart based on size threshold env var
    let meta = fs::metadata(artifact)?; let len = meta.len();
    let threshold = std::env::var("AETHER_MULTIPART_THRESHOLD_BYTES").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(u64::MAX);
    if len >= threshold && threshold>0 {
        return multipart_upload(artifact, root, base, digest, sig).await;
    }
    let presign_resp = client.post(&presign_url).json(&presign_body).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("presign request failed".into()), e))?;
    if !presign_resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("presign status {}", presign_resp.status()))).into()); }
    let presign_json: serde_json::Value = presign_resp.json().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid presign response".into()), e))?;
    let method = presign_json.get("method").and_then(|m| m.as_str()).unwrap_or("NONE");
    let storage_key = presign_json.get("storage_key").and_then(|m| m.as_str()).unwrap_or("").to_string();
    if method == "PUT" {
        let upload_url = presign_json.get("upload_url").and_then(|u| u.as_str()).ok_or_else(|| CliError::new(CliErrorKind::Runtime("missing upload_url".into())))?;
        // Upload artifact via PUT with optional progress bar
        let meta = fs::metadata(artifact)?; let len = meta.len();
        let mut put_req = client.put(upload_url);
        if let Some(hdrs) = presign_json.get("headers").and_then(|h| h.as_object()) {
            for (k,v) in hdrs.iter() { if let Some(val)=v.as_str() { put_req = put_req.header(k, val); } }
        }
        let use_progress = atty::is(atty::Stream::Stderr) && len > 512 * 1024; // show for larger files in tty
        let pb = if use_progress { let pb = ProgressBar::new(len); pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap().progress_chars("=>-")); Some(pb) } else { None };
        let start_put = Instant::now();
        if len <= 512 * 1024 { // buffer
            let bytes = tokio::fs::read(artifact).await.map_err(|e| CliError::with_source(CliErrorKind::Io("read artifact".into()), e))?;
            if let Some(pb) = &pb { pb.inc(len); pb.finish_and_clear(); }
            put_req = put_req.body(bytes);
        } else {
            use tokio::io::AsyncReadExt;
            let mut file = tokio::fs::File::open(artifact).await.map_err(|e| CliError::with_source(CliErrorKind::Io("open artifact".into()), e))?;
            let mut sent: u64 = 0; let mut buf = vec![0u8; 128 * 1024];
            // We build a streaming body manually updating progress
            let stream = async_stream::stream! {
                loop {
                    match file.read(&mut buf).await { Ok(0) => break, Ok(n) => { sent += n as u64; if let Some(pb)=&pb { pb.set_position(sent); } yield Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(&buf[..n])); }, Err(_e)=> { break; } }
                }
                if let Some(pb)=&pb { pb.finish_and_clear(); }
            };
            put_req = put_req.body(reqwest::Body::wrap_stream(stream));
        }
        let put_resp = put_req.send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("PUT upload failed".into()), e))?;
        if !put_resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("PUT status {}", put_resp.status()))).into()); }
        let put_duration = start_put.elapsed().as_secs_f64();
        // Complete step
        let size_bytes = fs::metadata(artifact).map(|m| m.len() as i64).unwrap_or(0);
        let signature_hex = if let Some(sig_path) = sig { fs::read_to_string(sig_path).ok().map(|s| s.trim().to_string()) } else { None };
        let complete_url = format!("{}/artifacts/complete", base.trim_end_matches('/'));
        let idempotency_key = format!("idem-{}", digest);
        let complete_body = serde_json::json!({"app_name": app_name, "digest": digest, "size_bytes": size_bytes, "signature": signature_hex, "idempotency_key": idempotency_key});
        let comp_resp = client.post(&complete_url).header("X-Aether-Upload-Duration", format!("{:.6}", put_duration)).json(&complete_body).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("complete request failed".into()), e))?;
        if !comp_resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("complete status {}", comp_resp.status()))).into()); }
        // Optionally create deployment referencing storage key
        let dep_body = serde_json::json!({"app_name": app_name, "artifact_url": storage_key});
        let dep_url = format!("{}/deployments", base.trim_end_matches('/'));
        let _ = client.post(&dep_url).json(&dep_body).send().await; // ignore error
        return Ok(storage_key);
    }
    // Already stored (method NONE) -> create deployment pointing to storage_key
    if method == "NONE" {
        let dep_body = serde_json::json!({"app_name": app_name, "artifact_url": storage_key});
        let dep_url = format!("{}/deployments", base.trim_end_matches('/'));
        let _ = client.post(&dep_url).json(&dep_body).send().await; // ignore error
        return Ok(storage_key);
    }
    Err(CliError::new(CliErrorKind::Runtime("unsupported presign method".into())).into())
}

async fn multipart_upload(artifact:&Path, root:&Path, base:&str, digest:&str, sig: Option<PathBuf>) -> Result<String> {
    let client = reqwest::Client::new();
    let pkg = parse_package_json(root);
    let app_name = pkg.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| "default-app".into());
    // init
    let init_url = format!("{}/artifacts/multipart/init", base.trim_end_matches('/'));
    let init_body = serde_json::json!({"app_name": app_name, "digest": digest});
    let init_resp = client.post(&init_url).json(&init_body).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("multipart init failed".into()), e))?;
    if !init_resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("multipart init status {}", init_resp.status()))).into()); }
    let init_json: serde_json::Value = init_resp.json().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid init response".into()), e))?;
    let upload_id = init_json.get("upload_id").and_then(|v| v.as_str()).ok_or_else(|| CliError::new(CliErrorKind::Runtime("missing upload_id".into())))?.to_string();
    let storage_key = init_json.get("storage_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // chunk file
    let part_size = std::env::var("AETHER_MULTIPART_PART_SIZE_BYTES").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(8*1024*1024);
    let meta = fs::metadata(artifact)?; let total = meta.len();
    let use_progress = atty::is(atty::Stream::Stderr) && total > part_size;
    let pb = if use_progress { let pb = ProgressBar::new(total); pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()); Some(pb) } else { None };
    let mut file = tokio::fs::File::open(artifact).await.map_err(|e| CliError::with_source(CliErrorKind::Io("open artifact".into()), e))?;
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; part_size as usize];
    let mut part_number: i32 = 1; let mut parts: Vec<(i32,String)> = Vec::new(); let start_all = Instant::now();
    loop {
        let mut read: usize = 0;
        while read < part_size as usize {
            match file.read(&mut buf[read..(part_size as usize)]).await { Ok(0)=> break, Ok(n)=> { read += n; if read >= part_size as usize { break; } }, Err(e)=> return Err(CliError::with_source(CliErrorKind::Io("read artifact".into()), e).into()) }
        }
        if read==0 { break; }
        let presign_part_url = format!("{}/artifacts/multipart/presign-part", base.trim_end_matches('/'));
        let presign_part_body = serde_json::json!({"digest": digest, "upload_id": upload_id, "part_number": part_number});
        let part_resp = client.post(&presign_part_url).json(&presign_part_body).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("presign part failed".into()), e))?;
        if !part_resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("presign part status {}", part_resp.status()))).into()); }
        let part_json: serde_json::Value = part_resp.json().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("invalid part response".into()), e))?;
        let url = part_json.get("url").and_then(|v| v.as_str()).ok_or_else(|| CliError::new(CliErrorKind::Runtime("missing part url".into())))?;
        let mut put_req = client.put(url);
        if let Some(hdrs) = part_json.get("headers").and_then(|h| h.as_object()) { for (k,v) in hdrs.iter() { if let Some(val)=v.as_str() { put_req = put_req.header(k, val); } } }
        let body_slice = &buf[..read];
        put_req = put_req.body(body_slice.to_vec());
        let resp = put_req.send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("part upload failed".into()), e))?;
        if !resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("part upload status {}", resp.status()))).into()); }
        let etag = resp.headers().get("ETag").and_then(|v| v.to_str().ok()).unwrap_or("").trim_matches('"').to_string();
        parts.push((part_number, etag));
        if let Some(pb)=&pb { pb.inc(read as u64); }
        part_number +=1;
    }
    if let Some(pb)=&pb { pb.finish_and_clear(); }
    let duration = start_all.elapsed().as_secs_f64();
    // complete
    let signature_hex = if let Some(sig_path) = sig { fs::read_to_string(sig_path).ok().map(|s| s.trim().to_string()) } else { None };
    let complete_url = format!("{}/artifacts/multipart/complete", base.trim_end_matches('/'));
    let idempotency_key = format!("idem-{}", digest);
    let parts_json: Vec<serde_json::Value> = parts.iter().map(|(n,e)| serde_json::json!({"part_number": n, "etag": e})).collect();
    let complete_body = serde_json::json!({"app_name": app_name, "digest": digest, "upload_id": upload_id, "size_bytes": fs::metadata(artifact).map(|m| m.len()).unwrap_or(0) as i64, "parts": parts_json, "signature": signature_hex, "idempotency_key": idempotency_key});
    let resp = client.post(&complete_url).header("X-Aether-Upload-Duration", format!("{:.6}", duration)).json(&complete_body).send().await.map_err(|e| CliError::with_source(CliErrorKind::Runtime("multipart complete failed".into()), e))?;
    if !resp.status().is_success() { return Err(CliError::new(CliErrorKind::Runtime(format!("multipart complete status {}", resp.status()))).into()); }
    Ok(storage_key)
}

// Benchmark helper (not part of public CLI API) kept always available for benches
#[allow(dead_code)]
pub fn collect_for_bench(root:&Path, patterns:&[glob::Pattern]) -> (Vec<PathBuf>, String, usize) {
    let (p,d,m) = collect_files_hash_and_manifest(root, patterns).expect("collect ok");
    (p,d,m.total_files)
}
