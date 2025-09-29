use crate::telemetry::{DEV_HOT_REFRESH_TOTAL, DEV_HOT_REFRESH_FAILURE_TOTAL, DEV_HOT_REFRESH_LATENCY};
use anyhow::Result;
use regex::Regex;
use std::time::Duration;
use kube::{Api, Client, api::{ListParams, LogParams}};
use k8s_openapi::api::core::v1::Pod;
use tokio::time::sleep;
use tracing::{warn, error};

// Simple exponential backoff with jitter
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
    let mut seen: HashMap<String, HashSet<u64>> = HashMap::new();
    loop {
        match pods.list(&ListParams::default()).await {
            Ok(list) => {
                for p in list.items {
                    let ann_ok = p.metadata.annotations.as_ref().and_then(|a| a.get("aether.dev/dev-hot")).map(|v| v=="true").unwrap_or(false);
                    if !ann_ok { continue; }
                    let Some(name) = p.metadata.name.clone() else { continue; };
                    let lp = LogParams { container: Some("fetcher".into()), tail_lines: Some(200), ..LogParams::default() };
                    if let Ok(text) = pods.logs(&name, &lp).await {
                        let entry = seen.entry(name.clone()).or_insert_with(HashSet::new);
                        for line in text.lines() { let h = fxhash::hash64(line.as_bytes()); if !entry.contains(&h) { parse_and_record(&name, line); entry.insert(h); if entry.len()>2000 { // simple cap
                                    // shrink
                                    let drain: Vec<u64> = entry.iter().copied().take(1000).collect(); // leave half (approx)
                                    for k in drain { entry.remove(&k); }
                                } } }
                    }
                }
            }
            Err(e) => { warn!(error=%e, "pod list failed"); }
        }
        sleep(Duration::from_secs(10)).await;
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
