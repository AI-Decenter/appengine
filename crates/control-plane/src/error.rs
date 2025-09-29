use axum::{response::{IntoResponse, Response}, Json, http::StatusCode};
use serde::Serialize;
use utoipa::ToSchema;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiErrorBody { pub code: &'static str, pub message: String }

#[derive(Debug, Clone)]
pub struct ApiError { pub status: StatusCode, pub code: &'static str, pub message: String }

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }
    pub fn service_unavailable() -> Self { Self::new(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "Required dependency not ready") }
    pub fn not_found(msg: impl Into<String>) -> Self { Self::new(StatusCode::NOT_FOUND, "not_found", msg) }
    pub fn conflict(msg: impl Into<String>) -> Self { Self::new(StatusCode::CONFLICT, "conflict", msg) }
    pub fn internal(msg: impl Into<String>) -> Self { Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", msg) }
    pub fn bad_request(msg: impl Into<String>) -> Self { Self::new(StatusCode::BAD_REQUEST, "bad_request", msg) }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.message) }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorBody { code: self.code, message: self.message };
        (self.status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
