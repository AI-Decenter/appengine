use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse { pub status: &'static str }

/// Health check endpoint
#[utoipa::path(get, path = "/health", responses( (status = 200, body = HealthResponse) ))]
pub async fn health() -> Json<HealthResponse> { Json(HealthResponse { status: "ok" }) }
