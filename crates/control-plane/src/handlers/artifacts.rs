use axum::{extract::{Path, State}, http::StatusCode};
use crate::AppState;
use crate::error::{ApiError, ApiResult};
use axum::response::IntoResponse;
use std::path::PathBuf;

pub async fn get_sbom(State(_state): State<AppState>, Path(digest): Path<String>) -> ApiResult<impl IntoResponse> {
    // SBOM expected at storage layout: /data/sbom/<digest>.sbom.json OR configurable base dir
    let dir = std::env::var("AETHER_SBOM_DIR").unwrap_or_else(|_| "./".into());
    let filename = format!("{}.sbom.json", digest);
    let primary = PathBuf::from(&dir).join(&filename);
    if primary.exists() {
        let bytes = match tokio::fs::read(&primary).await { Ok(b)=>b, Err(e)=> return Err(ApiError::internal(format!("read sbom: {e}"))) };
        return Ok((StatusCode::OK, [ ("Content-Type","application/json") ], bytes));
    }
    Err(ApiError::not_found("sbom not found"))
}
