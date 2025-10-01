use anyhow::Result;
use tracing::{info,warn};
use std::{time::{Duration, Instant}, fs, path::Path};
use sha2::{Sha256,Digest};
use crate::commands::deploy::{handle as deploy_handle, DeployOptions};
use tokio::time::sleep;

fn hash_workspace(root: &Path) -> String {
    let mut h = Sha256::new();
    fn walk(h:&mut Sha256, p:&Path) {
        if p.is_dir() {
            if let Ok(read) = fs::read_dir(p) { for e in read.flatten() { walk(h, &e.path()); } }
        } else if let Ok(meta) = fs::metadata(p) {
            if meta.is_file() {
                if let Ok(data)=fs::read(p) { h.update(p.to_string_lossy().as_bytes()); h.update(&data); }
            }
        }
    }
    walk(&mut h, root);
    format!("{:x}", h.finalize())
}

pub async fn handle(hot: bool, interval: String) -> Result<()> {
    let dur = humantime::parse_duration(&interval).unwrap_or(Duration::from_millis(500));
    let root = Path::new(".");
    if !root.join("package.json").exists() { anyhow::bail!("missing package.json"); }
    info!(hot, ?dur, "dev_loop_started");
    let mut last_digest = String::new();
    loop {
        let start_scan = Instant::now();
        let cur = hash_workspace(root);
        if cur != last_digest {
            info!(old=%last_digest, new=%cur, "change_detected_packaging");
            // Deploy with pack_only to skip installs, no_sbom for speed, dev_hot flag if hot
            match deploy_handle(DeployOptions { dry_run:false, pack_only:true, compression_level:6, out:None, no_upload:false, no_cache:true, no_sbom:true, cyclonedx:false, format:None, use_legacy_upload:false, dev_hot:hot }).await {
                Ok(()) => { last_digest = cur; }
                Err(e) => warn!(error=%e, "dev_deploy_failed"),
            }
        }
        let elapsed = start_scan.elapsed();
        if elapsed < dur { sleep(dur - elapsed).await; }
    }
}
