"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypePrettyCode from "rehype-pretty-code";
import {
  createHighlighterCoreSync,
  createJavaScriptRegexEngine,
  type HighlighterCore,
} from "shiki";
import githubLight from "@shikijs/themes/github-light";
import githubDark from "@shikijs/themes/github-dark";
import javascript from "@shikijs/langs/javascript";
import typescript from "@shikijs/langs/typescript";
import tsx from "@shikijs/langs/tsx";
import jsx from "@shikijs/langs/jsx";
import json from "@shikijs/langs/json";
import bash from "@shikijs/langs/bash";
import rust from "@shikijs/langs/rust";
import python from "@shikijs/langs/python";
import html from "@shikijs/langs/html";
import css from "@shikijs/langs/css";
import markdown from "@shikijs/langs/markdown";
import yaml from "@shikijs/langs/yaml";
import toml from "@shikijs/langs/toml";
import sql from "@shikijs/langs/sql";
import diff from "@shikijs/langs/diff";
import graphql from "@shikijs/langs/graphql";
import dockerfile from "@shikijs/langs/dockerfile";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  FileText,
  Pencil,
  Plug,
  Plus,
  Search,
  Trash2,
  FolderOpen,
  MoveRight,
  Brain,
  Volume2,
  VolumeX,
} from "lucide-react";
import { tts } from "@/lib/api";
import type { Message } from "@/lib/types";
import BrandMark from "./brand-mark";

const syncHighlighter: HighlighterCore = createHighlighterCoreSync({
  themes: [githubLight, githubDark],
  langs: [
    javascript,
    typescript,
    tsx,
    jsx,
    json,
    bash,
    rust,
    python,
    html,
    css,
    markdown,
    yaml,
    toml,
    sql,
    diff,
    graphql,
    dockerfile,
  ],
  engine: createJavaScriptRegexEngine(),
});

function stripMarkdown(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_~]{1,2}([^*_~]+)[*_~]{1,2}/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/^>+\s+/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/^\s*\d+\.\s+/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

// ── Tool rendering helpers ─────────────────────────────────────────────────────

interface ToolCallInfo {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

function parseToolCalls(raw: unknown[]): ToolCallInfo[] {
  return raw
    .map((item) => {
      const call = item as Record<string, unknown> | null;
      if (!call || typeof call.id !== "string" || typeof call.name !== "string") return null;
      return {
        id: call.id,
        name: call.name,
        arguments: (call.arguments as Record<string, unknown>) ?? {},
      };
    })
    .filter(Boolean) as ToolCallInfo[];
}

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
  personality_write: {
    icon: <Brain size={11} />,
    label: "Personality",
    color: "text-fuchsia-500 dark:text-fuchsia-400",
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

function inferToolResultType(content: string): { kind: string } | null {
  try {
    const parsed = JSON.parse(content);
    if (Array.isArray(parsed) && parsed.length > 0) {
      const first = parsed[0] as Record<string, unknown>;
      if ("path" in first && "updated_at" in first && !("content" in first)) {
        return { kind: "memory_list" };
      }
      if ("path" in first && "content" in first) {
        return { kind: "memory_search" };
      }
    }
    if (typeof parsed === "object" && parsed !== null && "ok" in parsed && parsed.ok === false) {
      return { kind: "error" };
    }
  } catch {
    // not JSON
  }
  if (content === "saved") return { kind: "write" };
  if (content === "appended") return { kind: "append" };
  if (content === "deleted") return { kind: "delete" };
  if (content === "moved") return { kind: "move" };
  if (content.startsWith("(no memory file")) return { kind: "not_found" };
  return { kind: "generic" };
}

// ── Tool call badges (shown inline inside assistant messages) ──────────────────

function ToolCallBadge({ call }: { call: ToolCallInfo }) {
  const meta = TOOL_META[call.name];
  const preview = toolArgsPreview(call.name, call.arguments);
  if (!meta) return null;
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-md border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800/50 px-2 py-0.5 text-[11px] font-medium ${meta.color}`}
    >
      {meta.icon}
      <span>{meta.label}</span>
      {preview && <span className="text-slate-400 dark:text-slate-500 font-mono">· {preview}</span>}
    </span>
  );
}

// ── Tool message (improved rendering for memory/personality results) ───────────

function ToolMessage({ message, calls }: { message: Message; calls: ToolCallInfo[] }) {
  const [expanded, setExpanded] = useState(false);
  const inferred = inferToolResultType(message.content);
  const isError = inferred?.kind === "error" || message.content.startsWith("(no memory");

  const matchingCall = message.tool_call_id
    ? calls.find((c) => c.id === message.tool_call_id)
    : undefined;

  const meta = matchingCall ? TOOL_META[matchingCall.name] : undefined;
  const toolIcon = meta?.icon ?? (isError ? <AlertTriangle size={11} /> : <Plug size={11} />);
  const toolName = matchingCall
    ? (TOOL_META[matchingCall.name]?.label ?? matchingCall.name)
    : isError
      ? "Error"
      : "Tool";
  const toolColor = isError
    ? "text-red-500 dark:text-red-400"
    : (meta?.color ?? "text-indigo-500 dark:text-indigo-400");
  const preview = matchingCall
    ? toolArgsPreview(matchingCall.name, matchingCall.arguments)
    : undefined;

  // Build result text from content
  function resultSummary(): string {
    if (inferred?.kind === "write" && matchingCall) {
      const path = matchingCall.arguments.path as string;
      if (matchingCall.name === "personality_write") {
        return `Updated ${matchingCall.arguments.name as string} personality`;
      }
      return `Saved ${path ?? "file"}`;
    }
    if (inferred?.kind === "append" && matchingCall) {
      return `Appended to ${(matchingCall.arguments.path as string) ?? "file"}`;
    }
    if (inferred?.kind === "delete" && matchingCall) {
      return `Deleted ${(matchingCall.arguments.path as string) ?? "file"}`;
    }
    if (inferred?.kind === "move" && matchingCall) {
      return `Moved ${(matchingCall.arguments.from as string) ?? "?"} → ${(matchingCall.arguments.to as string) ?? "?"}`;
    }
    if (inferred?.kind === "memory_list") {
      try {
        const files = JSON.parse(message.content) as { path: string }[];
        return `Found ${files.length} memory file${files.length === 1 ? "" : "s"}`;
      } catch {
        return "Listed memory files";
      }
    }
    if (inferred?.kind === "memory_search") {
      try {
        const results = JSON.parse(message.content) as { path: string }[];
        return `Search: ${results.length} result${results.length === 1 ? "" : "s"}`;
      } catch {
        return "Search results";
      }
    }
    if (inferred?.kind === "not_found") return "Not found";
    if (inferred?.kind === "generic" && matchingCall) {
      if (matchingCall.name === "memory_read") {
        const lines = message.content.split("\n");
        if (lines.length <= 3) return `Read ${matchingCall.arguments.path ?? "file"}`;
        return `Read ${matchingCall.arguments.path ?? "file"} (${lines.length} lines)`;
      }
    }
    if (message.content.length < 60) return message.content;
    return `${message.content.slice(0, 60)}…`;
  }

  return (
    <div className="flex justify-start">
      <div
        className={`max-w-[85%] rounded-xl border text-xs ${
          isError
            ? "border-red-200 bg-red-50 dark:border-red-900 dark:bg-red-950/50"
            : "border-slate-200 bg-slate-50 dark:border-slate-700 dark:bg-slate-900/50"
        }`}
      >
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex w-full items-center gap-1.5 px-3 py-2 text-left hover:text-slate-700 dark:hover:text-slate-200"
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          <span className={`flex items-center gap-1 ${toolColor}`}>
            {toolIcon}
            <span className="font-medium">{toolName}</span>
          </span>
          {preview && (
            <span className="truncate text-slate-400 dark:text-slate-500">· {preview}</span>
          )}
          {!matchingCall && (
            <span className="ml-auto text-slate-400 dark:text-slate-500">{resultSummary()}</span>
          )}
        </button>
        {expanded && (
          <pre className="overflow-x-auto border-t border-slate-200 dark:border-slate-700 px-3 py-2 font-mono text-slate-700 dark:text-slate-300 max-h-[300px] overflow-y-auto">
            {message.content}
          </pre>
        )}
      </div>
    </div>
  );
}

// ── Main MessageBubble ─────────────────────────────────────────────────────────

interface Props {
  message: Message;
  onRetry?: (messageId: string) => void;
  retrying?: boolean;
  accessToken: string;
  hasAudio?: boolean;
  /** Tool calls from the nearest preceding assistant message (for pairing with tool results). */
  toolCalls?: ToolCallInfo[];
}

export default function MessageBubble({
  message,
  onRetry,
  retrying = false,
  accessToken,
  hasAudio = false,
  toolCalls,
}: Props) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";

  const [playing, setPlaying] = useState(false);
  const [playError, setPlayError] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const urlRef = useRef<string | null>(null);

  const handleSpeak = useCallback(async () => {
    if (playing) {
      audioRef.current?.pause();
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
        urlRef.current = null;
      }
      audioRef.current = null;
      setPlaying(false);
      setPlayError(false);
      return;
    }

    setPlayError(false);
    try {
      const blob = await tts(stripMarkdown(message.content), accessToken);
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);

      audio.onended = () => {
        setPlaying(false);
        if (urlRef.current) {
          URL.revokeObjectURL(urlRef.current);
          urlRef.current = null;
        }
        audioRef.current = null;
      };
      audio.onerror = () => {
        setPlaying(false);
        setPlayError(true);
        if (urlRef.current) {
          URL.revokeObjectURL(urlRef.current);
          urlRef.current = null;
        }
        audioRef.current = null;
      };

      urlRef.current = url;
      audioRef.current = audio;
      setPlaying(true);
      await audio.play();
    } catch {
      setPlayError(true);
      setPlaying(false);
    }
  }, [message.content, accessToken, playing]);

  useEffect(() => {
    return () => {
      audioRef.current?.pause();
      if (urlRef.current) {
        URL.revokeObjectURL(urlRef.current);
      }
    };
  }, []);

  if (message.role === "tool") {
    return <ToolMessage message={message} calls={toolCalls ?? []} />;
  }

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[88%] rounded-2xl rounded-br-md bg-gradient-to-br from-indigo-600 to-indigo-500 px-4 py-3 text-sm text-white shadow-md shadow-indigo-950/10 sm:max-w-[78%]">
          <p className="whitespace-pre-wrap leading-relaxed">{message.content}</p>
        </div>
      </div>
    );
  }

  // Parse tool calls on assistant messages
  const calls =
    isAssistant && message.tool_calls?.length
      ? parseToolCalls(message.tool_calls as unknown[])
      : [];

  const active = message.status === "pending" || message.status === "streaming";
  const retryable = message.status === "failed" || message.status === "interrupted";
  const canSpeak = hasAudio && message.status === "complete" && !!message.content;

  if (!isUser && !active && !retryable && !message.content) return null;

  return (
    <div className="flex items-start gap-2.5">
      <BrandMark
        compact
        markOnly
        className="mt-0.5 [&>span:first-child]:size-7 [&>span:first-child]:rounded-[0.65rem]"
      />
      <div className="max-w-[calc(100%-2.5rem)] rounded-2xl rounded-tl-md border border-slate-200/70 bg-white/70 px-4 py-3 text-sm text-slate-900 shadow-sm backdrop-blur dark:border-slate-800 dark:bg-slate-900/65 dark:text-slate-100 sm:max-w-[85%]">
        {/* Tool call badges */}
        {calls.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-2 pb-2 border-b border-slate-100 dark:border-slate-800">
            {calls.map((call) => (
              <ToolCallBadge key={call.id} call={call} />
            ))}
          </div>
        )}

        {active && !message.content ? (
          <span className="flex gap-1 items-center py-0.5">
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:0ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:150ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:300ms]" />
          </span>
        ) : (
          <div className="prose prose-sm max-w-none">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[
                [
                  rehypePrettyCode,
                  {
                    theme: {
                      light: "github-light",
                      dark: "github-dark",
                    },
                    keepBackground: false,
                    getHighlighter: () => syncHighlighter,
                  },
                ],
              ]}
            >
              {message.content}
            </ReactMarkdown>
          </div>
        )}
        {active && message.content && (
          <p className="mt-2 text-xs text-slate-400 dark:text-slate-500">Still working…</p>
        )}
        {retryable && (
          <div className="mt-3 border-t border-slate-200 pt-3 dark:border-slate-700">
            <p className="text-xs text-red-600 dark:text-red-400">
              {message.error || "This response did not finish."}
            </p>
            {onRetry && (
              <button
                type="button"
                onClick={() => onRetry(message.id)}
                disabled={retrying}
                className="mt-2 rounded-lg border border-slate-300 px-2.5 py-1.5 text-xs font-medium text-slate-700 transition-colors hover:bg-slate-50 disabled:cursor-wait disabled:opacity-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
              >
                {retrying ? "Retrying…" : "Retry response"}
              </button>
            )}
          </div>
        )}
        {canSpeak && (
          <div className="mt-2 flex items-center gap-2">
            <button
              type="button"
              onClick={handleSpeak}
              className={`rounded-lg p-1.5 transition-colors ${
                playError
                  ? "text-red-400 hover:bg-red-50 dark:hover:bg-red-900/30"
                  : playing
                    ? "text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30"
                    : "text-slate-400 hover:bg-slate-100 hover:text-slate-600 dark:hover:bg-slate-800 dark:hover:text-slate-300"
              }`}
              aria-label={playing ? "Stop" : "Read aloud"}
            >
              {playing ? <VolumeX size={14} /> : <Volume2 size={14} />}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
