'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR, { useSWRConfig } from 'swr';
import { chat, getMessages, retryMessage, cancelGeneration, ApiError } from '@/lib/api';
import { useAuth } from '@/context/auth';
import type { Message, SseDone, SseStarted } from '@/lib/types';
import MessageBubble from './message-bubble';
import ChatInput from './chat-input';
import { Pencil, Trash2 } from 'lucide-react';

interface QueueItem {
  id: string;
  text: string;
}

interface Props {
  conversationId: string | null;
  onConversationCreated: (id: string) => void;
}

const isActive = (message: Message) => (
  message.status === 'pending' || message.status === 'streaming'
);

let nextQueueId = 1;

export default function ChatWindow({ conversationId, onConversationCreated }: Props) {
  const { accessToken, refreshAccessToken } = useAuth();
  const router = useRouter();
  const { mutate: mutateGlobal } = useSWRConfig();
  const bottomRef = useRef<HTMLDivElement>(null);
  const [inputValue, setInputValue] = useState('');
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const abortRef = useRef<AbortController | null>(null);
  const processingRef = useRef(false);

  const messageKey = accessToken && conversationId
    ? [`/conversations/${conversationId}/messages`, accessToken] as const
    : null;
  const {
    data: messages = [],
    error: historyError,
    mutate: refreshMessages,
    isLoading: historyLoading,
  } = useSWR<Message[]>(
    messageKey,
    ([, token]) => getMessages(conversationId!, token as string),
    {
      refreshInterval: data => data?.some(isActive) ? 500 : 0,
      revalidateOnFocus: true,
      keepPreviousData: false,
    },
  );

  // Track the actively-generating conversation separately so we never show
  // the stop button for stale data from a different conversation.
  const generationConvRef = useRef<string | null>(null);
  const active = (conversationId !== null && generationConvRef.current === conversationId) || messages.some(isActive);

  // Reset generation tracking when switching conversations
  const prevConvRef = useRef(conversationId);
  useEffect(() => {
    if (prevConvRef.current !== conversationId) {
      generationConvRef.current = null;
      prevConvRef.current = conversationId;
      setQueue([]);
    }
  }, [conversationId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, queue]);

  const refreshConversation = useCallback(async (id: string) => {
    if (!accessToken) return;
    await Promise.all([
      mutateGlobal([`/conversations/${id}/messages`, accessToken]),
      mutateGlobal(['/api/conversations', accessToken]),
    ]);
  }, [accessToken, mutateGlobal]);

  const streamHandlers = useCallback((fallbackConversationId?: string) => {
    let activeConversationId = fallbackConversationId;
    return {
      onStarted: (started: SseStarted) => {
        activeConversationId = started.conversation_id;
        if (!fallbackConversationId) {
          onConversationCreated(started.conversation_id);
          router.replace(`/chat/?id=${started.conversation_id}`, { scroll: false });
        }
        void refreshConversation(started.conversation_id);
      },
      onChunk: (_delta: string) => {
        const id = activeConversationId;
        if (id) void refreshConversation(id);
      },
      onDone: (done: SseDone) => {
        void refreshConversation(done.conversation_id);
      },
    };
  }, [onConversationCreated, refreshConversation, router]);

  const runWithRefresh = useCallback(async (
    operation: (token: string) => Promise<void>,
  ) => {
    if (!accessToken) {
      router.replace('/login/');
      return;
    }
    try {
      await operation(accessToken);
    } catch (caught) {
      if (caught instanceof ApiError && caught.status === 401) {
        const fresh = await refreshAccessToken();
        if (fresh) {
          await operation(fresh);
          return;
        }
        router.replace('/login/');
        return;
      }
      throw caught;
    }
  }, [accessToken, refreshAccessToken, router]);

  const sendMessage = useCallback(async (text: string) => {
    setError('');
    const controller = new AbortController();
    abortRef.current = controller;
    generationConvRef.current = conversationId;
    try {
      await runWithRefresh(token => chat({
        message: text,
        conversation_id: conversationId ?? undefined,
        token,
        ...streamHandlers(conversationId ?? undefined),
        signal: controller.signal,
      }));
    } catch (caught) {
      if (caught instanceof DOMException && caught.name === 'AbortError') {
        return;
      }
      setError(caught instanceof ApiError ? caught.message : 'Something went wrong.');
      if (conversationId) await refreshMessages();
    } finally {
      if (generationConvRef.current === conversationId) {
        generationConvRef.current = null;
      }
      if (abortRef.current === controller) {
        abortRef.current = null;
      }
    }
  }, [conversationId, refreshMessages, runWithRefresh, streamHandlers]);

  // Auto-process queue: when no active generation, send the oldest queued item
  useEffect(() => {
    if (!active && queue.length > 0 && !processingRef.current) {
      processingRef.current = true;
      const [first, ...rest] = queue;
      setQueue(rest);
      sendMessage(first.text).finally(() => {
        processingRef.current = false;
      });
    }
  }, [active, queue, sendMessage]);

  const handleSend = useCallback((text: string) => {
    const id = String(nextQueueId++);
    setQueue(prev => [...prev, { id, text }]);
  }, []);

  const handleStop = useCallback(async () => {
    setError('');
    generationConvRef.current = null;
    abortRef.current?.abort();
    abortRef.current = null;
    if (conversationId && accessToken) {
      try {
        await cancelGeneration(conversationId, accessToken);
        await refreshMessages();
      } catch (err) {
        if (err instanceof ApiError && err.status === 401) {
          const fresh = await refreshAccessToken();
          if (fresh) {
            await cancelGeneration(conversationId, fresh);
            await refreshMessages();
          }
        }
      }
    }
  }, [conversationId, accessToken, refreshMessages, refreshAccessToken]);

  const handleEditQueueItem = useCallback((id: string) => {
    const item = queue.find(q => q.id === id);
    if (item) {
      setInputValue(item.text);
      setQueue(prev => prev.filter(q => q.id !== id));
    }
  }, [queue]);

  const handleDeleteQueueItem = useCallback((id: string) => {
    setQueue(prev => prev.filter(q => q.id !== id));
  }, []);

  const retry = useCallback(async (messageId: string) => {
    if (!conversationId) return;
    setRetryingId(messageId);
    setError('');
    const controller = new AbortController();
    abortRef.current = controller;
    generationConvRef.current = conversationId;
    try {
      await runWithRefresh(token => retryMessage({
        conversationId,
        messageId,
        token,
        ...streamHandlers(conversationId),
        signal: controller.signal,
      }));
    } catch (caught) {
      if (caught instanceof DOMException && caught.name === 'AbortError') {
        return;
      }
      setError(caught instanceof ApiError ? caught.message : 'Could not retry the response.');
      await refreshMessages();
    } finally {
      if (generationConvRef.current === conversationId) {
        generationConvRef.current = null;
      }
      if (abortRef.current === controller) {
        abortRef.current = null;
      }
      setRetryingId(null);
    }
  }, [conversationId, refreshMessages, runWithRefresh, streamHandlers]);

  const empty = !historyLoading && messages.length === 0 && queue.length === 0;
  const visibleError = error || (historyError instanceof Error ? historyError.message : '');

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto">
        {empty ? (
          <div className="flex h-full items-center justify-center">
            <div className="space-y-2 text-center">
              <p className="text-xl font-medium text-slate-400 dark:text-slate-500">
                What can I help you with?
              </p>
              <p className="text-sm text-slate-400 dark:text-slate-500">
                Start typing below to begin a conversation.
              </p>
            </div>
          </div>
        ) : (
          <div className="mx-auto max-w-3xl space-y-4 px-4 py-6">
            {messages.map(message => (
              <MessageBubble
                key={message.id}
                message={message}
                onRetry={retry}
                retrying={retryingId === message.id}
                accessToken={accessToken ?? ''}
              />
            ))}
            {visibleError && (
              <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300">
                {visibleError}
              </div>
            )}
            <div ref={bottomRef} />
          </div>
        )}
      </div>

      {queue.length > 0 && (
        <div className="mx-auto w-full max-w-3xl space-y-2 px-4 pb-2">
          {queue.map(item => (
            <div
              key={item.id}
              className="flex items-start gap-2 rounded-xl border border-dashed border-slate-300 bg-slate-50/50 px-4 py-3 text-sm text-slate-600 dark:border-slate-600 dark:bg-slate-800/30 dark:text-slate-400"
            >
              <div className="flex-1 whitespace-pre-wrap leading-relaxed">{item.text}</div>
              <div className="flex shrink-0 gap-1">
                <button
                  type="button"
                  onClick={() => handleEditQueueItem(item.id)}
                  className="rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-slate-200 hover:text-slate-600 dark:hover:bg-slate-700 dark:hover:text-slate-300"
                  aria-label="Edit message"
                >
                  <Pencil size={14} />
                </button>
                <button
                  type="button"
                  onClick={() => handleDeleteQueueItem(item.id)}
                  className="rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-red-100 hover:text-red-600 dark:hover:bg-red-900/30 dark:hover:text-red-400"
                  aria-label="Delete message"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="mx-auto w-full max-w-3xl px-4 pb-4 pt-2">
        <ChatInput
          value={inputValue}
          onChange={setInputValue}
          onSend={handleSend}
          onStop={handleStop}
          disabled={retryingId !== null}
          active={active}
        />
      </div>
    </div>
  );
}
