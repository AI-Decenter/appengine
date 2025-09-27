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

#[derive(Serialize, utoipa::ToSchema)]
pub struct StartupResponse { pub status: &'static str, pub pending_migrations: i64 }

/// Startup probe: checks zero pending migrations (assumes migrations applied at boot)
#[utoipa::path(get, path = "/startupz", responses( (status=200, body=StartupResponse, description="OK"), (status=503, body=StartupResponse, description="Pending migrations") ))]
pub async fn startupz(State(state): State<AppState>) -> (axum::http::StatusCode, Json<StartupResponse>) {
	// Count applied vs available by querying _sqlx_migrations table (internal to sqlx migrate)
	let applied_res = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM _sqlx_migrations")
		.fetch_one(&state.db).await;
	let applied = match applied_res { Ok(v)=>v, Err(e)=> { tracing::warn!(?e, "startupz_migrations_count_failed"); 0 } };
	// For simplicity we embed total known migrations at compile time via sqlx::migrate! macro listing.
	// If mismatch -> pending.
	let total = sqlx::migrate!().migrations.len() as i64;
	let pending = (total - applied).max(0);
	if pending == 0 { (axum::http::StatusCode::OK, Json(StartupResponse { status: "ok", pending_migrations: pending })) }
	else { (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(StartupResponse { status: "pending", pending_migrations: pending })) }
}
