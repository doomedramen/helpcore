import type {
  AdminConfig,
  AdminConfigUpdate,
  ConversationSummary,
  CurrentUser,
  LoginResponse,
  Message,
  PluginInfo,
  PluginStoreResponse,
  RefreshResponse,
  SetupStatusResponse,
  SseDone,
  SseStarted,
} from './types';

class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function req<T>(path: string, init: RequestInit = {}, token?: string): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(init.headers as Record<string, string> | undefined),
  };

  const resp = await fetch(`/api${path}`, { ...init, headers });

  if (!resp.ok) {
    const text = await resp.text().catch(() => '');
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

export function login(email: string, password: string): Promise<LoginResponse> {
  return req('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
}

export function logout(refreshToken: string, token: string): Promise<void> {
  return req('/auth/logout', {
    method: 'POST',
    body: JSON.stringify({ refresh_token: refreshToken }),
  }, token);
}

export function refresh(refreshToken: string): Promise<RefreshResponse> {
  return req('/auth/refresh', {
    method: 'POST',
    body: JSON.stringify({ refresh_token: refreshToken }),
  });
}

export function getCurrentUser(token: string): Promise<CurrentUser> {
  return req('/auth/me', {}, token);
}

// ── Setup ─────────────────────────────────────────────────────────────────────

export function setupStatus(): Promise<SetupStatusResponse> {
  return req('/setup');
}

export function setupAdmin(args: {
  token: string;
  email: string;
  password: string;
  display_name?: string;
}): Promise<LoginResponse> {
  return req('/setup', { method: 'POST', body: JSON.stringify(args) });
}

// ── Conversations ─────────────────────────────────────────────────────────────

export function listConversations(token: string): Promise<ConversationSummary[]> {
  return req('/conversations', {}, token);
}

export function getMessages(id: string, token: string): Promise<Message[]> {
  return req(`/conversations/${id}/messages`, {}, token);
}

export function deleteConversation(id: string, token: string): Promise<void> {
  return req(`/conversations/${id}`, { method: 'DELETE' }, token);
}

export function cancelGeneration(id: string, token: string): Promise<void> {
  return req(`/conversations/${id}/cancel`, { method: 'POST' }, token);
}

export function retryMessage(args: {
  conversationId: string;
  messageId: string;
  token: string;
  onStarted: (started: SseStarted) => void;
  onChunk: (delta: string) => void;
  onDone: (done: SseDone) => void;
  signal?: AbortSignal;
}): Promise<void> {
  return consumeChatStream({
    url: `/api/conversations/${args.conversationId}/messages/${args.messageId}/retry`,
    token: args.token,
    method: 'POST',
    onStarted: args.onStarted,
    onChunk: args.onChunk,
    onDone: args.onDone,
    signal: args.signal,
  });
}

// ── Admin configuration ──────────────────────────────────────────────────────

export function getAdminConfig(token: string): Promise<AdminConfig> {
  return req('/admin/config', {}, token);
}

export function updateAdminConfig(config: AdminConfigUpdate, token: string): Promise<AdminConfig> {
  return req('/admin/config', {
    method: 'PUT',
    body: JSON.stringify(config),
  }, token);
}

// ── Plugins ──────────────────────────────────────────────────────────────────

export async function listPlugins(token: string): Promise<PluginInfo[]> {
  const response = await req<{ plugins: PluginInfo[] }>('/plugins', {}, token);
  return response.plugins;
}

export function listPluginStore(token: string): Promise<PluginStoreResponse> {
  return req('/plugins/store', {}, token);
}

export function setPluginEnabled(id: string, enabled: boolean, token: string): Promise<void> {
  return req(`/plugins/${id}/enable`, {
    method: 'PUT',
    body: JSON.stringify({ enabled }),
  }, token);
}

export function installPlugin(id: string, permissions: string[], token: string): Promise<void> {
  return req(`/plugins/${id}/install`, {
    method: 'POST',
    body: JSON.stringify({ permissions }),
  }, token);
}

export function updatePlugin(id: string, permissions: string[], token: string): Promise<void> {
  return req(`/plugins/${id}/update`, {
    method: 'POST',
    body: JSON.stringify({ permissions }),
  }, token);
}

export function rollbackPlugin(id: string, token: string): Promise<void> {
  return req(`/plugins/${id}/rollback`, { method: 'POST' }, token);
}

export function uninstallPlugin(id: string, token: string): Promise<void> {
  return req(`/plugins/${id}`, { method: 'DELETE' }, token);
}

export function configurePlugin(
  id: string,
  values: Record<string, unknown>,
  token: string,
): Promise<void> {
  return req(`/plugins/${id}/config`, {
    method: 'PUT',
    body: JSON.stringify({ values }),
  }, token);
}

// ── TTS ───────────────────────────────────────────────────────────────────────

export async function tts(text: string, token: string, voice?: string): Promise<Blob> {
  const resp = await fetch('/api/tts', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ text, voice }),
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => '');
    let message = `TTS error ${resp.status}`;
    try {
      message = (JSON.parse(body) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message, resp.status);
  }

  return resp.blob();
}

// ── Chat (SSE) ────────────────────────────────────────────────────────────────

export async function chat(args: {
  message: string;
  conversation_id?: string;
  token: string;
  onStarted: (started: SseStarted) => void;
  onChunk: (delta: string) => void;
  onDone: (done: SseDone) => void;
  signal?: AbortSignal;
}): Promise<void> {
  const { message, conversation_id, token, onStarted, onChunk, onDone, signal } = args;
  return consumeChatStream({
    url: '/api/chat',
    token,
    method: 'POST',
    body: JSON.stringify({ message, conversation_id }),
    onStarted,
    onChunk,
    onDone,
    signal,
  });
}

async function consumeChatStream(args: {
  url: string;
  token: string;
  method: 'POST';
  body?: string;
  onStarted: (started: SseStarted) => void;
  onChunk: (delta: string) => void;
  onDone: (done: SseDone) => void;
  signal?: AbortSignal;
}): Promise<void> {
  const resp = await fetch(args.url, {
    method: args.method,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${args.token}`,
    },
    body: args.body,
    signal: args.signal,
  });

  if (!resp.ok) {
    const text = await resp.text().catch(() => '');
    let message = `Server error ${resp.status}`;
    try {
      message = (JSON.parse(text) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message, resp.status);
  }

  const reader = resp.body!.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
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
    throw new ApiError('Response stream ended before the server reported completion.', 502);
  }
}

function takeSseEvents(buffer: string): { parts: string[]; remainder: string } {
  const normalized = buffer.replaceAll('\r\n', '\n');
  const parts = normalized.split('\n\n');
  return {
    remainder: parts.pop() ?? '',
    parts,
  };
}

function processChatEvent(
  raw: string,
  handlers: {
    onStarted: (started: SseStarted) => void;
    onChunk: (delta: string) => void;
    onDone: (done: SseDone) => void;
  },
): boolean {
  let eventName = '';
  const data: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    if (line.startsWith('event:')) eventName = line.slice(6).trim();
    else if (line.startsWith('data:')) data.push(line.slice(5).trimStart());
  }
  const eventData = data.join('\n');

  if (eventName === 'started') {
    handlers.onStarted(JSON.parse(eventData) as SseStarted);
  } else if (eventName === 'chunk') {
    handlers.onChunk((JSON.parse(eventData) as { delta: string }).delta);
  } else if (eventName === 'done') {
    handlers.onDone(JSON.parse(eventData) as SseDone);
    return true;
  } else if (eventName === 'error') {
    let message = eventData;
    try {
      message = (JSON.parse(eventData) as { message?: string }).message ?? message;
    } catch {}
    throw new ApiError(message || 'Response generation failed.', 500);
  }
  return false;
}

export { ApiError };
