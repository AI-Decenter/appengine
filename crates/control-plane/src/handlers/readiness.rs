/// Readiness probe
#[utoipa::path(get, path = "/readyz", responses( (status = 200, description = "Service ready" )))]
pub async fn readiness() -> &'static str { "ready" }
