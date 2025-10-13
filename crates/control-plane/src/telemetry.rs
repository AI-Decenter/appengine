pub static DEPLOYMENT_STATUS: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("deployment_status_total", "Deployment status transitions"), &["status"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static DEPLOYMENT_TIME_TO_RUNNING: Lazy<prometheus::Histogram> = Lazy::new(|| {
    let h = prometheus::Histogram::with_opts(histogram_opts!("deployment_time_to_running_seconds", "Time from creation to running")).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok();
    h
});
use prometheus::{TextEncoder, Encoder, Registry, IntCounterVec, HistogramVec, IntGauge, opts, histogram_opts};
use once_cell::sync::Lazy;
use axum::{response::IntoResponse, http::StatusCode};

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);
pub static HTTP_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    // Added outcome label (success/error) to separate traffic patterns
    let c = IntCounterVec::new(opts!("http_requests_total", "HTTP request count"), &["method", "path", "status", "outcome"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static HTTP_REQUEST_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    let h = HistogramVec::new(histogram_opts!(
        "http_request_duration_seconds",
        "HTTP request duration seconds"
    ), &["method","path"]).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok();
    h
});

pub static DB_POOL_SIZE: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new("db_pool_size", "Total connections in the DB pool").unwrap();
    REGISTRY.register(Box::new(g.clone())).ok();
    g
});
pub static DB_POOL_IDLE: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new("db_pool_idle", "Idle connections in the DB pool").unwrap();
    REGISTRY.register(Box::new(g.clone())).ok();
    g
});
pub static DB_POOL_IN_USE: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new("db_pool_in_use", "In-use (checked out) connections in the DB pool").unwrap();
    REGISTRY.register(Box::new(g.clone())).ok();
    g
});
pub static RUNNING_DEPLOYMENTS: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new("deployments_running_total", "Current number of running deployments").unwrap();
    REGISTRY.register(Box::new(g.clone())).ok();
    g
});
pub static ARTIFACT_VERIFY_FAILURE_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("artifact_verify_failure_total", "Artifact / signature verification failures"), &["app","reason"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

// Dev hot mode metrics (Issue 05 follow-ups)
// Build metadata label (commit sha) if provided at build time via env! macro fallback to "unknown"
pub fn build_commit() -> &'static str { option_env!("GIT_COMMIT_SHA").unwrap_or("unknown") }
pub static DEV_HOT_REFRESH_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("dev_hot_refresh_total", "Successful dev-hot refreshes"), &["app","commit"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static DEV_HOT_REFRESH_FAILURE_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("dev_hot_refresh_failure_total", "Failed dev-hot refresh attempts"), &["app","reason","commit"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static DEV_HOT_REFRESH_LATENCY: Lazy<prometheus::HistogramVec> = Lazy::new(|| {
    let h = prometheus::HistogramVec::new(histogram_opts!("dev_hot_refresh_latency_seconds","Time to download and extract new artifact"), &["app","commit"]).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok();
    h
});
pub static DEV_HOT_REFRESH_CONSEC_FAIL: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new("dev_hot_refresh_consecutive_failures", "Consecutive dev-hot refresh failures (per observed app) aggregated latest")
        .unwrap();
    REGISTRY.register(Box::new(g.clone())).ok();
    g
});
pub static DEV_HOT_SIGNATURE_FAIL_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("dev_hot_signature_fail_total", "Dev-hot signature verification failures"), &["app","commit"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static ATTESTATION_SIGNED_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("attestation_signed_total", "DSSE attestations successfully signed"), &["app"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static PROVENANCE_EMITTED_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("provenance_emitted_total", "Provenance documents written"), &["app"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});
pub static PROVENANCE_WAIT_TIME: Lazy<prometheus::Histogram> = Lazy::new(|| {
    let h = prometheus::Histogram::with_opts(histogram_opts!("provenance_wait_time_seconds", "Time spent waiting for provenance (enforced mode)")).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok();
    h
});
pub static SBOM_INVALID_TOTAL: Lazy<prometheus::IntCounter> = Lazy::new(|| {
    let c = prometheus::IntCounter::new("sbom_invalid_total", "Total invalid or mismatched SBOM uploads").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

// Coverage metrics gauges (updated periodically elsewhere)
pub static ARTIFACTS_WITH_SBOM: Lazy<IntGauge> = Lazy::new(|| { let g = IntGauge::new("artifacts_with_sbom_total", "Artifacts having an SBOM").unwrap(); REGISTRY.register(Box::new(g.clone())).ok(); g });
pub static ARTIFACTS_WITH_PROVENANCE: Lazy<IntGauge> = Lazy::new(|| { let g = IntGauge::new("artifacts_with_provenance_total", "Artifacts having provenance v2 doc").unwrap(); REGISTRY.register(Box::new(g.clone())).ok(); g });
pub static ARTIFACTS_SIGNED: Lazy<IntGauge> = Lazy::new(|| { let g = IntGauge::new("artifacts_signed_total", "Artifacts with signature present").unwrap(); REGISTRY.register(Box::new(g.clone())).ok(); g });

pub fn normalize_path(raw: &str) -> String {
    // Broader normalization:
    // - Replace UUID segments with :id
    // - Replace purely numeric segments with :id
    // - Special-case app name position in /apps/{app}/...
    let uuid_like = |s: &str| s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4;
    let mut parts: Vec<String> = raw.split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            if seg.is_empty() { return seg.to_string(); }
            if seg.chars().all(|c| c.is_ascii_digit()) || uuid_like(seg) { return ":id".to_string(); }
            seg.to_string()
        }).collect();
    if parts.len() >= 2 && parts[0] == "apps" { parts[1] = ":app_name".into(); }
    if parts.is_empty() { return raw.to_string(); }
    format!("/{}", parts.join("/"))
}

pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buf = Vec::new();
    if encoder.encode(&metric_families, &mut buf).is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    ([("Content-Type","text/plain; version=0.0.4")], buf).into_response()
}
