use axum::{Json};
use serde::Serialize;
use std::time::Instant;
use once_cell::sync::Lazy;

static START: Lazy<Instant> = Lazy::new(Instant::now);

#[derive(Serialize)]
pub struct HealthResponse { pub status: &'static str }

pub async fn health() -> Json<HealthResponse> { Json(HealthResponse { status: "ok" }) }
