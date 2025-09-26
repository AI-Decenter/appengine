use prometheus::{TextEncoder, Encoder, Registry, IntCounterVec, opts};
use once_cell::sync::Lazy;
use axum::{response::IntoResponse, http::StatusCode};

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);
pub static HTTP_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    let c = IntCounterVec::new(opts!("http_requests_total", "HTTP request count"), &["method", "path", "status"]).unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buf = Vec::new();
    if encoder.encode(&metric_families, &mut buf).is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    ([("Content-Type","text/plain; version=0.0.4")], buf).into_response()
}
