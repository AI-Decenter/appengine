use axum::{extract::State, Json};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;
use crate::AppState;
use std::fs;use std::path::PathBuf;use tracing::{info,error};

#[derive(Deserialize)]
pub struct UploadForm { pub app_name: String }

pub async fn upload_artifact(State(_state): State<AppState>, mut multipart: axum::extract::Multipart) -> impl IntoResponse {
    let mut app_name: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().map(|s| s.to_string());
        match name.as_deref() {
            Some("app_name") => {
                if let Ok(val) = field.text().await { app_name = Some(val); }
            }
            Some("artifact") => {
                if let Ok(bytes) = field.bytes().await { file_bytes = Some(bytes.to_vec()); }
            }
            _ => {}
        }
    }
    let Some(app) = app_name else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"missing app_name"}))); };
    let Some(bytes) = file_bytes else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"missing artifact file"}))); };
    // store file
    let dir = std::env::var("ARTIFACT_STORE_DIR").unwrap_or_else(|_| "./data/artifacts".into());
    if let Err(e)=fs::create_dir_all(&dir) { error!(?e, "create_store_dir_failed"); return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"store dir"}))); }
    let file_id = Uuid::new_v4();
    let path = PathBuf::from(dir).join(format!("{file_id}.tar.gz"));
    if let Err(e)=fs::write(&path, &bytes) { error!(?e, "write_artifact_failed"); return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"write failed"}))); }
    info!(app=%app, artifact_path=%path.display(), size=bytes.len(), "artifact_uploaded");
    let url = format!("file://{}", path.display());
    (StatusCode::OK, Json(serde_json::json!({"artifact_url": url })))
}
