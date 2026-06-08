"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import {
  ApiError,
  cancelGeneration,
  chat,
  getMessages,
  listConversations,
  listPlugins,
  listProviders,
  retryMessage,
} from "@/lib/api";
import { useAuth } from "@/context/auth";
import type { ConversationSummary, Message, SseDone, SseStarted } from "@/lib/types";
import MessageBubble from "./message-bubble";
import ChatInput from "./chat-input";
import BrandMark from "./brand-mark";
import { ArrowUpRight, Pencil, Terminal, Trash2 } from "lucide-react";

interface QueueItem {
  id: string;
  text: string;
  providerId: string;
}

interface Props {
  conversationId: string | null;
  onConversationCreated: (id: string) => void;
}

const isActive = (message: Message) =>
  message.status === "pending" || message.status === "streaming";

let nextQueueId = 1;

export default function ChatWindow({ conversationId, onConversationCreated }: Props) {
  const { accessToken, currentUser, refreshAccessToken } = useAuth();
  const router = useRouter();
  const { mutate: mutateGlobal } = useSWRConfig();
  const bottomRef = useRef<HTMLDivElement>(null);
  const [inputValue, setInputValue] = useState("");
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [showToolLogs, setShowToolLogs] = useState(true);
  useEffect(() => {
    const stored = localStorage.getItem("showToolLogs");
    if (stored !== null) setShowToolLogs(stored === "true");
  }, []);
  const toggleToolLogs = useCallback(() => {
    setShowToolLogs((v) => {
      localStorage.setItem("showToolLogs", String(!v));
      return !v;
    });
  }, []);
  const [selectedProviderId, setSelectedProviderId] = useState("");
  const providerSelectionsRef = useRef<Record<string, string>>({});
  const abortRef = useRef<AbortController | null>(null);
  const processingRef = useRef(false);

  const messageKey =
    accessToken && conversationId
      ? ([`/conversations/${conversationId}/messages`, accessToken] as const)
      : null;
  const {
    data: messages = [],
    error: historyError,
    mutate: refreshMessages,
    isLoading: historyLoading,
  } = useSWR<Message[]>(messageKey, ([, token]) => getMessages(conversationId!, token as string), {
    refreshInterval: (data) => (data?.some(isActive) ? 500 : 0),
    revalidateOnFocus: true,
    keepPreviousData: false,
  });

  const { data: pluginCaps } = useSWR(
    accessToken ? ["/api/plugins/caps", accessToken] : null,
    ([, token]) => listPlugins(token),
  );
  const hasAudio = pluginCaps?.capabilities.includes("audio") ?? false;

  const { data: providerData } = useSWR(
    accessToken ? ["/api/providers", accessToken] : null,
    ([, token]) => listProviders(token),
  );
  const providers = useMemo(() => providerData?.providers ?? [], [providerData]);
  const { data: conversations } = useSWR<ConversationSummary[]>(
    accessToken ? ["/api/conversations", accessToken] : null,
    ([, token]) => listConversations(token as string),
  );

  useEffect(() => {
    if (!providerData || (conversationId && !conversations)) return;
    const key = conversationId ?? "__new__";
    const conversationProvider = conversationId
      ? conversations?.find((conversation) => conversation.id === conversationId)?.provider_id
      : null;
    const remembered = providerSelectionsRef.current[key] ?? conversationProvider;
    const selection = providers.some((provider) => provider.id === remembered)
      ? remembered!
      : (providers[0]?.id ?? "");
    providerSelectionsRef.current[key] = selection;
    setSelectedProviderId(selection);
  }, [conversationId, conversations, providerData, providers]);

  const handleProviderChange = useCallback(
    (providerId: string) => {
      providerSelectionsRef.current[conversationId ?? "__new__"] = providerId;
      setSelectedProviderId(providerId);
    },
    [conversationId],
  );

  // Track the actively-generating conversation separately so we never show
  // the stop button for stale data from a different conversation.
  const generationConvRef = useRef<string | null>(null);
  const active =
    (conversationId !== null && generationConvRef.current === conversationId) ||
    messages.some(isActive);

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
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, queue]);

  const refreshConversation = useCallback(
    async (id: string) => {
      if (!accessToken) return;
      await Promise.all([
        mutateGlobal([`/conversations/${id}/messages`, accessToken]),
        mutateGlobal(["/api/conversations", accessToken]),
      ]);
    },
    [accessToken, mutateGlobal],
  );

  const streamHandlers = useCallback(
    (fallbackConversationId: string | undefined, providerId: string) => {
      let activeConversationId = fallbackConversationId;
      return {
        onStarted: (started: SseStarted) => {
          activeConversationId = started.conversation_id;
          if (!fallbackConversationId) {
            providerSelectionsRef.current[started.conversation_id] = providerId;
            setSelectedProviderId(providerId);
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
    },
    [onConversationCreated, refreshConversation, router],
  );

  const runWithRefresh = useCallback(
    async (operation: (token: string) => Promise<void>) => {
      if (!accessToken) {
        router.replace("/login/");
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
          router.replace("/login/");
          return;
        }
        throw caught;
      }
    },
    [accessToken, refreshAccessToken, router],
  );

  const sendMessage = useCallback(
    async (item: QueueItem) => {
      setError("");
      const controller = new AbortController();
      abortRef.current = controller;
      generationConvRef.current = conversationId;
      try {
        await runWithRefresh((token) =>
          chat({
            message: item.text,
            conversation_id: conversationId ?? undefined,
            provider_id: item.providerId,
            token,
            ...streamHandlers(conversationId ?? undefined, item.providerId),
            signal: controller.signal,
          }),
        );
      } catch (caught) {
        if (caught instanceof DOMException && caught.name === "AbortError") {
          return;
        }
        setError(caught instanceof ApiError ? caught.message : "Something went wrong.");
        if (conversationId) await refreshMessages();
      } finally {
        if (generationConvRef.current === conversationId) {
          generationConvRef.current = null;
        }
        if (abortRef.current === controller) {
          abortRef.current = null;
        }
      }
    },
    [conversationId, refreshMessages, runWithRefresh, streamHandlers],
  );

  // Auto-process queue: when no active generation, send the oldest queued item
  useEffect(() => {
    if (!active && queue.length > 0 && !processingRef.current) {
      processingRef.current = true;
      const [first, ...rest] = queue;
      setQueue(rest);
      sendMessage(first).finally(() => {
        processingRef.current = false;
      });
    }
  }, [active, queue, sendMessage]);

  const handleSend = useCallback(
    (text: string) => {
      if (!selectedProviderId) return;
      const id = String(nextQueueId++);
      setQueue((prev) => [...prev, { id, text, providerId: selectedProviderId }]);
    },
    [selectedProviderId],
  );

  const handleStop = useCallback(async () => {
    setError("");
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

  const handleEditQueueItem = useCallback(
    (id: string) => {
      const item = queue.find((q) => q.id === id);
      if (item) {
        setInputValue(item.text);
        handleProviderChange(item.providerId);
        setQueue((prev) => prev.filter((q) => q.id !== id));
      }
    },
    [handleProviderChange, queue],
  );

  const handleDeleteQueueItem = useCallback((id: string) => {
    setQueue((prev) => prev.filter((q) => q.id !== id));
  }, []);

  const retry = useCallback(
    async (messageId: string) => {
      if (!conversationId) return;
      setRetryingId(messageId);
      setError("");
      const controller = new AbortController();
      abortRef.current = controller;
      generationConvRef.current = conversationId;
      try {
        await runWithRefresh((token) =>
          retryMessage({
            conversationId,
            messageId,
            providerId: selectedProviderId,
            token,
            ...streamHandlers(conversationId, selectedProviderId),
            signal: controller.signal,
          }),
        );
      } catch (caught) {
        if (caught instanceof DOMException && caught.name === "AbortError") {
          return;
        }
        setError(caught instanceof ApiError ? caught.message : "Could not retry the response.");
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
    },
    [conversationId, refreshMessages, runWithRefresh, selectedProviderId, streamHandlers],
  );

  const toolCount = messages.filter((m) => m.role === "tool").length;
  const visibleMessages = showToolLogs ? messages : messages.filter((m) => m.role !== "tool");
  const empty = !historyLoading && messages.length === 0 && queue.length === 0;
  const visibleError = error || (historyError instanceof Error ? historyError.message : "");
  const currentConversation = conversations?.find(
    (conversation) => conversation.id === conversationId,
  );
  const firstName = currentUser?.display_name?.split(/\s+/)[0];
  const suggestionPool = [
    {
      label: "Refactor code",
      prompt:
        "Review this codebase and suggest refactoring improvements for readability and maintainability.",
    },
    {
      label: "Debug an issue",
      prompt:
        "I'm running into a bug. Help me trace the root cause, understand why it's happening, and fix it.",
    },
    {
      label: "Write tests",
      prompt:
        "Generate thorough unit and integration tests for the following code, covering edge cases and error paths.",
    },
    {
      label: "Design an API",
      prompt:
        "Help me design a REST API for this feature. Think through the endpoints, request/response shapes, error handling, and auth.",
    },
    {
      label: "Optimize performance",
      prompt:
        "Profile this code and suggest concrete performance optimizations. Identify bottlenecks and propose solutions.",
    },
    {
      label: "Review changes",
      prompt:
        "Review these changes for correctness, security concerns, edge cases, and code style issues.",
    },
    {
      label: "Explain code",
      prompt:
        "Walk me through what this code does in detail — architecture, control flow, and key decisions.",
    },
    {
      label: "Database schema",
      prompt:
        "Design a database schema for these requirements. Consider indexing, relationships, and migration strategy.",
    },
    {
      label: "Architecture decision",
      prompt:
        "Help me evaluate tradeoffs between different approaches for this system design. Consider scalability, complexity, and maintainability.",
    },
    {
      label: "Draft documentation",
      prompt:
        "Write clear, concise documentation for this module — include usage examples, API reference, and gotchas.",
    },
    {
      label: "Fix type errors",
      prompt:
        "I have a tricky TypeScript type error. Help me understand it and resolve it properly without resorting to `any`.",
    },
    {
      label: "Plan a feature",
      prompt:
        "Help me break down this feature into concrete, shippable steps. Start with the core logic and iterate outward.",
    },
    {
      label: "Code migration",
      prompt:
        "Help me plan and execute a migration to a newer library version or framework, minimizing risk and regressions.",
    },
    {
      label: "Security audit",
      prompt:
        "Review this code for security vulnerabilities — injection, auth issues, data exposure, and unsafe defaults.",
    },
    {
      label: "Improve error handling",
      prompt:
        "Audit the error handling in this code. Identify unhandled paths and suggest a consistent error handling strategy.",
    },
    {
      label: "Write an essay",
      prompt:
        "Help me write a clear, well-structured essay. Start by outlining the key arguments and flow.",
    },
    {
      label: "Brainstorm ideas",
      prompt:
        "Help me brainstorm creative ideas for this topic. Push beyond the obvious and explore unusual angles.",
    },
    {
      label: "Make a decision",
      prompt:
        "Help me think through a difficult decision. Weigh the pros, cons, risks, and what matters most.",
    },
    {
      label: "Learn something new",
      prompt:
        "Explain this concept to me from first principles. Assume I'm smart but know nothing about the topic.",
    },
    {
      label: "Improve my writing",
      prompt: "Review this text and suggest improvements for clarity, tone, flow, and impact.",
    },
    {
      label: "Summarize a document",
      prompt:
        "Read through this and give me a concise summary with the key takeaways and action items.",
    },
    {
      label: "Prepare for a meeting",
      prompt:
        "Help me prepare for an upcoming meeting. Think through the agenda, key points, and likely questions.",
    },
    {
      label: "Create a workout plan",
      prompt: "Design a fitness plan tailored to my goals, available equipment, and schedule.",
    },
    {
      label: "Plan a trip",
      prompt:
        "Help me plan a trip — suggest an itinerary, logistics, budget estimate, and things I might overlook.",
    },
    {
      label: "Write a speech",
      prompt:
        "Help me draft a speech or presentation. Keep it engaging, structured, and appropriate for the audience.",
    },
    {
      label: "Analyze a contract",
      prompt:
        "Review this contract or terms document. Flag unusual clauses, risks, and what I should negotiate.",
    },
    {
      label: "Build a habit",
      prompt:
        "Help me design a system to build or break a habit. Focus on practical, sustainable strategies.",
    },
    {
      label: "Negotiate better",
      prompt:
        "Help me prepare for a negotiation. Role-play scenarios, suggest tactics, and identify my leverage.",
    },
    {
      label: "Structure my thinking",
      prompt:
        "I have scattered thoughts on a complex topic. Help me organize them into a clear framework.",
    },
    {
      label: "Give career advice",
      prompt:
        "Act as a career coach. Help me think through my next move, growth areas, and how to position myself.",
    },
    {
      label: "Solve a math problem",
      prompt:
        "Walk me through this math or logic problem step by step. Explain the intuition, not just the mechanics.",
    },
    {
      label: "Write a story",
      prompt:
        "Help me develop a story — characters, plot, setting, and voice. Let's start with the core conflict.",
    },
    {
      label: "Cook something great",
      prompt:
        "Suggest recipes based on the ingredients I have. Focus on flavor, technique, and minimal waste.",
    },
    {
      label: "Research a topic",
      prompt:
        "Help me research this topic deeply. Identify the best sources, key debates, and what's worth reading.",
    },
    {
      label: "Manage my time",
      prompt:
        "Help me structure my week for deep focus. Balance urgent tasks with long-term priorities.",
    },
  ];

  const suggestions = useMemo(
    () => [...suggestionPool].sort(() => Math.random() - 0.5).slice(0, 3),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // Build a map of all tool calls from assistant messages for pairing with tool results.
  const allToolCalls = useMemo(() => {
    const result: { id: string; name: string; arguments: Record<string, unknown> }[] = [];
    for (const msg of messages) {
      if (msg.role === "assistant" && Array.isArray(msg.tool_calls)) {
        for (const item of msg.tool_calls as Record<string, unknown>[]) {
          if (typeof item.id === "string" && typeof item.name === "string") {
            result.push({
              id: item.id,
              name: item.name,
              arguments: (item.arguments as Record<string, unknown>) ?? {},
            });
          }
        }
      }
    }
    return result;
  }, [messages]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-slate-200/70 bg-white/55 px-4 backdrop-blur-xl dark:border-slate-800 dark:bg-slate-950/35">
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold text-slate-900 dark:text-slate-100">
            {currentConversation?.title || "New conversation"}
          </p>
          <p className="text-[11px] text-slate-400 dark:text-slate-600">
            {providers.find((provider) => provider.id === selectedProviderId)?.name ||
              "Choose a provider below"}
          </p>
        </div>
        {toolCount > 0 && (
          <button
            type="button"
            onClick={toggleToolLogs}
            className={`flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs transition ${
              showToolLogs
                ? "bg-indigo-50 text-indigo-700 dark:bg-indigo-950/60 dark:text-indigo-300"
                : "text-slate-400 hover:bg-white hover:text-slate-700 dark:text-slate-500 dark:hover:bg-slate-800 dark:hover:text-slate-300"
            }`}
          >
            <Terminal size={12} />
            <span>Tool logs</span>
            {!showToolLogs && <span className="ml-0.5 tabular-nums">({toolCount})</span>}
          </button>
        )}
      </div>

      <div className="flex-1 overflow-y-auto">
        {empty ? (
          <div className="flex min-h-full items-center justify-center px-4 py-10">
            <div className="w-full max-w-2xl text-center">
              <BrandMark className="mb-7 justify-center" />
              <p className="text-3xl font-semibold tracking-[-0.045em] text-slate-950 sm:text-4xl dark:text-white">
                {firstName ? `What are we working on, ${firstName}?` : "What are we working on?"}
              </p>
              <p className="mx-auto mt-3 max-w-md text-sm leading-6 text-slate-500 dark:text-slate-400">
                Start with a rough thought. HelpCore can help you shape it, plan it, or move it
                forward.
              </p>
              <div className="mt-8 grid gap-2 sm:grid-cols-3">
                {suggestions.map((suggestion) => (
                  <button
                    key={suggestion.label}
                    type="button"
                    onClick={() => setInputValue(suggestion.prompt)}
                    className="group flex items-center justify-between rounded-2xl border border-slate-200/80 bg-white/65 px-4 py-3 text-left text-sm font-medium text-slate-700 shadow-sm backdrop-blur transition hover:-translate-y-0.5 hover:border-indigo-200 hover:bg-white hover:shadow-md dark:border-slate-800 dark:bg-slate-900/55 dark:text-slate-300 dark:hover:border-indigo-900 dark:hover:bg-slate-900"
                  >
                    {suggestion.label}
                    <ArrowUpRight
                      size={14}
                      className="text-slate-300 transition group-hover:text-indigo-500 dark:text-slate-700"
                    />
                  </button>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="mx-auto max-w-4xl space-y-5 px-3 py-5 sm:px-5 md:py-7">
            {visibleMessages.map((message) => (
              <MessageBubble
                key={message.id}
                message={message}
                onRetry={retry}
                retrying={retryingId === message.id}
                accessToken={accessToken ?? ""}
                hasAudio={hasAudio}
                toolCalls={allToolCalls}
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
        <div className="mx-auto w-full max-w-4xl space-y-2 px-3 pb-2 sm:px-5">
          {queue.map((item) => (
            <div
              key={item.id}
              className="flex items-start gap-2 rounded-2xl border border-dashed border-indigo-200 bg-indigo-50/45 px-4 py-3 text-sm text-slate-600 dark:border-indigo-900 dark:bg-indigo-950/20 dark:text-slate-400"
            >
              <div className="flex-1">
                <div className="whitespace-pre-wrap leading-relaxed">{item.text}</div>
                <div className="mt-1 text-xs text-slate-400 dark:text-slate-500">
                  {providers.find((provider) => provider.id === item.providerId)?.name ??
                    `Provider ${item.providerId.slice(0, 8)}…`}
                </div>
              </div>
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
      <div className="mx-auto w-full max-w-4xl px-3 pb-3 pt-2 sm:px-5 sm:pb-5">
        <ChatInput
          value={inputValue}
          onChange={setInputValue}
          onSend={handleSend}
          onStop={handleStop}
          disabled={!selectedProviderId}
          active={active}
          providers={providers}
          selectedProviderId={selectedProviderId}
          onProviderChange={handleProviderChange}
        />
      </div>
    </div>
  );
}
