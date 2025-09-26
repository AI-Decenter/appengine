use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateDeploymentRequest {}

#[derive(Serialize)]
pub struct CreateDeploymentResponse { pub id: Uuid, pub status: &'static str }

pub async fn create_deployment(Json(_req): Json<CreateDeploymentRequest>) -> (StatusCode, Json<CreateDeploymentResponse>) {
    (StatusCode::CREATED, Json(CreateDeploymentResponse { id: Uuid::new_v4(), status: "accepted" }))
}
