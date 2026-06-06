export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  force_password_change?: boolean;
}

export interface RefreshResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
}

export interface CurrentUser {
  id: string;
  email: string;
  display_name: string | null;
  role: 'admin' | 'member';
  timezone: string;
}

// ── Personality ────────────────────────────────────────────────────────────────

export interface PersonalityFile {
  name: string;
  content: string;
}

// ── Memory ────────────────────────────────────────────────────────────────────

export interface MemoryEntry {
  path: string;
  updated_at: string;
}

export interface MemoryListResponse {
  files: MemoryEntry[];
}

// ── API Keys ──────────────────────────────────────────────────────────────────

export interface ApiKeyInfo {
  id: string;
  name: string;
  key_prefix: string;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
}

export interface ListApiKeysResponse {
  keys: ApiKeyInfo[];
}

export interface CreateApiKeyResponse {
  id: string;
  name: string;
  key_prefix: string;
  key: string;
}

// ── Admin users ───────────────────────────────────────────────────────────────

export interface AdminUserSummary {
  id: string;
  email: string;
  display_name: string | null;
  role: 'admin' | 'member';
  status: 'active' | 'deactivated';
  created_at: string;
}

export interface AdminListUsersResponse {
  users: AdminUserSummary[];
}

// ── Provider grants ───────────────────────────────────────────────────────────

export interface ProviderGrantInfo {
  provider_id: string;
  enabled: boolean;
}

export interface ListProviderGrantsResponse {
  grants: ProviderGrantInfo[];
  ungrated_providers: string[];
}

export interface SetupStatusResponse {
  setup_required: boolean;
}

export interface ConversationSummary {
  id: string;
  title: string;
  provider_id: string | null;
  model: string | null;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'summary' | 'tool';
  content: string;
  tool_call_id: string | null;
  tool_calls: unknown[] | null;
  sequence: number;
  created_at: string;
  status: 'pending' | 'streaming' | 'complete' | 'failed' | 'interrupted';
  error: string | null;
  updated_at: string;
}

export interface SseStarted {
  conversation_id: string;
  user_message_id: string;
  message_id: string;
}

export interface SseDone {
  conversation_id: string;
  message_id: string;
}

export interface ProviderInfo {
  id: string;
  name: string;
  default_model: string;
}

export interface ProviderListResponse {
  providers: ProviderInfo[];
}

export interface ConfigField {
  key: string;
  label: string;
  type: 'text' | 'url' | 'number' | 'select' | 'boolean' | 'secret';
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
export interface SecretStatus { configured: boolean; }
export type ConfigValue = ConfigScalar | SecretStatus;
export type ConfigValues = Record<string, ConfigValue>;

export interface PluginListResponse {
  plugins: PluginInfo[];
  capabilities: string[];
}

export interface PluginInfo {
  id: string;
  name: string;
  description: string;
  active_version: string;
  previous_version: string | null;
  available_version: string | null;
  tier: 'wasm' | 'bridge';
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

export interface PluginStoreItem {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: 'wasm' | 'bridge';
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

export interface PluginStoreResponse {
  registry_url: string;
  plugins: PluginStoreItem[];
}

export type ProviderType =
  | 'anthropic'
  | 'deepseek'
  | 'openai'
  | 'ollama'
  | 'openai_compatible';
export type ProviderRole = 'chat' | 'code' | 'image_gen' | 'video_gen' | 'embeddings';

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

export interface AdminProviderUpdate extends Omit<AdminProviderConfig, 'api_key_configured'> {
  api_key?: string | null;
  clear_api_key?: boolean;
}

export interface AdminConfigUpdate {
  server: AdminConfig['server'];
  logging_level: string;
  registry_url: string;
  plugin_blacklist: string[];
  providers: AdminProviderUpdate[];
}
