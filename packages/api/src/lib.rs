//! Shared request/response types for helpcore.
//!
//! All types derive both Serialize and Deserialize — the server receives
//! requests and sends responses; the CLI sends requests and receives responses.

use serde::{Deserialize, Serialize};

// ── Auth ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: String,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    pub setup_required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupRequest {
    pub token: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

// ── Chat ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: Option<String>,
    pub message: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

/// Sent as SSE `event: chunk` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseChunk {
    pub delta: String,
}

/// Sent as SSE `event: done` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseDone {
    pub conversation_id: String,
    pub message_id: String,
}

// ── Conversations ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub message_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub role: String,
    pub content: String,
    pub sequence: i64,
    pub created_at: String,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
