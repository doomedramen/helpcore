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
  listProviders,
  retryMessage,
} from "@/lib/api";
import { useAuth } from "@/context/auth";
import type { ConversationSummary, Message, SseDone, SseStarted } from "@/lib/types";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Message as AIMessage,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import {
  PromptInput,
  type PromptInputMessage,
  PromptInputBody,
  PromptInputTextarea,
  PromptInputSubmit,
  PromptInputFooter,
  PromptInputTools,
} from "@/components/ai-elements/prompt-input";
import { Suggestions, Suggestion } from "@/components/ai-elements/suggestion";
import { Shimmer } from "@/components/ai-elements/shimmer";
import BrandMark from "./brand-mark";
import ToolMessageBubble from "./tool-message-bubble";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/app/components/ui/select";
import { Badge } from "@/app/components/ui/badge";
import {
  FileText,
  FolderOpen,
  MoveRight,
  Pencil,
  Plus,
  Search,
  Terminal,
  Trash2,
  Shuffle,
  Sparkles,
} from "lucide-react";

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

// Tool meta for inline badges
const TOOL_META: Record<string, { icon: React.ReactNode; label: string; color: string }> = {
  memory_list: {
    icon: <FolderOpen size={11} />,
    label: "List memory",
    color: "text-slate-500 dark:text-slate-400",
  },
  memory_read: {
    icon: <FileText size={11} />,
    label: "Read",
    color: "text-blue-500 dark:text-blue-400",
  },
  memory_write: {
    icon: <Pencil size={11} />,
    label: "Write",
    color: "text-emerald-500 dark:text-emerald-400",
  },
  memory_append: {
    icon: <Plus size={11} />,
    label: "Append",
    color: "text-emerald-500 dark:text-emerald-400",
  },
  memory_move: {
    icon: <MoveRight size={11} />,
    label: "Move",
    color: "text-amber-500 dark:text-amber-400",
  },
  memory_delete: {
    icon: <Trash2 size={11} />,
    label: "Delete",
    color: "text-red-500 dark:text-red-400",
  },
  memory_search: {
    icon: <Search size={11} />,
    label: "Search",
    color: "text-violet-500 dark:text-violet-400",
  },
};

function toolArgsPreview(name: string, args: Record<string, unknown>): string {
  if (name === "personality_write") return `/${(args.name as string) ?? "?"}`;
  const path = args.path ?? args.from ?? null;
  if (path && typeof path === "string") {
    const label = path.split("/").pop() ?? path;
    if (name === "memory_move") return `${label} → ${(args.to as string) ?? "?"}`;
    return label;
  }
  return "";
}

export default function ChatWindow({ conversationId, onConversationCreated }: Props) {
  const { accessToken, currentUser, refreshAccessToken } = useAuth();
  const router = useRouter();
  const { mutate: mutateGlobal } = useSWRConfig();
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
  const [shuffleKey, setShuffleKey] = useState(0);
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
      ? conversations?.find((c) => c.id === conversationId)?.provider_id
      : null;
    const remembered = providerSelectionsRef.current[key] ?? conversationProvider;
    const selection = providers.some((p) => p.id === remembered)
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

  const generationConvRef = useRef<string | null>(null);
  const active =
    (conversationId !== null && generationConvRef.current === conversationId) ||
    messages.some(isActive);

  const prevConvRef = useRef(conversationId);
  useEffect(() => {
    if (prevConvRef.current !== conversationId) {
      generationConvRef.current = null;
      prevConvRef.current = conversationId;
      setQueue([]);
    }
  }, [conversationId]);

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

  const handlePromptSubmit = useCallback(
    (message: PromptInputMessage) => {
      if (message.text.trim()) {
        handleSend(message.text.trim());
      }
    },
    [handleSend],
  );

  const handleSuggestionClick = useCallback((text: string) => {
    const textarea = document.querySelector(
      'textarea[name="message"]',
    ) as HTMLTextAreaElement | null;
    if (textarea) {
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
      setter?.call(textarea, text);
      textarea.dispatchEvent(new Event("input", { bubbles: true }));
      textarea.focus();
    }
  }, []);

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
  const currentConversation = conversations?.find((c) => c.id === conversationId);
  const firstName = currentUser?.display_name?.split(/\s+/)[0];

  const suggestionPool = useMemo(
    () => [
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
        label: "Explain code",
        prompt:
          "Walk me through what this code does in detail — architecture, control flow, and key decisions.",
      },
      {
        label: "Plan a feature",
        prompt:
          "Help me break down this feature into concrete, shippable steps. Start with the core logic and iterate outward.",
      },
      {
        label: "Brainstorm ideas",
        prompt:
          "Help me brainstorm creative ideas for this topic. Push beyond the obvious and explore unusual angles.",
      },
      {
        label: "Summarize a document",
        prompt:
          "Read through this and give me a concise summary with the key takeaways and action items.",
      },
      {
        label: "Learn something new",
        prompt:
          "Explain this concept to me from first principles. Assume I'm smart but know nothing about the topic.",
      },
    ],
    [],
  );

  // shuffleKey intentionally triggers a re-shuffle of suggestions
  const suggestions = useMemo(
    () => [...suggestionPool].sort(() => Math.random() - 0.5).slice(0, 3),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [suggestionPool, shuffleKey],
  );

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
      {/* Header */}
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-slate-200/70 bg-white/55 px-4 backdrop-blur-xl dark:border-slate-800 dark:bg-slate-950/35">
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold text-slate-900 dark:text-slate-100">
            {currentConversation?.title || "New conversation"}
          </p>
          <p className="text-[11px] text-slate-400 dark:text-slate-600">
            {providers.find((p) => p.id === selectedProviderId)?.name || "Choose a provider below"}
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

      {/* Conversation */}
      <Conversation>
        <ConversationContent>
          {empty ? (
            <ConversationEmptyState
              icon={
                <BrandMark
                  compact
                  markOnly
                  className="[&>span:first-child]:size-12 [&>span:first-child]:rounded-[1rem]"
                />
              }
              title={
                firstName ? `What are we working on, ${firstName}?` : "What are we working on?"
              }
              description="Start with a rough thought. HelpCore can help you shape it, plan it, or move it forward."
            >
              <div className="mt-6 flex items-center justify-center gap-2">
                <Suggestions className="justify-center">
                  {suggestions.map((s) => (
                    <Suggestion key={s.label} suggestion={s.prompt} onClick={handleSuggestionClick}>
                      {s.label}
                    </Suggestion>
                  ))}
                </Suggestions>
                <button
                  type="button"
                  onClick={() => setShuffleKey((k) => k + 1)}
                  className="inline-flex shrink-0 cursor-pointer items-center rounded-full border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted"
                  aria-label="Shuffle suggestions"
                >
                  <Shuffle className="mr-1 size-3" />
                  Shuffle
                </button>
              </div>
            </ConversationEmptyState>
          ) : (
            <div className="mx-auto w-full max-w-4xl space-y-5">
              {visibleMessages.map((message) => {
                if (message.role === "tool") {
                  return (
                    <ToolMessageBubble key={message.id} message={message} calls={allToolCalls} />
                  );
                }

                if (message.role === "user") {
                  return (
                    <AIMessage key={message.id} from="user">
                      <MessageContent>
                        <MessageResponse>{message.content}</MessageResponse>
                      </MessageContent>
                    </AIMessage>
                  );
                }

                // Assistant or summary
                const calls =
                  message.role === "assistant" && message.tool_calls?.length
                    ? ((message.tool_calls as Record<string, unknown>[])
                        .map((item) => {
                          if (typeof item.id === "string" && typeof item.name === "string") {
                            return {
                              id: item.id,
                              name: item.name,
                              arguments: (item.arguments as Record<string, unknown>) ?? {},
                            };
                          }
                          return null;
                        })
                        .filter(Boolean) as {
                        id: string;
                        name: string;
                        arguments: Record<string, unknown>;
                      }[])
                    : [];

                const msgActive = message.status === "pending" || message.status === "streaming";
                const retryable = message.status === "failed" || message.status === "interrupted";

                if (!msgActive && !retryable && !message.content && calls.length === 0) return null;

                return (
                  <AIMessage key={message.id} from="assistant">
                    <MessageContent>
                      {/* Tool call badges */}
                      {calls.length > 0 && (
                        <div className="flex flex-wrap gap-1 pb-2 mb-2 border-b border-border">
                          {calls.map((call) => {
                            const meta = TOOL_META[call.name];
                            const preview = toolArgsPreview(call.name, call.arguments);
                            if (!meta) return null;
                            return (
                              <Badge
                                key={call.id}
                                variant="secondary"
                                className="gap-1 text-[11px] font-medium"
                              >
                                <span className={meta.color}>{meta.icon}</span>
                                {meta.label}
                                {preview && (
                                  <span className="font-mono text-muted-foreground">
                                    · {preview}
                                  </span>
                                )}
                              </Badge>
                            );
                          })}
                        </div>
                      )}

                      {msgActive && !message.content ? (
                        <Shimmer duration={1}>Thinking…</Shimmer>
                      ) : (
                        <MessageResponse>{message.content}</MessageResponse>
                      )}

                      {msgActive && message.content && (
                        <Shimmer as="span" className="mt-2 text-xs" duration={1}>
                          Still working…
                        </Shimmer>
                      )}

                      {retryable && (
                        <div className="mt-3 border-t pt-3">
                          <p className="text-xs text-destructive">
                            {message.error || "This response did not finish."}
                          </p>
                          <button
                            type="button"
                            onClick={() => retry(message.id)}
                            disabled={retryingId === message.id}
                            className="mt-2 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors hover:bg-muted disabled:cursor-wait disabled:opacity-50"
                          >
                            {retryingId === message.id ? "Retrying…" : "Retry response"}
                          </button>
                        </div>
                      )}
                    </MessageContent>
                  </AIMessage>
                );
              })}

              {visibleError && (
                <div className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  {visibleError}
                </div>
              )}
            </div>
          )}
        </ConversationContent>
        <ConversationScrollButton />
      </Conversation>

      {/* Queue */}
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
                  {providers.find((p) => p.id === item.providerId)?.name ??
                    `Provider ${item.providerId.slice(0, 8)}…`}
                </div>
              </div>
              <div className="flex shrink-0 gap-1">
                <button
                  type="button"
                  onClick={() => setQueue((prev) => prev.filter((q) => q.id !== item.id))}
                  className="rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-red-100 hover:text-red-600 dark:hover:bg-red-900/30 dark:hover:text-red-400"
                  aria-label="Remove from queue"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Input */}
      <div className="mx-auto w-full max-w-4xl px-3 pb-3 pt-2 sm:px-5 sm:pb-5">
        <PromptInput onSubmit={handlePromptSubmit} globalDrop>
          <PromptInputBody>
            <PromptInputTextarea
              placeholder={
                providers.length === 0 ? "Configure a chat provider to begin." : "Message helpcore…"
              }
            />
          </PromptInputBody>
          <PromptInputFooter>
            <PromptInputTools>
              <Sparkles size={13} className="shrink-0 text-indigo-500 dark:text-indigo-400" />
              <Select
                value={selectedProviderId}
                onValueChange={(v) => handleProviderChange(v ?? "")}
                disabled={providers.length === 0}
              >
                <SelectTrigger
                  size="sm"
                  className="h-7 max-w-[13rem] border-border/30 px-1.5 text-xs shadow-none hover:bg-muted data-[popup-open]:bg-muted"
                >
                  <SelectValue placeholder="No chat provider">
                    {providers.find((p) => p.id === selectedProviderId)?.name ?? selectedProviderId}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent side="top" className="!min-w-[14rem]">
                  {providers.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.name} · {p.default_model}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </PromptInputTools>
            <div className="flex items-center gap-2">
              <span className="hidden text-[11px] text-muted-foreground sm:block">
                Enter to send · Shift + Enter for new line
              </span>
              <PromptInputSubmit status={active ? "streaming" : "ready"} onStop={handleStop} />
            </div>
          </PromptInputFooter>
        </PromptInput>
      </div>
    </div>
  );
}
