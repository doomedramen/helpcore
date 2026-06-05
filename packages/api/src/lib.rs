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

// ── Personality ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityResponse {
    pub name:    String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityWriteRequest {
    pub content: String,
}

// ── Memory ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub path:       String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryListResponse {
    pub files: Vec<MemoryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryReadResponse {
    pub path:    String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryWriteRequest {
    pub content: String,
}

// ── Compact ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CompactResponse {
    pub conversation_id:    String,
    pub messages_compacted: usize,
    pub summary_length:     usize,
}

// ── Plugins ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id:      String,
    pub name:    String,
    pub version: String,
    pub tier:    String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginTokenRequest {
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginTokenResponse {
    pub token_id: String,
    /// The raw token value — shown only once; store it securely.
    pub token:    String,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
