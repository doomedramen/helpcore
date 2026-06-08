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

function inferToolResultType(content: string): { kind: string; data?: any } | null {
  let resultValue: any = content;

  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === "object" && "ok" in parsed) {
      if (parsed.ok === false) return { kind: "error" };
      resultValue = parsed.result;
      // Result might be a JSON string itself
      if (typeof resultValue === "string") {
        try {
          const nested = JSON.parse(resultValue);
          resultValue = nested;
        } catch {
          // not nested JSON
        }
      }
    } else {
      resultValue = parsed;
    }
  } catch {
    // not JSON
  }

  if (typeof resultValue === "object" && resultValue !== null && "action" in resultValue) {
    return { kind: resultValue.action, data: resultValue };
  }

  if (Array.isArray(resultValue) && resultValue.length > 0) {
    const first = resultValue[0] as Record<string, unknown>;
    if (first && typeof first === "object") {
      if ("path" in first && "updated_at" in first && !("content" in first)) {
        return { kind: "memory_list" };
      }
      if ("path" in first && "content" in first) {
        return { kind: "memory_search" };
      }
    }
  }

  if (resultValue === "saved") return { kind: "write" };
  if (resultValue === "appended") return { kind: "append" };
  if (resultValue === "deleted") return { kind: "delete" };
  if (resultValue === "moved") return { kind: "move" };
  if (typeof resultValue === "string" && resultValue.startsWith("(no memory file")) {
    return { kind: "not_found" };
  }

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
  const kind = inferred?.kind;
  const data = inferred?.data;

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
    const kind = inferred?.kind;
    const data = inferred?.data;

    if ((kind === "write" || kind === "memory_write") && matchingCall) {
      const path = (data?.path as string) ?? (matchingCall.arguments.path as string);
      return `Saved ${path ?? "file"}`;
    }
    if ((kind === "personality_write" || kind === "personality_append") && data) {
      return `Updated ${data.name} personality`;
    }
    if ((kind === "append" || kind === "memory_append") && matchingCall) {
      const path = (data?.path as string) ?? (matchingCall.arguments.path as string);
      return `Appended to ${path ?? "file"}`;
    }
    if ((kind === "memory_read" || kind === "read") && (data || matchingCall)) {
      const content = data?.content ?? unwrapResult(message.content);
      const path = data?.path ?? matchingCall?.arguments.path ?? "file";
      const lines = typeof content === "string" ? content.split("\n") : [];
      if (lines.length <= 3) return `Read ${path}`;
      return `Read ${path} (${lines.length} lines)`;
    }
    if (kind === "delete" && (data || matchingCall)) {
      const path = data?.path ?? matchingCall?.arguments.path ?? "file";
      return `Deleted ${path}`;
    }
    if (kind === "move" && (data || matchingCall)) {
      const from = data?.from ?? matchingCall?.arguments.from ?? "?";
      const to = data?.to ?? matchingCall?.arguments.to ?? "?";
      return `Moved ${from} → ${to}`;
    }
    const finalContent = unwrapResult(message.content);
    const displayContent =
      typeof finalContent === "string" ? finalContent : JSON.stringify(finalContent);
    if (displayContent.length < 60) return displayContent;
    return `${displayContent.slice(0, 60)}…`;
  }

  function unwrapResult(content: string): any {
    try {
      const parsed = JSON.parse(content);
      if (parsed && typeof parsed === "object" && "ok" in parsed) {
        return parsed.result;
      }
      return parsed;
    } catch {
      return content;
    }
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
          <div className="overflow-x-auto border-t border-border px-3 py-2 font-mono text-foreground max-h-[400px] overflow-y-auto">
            {kind === "move" || kind === "memory_move" ? (
              <div className="flex flex-col gap-2 py-1">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] uppercase text-muted-foreground w-12">From</span>
                  <span className="rounded bg-muted px-1.5 py-0.5 text-xs">
                    {data?.from ?? matchingCall?.arguments.from}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] uppercase text-muted-foreground w-12">To</span>
                  <span className="rounded bg-emerald-500/10 text-emerald-200/90 px-1.5 py-0.5 text-xs">
                    {data?.to ?? matchingCall?.arguments.to}
                  </span>
                </div>
              </div>
            ) : kind === "delete" || kind === "memory_delete" ? (
              <div className="flex flex-col gap-1">
                <span className="text-[10px] uppercase text-muted-foreground">Deleted Content</span>
                <pre className="whitespace-pre-wrap rounded bg-red-500/5 p-2 text-red-200/70 line-through">
                  {data?.old_content ?? unwrapResult(message.content)}
                </pre>
              </div>
            ) : kind === "memory_list" || kind === "memory_search" ? (
              <div className="flex flex-col gap-2">
                <span className="text-[10px] uppercase text-muted-foreground">
                  {kind === "memory_list" ? "Files" : "Search Results"}
                </span>
                <div className="flex flex-wrap gap-2">
                  {(() => {
                    try {
                      const content = unwrapResult(message.content);
                      const files = (
                        typeof content === "string" ? JSON.parse(content) : content
                      ) as { path: string }[];
                      return files.map((f, i) => (
                        <div
                          key={`${f.path}-${i}`}
                          className="flex items-center gap-1.5 rounded border border-border bg-background px-2 py-1 text-xs"
                        >
                          <FileText size={12} className="text-muted-foreground" />
                          <span>{f.path}</span>
                        </div>
                      ));
                    } catch {
                      return (
                        <span className="text-muted-foreground italic">Error loading list</span>
                      );
                    }
                  })()}
                </div>
              </div>
            ) : inferred?.data?.content !== undefined ||
              kind === "read" ||
              kind === "memory_read" ? (
              <div className="flex flex-col gap-2">
                {inferred?.data?.old_content && (
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] uppercase text-muted-foreground">Before</span>
                    <pre className="whitespace-pre-wrap rounded bg-red-500/5 p-2 text-red-200/70 line-through">
                      {inferred.data.old_content}
                    </pre>
                  </div>
                )}
                <div className="flex flex-col gap-1">
                  <span className="text-[10px] uppercase text-muted-foreground">
                    {inferred?.data?.old_content ? "After" : "Content"}
                  </span>
                  <pre className="whitespace-pre-wrap rounded bg-emerald-500/5 p-2 text-emerald-200/90">
                    {inferred?.data?.content ?? unwrapResult(message.content)}
                  </pre>
                </div>
              </div>
            ) : (
              <pre className="whitespace-pre-wrap">
                {typeof unwrapResult(message.content) === "string"
                  ? unwrapResult(message.content)
                  : JSON.stringify(unwrapResult(message.content), null, 2)}
              </pre>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
