import type {
  AdminConfig,
  AdminConfigUpdate,
  AdminListUsersResponse,
  AdminUserSummary,
  CompactResponse,
  ConversationSummary,
  CreateApiKeyResponse,
  CurrentUser,
  ListApiKeysResponse,
  ListProviderGrantsResponse,
  LoginResponse,
  MemoryListResponse,
  Message,
  PersonalityFile,
  PluginListResponse,
  PluginStoreResponse,
  ProviderGrantInfo,
  ProviderListResponse,
  RefreshResponse,
  SetupStatusResponse,
  SseContext,
  SseDone,
  SseInterrupted,
  SseStarted,
  SseToolCall,
  SseToolResult,
} from "./types";

class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function req<T>(path: string, init: RequestInit = {}, token?: string): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(init.headers as Record<string, string> | undefined),
  };

  const resp = await fetch(`/api${path}`, { ...init, headers });

  if (!resp.ok) {
    const text = await resp.text().catch(() => "");
    let message = `Server error ${resp.status}`;
    try {
      message = (JSON.parse(text) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message, resp.status);
  }

  if (resp.status === 204) return undefined as T;
  const text = await resp.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/**
 * Authenticate with email and password.
 * @returns The login response containing access and refresh tokens.
 */
export function login(email: string, password: string): Promise<LoginResponse> {
  return req("/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}

/**
 * Invalidate the session and clear the auth cookie.
 * The refresh token is read from the HttpOnly cookie set at login.
 * @param token - The current access token.
 */
export function logout(token: string): Promise<void> {
  return req(
    "/auth/logout",
    {
      method: "POST",
      body: JSON.stringify({}),
    },
    token,
  );
}

/**
 * Exchange the session cookie for new tokens (access + refresh).
 * The refresh token is read from the HttpOnly cookie set at login.
 * @returns A new token pair.
 */
export function refresh(): Promise<RefreshResponse> {
  return req("/auth/refresh", {
    method: "POST",
    body: JSON.stringify({}),
  });
}

/**
 * Fetch the current user's profile.
 * @param token - Access token.
 */
export function getCurrentUser(token: string): Promise<CurrentUser> {
  return req("/auth/me", {}, token);
}

// ── Setup ─────────────────────────────────────────────────────────────────────

/**
 * Check whether initial setup is still required.
 * @returns Setup status response.
 */
export function setupStatus(): Promise<SetupStatusResponse> {
  return req("/setup");
}

/**
 * Complete initial admin setup.
 * @param args.token - Setup token (from the initial config page).
 * @param args.email - Admin email.
 * @param args.password - Admin password.
 * @param args.display_name - Optional display name.
 * @returns Login response on success.
 */
export function setupAdmin(args: {
  token: string;
  email: string;
  password: string;
  display_name?: string;
}): Promise<LoginResponse> {
  return req("/setup", { method: "POST", body: JSON.stringify(args) });
}

// ── Conversations ─────────────────────────────────────────────────────────────

/**
 * List all conversations for the authenticated user.
 * @param token - Access token.
 */
export function listConversations(token: string): Promise<ConversationSummary[]> {
  return req("/conversations", {}, token);
}

/**
 * Fetch all messages in a conversation.
 * @param id - Conversation ID.
 * @param token - Access token.
 */
export function getMessages(id: string, token: string): Promise<Message[]> {
  return req(`/conversations/${id}/messages`, {}, token);
}

/**
 * Delete a conversation and all its messages.
 * @param id - Conversation ID.
 * @param token - Access token.
 */
export function deleteConversation(id: string, token: string): Promise<void> {
  return req(`/conversations/${id}`, { method: "DELETE" }, token);
}

/**
 * Rename a conversation.
 * @param id - Conversation ID.
 * @param title - The new title.
 * @param token - Access token.
 */
export function renameConversation(id: string, title: string, token: string): Promise<void> {
  return req(`/conversations/${id}`, { method: "PATCH", body: JSON.stringify({ title }) }, token);
}

/**
 * Cancel an in-progress generation for a conversation.
 * @param id - Conversation ID.
 * @param token - Access token.
 */
export function cancelGeneration(id: string, token: string): Promise<void> {
  return req(`/conversations/${id}/cancel`, { method: "POST" }, token);
}

/**
 * Compacts a conversation's history by summarising the oldest messages.
 * @param id - Conversation ID.
 * @param token - Access token.
 * @returns Compaction result with the number of messages compacted and summary length.
 */
export function compactConversation(id: string, token: string): Promise<CompactResponse> {
  return req(`/conversations/${id}/compact`, { method: "POST" }, token);
}

/**
 * List available LLM providers.
 * @param token - Access token.
 */
export function listProviders(token: string): Promise<ProviderListResponse> {
  return req("/providers", {}, token);
}

/**
 * Ask the model to retry a previous message, optionally with a different provider.
 * @param args.conversationId - The conversation ID.
 * @param args.messageId - The message to retry from.
 * @param args.providerId - Optional provider override.
 * @param args.token - Access token.
 * @param args.onStarted - Called when the SSE stream starts.
 * @param args.onContext - Called with context window usage data.
 * @param args.onChunk - Called with each text delta.
 * @param args.onToolCall - Optional, called when the model requests a tool.
 * @param args.onToolResult - Optional, called with the result of a tool callback.
 * @param args.onDone - Called when the stream completes.
 * @param args.onInterrupted - Optional, called when the tool-call limit is reached and the response pauses.
 * @param args.signal - Optional AbortSignal to cancel the stream.
 */
export function retryMessage(args: {
  conversationId: string;
  messageId: string;
  providerId?: string;
  token: string;
  onStarted: (started: SseStarted) => void;
  onContext?: (context: SseContext) => void;
  onChunk: (delta: string) => void;
  onToolCall?: (call: SseToolCall) => void;
  onToolResult?: (result: SseToolResult) => void;
  onDone: (done: SseDone) => void;
  onInterrupted?: (data: SseInterrupted) => void;
  signal?: AbortSignal;
}): Promise<void> {
  return consumeChatStream({
    url: `/api/conversations/${args.conversationId}/messages/${args.messageId}/retry`,
    token: args.token,
    method: "POST",
    body: JSON.stringify({ provider_id: args.providerId ?? null }),
    onStarted: args.onStarted,
    onContext: args.onContext,
    onChunk: args.onChunk,
    onToolCall: args.onToolCall,
    onToolResult: args.onToolResult,
    onDone: args.onDone,
    onInterrupted: args.onInterrupted,
    signal: args.signal,
  });
}

// ── Profile / me ─────────────────────────────────────────────────────────────

/**
 * Update the current user's profile fields.
 * @param data - Partial profile fields to update.
 * @param token - Access token.
 */
export function updateMe(
  data: { display_name?: string | null; timezone?: string; memory_leaning?: string },
  token: string,
): Promise<void> {
  return req("/auth/me", { method: "PATCH", body: JSON.stringify(data) }, token);
}

/**
 * Change the current user's password.
 * @param data.current_password - Current password.
 * @param data.new_password - New password.
 * @param token - Access token.
 */
export function changePassword(
  data: { current_password: string; new_password: string },
  token: string,
): Promise<void> {
  return req("/auth/me/password", { method: "POST", body: JSON.stringify(data) }, token);
}

// ── API keys ──────────────────────────────────────────────────────────────────

/**
 * List all API keys for the current user.
 * @param token - Access token.
 */
export function listApiKeys(token: string): Promise<ListApiKeysResponse> {
  return req("/auth/api-keys", {}, token);
}

/**
 * Create a new API key.
 * @param data.name - Human-readable name for the key.
 * @param data.expires_at - Optional expiration timestamp.
 * @param token - Access token.
 * @returns The created key with the raw key value (shown only once).
 */
export function createApiKey(
  data: { name: string; expires_at?: string },
  token: string,
): Promise<CreateApiKeyResponse> {
  return req("/auth/api-keys", { method: "POST", body: JSON.stringify(data) }, token);
}

/**
 * Revoke (delete) an API key.
 * @param id - The key ID to revoke.
 * @param token - Access token.
 */
export function revokeApiKey(id: string, token: string): Promise<void> {
  return req(`/auth/api-keys/${id}`, { method: "DELETE" }, token);
}

// ── Personality ────────────────────────────────────────────────────────────────

/**
 * Fetch a personality file by name.
 * @param name - Personality file name.
 * @param token - Access token.
 */
export function getPersonality(name: string, token: string): Promise<PersonalityFile> {
  return req(`/personality/${name}`, {}, token);
}

/**
 * Create or update a personality file.
 * @param name - Personality file name.
 * @param content - File content.
 * @param token - Access token.
 */
export function putPersonality(name: string, content: string, token: string): Promise<void> {
  return req(`/personality/${name}`, { method: "PUT", body: JSON.stringify({ content }) }, token);
}

// ── Memory ────────────────────────────────────────────────────────────────────

/**
 * List all memory files.
 * @param token - Access token.
 */
export function listMemory(token: string): Promise<MemoryListResponse> {
  return req("/memory", {}, token);
}

/**
 * Fetch a single memory file's content.
 * @param path - File path.
 * @param token - Access token.
 */
export function getMemoryFile(
  path: string,
  token: string,
): Promise<{ path: string; content: string }> {
  return req(`/memory/${path}`, {}, token);
}

/**
 * Create or update a memory file.
 * @param path - File path.
 * @param content - File content.
 * @param token - Access token.
 */
export function putMemoryFile(path: string, content: string, token: string): Promise<void> {
  return req(`/memory/${path}`, { method: "PUT", body: JSON.stringify({ content }) }, token);
}

/**
 * Delete a memory file.
 * @param path - File path.
 * @param token - Access token.
 */
export function deleteMemoryFile(path: string, token: string): Promise<void> {
  return req(`/memory/${path}`, { method: "DELETE" }, token);
}

// ── Admin users ───────────────────────────────────────────────────────────────

/**
 * List all users (admin-only).
 * @param token - Access token.
 */
export function listAdminUsers(token: string): Promise<AdminListUsersResponse> {
  return req("/admin/users", {}, token);
}

/**
 * Create a new user (admin-only).
 * @param data.email - New user's email.
 * @param data.password - New user's password.
 * @param data.display_name - Optional display name.
 * @param token - Access token.
 */
export function createAdminUser(
  data: { email: string; password: string; display_name?: string },
  token: string,
): Promise<AdminUserSummary> {
  return req("/admin/users", { method: "POST", body: JSON.stringify(data) }, token);
}

/**
 * Activate or deactivate a user (admin-only).
 * @param id - User ID.
 * @param data.status - New status ("active" or "deactivated").
 * @param token - Access token.
 */
export function updateAdminUser(
  id: string,
  data: { status: "active" | "deactivated" },
  token: string,
): Promise<void> {
  return req(`/admin/users/${id}`, { method: "PATCH", body: JSON.stringify(data) }, token);
}

/**
 * Reset a user's password (admin-only).
 * @param id - User ID.
 * @param password - New password.
 * @param token - Access token.
 */
export function resetAdminUserPassword(id: string, password: string, token: string): Promise<void> {
  return req(
    `/admin/users/${id}/password-reset`,
    { method: "POST", body: JSON.stringify({ password }) },
    token,
  );
}

/**
 * Get provider grants for a user (admin-only).
 * @param userId - User ID.
 * @param token - Access token.
 */
export function listProviderGrants(
  userId: string,
  token: string,
): Promise<ListProviderGrantsResponse> {
  return req(`/admin/users/${userId}/providers`, {}, token);
}

/**
 * Set provider grants for a user (admin-only).
 * @param userId - User ID.
 * @param grants - Updated list of provider grants.
 * @param token - Access token.
 */
export function setProviderGrants(
  userId: string,
  grants: ProviderGrantInfo[],
  token: string,
): Promise<void> {
  return req(
    `/admin/users/${userId}/providers`,
    { method: "PUT", body: JSON.stringify({ grants }) },
    token,
  );
}

// ── Admin configuration ──────────────────────────────────────────────────────

/**
 * Fetch the full admin configuration (admin-only).
 * @param token - Access token.
 */
export function getAdminConfig(token: string): Promise<AdminConfig> {
  return req("/admin/config", {}, token);
}

/**
 * Update the admin configuration (admin-only).
 * @param config - The updated config payload.
 * @param token - Access token.
 */
export function updateAdminConfig(config: AdminConfigUpdate, token: string): Promise<AdminConfig> {
  return req(
    "/admin/config",
    {
      method: "PUT",
      body: JSON.stringify(config),
    },
    token,
  );
}

// ── Plugins ──────────────────────────────────────────────────────────────────

/**
 * List installed plugins and capabilities.
 * @param token - Access token.
 */
export async function listPlugins(token: string): Promise<PluginListResponse> {
  return req<PluginListResponse>("/plugins", {}, token);
}

/**
 * List available plugins from the store / registry.
 * @param token - Access token.
 */
export function listPluginStore(token: string): Promise<PluginStoreResponse> {
  return req("/plugins/store", {}, token);
}

/**
 * Enable or disable an installed plugin.
 * @param id - Plugin ID.
 * @param enabled - Whether the plugin should be enabled.
 * @param token - Access token.
 */
export function setPluginEnabled(id: string, enabled: boolean, token: string): Promise<void> {
  return req(
    `/plugins/${id}/enable`,
    {
      method: "PUT",
      body: JSON.stringify({ enabled }),
    },
    token,
  );
}

/**
 * Install a plugin from the store.
 * @param id - Plugin ID.
 * @param permissions - Permissions to grant the plugin.
 * @param token - Access token.
 */
export function installPlugin(id: string, permissions: string[], token: string): Promise<void> {
  return req(
    `/plugins/${id}/install`,
    {
      method: "POST",
      body: JSON.stringify({ permissions }),
    },
    token,
  );
}

/**
 * Update an installed plugin to the latest version.
 * @param id - Plugin ID.
 * @param permissions - Permissions to grant the updated plugin.
 * @param token - Access token.
 */
export function updatePlugin(id: string, permissions: string[], token: string): Promise<void> {
  return req(
    `/plugins/${id}/update`,
    {
      method: "POST",
      body: JSON.stringify({ permissions }),
    },
    token,
  );
}

/**
 * Rollback a plugin to its previous version.
 * @param id - Plugin ID.
 * @param token - Access token.
 */
export function rollbackPlugin(id: string, token: string): Promise<void> {
  return req(`/plugins/${id}/rollback`, { method: "POST" }, token);
}

/**
 * Uninstall a plugin.
 * @param id - Plugin ID.
 * @param token - Access token.
 */
export function uninstallPlugin(id: string, token: string): Promise<void> {
  return req(`/plugins/${id}`, { method: "DELETE" }, token);
}

/**
 * Update a plugin's configuration values.
 * @param id - Plugin ID.
 * @param values - Key-value config values to set.
 * @param token - Access token.
 */
export function configurePlugin(
  id: string,
  values: Record<string, unknown>,
  token: string,
): Promise<void> {
  return req(
    `/plugins/${id}/config`,
    {
      method: "PUT",
      body: JSON.stringify({ values }),
    },
    token,
  );
}

// ── TTS ───────────────────────────────────────────────────────────────────────

/**
 * Convert text to speech and return the audio as a Blob.
 * @param text - The text to synthesize.
 * @param token - Access token.
 * @param voice - Optional voice ID.
 * @returns An audio Blob.
 */
export async function tts(text: string, token: string, voice?: string): Promise<Blob> {
  const resp = await fetch("/api/tts", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ text, voice }),
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    let message = `TTS error ${resp.status}`;
    try {
      message = (JSON.parse(body) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message, resp.status);
  }

  return resp.blob();
}

// ── Content rendering feedback ─────────────────────────────────────────────────

/**
 * Submit rendering feedback (best-effort, failures are silently ignored).
 * @param conversationId - The conversation ID.
 * @param token - Access token.
 * @param type - Feedback type.
 * @param message - Feedback message.
 */
export async function submitFeedback(
  conversationId: string,
  token: string,
  type: string,
  message: string,
): Promise<void> {
  const resp = await fetch(`/api/conversations/${conversationId}/feedback`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ type, message }),
  });

  if (!resp.ok) {
    // Silently ignore — feedback is best-effort
    console.warn("Failed to submit rendering feedback:", resp.status);
  }
}

// ── Chat (SSE) ────────────────────────────────────────────────────────────────

/**
 * Start or continue a chat conversation via SSE streaming.
 * @param args.message - The user's chat message.
 * @param args.conversation_id - Optional existing conversation ID (omit to start a new one).
 * @param args.provider_id - Optional provider override.
 * @param args.token - Access token.
 * @param args.onStarted - Called when the SSE stream starts.
 * @param args.onContext - Called with context window usage data.
 * @param args.onChunk - Called with each text delta.
 * @param args.onToolCall - Optional, called when the model requests a tool.
 * @param args.onToolResult - Optional, called with the result of a tool callback.
 * @param args.onDone - Called when the stream completes.
 * @param args.onInterrupted - Optional, called when the tool-call limit is reached and the response pauses.
 * @param args.signal - Optional AbortSignal to cancel the stream.
 */
export async function chat(args: {
  message: string;
  conversation_id?: string;
  provider_id?: string;
  token: string;
  onStarted: (started: SseStarted) => void;
  onContext?: (context: SseContext) => void;
  onChunk: (delta: string) => void;
  onToolCall?: (call: SseToolCall) => void;
  onToolResult?: (result: SseToolResult) => void;
  onDone: (done: SseDone) => void;
  onInterrupted?: (data: SseInterrupted) => void;
  signal?: AbortSignal;
}): Promise<void> {
  const {
    message,
    conversation_id,
    provider_id,
    token,
    onStarted,
    onContext,
    onChunk,
    onToolCall,
    onToolResult,
    onDone,
    onInterrupted,
    signal,
  } = args;
  return consumeChatStream({
    url: "/api/chat",
    token,
    method: "POST",
    body: JSON.stringify({ message, conversation_id, provider_id }),
    onStarted,
    onContext,
    onChunk,
    onToolCall,
    onToolResult,
    onDone,
    onInterrupted,
    signal,
  });
}

async function consumeChatStream(args: {
  url: string;
  token: string;
  method: "POST";
  body?: string;
  onStarted: (started: SseStarted) => void;
  onContext?: (context: SseContext) => void;
  onChunk: (delta: string) => void;
  onToolCall?: (call: SseToolCall) => void;
  onToolResult?: (result: SseToolResult) => void;
  onDone: (done: SseDone) => void;
  onInterrupted?: (data: SseInterrupted) => void;
  signal?: AbortSignal;
}): Promise<void> {
  const resp = await fetch(args.url, {
    method: args.method,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${args.token}`,
    },
    body: args.body,
    signal: args.signal,
  });

  if (!resp.ok) {
    const text = await resp.text().catch(() => "");
    let message = `Server error ${resp.status}`;
    try {
      message = (JSON.parse(text) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message, resp.status);
  }

  const reader = resp.body!.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let terminal = false;

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      buffer += decoder.decode();
      break;
    }

    buffer += decoder.decode(value, { stream: true });
    const events = takeSseEvents(buffer);
    buffer = events.remainder;
    for (const event of events.parts) {
      terminal = processChatEvent(event, args) || terminal;
    }
  }

  if (buffer.trim()) {
    terminal = processChatEvent(buffer, args) || terminal;
  }
  if (!terminal) {
    throw new ApiError("Response stream ended before the server reported completion.", 502);
  }
}

function takeSseEvents(buffer: string): { parts: string[]; remainder: string } {
  const normalized = buffer.replaceAll("\r\n", "\n");
  const parts = normalized.split("\n\n");
  return {
    remainder: parts.pop() ?? "",
    parts,
  };
}

function processChatEvent(
  raw: string,
  handlers: {
    onStarted: (started: SseStarted) => void;
    onContext?: (context: SseContext) => void;
    onChunk: (delta: string) => void;
    onToolCall?: (call: SseToolCall) => void;
    onToolResult?: (result: SseToolResult) => void;
    onDone: (done: SseDone) => void;
    onInterrupted?: (data: SseInterrupted) => void;
  },
): boolean {
  let eventName = "";
  const data: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    if (line.startsWith("event:")) eventName = line.slice(6).trim();
    else if (line.startsWith("data:")) data.push(line.slice(5).trimStart());
  }
  const eventData = data.join("\n");

  if (eventName === "started") {
    handlers.onStarted(JSON.parse(eventData) as SseStarted);
  } else if (eventName === "context") {
    handlers.onContext?.(JSON.parse(eventData) as SseContext);
  } else if (eventName === "chunk") {
    handlers.onChunk((JSON.parse(eventData) as { delta: string }).delta);
  } else if (eventName === "tool_call") {
    handlers.onToolCall?.(JSON.parse(eventData) as SseToolCall);
  } else if (eventName === "tool_result") {
    handlers.onToolResult?.(JSON.parse(eventData) as SseToolResult);
  } else if (eventName === "done") {
    handlers.onDone(JSON.parse(eventData) as SseDone);
    return true;
  } else if (eventName === "interrupted") {
    handlers.onInterrupted?.(JSON.parse(eventData) as SseInterrupted);
    return true;
  } else if (eventName === "error") {
    let message = eventData;
    try {
      message = (JSON.parse(eventData) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message || "Response generation failed.", 500);
  }
  return false;
}

/** Error thrown by API calls when the server returns a non-OK status. */
export { ApiError };
