//! Shared request/response types for helpcore.
//!
//! All types derive both Serialize and Deserialize — the server receives
//! requests and sends responses; the CLI sends requests and receives responses.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};

// ── Auth ─────────────────────────────────────────────────────────────────────

/// Request to authenticate a user with email and password.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    /// User's email address.
    pub email: String,
    /// User's password.
    pub password: String,
}

/// Response returned on successful login.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    /// JWT access token for authenticated requests.
    pub access_token: String,
    /// Long-lived refresh token for obtaining new access tokens.
    pub refresh_token: String,
    /// Token type, e.g. "Bearer".
    pub token_type: String,
    /// Whether the user must change their password before proceeding.
    #[serde(default)]
    pub force_password_change: bool,
}

/// Request to exchange a refresh token for a new access token.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshRequest {
    /// The refresh token obtained during login or a previous refresh.
    /// Omit when the token is supplied via the `helpcore_refresh` HttpOnly cookie.
    pub refresh_token: Option<String>,
}

/// Response containing a fresh access and refresh token pair.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshResponse {
    /// New JWT access token.
    pub access_token: String,
    /// New refresh token (old one is invalidated).
    pub refresh_token: String,
    /// Token type, e.g. "Bearer".
    pub token_type: String,
}

/// Request to invalidate a refresh token (logout).
#[derive(Debug, Serialize, Deserialize)]
pub struct LogoutRequest {
    /// The refresh token to invalidate.
    /// Omit when the token is supplied via the `helpcore_refresh` HttpOnly cookie.
    pub refresh_token: Option<String>,
}

/// Response containing the currently authenticated user's profile.
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    /// Unique user identifier.
    pub id: String,
    /// User's email address.
    pub email: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// User role, e.g. "admin" or "user".
    pub role: String,
    /// IANA timezone string.
    pub timezone: String,
    /// Memory leaning preference for the assistant.
    pub memory_leaning: String,
}

/// Request to update the current user's own profile fields.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMeRequest {
    /// New display name.
    pub display_name: Option<String>,
    /// New timezone.
    pub timezone: Option<String>,
    /// New memory leaning preference.
    pub memory_leaning: Option<String>,
}

/// Request to change the current user's password.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    /// Current password for verification.
    pub current_password: String,
    /// New password to set.
    pub new_password: String,
}

// ── API Keys ──────────────────────────────────────────────────────────────────

/// Request to create a new API key.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    /// Human-readable name for the key.
    pub name: String,
    /// Optional ISO-8601 expiry date.
    pub expires_at: Option<String>,
}

/// Response containing the newly created API key.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    /// Unique identifier of the API key.
    pub id: String,
    /// Human-readable name of the API key.
    pub name: String,
    /// Visible prefix for identifying the key.
    pub key_prefix: String,
    /// The full key value — shown only once. Store it securely.
    pub key: String,
}

/// Metadata about an existing API key (without the secret value).
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    /// Unique identifier of the API key.
    pub id: String,
    /// Human-readable name of the API key.
    pub name: String,
    /// Visible prefix for identifying the key.
    pub key_prefix: String,
    /// ISO-8601 timestamp when the key was created.
    pub created_at: String,
    /// ISO-8601 timestamp when the key was last used, if ever.
    pub last_used_at: Option<String>,
    /// Optional ISO-8601 expiry date.
    pub expires_at: Option<String>,
}

/// Response listing all API keys belonging to the authenticated user.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListApiKeysResponse {
    /// List of API key metadata.
    pub keys: Vec<ApiKeyInfo>,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

/// Response indicating whether initial setup is required.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    /// True if the instance has not been set up yet.
    pub setup_required: bool,
}

/// Request to perform initial instance setup (first admin account).
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupRequest {
    /// Setup token provided by the administrator.
    pub token: String,
    /// Admin user's email address.
    pub email: String,
    /// Admin user's password.
    pub password: String,
    /// Optional display name for the admin user.
    pub display_name: Option<String>,
}

// ── Chat ──────────────────────────────────────────────────────────────────────

/// Request to start or continue a chat conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Existing conversation ID to continue, or None to start a new one.
    pub conversation_id: Option<String>,
    /// The user's message content.
    pub message: String,
    /// Provider ID to use for this turn.
    pub provider_id: Option<String>,
    /// Model name override for this turn.
    pub model: Option<String>,
}

/// Information about an available AI provider.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Unique provider identifier.
    pub id: String,
    /// Human-readable provider name.
    pub name: String,
    /// Default model used by this provider.
    pub default_model: String,
    /// Maximum context window in tokens.
    pub context_limit: u32,
}

/// Response listing all available AI providers.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderListResponse {
    /// Available providers.
    pub providers: Vec<ProviderInfo>,
}

/// Sent as SSE `event: chunk` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseChunk {
    /// Text delta to append to the assistant's response.
    pub delta: String,
}

/// Sent as the first SSE event after the turn has been persisted.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseStarted {
    /// The conversation ID for this exchange.
    pub conversation_id: String,
    /// ID of the user message that initiated this turn.
    pub user_message_id: String,
    /// ID of the assistant message being streamed.
    pub message_id: String,
}

/// Sent as SSE `event: done` data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseDone {
    /// The conversation ID of the completed exchange.
    pub conversation_id: String,
    /// ID of the completed assistant message.
    pub message_id: String,
}

/// Sent as SSE `event: context` data — reports context window usage.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseContext {
    /// Estimated tokens currently in context.
    pub used_tokens: usize,
    /// Maximum context window size in tokens.
    pub max_tokens: u32,
}

/// Request to retry the last assistant turn in a conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct RetryRequest {
    /// Optional provider ID override for the retry.
    #[serde(default)]
    pub provider_id: Option<String>,
}

/// A tool call initiated by the assistant, sent via SSE.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseToolCall {
    /// Unique identifier for this tool call.
    pub id: String,
    /// Name of the tool being invoked.
    pub name: String,
    /// JSON arguments for the tool call.
    pub arguments: serde_json::Value,
}

/// The result of a tool execution, sent via SSE.
#[derive(Debug, Serialize, Deserialize)]
pub struct SseToolResult {
    /// Identifier matching the originating tool call.
    pub id: String,
    /// Name of the tool that was executed.
    pub name: String,
    /// The tool execution result as a string.
    pub result: String,
    /// Whether the result was truncated for context limits.
    #[serde(default)]
    pub truncated: bool,
}

// ── Conversations ─────────────────────────────────────────────────────────────

/// Request to rename an existing conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct RenameConversationRequest {
    /// New conversation title.
    pub title: String,
}

/// Summary of a conversation (without full message history).
#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// Unique conversation identifier.
    pub id: String,
    /// Conversation title.
    pub title: String,
    /// Provider ID used for the conversation.
    pub provider_id: Option<String>,
    /// Model name used for the conversation.
    pub model: Option<String>,
    /// Number of messages in the conversation.
    pub message_count: i64,
    /// ISO-8601 timestamp of conversation creation.
    pub created_at: String,
    /// ISO-8601 timestamp of last activity.
    pub updated_at: String,
}

/// A single message within a conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageSummary {
    /// Unique message identifier.
    pub id: String,
    /// Message role: "user", "assistant", or "tool".
    pub role: String,
    /// Message content text.
    pub content: String,
    /// Tool call ID this message responds to (for tool results).
    pub tool_call_id: Option<String>,
    /// Tool calls requested by the assistant, as a JSON array.
    pub tool_calls: Option<serde_json::Value>,
    /// Sequence number within the conversation.
    pub sequence: i64,
    /// ISO-8601 timestamp of message creation.
    pub created_at: String,
    /// Current delivery/processing status of the message.
    pub status: MessageStatus,
    /// Error message if the message failed.
    pub error: Option<String>,
    /// ISO-8601 timestamp of last update.
    pub updated_at: String,
}

/// Delivery and processing status of a chat message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    /// Message is queued and waiting to be processed.
    Pending,
    /// Response is currently being streamed to the client.
    Streaming,
    /// Message has been fully delivered and processed.
    Complete,
    /// Message processing failed.
    Failed,
    /// Message processing was interrupted by the user.
    Interrupted,
}

impl MessageStatus {
    /// Returns true if the message is still being processed.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Streaming)
    }
}

// ── Personality ───────────────────────────────────────────────────────────────

/// Response containing the current assistant personality definition.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityResponse {
    /// Personality name.
    pub name: String,
    /// Personality content (system prompt).
    pub content: String,
    /// ISO-8601 timestamp of last modification.
    pub updated_at: String,
}

/// Request to update the assistant personality content.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityWriteRequest {
    /// New personality content (system prompt).
    pub content: String,
}

// ── Memory ────────────────────────────────────────────────────────────────────

/// A file entry in the assistant's memory.
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// File path relative to the memory root.
    pub path: String,
    /// ISO-8601 timestamp of last modification.
    pub updated_at: String,
}

/// Response listing all files in the assistant's memory.
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryListResponse {
    /// Memory file entries.
    pub files: Vec<MemoryEntry>,
}

/// Response containing the content of a memory file.
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryReadResponse {
    /// Path of the memory file.
    pub path: String,
    /// File content.
    pub content: String,
}

/// Request to write (create or update) a memory file.
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryWriteRequest {
    /// New file content.
    pub content: String,
}

// ── Compact ───────────────────────────────────────────────────────────────────

/// Response summarizing the result of a conversation compaction.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompactResponse {
    /// ID of the compacted conversation.
    pub conversation_id: String,
    /// Number of messages compacted into the summary.
    pub messages_compacted: usize,
    /// Length of the generated summary in characters.
    pub summary_length: usize,
}

// ── Generate Title ────────────────────────────────────────────────────────────

/// Response from the generate-title endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateTitleResponse {
    /// The generated conversation title.
    pub title: String,
}

// ── Plugins ───────────────────────────────────────────────────────────────────

/// A single field in a plugin's configuration schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    /// Unique key identifying this config field.
    pub key: String,
    /// Human-readable label for the field.
    pub label: String,
    /// One of: "text" | "url" | "number" | "select" | "boolean" | "secret"
    #[serde(rename = "type")]
    pub field_type: String,
    /// Whether this field must be filled.
    #[serde(default)]
    pub required: bool,
    /// Help text shown to the user.
    pub hint: Option<String>,
    /// Default value if none is provided.
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

/// Information about an installed plugin.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique plugin identifier.
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Plugin description.
    pub description: String,
    /// Currently active version string.
    pub active_version: String,
    /// Previously active version, if any.
    pub previous_version: Option<String>,
    /// Latest available version.
    pub available_version: Option<String>,
    /// Plugin tier (e.g. "core", "community").
    pub tier: String,
    /// Permissions required by the plugin.
    pub permissions: Vec<String>,
    /// Hosts the plugin is allowed to contact (may contain `*` for any host).
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Capabilities provided by the plugin.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Whether the plugin has been configured.
    pub configured: bool,
    /// Whether a newer version is available.
    pub update_available: bool,
    /// Whether the plugin is blocked (e.g. blacklisted).
    pub blocked: bool,
    /// Whether this plugin can be managed by the user.
    pub user_managed: bool,
    /// Configuration schema fields.
    pub config_schema: Vec<ConfigField>,
    /// Current config values. Non-secret fields contain their stored value.
    /// Secret fields contain `{"configured": bool}` — the value is never returned.
    pub config_values: serde_json::Value,
}

/// Response listing all installed plugins and available capabilities.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginListResponse {
    /// Installed plugins.
    pub plugins: Vec<PluginInfo>,
    /// Capabilities available across all installed plugins (e.g. "audio").
    pub capabilities: Vec<String>,
}

/// Request to generate a plugin-scoped API token.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginTokenRequest {
    /// Permissions to grant the token.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Response containing the newly created plugin token.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginTokenResponse {
    /// Unique identifier for the token.
    pub token_id: String,
    /// The raw token value — shown only once; store it securely.
    pub token: String,
}

/// An item available in the plugin store/registry.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginStoreItem {
    /// Unique plugin identifier.
    pub id: String,
    /// Plugin name.
    pub name: String,
    /// Plugin description.
    pub description: String,
    /// Plugin version.
    pub version: String,
    /// Plugin tier (e.g. "core", "community").
    pub tier: String,
    /// Plugin author.
    pub author: String,
    /// Plugin homepage URL.
    pub homepage: String,
    /// Optional setup guide URL.
    pub setup_guide: Option<String>,
    /// Permissions required by the plugin.
    pub permissions: Vec<String>,
    /// Hosts the plugin is allowed to contact (may contain `*` for any host).
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Capabilities provided by the plugin.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Whether the plugin can be installed.
    pub installable: bool,
    /// Whether the plugin is currently installed.
    pub installed: bool,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Whether the plugin is blocked.
    pub blocked: bool,
    /// Currently active version on this instance.
    pub active_version: Option<String>,
    /// Previously active version on this instance.
    pub previous_version: Option<String>,
    /// Whether the plugin has been configured.
    pub configured: bool,
    /// Whether a newer version is available for the installed plugin.
    pub update_available: bool,
}

/// Response listing available plugins from the registry.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginStoreResponse {
    /// URL of the plugin registry.
    pub registry_url: String,
    /// Available plugins.
    pub plugins: Vec<PluginStoreItem>,
}

/// Request to install a plugin from the registry.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInstallRequest {
    /// Permissions to grant the plugin.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Request to enable or disable an installed plugin.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginEnableRequest {
    /// Whether the plugin should be enabled.
    pub enabled: bool,
}

/// Request to update plugin configuration values.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginConfigureRequest {
    /// Flat map of all config values keyed by field `key`.
    /// Secret fields: provide the new value to update, omit (or set null) to keep existing.
    /// Secret fields with an empty string value are cleared.
    pub values: serde_json::Value,
}

// ── Admin configuration ──────────────────────────────────────────────────────

/// Server configuration viewable and editable by admins.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminServerConfig {
    /// Server display name.
    pub name: String,
    /// Server URL (used for links in emails, etc.).
    pub url: String,
    /// Server listen port.
    pub port: u16,
}

/// Provider configuration viewable and editable by admins.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminProviderConfig {
    /// Unique provider identifier.
    pub id: String,
    /// Human-readable provider name.
    pub name: String,
    /// Provider type (e.g. "openai", "anthropic", "ollama").
    pub provider_type: String,
    /// Whether an API key is configured.
    pub api_key_configured: bool,
    /// API endpoint URL override.
    pub url: Option<String>,
    /// Default model used by this provider.
    pub default_model: String,
    /// Roles that can use this provider.
    pub roles: Vec<String>,
    /// Context window size override.
    pub num_ctx: Option<u32>,
    /// Max prediction tokens override.
    pub num_predict: Option<u32>,
}

/// Sandbox configuration viewable and editable by admins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSandboxConfig {
    /// Whether the sandbox is enabled.
    pub enabled: bool,
    /// Docker image to use for sandbox containers.
    pub image: String,
    /// Max execution time in seconds per command.
    pub timeout: u64,
    /// Max memory in MB.
    pub memory_mb: u64,
    /// Optional Docker host URL override.
    pub host: Option<String>,
}

/// Response containing the full admin-viewable server configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminConfigResponse {
    /// Filesystem path to the configuration file.
    pub config_path: String,
    /// Whether the config file is writable by the server process.
    pub config_writable: bool,
    /// Reason why the config is not writable, if applicable.
    pub config_writability_error: Option<String>,
    /// Server configuration.
    pub server: AdminServerConfig,
    /// Current logging level.
    pub logging_level: String,
    /// Plugin registry URL.
    pub registry_url: String,
    /// Plugin IDs that are blocked.
    pub plugin_blacklist: Vec<String>,
    /// Configured AI providers.
    pub providers: Vec<AdminProviderConfig>,
    /// Sandbox configuration.
    pub sandbox: AdminSandboxConfig,
    /// Whether a server restart is needed for changes to take effect.
    pub restart_required: bool,
}

/// A single provider entry in an admin configuration update.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminProviderUpdate {
    /// Provider identifier.
    pub id: String,
    /// Human-readable provider name.
    pub name: String,
    /// Provider type.
    pub provider_type: String,
    /// New API key value (omitted if not changing).
    pub api_key: Option<String>,
    /// Set to true to clear the existing API key.
    #[serde(default)]
    pub clear_api_key: bool,
    /// API endpoint URL override.
    pub url: Option<String>,
    /// Default model for this provider.
    pub default_model: String,
    /// Roles that can use this provider.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Context window size override.
    pub num_ctx: Option<u32>,
    /// Max prediction tokens override.
    pub num_predict: Option<u32>,
}

/// Request to update the server configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminConfigUpdateRequest {
    /// Updated server settings.
    pub server: AdminServerConfig,
    /// New logging level.
    pub logging_level: String,
    /// New plugin registry URL.
    pub registry_url: String,
    /// New plugin blacklist.
    #[serde(default)]
    pub plugin_blacklist: Vec<String>,
    /// Updated provider configurations.
    #[serde(default)]
    pub providers: Vec<AdminProviderUpdate>,
    /// Updated sandbox configuration.
    pub sandbox: AdminSandboxConfig,
}

// ── Admin user management ─────────────────────────────────────────────────────

/// Summary of a user for admin user listing.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUserSummary {
    /// Unique user identifier.
    pub id: String,
    /// User's email address.
    pub email: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// User role.
    pub role: String,
    /// Account status (e.g. "active", "deactivated").
    pub status: String,
    /// ISO-8601 timestamp of account creation.
    pub created_at: String,
}

/// Request for an admin to create a new user account.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminCreateUserRequest {
    /// New user's email address.
    pub email: String,
    /// New user's initial password.
    pub password: String,
    /// Optional display name.
    pub display_name: Option<String>,
}

/// Request for an admin to update a user's account status.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminUpdateUserRequest {
    /// "active" or "deactivated"
    pub status: Option<String>,
}

/// Request for an admin to reset a user's password.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminResetPasswordRequest {
    /// New password to set.
    pub password: String,
}

/// Response listing all users (admin view).
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminListUsersResponse {
    /// List of user summaries.
    pub users: Vec<AdminUserSummary>,
}

// ── Admin provider grants ─────────────────────────────────────────────────────

/// A grant controlling a user's access to a specific provider.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderGrantInfo {
    /// Provider identifier this grant applies to.
    pub provider_id: String,
    /// Whether the provider is enabled for the user.
    pub enabled: bool,
}

/// Response listing provider grants for a user.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListProviderGrantsResponse {
    /// Explicit grants for specific providers.
    pub grants: Vec<ProviderGrantInfo>,
    /// Provider IDs that have no explicit grant (default: enabled).
    pub ungranted_providers: Vec<String>,
}

/// Request to set provider grants for a user.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetProviderGrantsRequest {
    /// Each entry sets the enabled state for a provider. Omitted providers are left unchanged.
    pub grants: Vec<ProviderGrantInfo>,
}

// ── Errors ────────────────────────────────────────────────────────────────────

/// Standard API error response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}
