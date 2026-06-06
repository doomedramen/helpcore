export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
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
}

export interface SetupStatusResponse {
  setup_required: boolean;
}

export interface ConversationSummary {
  id: string;
  title: string;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'summary' | 'tool';
  content: string;
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

export interface PluginInfo {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: 'wasm' | 'bridge';
  permissions: string[];
  enabled: boolean;
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
  installed: boolean;
  enabled: boolean;
  blocked: boolean;
}

export interface PluginStoreResponse {
  registry_url: string;
  plugins: PluginStoreItem[];
}

export type ProviderType = 'ollama' | 'anthropic' | 'openai' | 'openai_compatible';
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
