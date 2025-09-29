use crate::telemetry::{DEV_HOT_REFRESH_TOTAL, DEV_HOT_REFRESH_FAILURE_TOTAL, DEV_HOT_REFRESH_LATENCY};
use anyhow::Result;
use regex::Regex;
use std::time::Duration;
use kube::{Api, Client, api::{ListParams, LogParams}};
use k8s_openapi::api::core::v1::Pod;
use tokio::time::sleep;
use tracing::{warn, error, info};

// Simple exponential backoff with jitter; attempt starts at 1
async fn backoff_retry(attempt: u32, base: Duration, max: Duration) {
    let exp = base * 2u32.saturating_pow(attempt.min(10));
    let capped = if exp > max { max } else { exp };
    let jitter = fastrand::u64(..(capped.as_millis() as u64 / 3 + 1));
    sleep(capped + Duration::from_millis(jitter)).await;
}

pub async fn spawn_dev_hot_log_ingestion() -> Result<()> {
    if std::env::var("AETHER_DEV_HOT_INGEST").unwrap_or_default() != "1" { return Ok(()); }
    let client = Client::try_default().await?;
    tokio::spawn(async move {
        if let Err(e) = run_ingest_loop(client).await { error!(error=%e, "dev-hot ingest loop terminated"); }
    });
    Ok(())
}

async fn run_ingest_loop(client: Client) -> Result<()> {
    let namespace = std::env::var("AETHER_NAMESPACE").unwrap_or_else(|_| "default".to_string());
    let pods: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    use std::collections::{HashMap, HashSet};
    use rustc_hash::FxHasher;
    use std::hash::Hasher;
    let mut seen: HashMap<String, HashSet<u64>> = HashMap::new();
    let poll_secs: u64 = std::env::var("AETHER_DEV_HOT_INGEST_POLL_SEC").ok().and_then(|v| v.parse().ok()).unwrap_or(10).max(1);
    let mut err_attempt: u32 = 0;
    info!(namespace, poll_secs, "dev_hot_ingest_loop_started");
    loop {
        match pods.list(&ListParams::default()).await {
            Ok(list) => {
                err_attempt = 0; // reset on success
                for p in list.items {
                    let ann_ok = p.metadata.annotations.as_ref().and_then(|a| a.get("aether.dev/dev-hot")).map(|v| v=="true").unwrap_or(false);
                    if !ann_ok { continue; }
                    let Some(name) = p.metadata.name.clone() else { continue; };
                    let lp = LogParams { container: Some("fetcher".into()), tail_lines: Some(200), ..LogParams::default() };
                    match pods.logs(&name, &lp).await {
                        Ok(text) => {
                            let entry = seen.entry(name.clone()).or_default();
                            for line in text.lines() {
                                let mut hasher = FxHasher::default();
                                hasher.write(line.as_bytes());
                                let h = hasher.finish();
                                if !entry.contains(&h) {
                                    parse_and_record(&name, line);
                                    entry.insert(h);
                                    if entry.len() > 2000 { // simple cap shrink ~50%
                                        let drain: Vec<u64> = entry.iter().copied().take(1000).collect();
                                        for k in drain { entry.remove(&k); }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(pod=%name, error=%e, "fetcher_logs_failed");
                        }
                    }
                }
            }
            Err(e) => {
                err_attempt = err_attempt.saturating_add(1);
                warn!(attempt=err_attempt, error=%e, "pod_list_failed_backing_off");
                backoff_retry(err_attempt, Duration::from_millis(500), Duration::from_secs(5)).await;
                continue; // skip normal poll sleep (already backed off)
            }
        }
        sleep(Duration::from_secs(poll_secs)).await;
    }
}

fn parse_and_record(_pod: &str, line: &str) {
    if let Some(caps) = RE_OK.with(|r| r.captures(line)) { // success
        let app = caps.get(1).unwrap().as_str();
        let ms: f64 = caps.get(3).unwrap().as_str().parse::<f64>().unwrap_or(0.0);
        DEV_HOT_REFRESH_TOTAL.with_label_values(&[app]).inc();
        DEV_HOT_REFRESH_LATENCY.with_label_values(&[app]).observe(ms / 1000.0);
    } else if let Some(caps) = RE_FAIL.with(|r| r.captures(line)) {
        let app = caps.get(1).unwrap().as_str();
        let reason = caps.get(2).unwrap().as_str();
        DEV_HOT_REFRESH_FAILURE_TOTAL.with_label_values(&[app, reason]).inc();
    }
}

thread_local! {
    static RE_OK: Regex = Regex::new(r"^REFRESH_OK app=([^\s]+) digest=([0-9a-f]{64}) ms=(\d+)").unwrap();
    static RE_FAIL: Regex = Regex::new(r"^REFRESH_FAIL app=([^\s]+) reason=([A-Za-z0-9_-]+) ms=(\d+)").unwrap();
}
