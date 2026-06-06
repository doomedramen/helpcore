pub mod admin;
pub mod auth;
pub mod chat;
pub mod memory;
pub mod plugin;
pub mod provider;
pub mod setup;
pub mod tts;

use axum::{Json, http::StatusCode};
use serde_json::{Value, json};

pub async fn health() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })))
}
