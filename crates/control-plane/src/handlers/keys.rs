use axum::{extract::State, Json};
use serde::Serialize;
use crate::{AppState, error::ApiResult, error::ApiError};

#[derive(Serialize)]
pub struct KeyMeta { pub key_id: String, pub status: String, pub created: Option<String>, pub not_before: Option<String>, pub not_after: Option<String> }

pub async fn list_keys(State(_state): State<AppState>) -> ApiResult<Json<Vec<KeyMeta>>> {
    let dir = std::env::var("AETHER_PROVENANCE_DIR").unwrap_or_else(|_| "/tmp/provenance".into());
    let path = std::path::Path::new(&dir).join("provenance_keys.json");
    if !path.exists() { return Ok(Json(vec![])); }
    let content = tokio::fs::read_to_string(&path).await.map_err(|e| ApiError::internal(format!("read keystore: {e}")))?;
    let val: serde_json::Value = serde_json::from_str(&content).map_err(|e| ApiError::internal(format!("parse keystore: {e}")))?;
    let mut out = Vec::new();
    if let Some(arr) = val.as_array() {
        for k in arr {
            let key_id = k.get("key_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status = k.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let created = k.get("created").and_then(|v| v.as_str()).map(|s| s.to_string());
            let nb = k.get("not_before").and_then(|v| v.as_str()).map(|s| s.to_string());
            let na = k.get("not_after").and_then(|v| v.as_str()).map(|s| s.to_string());
            if !key_id.is_empty() { out.push(KeyMeta { key_id, status, created, not_before: nb, not_after: na }); }
        }
    }
    Ok(Json(out))
}
