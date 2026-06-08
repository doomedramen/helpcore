#![allow(missing_docs)]

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Unified error type for all API responses.
///
/// Each variant maps to a specific HTTP status code and JSON error body
/// via the [`IntoResponse`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 401 — authentication required.
    #[error("authentication required")]
    Unauthorized,

    /// 403 — access denied.
    #[error("access denied")]
    Forbidden,

    /// 404 — resource not found.
    #[error("{0}")]
    NotFound(String),

    /// 400 — malformed request.
    #[error("{0}")]
    BadRequest(String),

    /// 409 — resource conflict.
    #[error("{0}")]
    Conflict(String),

    /// 502 — upstream service error.
    #[error("{0}")]
    Upstream(String),

    /// 500 — internal server error. Logs the error and returns a generic message.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Converts a JSON (de)serialization error into an internal server error.
impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.into())
    }
}

/// Maps each error variant to an HTTP status code and a JSON body
/// with `code` and `message` fields.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            AppError::Upstream(msg) => (StatusCode::BAD_GATEWAY, "upstream_error", msg.clone()),
            AppError::Internal(e) => {
                tracing::error!(error = %e, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error".to_string(),
                )
            }
        };

        (status, Json(json!({ "code": code, "message": message }))).into_response()
    }
}
