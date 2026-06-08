//! Axum route handler functions for the Helpcore API.
//!
//! Each sub-module corresponds to a group of related endpoints
//! (auth, chat, memory, admin, etc.). The top-level [`health`] handler
//! serves the `/api/health` liveness check.

pub mod admin;
pub mod api_key;
pub mod auth;
pub mod chat;
pub mod memory;
pub mod plugin;
pub mod provider;
pub mod setup;
pub mod tts;
pub mod users;

use axum::{Json, http::StatusCode};
use serde_json::{Value, json};

/// GET /api/health — returns server status and version.
pub async fn health() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })),
    )
}
