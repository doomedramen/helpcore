/** Response from a successful login. */
export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  force_password_change?: boolean;
}

/** Response from a successful token refresh. */
export interface RefreshResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
}

/** The currently authenticated user's profile. */
export interface CurrentUser {
  id: string;
  email: string;
  display_name: string | null;
  role: "admin" | "member";
  timezone: string;
  memory_leaning: string;
}

// ── Personality ────────────────────────────────────────────────────────────────

/** A named personality file containing custom AI instructions. */
export interface PersonalityFile {
  name: string;
  content: string;
  updated_at: string;
}

// ── Memory ────────────────────────────────────────────────────────────────────

/** A single entry in the memory file index. */
export interface MemoryEntry {
  path: string;
  updated_at: string;
}

/** Response listing all memory files. */
export interface MemoryListResponse {
  files: MemoryEntry[];
}

// ── API Keys ──────────────────────────────────────────────────────────────────

/** Metadata for a user-created API key (never contains the full key). */
export interface ApiKeyInfo {
  id: string;
  name: string;
  key_prefix: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
}

/** Response listing all API keys for the current user. */
export interface ListApiKeysResponse {
  keys: ApiKeyInfo[];
}

/** Response from creating a new API key — includes the raw key once. */
export interface CreateApiKeyResponse {
  id: string;
  name: string;
  key_prefix: string;
  key: string;
}

// ── Admin users ───────────────────────────────────────────────────────────────

/** Summary row for a user in the admin panel. */
export interface AdminUserSummary {
  id: string;
  email: string;
  display_name: string | null;
  role: "admin" | "member";
  status: "active" | "deactivated";
  created_at: string;
}

/** Response listing all users (admin-only). */
export interface AdminListUsersResponse {
  users: AdminUserSummary[];
}

// ── Provider grants ───────────────────────────────────────────────────────────

/** Per-provider access toggle for a user. */
export interface ProviderGrantInfo {
  provider_id: string;
  enabled: boolean;
}

/** Response listing provider grants for a user. */
export interface ListProviderGrantsResponse {
  grants: ProviderGrantInfo[];
  ungranted_providers: string[];
}

/** Tells the frontend whether initial setup has been completed. */
export interface SetupStatusResponse {
  setup_required: boolean;
}

/** Lightweight summary of a conversation shown in the sidebar. */
export interface ConversationSummary {
  id: string;
  title: string;
  provider_id: string | null;
  model: string | null;
  message_count: number;
  created_at: string;
  updated_at: string;
}

/** A single message within a conversation. */
export interface Message {
  id: string;
  role: "user" | "assistant" | "summary" | "tool";
  content: string;
  tool_call_id: string | null;
  tool_calls: unknown[] | null;
  sequence: number;
  created_at: string;
  status: "pending" | "streaming" | "complete" | "failed" | "interrupted";
  error: string | null;
  updated_at: string;
}

/** SSE event emitted when a streaming response begins. */
export interface SseStarted {
  conversation_id: string;
  user_message_id: string;
  message_id: string;
}

/** SSE event describing a tool the model requested to call. */
export interface SseToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/** SSE event with the result of a tool execution. */
export interface SseToolResult {
  id: string;
  name: string;
  result: string;
}

/** SSE event signalling the stream has finished cleanly. */
export interface SseDone {
  conversation_id: string;
  message_id: string;
}

/** SSE event emitted when the tool-call round limit is reached. */
export interface SseInterrupted {
  message: string;
}

/** SSE event reporting context window usage. */
export interface SseContext {
  used_tokens: number;
  max_tokens: number;
}

/** Response from a conversation compaction. */
export interface CompactResponse {
  conversation_id: string;
  messages_compacted: number;
  summary_length: number;
}

/** Brief info about a configured LLM provider. */
export interface ProviderInfo {
  id: string;
  name: string;
  default_model: string;
  context_limit: number;
}

/** Response listing all available providers. */
export interface ProviderListResponse {
  providers: ProviderInfo[];
}

/** Describes a single configuration field for a provider or plugin. */
export interface ConfigField {
  key: string;
  label: string;
  type: "text" | "url" | "number" | "select" | "boolean" | "secret";
  required: boolean;
  hint?: string;
  default?: string;
  options: string[];
  min?: number;
  max?: number;
  role?: string;
}

/** Value for a non-secret config field. */
export type ConfigScalar = string | number | boolean | null;
/** Value returned for a secret field — never the raw value. */
export interface SecretStatus {
  configured: boolean;
}
/** Union of all possible config field values. */
export type ConfigValue = ConfigScalar | SecretStatus;
/** Map of config field keys to their values. */
export type ConfigValues = Record<string, ConfigValue>;

/** Response listing installed plugins and available capabilities. */
export interface PluginListResponse {
  plugins: PluginInfo[];
  capabilities: string[];
}

/** Full metadata for an installed plugin. */
export interface PluginInfo {
  id: string;
  name: string;
  description: string;
  active_version: string;
  previous_version: string | null;
  available_version: string | null;
  tier: "wasm" | "bridge";
  permissions: string[];
  provides: string[];
  enabled: boolean;
  configured: boolean;
  update_available: boolean;
  blocked: boolean;
  user_managed: boolean;
  config_schema: ConfigField[];
  config_values: ConfigValues;
}

/** A plugin as it appears in the store / registry. */
export interface PluginStoreItem {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: "wasm" | "bridge";
  author: string;
  homepage: string;
  setup_guide: string | null;
  permissions: string[];
  provides: string[];
  installable: boolean;
  installed: boolean;
  enabled: boolean;
  blocked: boolean;
  active_version: string | null;
  previous_version: string | null;
  configured: boolean;
  update_available: boolean;
}

/** Response from the plugin store / registry. */
export interface PluginStoreResponse {
  registry_url: string;
  plugins: PluginStoreItem[];
}

/** Supported LLM provider backends. */
export type ProviderType = "anthropic" | "deepseek" | "openai" | "ollama" | "openai_compatible";
/** Roles a provider can fulfill. */
export type ProviderRole = "chat" | "code" | "image_gen" | "video_gen" | "embeddings";

/** Admin-level provider configuration (includes credentials status). */
export interface AdminProviderConfig {
  id: string;
  name: string;
  provider_type: ProviderType;
  api_key_configured: boolean;
  url: string | null;
  default_model: string;
  roles: ProviderRole[];
  num_ctx: number | null;
  num_predict: number | null;
}

/** The full admin configuration payload. */
export interface AdminConfig {
  config_path: string;
  config_writable: boolean;
  config_writability_error: string | null;
  server: {
    name: string;
    url: string;
    port: number;
  };
  logging_level: string;
  registry_url: string;
  plugin_blacklist: string[];
  providers: AdminProviderConfig[];
  restart_required: boolean;
}

/** Provider config submitted from the admin form (may include a new API key). */
export interface AdminProviderUpdate extends Omit<AdminProviderConfig, "api_key_configured"> {
  api_key?: string | null;
  clear_api_key?: boolean;
}

/** Admin configuration update payload. */
export interface AdminConfigUpdate {
  server: AdminConfig["server"];
  logging_level: string;
  registry_url: string;
  plugin_blacklist: string[];
  providers: AdminProviderUpdate[];
}
