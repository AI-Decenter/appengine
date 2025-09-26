use axum::{Json, extract::{Path, State}};
use serde::{Serialize, Deserialize};
use crate::{AppState, models::Application};
use axum::http::StatusCode;

#[derive(Deserialize)]
pub struct CreateAppReq { pub name: String }

#[derive(Serialize)]
pub struct CreateAppResp { pub id: uuid::Uuid, pub name: String }

pub async fn create_app(State(state): State<AppState>, Json(body): Json<CreateAppReq>) -> Result<(StatusCode, Json<CreateAppResp>), StatusCode> {
    let pool = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rec: Application = sqlx::query_as::<_, Application>("INSERT INTO applications (name) VALUES ($1) RETURNING id, name, created_at, updated_at")
        .bind(&body.name)
        .fetch_one(pool).await.map_err(|e| {
            if let Some(db_code) = e.as_database_error().and_then(|d| d.code()) { if db_code == "23505" { return StatusCode::CONFLICT; } }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(CreateAppResp { id: rec.id, name: rec.name })))
}

#[derive(Serialize)]
pub struct ListAppItem { pub id: uuid::Uuid, pub name: String }

pub async fn list_apps(State(state): State<AppState>) -> Result<Json<Vec<ListAppItem>>, StatusCode> {
    let pool = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows: Vec<Application> = sqlx::query_as::<_, Application>("SELECT id, name, created_at, updated_at FROM applications ORDER BY created_at DESC")
        .fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|a| ListAppItem { id: a.id, name: a.name }).collect()))
}

pub async fn app_logs(Path(_app_name): Path<String>) -> (StatusCode, String) { (StatusCode::OK, String::new()) }
