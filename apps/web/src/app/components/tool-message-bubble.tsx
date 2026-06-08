"use client";

import { useState } from "react";
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
} from "lucide-react";
import type { Message } from "@/lib/types";

interface ToolCallInfo {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
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

export default function ToolMessageBubble({
  message,
  calls,
}: {
  message: Message;
  calls: ToolCallInfo[];
}) {
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
          isError ? "border-destructive/20 bg-destructive/10" : "border-border bg-muted/50"
        }`}
      >
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex w-full items-center gap-1.5 px-3 py-2 text-left hover:text-foreground"
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          <span className={`flex items-center gap-1 ${toolColor}`}>
            {toolIcon}
            <span className="font-medium">{toolName}</span>
          </span>
          {preview && <span className="truncate text-muted-foreground">· {preview}</span>}
          {!matchingCall && (
            <span className="ml-auto text-muted-foreground">{resultSummary()}</span>
          )}
        </button>
        {expanded && (
          <pre className="overflow-x-auto border-t border-border px-3 py-2 font-mono text-foreground max-h-[300px] overflow-y-auto">
            {message.content}
          </pre>
        )}
      </div>
    </div>
  );
}
