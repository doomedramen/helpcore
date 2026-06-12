"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import {
  ApiError,
  cancelGeneration,
  chat,
  compactConversation,
  generateTitle,
  getPendingInteraction,
  getMessages,
  listConversations,
  listProviders,
  renameConversation,
  respondToInteraction,
  retryMessage,
  submitFeedback,
} from "@/lib/api";
import { useAuth } from "@/context/auth";
import type {
  ConversationSummary,
  Message,
  PendingInteraction,
  SseContext,
  SseDone,
  SseStarted,
  SseToolCall,
  SseToolResult,
  SseUsage,
} from "@/lib/types";
import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import { type StickToBottomContext } from "use-stick-to-bottom";
import {
  Message as AIMessage,
  MessageAction,
  MessageActions,
  MessageContent,
} from "@/components/ai-elements/message";
import {
  PromptInput,
  PromptInputActionAddAttachments,
  PromptInputActionAddScreenshot,
  PromptInputActionMenu,
  PromptInputActionMenuContent,
  PromptInputActionMenuTrigger,
  PromptInputBody,
  PromptInputButton,
  type PromptInputMessage,
  PromptInputFooter,
  PromptInputProvider,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
} from "@/components/ai-elements/prompt-input";
import {
  ModelSelector,
  ModelSelectorTrigger,
  ModelSelectorContent,
  ModelSelectorInput,
  ModelSelectorItem,
  ModelSelectorList,
  ModelSelectorName,
} from "@/components/ai-elements/model-selector";
import {
  Queue,
  QueueItem,
  QueueItemContent,
  QueueItemActions,
  QueueItemAction,
} from "@/components/ai-elements/queue";
import { Suggestions, Suggestion } from "@/components/ai-elements/suggestion";
import { Shimmer } from "@/components/ai-elements/shimmer";
import {
  Context,
  ContextContent,
  ContextContentBody,
  ContextContentHeader,
  ContextTrigger,
} from "@/components/ai-elements/context";
import { SlashCommandMenu } from "./slash-command-menu";
import { type SlashCommand } from "@/lib/commands";
import BrandMark from "./brand-mark";
import ToolMessageAdapter, { type ToolCallInfo } from "./tool-adapter";
import InteractionComposer from "./interaction-composer";
import { MessageContentWithAssets } from "./asset-renderer";
import PromptAttachments from "./prompt-attachments";
import CodeBlockInjector from "./code-copy-button";
import { cn } from "@/lib/utils";
import { Check, Copy, ScrollText, Shuffle } from "lucide-react";

interface QueueItem {
  id: string;
  text: string;
  providerId: string;
}

interface LiveToolCall extends ToolCallInfo {
  result?: string;
}

interface Props {
  conversationId: string | null;
  onConversationCreated: (id: string) => void;
  onConversationNotFound?: () => void;
}

const isActive = (message: Message) =>
  message.status === "pending" || message.status === "streaming";

function MessageCopyAction({ content, className }: { content: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef(0);
  useEffect(() => () => clearTimeout(timeoutRef.current), []);

  const copy = useCallback(async () => {
    if (typeof navigator?.clipboard?.writeText !== "function") return;
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      timeoutRef.current = window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* clipboard unavailable */
    }
  }, [content]);

  const Icon = copied ? Check : Copy;
  return (
    <MessageActions
      className={cn(
        "opacity-70 transition-opacity md:opacity-0 md:group-hover:opacity-100",
        className,
      )}
    >
      <MessageAction
        tooltip={copied ? "Copied" : "Copy message"}
        label="Copy message"
        onClick={copy}
        className="text-muted-foreground hover:text-foreground"
      >
        <Icon size={14} />
      </MessageAction>
    </MessageActions>
  );
}

let nextQueueId = 1;

export default function ChatWindow({
  conversationId,
  onConversationCreated,
  onConversationNotFound,
}: Props) {
  const { accessToken, currentUser, refreshAccessToken } = useAuth();
  const router = useRouter();
  const { mutate: mutateGlobal } = useSWRConfig();
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [infoMessage, setInfoMessage] = useState("");
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState("");
  const [modelSelectorOpen, setModelSelectorOpen] = useState(false);
  const [shuffleKey, setShuffleKey] = useState(0);
  const [liveToolCalls, setLiveToolCalls] = useState<Record<string, LiveToolCall>>({});
  const [interactionSubmitting, setInteractionSubmitting] = useState(false);
  const [interactionError, setInteractionError] = useState("");
  const [contextUsage, setContextUsage] = useState<{
    usedTokens: number;
    maxTokens: number;
  } | null>(null);
  const [streamUsage, setStreamUsage] = useState<SseUsage | null>(null);
  const providerSelectionsRef = useRef<Record<string, string>>({});
  const abortRef = useRef<AbortController | null>(null);
  const processingRef = useRef(false);
  const stickToBottomRef = useRef<StickToBottomContext | null>(null);
  const prevMessageCountRef = useRef(0);
  const prevActiveRef = useRef(false);

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
  const interactionKey =
    accessToken && conversationId
      ? ([`/conversations/${conversationId}/interaction`, accessToken] as const)
      : null;
  const { data: pendingInteraction = null, mutate: refreshInteraction } =
    useSWR<PendingInteraction | null>(interactionKey, ([, token]) =>
      getPendingInteraction(conversationId!, token as string),
    );

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

  useEffect(() => {
    if (onConversationNotFound && historyError instanceof ApiError && historyError.status === 404) {
      onConversationNotFound();
      setInfoMessage("Conversation not found. Starting a new chat.");
    }
  }, [historyError, onConversationNotFound]);

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
      setLiveToolCalls({});
      setContextUsage(null);
      setStreamUsage(null);
      setInteractionError("");
      setInteractionSubmitting(false);
      prevMessageCountRef.current = 0;
    }
  }, [conversationId]);

  useEffect(() => {
    const ctx = stickToBottomRef.current;
    if (!ctx) return;
    if (messages.length > prevMessageCountRef.current) {
      ctx.scrollToBottom({ ignoreEscapes: true });
    }
    prevMessageCountRef.current = messages.length;
  }, [messages.length]);

  useEffect(() => {
    const ctx = stickToBottomRef.current;
    if (!ctx) return;
    if (active && !prevActiveRef.current) {
      ctx.scrollToBottom({ ignoreEscapes: true });
    }
    prevActiveRef.current = active;
  }, [active]);

  useEffect(() => {
    if (infoMessage) {
      const timer = setTimeout(() => setInfoMessage(""), 5000);
      return () => clearTimeout(timer);
    }
  }, [infoMessage]);

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

  const handleContentError = useCallback(
    (type: string, message: string) => {
      if (!conversationId || !accessToken) return;
      submitFeedback(conversationId, accessToken, type, message).catch(() => {
        // best-effort — no user-facing feedback needed
      });
    },
    [conversationId, accessToken],
  );

  const streamHandlers = useCallback(
    (fallbackConversationId: string | undefined, providerId: string) => {
      let activeConversationId = fallbackConversationId;
      const streamToolCallIds = new Set<string>();
      const clearStreamToolCalls = () => {
        setLiveToolCalls((prev) => {
          const next = { ...prev };
          for (const id of streamToolCallIds) delete next[id];
          return next;
        });
      };
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
        onContext: (ctx: SseContext) => {
          setContextUsage({ usedTokens: ctx.used_tokens, maxTokens: ctx.max_tokens });
        },
        onChunk: (_delta: string) => {
          const id = activeConversationId;
          if (id) void refreshConversation(id);
        },
        onToolCall: (call: SseToolCall) => {
          streamToolCallIds.add(call.id);
          setLiveToolCalls((prev) => ({ ...prev, [call.id]: call }));
        },
        onToolResult: (result: SseToolResult) => {
          streamToolCallIds.add(result.id);
          setLiveToolCalls((prev) => ({
            ...prev,
            [result.id]: {
              id: result.id,
              name: result.name,
              arguments: prev[result.id]?.arguments ?? {},
              result: result.result,
            },
          }));
        },
        onDone: (done: SseDone) => {
          if (done.usage) setStreamUsage(done.usage);
          void refreshInteraction(null, { revalidate: false });
          void refreshConversation(done.conversation_id).finally(clearStreamToolCalls);
        },
        onInputRequired: (data: { interaction: PendingInteraction }) => {
          setInteractionError("");
          void refreshInteraction(data.interaction, { revalidate: false });
          void refreshConversation(data.interaction.conversation_id).finally(clearStreamToolCalls);
        },
        onInterrupted: () => {
          if (activeConversationId) {
            void refreshConversation(activeConversationId).finally(clearStreamToolCalls);
          } else {
            clearStreamToolCalls();
          }
        },
      };
    },
    [onConversationCreated, refreshConversation, refreshInteraction, router],
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
        setLiveToolCalls({});
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
    if (!active && !pendingInteraction && queue.length > 0 && !processingRef.current) {
      processingRef.current = true;
      const [first, ...rest] = queue;
      setQueue(rest);
      sendMessage(first).finally(() => {
        processingRef.current = false;
      });
    }
  }, [active, pendingInteraction, queue, sendMessage]);

  const respondInteraction = useCallback(
    async (response: Record<string, unknown>) => {
      if (!conversationId || !pendingInteraction) return;
      setInteractionSubmitting(true);
      setInteractionError("");
      void refreshInteraction(null, { revalidate: false });
      const controller = new AbortController();
      abortRef.current = controller;
      generationConvRef.current = conversationId;
      try {
        await runWithRefresh((token) =>
          respondToInteraction({
            conversationId,
            interactionId: pendingInteraction.id,
            response,
            token,
            ...streamHandlers(conversationId, selectedProviderId),
            signal: controller.signal,
          }),
        );
      } catch (caught) {
        setLiveToolCalls({});
        if (caught instanceof DOMException && caught.name === "AbortError") return;
        setInteractionError(
          caught instanceof ApiError ? caught.message : "Could not submit your response.",
        );
        await refreshInteraction();
        await refreshMessages();
      } finally {
        if (generationConvRef.current === conversationId) {
          generationConvRef.current = null;
        }
        if (abortRef.current === controller) abortRef.current = null;
        setInteractionSubmitting(false);
      }
    },
    [
      conversationId,
      pendingInteraction,
      refreshInteraction,
      refreshMessages,
      runWithRefresh,
      selectedProviderId,
      streamHandlers,
    ],
  );

  const handleSend = useCallback(
    (text: string) => {
      if (!selectedProviderId) return;
      const id = String(nextQueueId++);
      setQueue((prev) => [...prev, { id, text, providerId: selectedProviderId }]);
    },
    [selectedProviderId],
  );

  const handleCompact = useCallback(async () => {
    if (!conversationId || !accessToken) return;
    setInfoMessage("Compacting…");
    try {
      const result = await compactConversation(conversationId, accessToken);
      if (result.messages_compacted > 0) {
        setInfoMessage("");
        await refreshConversation(conversationId);
      } else {
        setInfoMessage("Nothing to compact — conversation is already short.");
      }
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        const fresh = await refreshAccessToken();
        if (fresh) {
          const result = await compactConversation(conversationId, fresh);
          if (result.messages_compacted > 0) {
            setInfoMessage("");
            await refreshConversation(conversationId);
          } else {
            setInfoMessage("Nothing to compact — conversation is already short.");
          }
          return;
        }
      }
      setError(err instanceof ApiError ? err.message : "Compaction failed. Try again.");
    }
  }, [conversationId, accessToken, refreshConversation, refreshAccessToken]);

  const handleRename = useCallback(
    async (args?: string) => {
      if (!conversationId || !accessToken) return;
      const providedTitle = args?.trim();
      if (providedTitle) {
        try {
          await renameConversation(conversationId, providedTitle, accessToken);
          await refreshConversation(conversationId);
          setInfoMessage("");
        } catch (err) {
          if (err instanceof ApiError && err.status === 401) {
            const fresh = await refreshAccessToken();
            if (fresh) {
              await renameConversation(conversationId, providedTitle, fresh);
              await refreshConversation(conversationId);
              setInfoMessage("");
              return;
            }
          }
          setError(err instanceof ApiError ? err.message : "Rename failed. Try again.");
        }
        return;
      }
      setInfoMessage("Generating title…");
      try {
        await generateTitle(conversationId, accessToken);
        await refreshConversation(conversationId);
        setInfoMessage("");
      } catch (err) {
        if (err instanceof ApiError && err.status === 401) {
          const fresh = await refreshAccessToken();
          if (fresh) {
            await generateTitle(conversationId, fresh);
            await refreshConversation(conversationId);
            setInfoMessage("");
            return;
          }
        }
        setError(err instanceof ApiError ? err.message : "Title generation failed. Try again.");
      }
    },
    [conversationId, accessToken, refreshConversation, refreshAccessToken],
  );

  const commandActions = useMemo<SlashCommand[]>(
    () => [
      {
        slash: "/compact",
        label: "Compact conversation",
        description: "Summarize oldest messages to free context",
        action: handleCompact,
      },
      {
        slash: "/rename",
        label: "Rename conversation",
        description: "Generate or set a conversation title",
        action: handleRename,
      },
    ],
    [handleCompact, handleRename],
  );

  const handlePromptSubmit = useCallback(
    (message: PromptInputMessage) => {
      const hasText = message.text.trim().length > 0;
      const hasFiles = message.files.length > 0;

      if (!(hasText || hasFiles)) {
        return;
      }

      const trimmed = message.text.trim();

      // Slash commands — dispatched locally without sending to the model.
      if (trimmed.startsWith("/")) {
        const parts = trimmed.split(/\s+/);
        const cmd = parts[0].toLowerCase();
        const match = commandActions.find((c) => c.slash === cmd);

        if (match) {
          const args = parts.slice(1).join(" ");
          setInfoMessage(`Running ${cmd}…`);
          Promise.resolve(match.action(args || undefined)).catch((err) => {
            setError(err instanceof ApiError ? err.message : `Command failed: ${cmd}`);
          });
          return;
        }

        // Unknown command — send as a regular message so the model can respond.
        handleSend(trimmed);
        return;
      }

      handleSend(trimmed);
    },
    [commandActions, handleSend],
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
    setLiveToolCalls({});
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
        setLiveToolCalls({});
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

  const empty =
    !historyLoading &&
    messages.length === 0 &&
    queue.length === 0 &&
    Object.keys(liveToolCalls).length === 0;
  const historyErrorMessage =
    historyError instanceof ApiError && historyError.status === 404
      ? ""
      : historyError instanceof Error
        ? historyError.message
        : "";
  const visibleError = error || historyErrorMessage;
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

  const toolCallsById = useMemo(() => {
    const result = new Map<string, ToolCallInfo>();
    for (const msg of messages) {
      if (msg.role === "assistant" && Array.isArray(msg.tool_calls)) {
        for (const item of msg.tool_calls as Record<string, unknown>[]) {
          if (typeof item.id === "string" && typeof item.name === "string") {
            result.set(item.id, {
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

  const toolResultsByCallId = useMemo(() => {
    const result = new Map<string, string>();
    for (const message of messages) {
      if (message.role === "tool" && message.tool_call_id) {
        result.set(message.tool_call_id, message.content);
      }
    }
    return result;
  }, [messages]);

  const pendingLiveToolCalls = Object.values(liveToolCalls).filter(
    (call) => !toolCallsById.has(call.id),
  );

  const selectedProvider = providers.find((p) => p.id === selectedProviderId);
  const contextMaxTokens = contextUsage?.maxTokens ?? selectedProvider?.context_limit ?? 0;
  const contextUsedTokens = contextUsage?.usedTokens ?? 0;

  const contextModelUsage = useMemo(() => {
    if (!streamUsage) return undefined;
    return {
      inputTokens: streamUsage.input_tokens,
      inputTokenDetails: {
        noCacheTokens: undefined,
        cacheReadTokens: streamUsage.cache_read_tokens ?? undefined,
        cacheWriteTokens: streamUsage.cache_write_tokens ?? undefined,
      },
      outputTokens: streamUsage.output_tokens,
      outputTokenDetails: {
        textTokens: undefined,
        reasoningTokens: undefined,
      },
      totalTokens: streamUsage.input_tokens + streamUsage.output_tokens,
    };
  }, [streamUsage]);

  const contextModelId = currentConversation?.model ?? undefined;

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
        <div className="flex items-center gap-1.5">
          {contextMaxTokens > 0 && conversationId && (
            <Context
              usedTokens={contextUsedTokens}
              maxTokens={contextMaxTokens}
              usage={contextModelUsage}
              modelId={contextModelId}
            >
              <ContextTrigger />
              <ContextContent>
                <ContextContentHeader />
                <ContextContentBody>
                  <button
                    type="button"
                    onClick={handleCompact}
                    className="w-full rounded-md border px-2.5 py-1.5 text-xs font-medium transition-colors hover:bg-muted"
                  >
                    Compact conversation
                  </button>
                </ContextContentBody>
              </ContextContent>
            </Context>
          )}
        </div>
      </div>

      {/* Conversation */}
      <Conversation contextRef={stickToBottomRef}>
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
              <div className="mt-6 flex w-full min-w-0 items-center justify-center gap-2">
                <div className="min-w-0 max-w-full">
                  <Suggestions>
                    {suggestions.map((s) => (
                      <Suggestion
                        key={s.label}
                        suggestion={s.prompt}
                        onClick={handleSuggestionClick}
                      >
                        {s.label}
                      </Suggestion>
                    ))}
                  </Suggestions>
                </div>
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
              {messages.map((message) => {
                if (message.role === "tool") {
                  if (message.tool_call_id && toolCallsById.has(message.tool_call_id)) return null;
                  return <ToolMessageAdapter key={message.id} result={message.content} />;
                }

                if (message.role === "user") {
                  return (
                    <AIMessage key={message.id} from="user">
                      <MessageContent>
                        <CodeBlockInjector>
                          <MessageContentWithAssets>{message.content}</MessageContentWithAssets>
                        </CodeBlockInjector>
                      </MessageContent>
                      <MessageCopyAction content={message.content} className="justify-end" />
                    </AIMessage>
                  );
                }

                // Summary
                if (message.role === "summary") {
                  return (
                    <AIMessage key={message.id} from="assistant">
                      <MessageContent>
                        <div className="mb-2 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
                          <ScrollText size={13} aria-hidden="true" />
                          Conversation compacted
                        </div>
                        <CodeBlockInjector>
                          <MessageContentWithAssets onContentError={handleContentError}>
                            {message.content}
                          </MessageContentWithAssets>
                        </CodeBlockInjector>
                      </MessageContent>
                    </AIMessage>
                  );
                }

                // Assistant
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
                      {calls.length > 0 && (
                        <div className="mt-3 space-y-2">
                          {calls.map((call) => {
                            return (
                              <ToolMessageAdapter
                                key={call.id}
                                call={call}
                                result={
                                  toolResultsByCallId.get(call.id) ?? liveToolCalls[call.id]?.result
                                }
                              />
                            );
                          })}
                        </div>
                      )}

                      {msgActive && !message.content ? (
                        <Shimmer duration={1}>Thinking…</Shimmer>
                      ) : (
                        <CodeBlockInjector>
                          <MessageContentWithAssets onContentError={handleContentError}>
                            {message.content}
                          </MessageContentWithAssets>
                        </CodeBlockInjector>
                      )}

                      {msgActive && message.content && (
                        <Shimmer as="span" className="mt-2 text-xs" duration={1}>
                          Still working…
                        </Shimmer>
                      )}

                      {retryable && (
                        <div className="mt-3 border-t pt-3">
                          <p
                            className={`text-xs ${message.status === "interrupted" ? "text-amber-600 dark:text-amber-400" : "text-destructive"}`}
                          >
                            {message.status === "interrupted"
                              ? message.error || "This response was interrupted."
                              : message.error || "This response did not finish."}
                          </p>
                          <button
                            type="button"
                            onClick={() => retry(message.id)}
                            disabled={retryingId === message.id}
                            className={`mt-2 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors disabled:cursor-wait disabled:opacity-50 ${
                              message.status === "interrupted"
                                ? "border-amber-300 bg-amber-50 text-amber-700 hover:bg-amber-100 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-300 dark:hover:bg-amber-950/60"
                                : "hover:bg-muted"
                            }`}
                          >
                            {retryingId === message.id ? "Retrying…" : "Retry response"}
                          </button>
                        </div>
                      )}
                    </MessageContent>
                    {!msgActive && message.content && (
                      <MessageCopyAction content={message.content} />
                    )}
                  </AIMessage>
                );
              })}

              {pendingLiveToolCalls.length > 0 && (
                <div className="space-y-2">
                  {pendingLiveToolCalls.map((call) => (
                    <ToolMessageAdapter key={call.id} call={call} result={call.result} />
                  ))}
                </div>
              )}

              {infoMessage && (
                <div className="rounded-lg border border-blue-200 bg-blue-50 px-3 py-2 text-sm text-blue-700 dark:border-blue-800 dark:bg-blue-950/40 dark:text-blue-300">
                  {infoMessage}
                </div>
              )}
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
        <div className="mx-auto w-full max-w-4xl px-3 pb-2 sm:px-5">
          <Queue>
            <ul className="flex flex-col gap-1">
              {queue.map((item) => (
                <QueueItem key={item.id}>
                  <QueueItemContent>{item.text}</QueueItemContent>
                  <QueueItemActions>
                    <QueueItemAction
                      onClick={() => setQueue((prev) => prev.filter((q) => q.id !== item.id))}
                      aria-label="Remove from queue"
                    >
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      >
                        <path d="M3 6h18" />
                        <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
                        <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                        <line x1="10" y1="11" x2="10" y2="17" />
                        <line x1="14" y1="11" x2="14" y2="17" />
                      </svg>
                    </QueueItemAction>
                  </QueueItemActions>
                </QueueItem>
              ))}
            </ul>
          </Queue>
        </div>
      )}

      {/* Input */}
      <div className="mx-auto w-full max-w-4xl px-3 pb-3 pt-2 sm:px-5 sm:pb-5">
        {pendingInteraction ? (
          <InteractionComposer
            interaction={pendingInteraction}
            submitting={interactionSubmitting}
            error={interactionError}
            onSubmit={respondInteraction}
            onDismiss={() => respondInteraction({ kind: "dismiss" })}
          />
        ) : (
          <PromptInputProvider>
            <PromptInput globalDrop multiple onSubmit={handlePromptSubmit}>
              <PromptAttachments />
              <SlashCommandMenu commands={commandActions} />
              <PromptInputBody>
                <PromptInputTextarea
                  placeholder={
                    providers.length === 0
                      ? "Configure a chat provider to begin."
                      : "Message helpcore…"
                  }
                />
              </PromptInputBody>
              <PromptInputFooter>
                <PromptInputTools>
                  <PromptInputActionMenu>
                    <PromptInputActionMenuTrigger />
                    <PromptInputActionMenuContent>
                      <PromptInputActionAddAttachments />
                      <PromptInputActionAddScreenshot />
                    </PromptInputActionMenuContent>
                  </PromptInputActionMenu>
                  <ModelSelector
                    open={modelSelectorOpen}
                    onOpenChange={(open) => setModelSelectorOpen(open)}
                  >
                    <ModelSelectorTrigger
                      render={
                        <PromptInputButton size="xs" className="max-w-[13rem]">
                          {(() => {
                            const p = providers.find((p) => p.id === selectedProviderId);
                            return p ? (
                              <ModelSelectorName>{p.name}</ModelSelectorName>
                            ) : (
                              <span className="text-muted-foreground">
                                {providers.length === 0 ? "No chat provider" : "Select provider"}
                              </span>
                            );
                          })()}
                        </PromptInputButton>
                      }
                    />
                    <ModelSelectorContent>
                      <ModelSelectorInput placeholder="Search providers..." />
                      <ModelSelectorList>
                        {providers.map((p) => (
                          <ModelSelectorItem
                            key={p.id}
                            value={p.id}
                            onSelect={() => {
                              handleProviderChange(p.id);
                              setModelSelectorOpen(false);
                            }}
                          >
                            <ModelSelectorName>{p.name}</ModelSelectorName>
                            <span className="ml-auto text-xs text-muted-foreground">
                              {p.default_model}
                            </span>
                          </ModelSelectorItem>
                        ))}
                      </ModelSelectorList>
                    </ModelSelectorContent>
                  </ModelSelector>
                </PromptInputTools>
                <div className="flex items-center gap-2">
                  <span className="hidden text-[11px] text-muted-foreground sm:block">
                    Enter to send · Shift + Enter for new line
                  </span>
                  <PromptInputSubmit status={active ? "streaming" : "ready"} onStop={handleStop} />
                </div>
              </PromptInputFooter>
            </PromptInput>
          </PromptInputProvider>
        )}
      </div>
    </div>
  );
}
