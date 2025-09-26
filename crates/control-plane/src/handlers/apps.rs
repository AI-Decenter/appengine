use axum::{Json, extract::Path};
use serde::Serialize;

pub async fn list_apps() -> Json<Vec<ListAppItem>> { Json(vec![]) }

#[derive(Serialize)]
pub struct ListAppItem { pub name: String }

pub async fn app_logs(Path(_app_name): Path<String>) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::OK, String::new())
}
