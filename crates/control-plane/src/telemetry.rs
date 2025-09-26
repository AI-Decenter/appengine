use prometheus::{TextEncoder, Encoder, Registry, IntCounterVec, HistogramVec, opts, histogram_opts};
use once_cell::sync::Lazy;
use axum::{response::IntoResponse, http::StatusCode};

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);
pub static HTTP_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("http_requests_total", "HTTP request count"), &["method", "path", "status"]).unwrap();
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

pub fn normalize_path(raw: &str) -> String {
    // Simple normalization: collapse dynamic app name in /apps/{name}/...
    // Known patterns: /apps/{app_name}/deployments, /apps/{app_name}/logs
    let parts: Vec<&str> = raw.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 3 && parts[0] == "apps" && (parts[2] == "deployments" || parts[2] == "logs") {
        return format!("/apps/:app_name/{}", parts[2]);
    }
    raw.to_string()
}

pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buf = Vec::new();
    if encoder.encode(&metric_families, &mut buf).is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    ([("Content-Type","text/plain; version=0.0.4")], buf).into_response()
}
