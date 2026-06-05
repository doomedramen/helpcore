'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR from 'swr';
import { chat, getMessages, ApiError } from '@/lib/api';
import { useAuth } from '@/context/auth';
import type { Message } from '@/lib/types';
import MessageBubble from './message-bubble';
import ChatInput from './chat-input';

interface LocalMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
}

interface Props {
  conversationId: string | null;
  onConversationCreated: (id: string) => void;
}

export default function ChatWindow({ conversationId, onConversationCreated }: Props) {
  const { accessToken, refreshAccessToken } = useAuth();
  const router = useRouter();
  const bottomRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState('');

  // Fetch historical messages when conversation changes
  const { data: history } = useSWR<Message[]>(
    accessToken && conversationId ? [`/conversations/${conversationId}/messages`, accessToken] : null,
    ([, token]) => getMessages(conversationId!, token as string),
  );

  useEffect(() => {
    if (history) {
      setMessages(history.map(m => ({ ...m, streaming: false })));
    }
  }, [history]);

  // Clear messages when switching to a new (null) conversation
  useEffect(() => {
    if (!conversationId) setMessages([]);
  }, [conversationId]);

  // Scroll to bottom on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const sendMessage = useCallback(async (text: string) => {
    if (!accessToken) {
      router.replace('/login/');
      return;
    }

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    const userMsg: LocalMessage = { id: `user-${Date.now()}`, role: 'user', content: text };
    const assistantMsg: LocalMessage = { id: 'streaming', role: 'assistant', content: '', streaming: true };

    setMessages(prev => [...prev, userMsg, assistantMsg]);
    setStreaming(true);
    setError('');

    const tryChat = async (token: string): Promise<void> => {
      await chat({
        message: text,
        conversation_id: conversationId ?? undefined,
        token,
        signal: controller.signal,
        onChunk: delta => {
          setMessages(prev =>
            prev.map(m => m.id === 'streaming' ? { ...m, content: m.content + delta } : m),
          );
        },
        onDone: done => {
          setMessages(prev =>
            prev.map(m => m.id === 'streaming' ? { ...m, id: done.message_id, streaming: false } : m),
          );
          if (!conversationId) {
            onConversationCreated(done.conversation_id);
            router.replace(`/chat/?id=${done.conversation_id}`, { scroll: false });
          }
        },
      });
    };

    try {
      await tryChat(accessToken);
    } catch (err) {
      if ((err as Error).name === 'AbortError') return;

      // Try once to refresh the token on 401
      if (err instanceof ApiError && err.status === 401) {
        const fresh = await refreshAccessToken();
        if (fresh) {
          try {
            await tryChat(fresh);
            return;
          } catch {}
        }
        router.replace('/login/');
        return;
      }

      setMessages(prev => prev.filter(m => m.id !== 'streaming'));
      setError(err instanceof ApiError ? err.message : 'Something went wrong.');
    } finally {
      setStreaming(false);
    }
  }, [accessToken, conversationId, onConversationCreated, refreshAccessToken, router]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto">
        {messages.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center space-y-2">
              <p className="text-xl font-medium text-slate-400 dark:text-slate-500">What can I help you with?</p>
              <p className="text-sm text-slate-400 dark:text-slate-500">Start typing below to begin a conversation.</p>
            </div>
          </div>
        ) : (
          <div className="max-w-3xl mx-auto px-4 py-6 space-y-4">
            {messages.map(m => (
              <MessageBubble key={m.id} message={m} />
            ))}
            {error && (
              <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300">
                {error}
              </div>
            )}
            <div ref={bottomRef} />
          </div>
        )}
      </div>

      <div className="max-w-3xl mx-auto w-full">
        <ChatInput
          value={inputValue}
          onChange={setInputValue}
          onSend={sendMessage}
          disabled={streaming}
        />
      </div>
    </div>
  );
}
