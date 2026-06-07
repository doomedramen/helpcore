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
    #[serde(default)]
    pub force_password_change: bool,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub timezone: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// ── API Keys ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Optional ISO-8601 expiry date.
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    /// The full key value — shown only once. Store it securely.
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListApiKeysResponse {
    pub keys: Vec<ApiKeyInfo>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub default_model: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderInfo>,
}

/// Sent as SSE `event: chunk` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseChunk {
    pub delta: String,
}

/// Sent as the first SSE event after the turn has been persisted.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseStarted {
    pub conversation_id: String,
    pub user_message_id: String,
    pub message_id: String,
}

/// Sent as SSE `event: done` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseDone {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetryRequest {
    #[serde(default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SseToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SseToolResult {
    pub id: String,
    pub name: String,
    pub result: String,
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
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub sequence: i64,
    pub created_at: String,
    pub status: MessageStatus,
    pub error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Pending,
    Streaming,
    Complete,
    Failed,
    Interrupted,
}

impl MessageStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Streaming)
    }
}

// ── Personality ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityResponse {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityWriteRequest {
    pub content: String,
}

// ── Memory ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub path: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryListResponse {
    pub files: Vec<MemoryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryReadResponse {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryWriteRequest {
    pub content: String,
}

// ── Compact ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CompactResponse {
    pub conversation_id: String,
    pub messages_compacted: usize,
    pub summary_length: usize,
}

// ── Plugins ───────────────────────────────────────────────────────────────────

/// A single field in a plugin's configuration schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    /// One of: "text" | "url" | "number" | "select" | "boolean" | "secret"
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    pub hint: Option<String>,
    pub default: Option<String>,
    /// Valid options for "select" fields.
    #[serde(default)]
    pub options: Vec<String>,
    /// Inclusive minimum for "number" fields.
    pub min: Option<f64>,
    /// Inclusive maximum for "number" fields.
    pub max: Option<f64>,
    /// Semantic role. "bridge_endpoint" marks the field used for routing/health-checks.
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub active_version: String,
    pub previous_version: Option<String>,
    pub available_version: Option<String>,
    pub tier: String,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    pub enabled: bool,
    pub configured: bool,
    pub update_available: bool,
    pub blocked: bool,
    pub user_managed: bool,
    pub config_schema: Vec<ConfigField>,
    /// Current config values. Non-secret fields contain their stored value.
    /// Secret fields contain `{"configured": bool}` — the value is never returned.
    pub config_values: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginInfo>,
    /// Capabilities available across all installed plugins (e.g. "audio").
    pub capabilities: Vec<String>,
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
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginStoreItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub tier: String,
    pub author: String,
    pub homepage: String,
    pub setup_guide: Option<String>,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    pub installable: bool,
    pub installed: bool,
    pub enabled: bool,
    pub blocked: bool,
    pub active_version: Option<String>,
    pub previous_version: Option<String>,
    pub configured: bool,
    pub update_available: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginStoreResponse {
    pub registry_url: String,
    pub plugins: Vec<PluginStoreItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInstallRequest {
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginEnableRequest {
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginConfigureRequest {
    /// Flat map of all config values keyed by field `key`.
    /// Secret fields: provide the new value to update, omit (or set null) to keep existing.
    /// Secret fields with an empty string value are cleared.
    pub values: serde_json::Value,
}

// ── Admin configuration ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminServerConfig {
    pub name: String,
    pub url: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub api_key_configured: bool,
    pub url: Option<String>,
    pub default_model: String,
    pub roles: Vec<String>,
    pub num_ctx: Option<u32>,
    pub num_predict: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminConfigResponse {
    pub config_path: String,
    pub config_writable: bool,
    pub config_writability_error: Option<String>,
    pub server: AdminServerConfig,
    pub logging_level: String,
    pub registry_url: String,
    pub plugin_blacklist: Vec<String>,
    pub providers: Vec<AdminProviderConfig>,
    pub restart_required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminProviderUpdate {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    pub url: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub num_ctx: Option<u32>,
    pub num_predict: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminConfigUpdateRequest {
    pub server: AdminServerConfig,
    pub logging_level: String,
    pub registry_url: String,
    #[serde(default)]
    pub plugin_blacklist: Vec<String>,
    #[serde(default)]
    pub providers: Vec<AdminProviderUpdate>,
}

// ── Admin user management ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUserSummary {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminCreateUserRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUpdateUserRequest {
    /// "active" or "deactivated"
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminResetPasswordRequest {
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminListUsersResponse {
    pub users: Vec<AdminUserSummary>,
}

// ── Admin provider grants ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderGrantInfo {
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProviderGrantsResponse {
    pub grants: Vec<ProviderGrantInfo>,
    /// Provider IDs that have no explicit grant (default: enabled).
    pub ungrated_providers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetProviderGrantsRequest {
    /// Each entry sets the enabled state for a provider. Omitted providers are left unchanged.
    pub grants: Vec<ProviderGrantInfo>,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
