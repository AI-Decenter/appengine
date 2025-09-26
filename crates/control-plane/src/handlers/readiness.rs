use axum::{Json, extract::State};
use serde::Serialize;
use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadinessResponse { pub status: &'static str }

/// Readiness probe: checks DB connectivity (simple SELECT 1)
#[utoipa::path(get, path = "/readyz", responses(
	(status = 200, body = ReadinessResponse, description = "Service ready"),
	(status = 503, body = ReadinessResponse, description = "Dependency not ready")
))]
pub async fn readiness(State(state): State<AppState>) -> (axum::http::StatusCode, Json<ReadinessResponse>) {
	let ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
	if ok { (axum::http::StatusCode::OK, Json(ReadinessResponse { status: "ready" })) }
	else { (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(ReadinessResponse { status: "degraded" })) }
}
