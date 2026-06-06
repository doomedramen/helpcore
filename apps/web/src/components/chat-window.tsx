'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR, { useSWRConfig } from 'swr';
import { chat, getMessages, retryMessage, ApiError } from '@/lib/api';
import { useAuth } from '@/context/auth';
import type { Message, SseDone, SseStarted } from '@/lib/types';
import MessageBubble from './message-bubble';
import ChatInput from './chat-input';

interface Props {
  conversationId: string | null;
  onConversationCreated: (id: string) => void;
}

const isActive = (message: Message) => (
  message.status === 'pending' || message.status === 'streaming'
);

export default function ChatWindow({ conversationId, onConversationCreated }: Props) {
  const { accessToken, refreshAccessToken } = useAuth();
  const router = useRouter();
  const { mutate: mutateGlobal } = useSWRConfig();
  const bottomRef = useRef<HTMLDivElement>(null);
  const [inputValue, setInputValue] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [error, setError] = useState('');

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
    },
  );

  const active = messages.some(isActive);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

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
    setSubmitting(true);
    setError('');
    try {
      await runWithRefresh(token => chat({
        message: text,
        conversation_id: conversationId ?? undefined,
        token,
        ...streamHandlers(conversationId ?? undefined),
      }));
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.message : 'Something went wrong.');
      if (conversationId) await refreshMessages();
    } finally {
      setSubmitting(false);
    }
  }, [conversationId, refreshMessages, runWithRefresh, streamHandlers]);

  const retry = useCallback(async (messageId: string) => {
    if (!conversationId) return;
    setRetryingId(messageId);
    setError('');
    try {
      await runWithRefresh(token => retryMessage({
        conversationId,
        messageId,
        token,
        ...streamHandlers(conversationId),
      }));
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.message : 'Could not retry the response.');
      await refreshMessages();
    } finally {
      setRetryingId(null);
    }
  }, [conversationId, refreshMessages, runWithRefresh, streamHandlers]);

  const empty = !historyLoading && messages.length === 0;
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

      <div className="mx-auto w-full max-w-3xl px-4 pb-4 pt-2">
        <ChatInput
          value={inputValue}
          onChange={setInputValue}
          onSend={sendMessage}
          disabled={active || submitting || retryingId !== null}
        />
      </div>
    </div>
  );
}
