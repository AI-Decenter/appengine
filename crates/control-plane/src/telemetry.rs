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
