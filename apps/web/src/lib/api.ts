import type {
  ConversationSummary,
  LoginResponse,
  Message,
  RefreshResponse,
  SetupStatusResponse,
  SseDone,
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

  return resp.json() as Promise<T>;
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

// ── Chat (SSE) ────────────────────────────────────────────────────────────────

export async function chat(args: {
  message: string;
  conversation_id?: string;
  token: string;
  onChunk: (delta: string) => void;
  onDone: (done: SseDone) => void;
  signal?: AbortSignal;
}): Promise<void> {
  const { message, conversation_id, token, onChunk, onDone, signal } = args;

  const resp = await fetch('/api/chat', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ message, conversation_id }),
    signal,
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

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';

    for (const part of parts) {
      let eventName = '';
      let eventData = '';

      for (const line of part.split('\n')) {
        if (line.startsWith('event: ')) eventName = line.slice(7).trim();
        else if (line.startsWith('data: ')) eventData = line.slice(6).trim();
      }

      if (eventName === 'chunk') {
        const parsed = JSON.parse(eventData) as { delta: string };
        onChunk(parsed.delta);
      } else if (eventName === 'done') {
        onDone(JSON.parse(eventData) as SseDone);
      } else if (eventName === 'error') {
        throw new ApiError(eventData, 500);
      }
    }
  }
}

export { ApiError };
