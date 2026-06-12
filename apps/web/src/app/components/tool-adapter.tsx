"use client";

import { useMemo } from "react";
import type { ReactNode } from "react";
import { diffLines } from "diff";
import {
  AlertTriangle,
  BarChart3,
  FileText,
  FolderOpen,
  GitGraph,
  MoveRight,
  Pencil,
  Plus,
  Search,
  Trash2,
  Brain,
  WrenchIcon,
  ChevronDownIcon,
  CheckCircleIcon,
  XCircleIcon,
} from "lucide-react";
import type { Message } from "@/lib/types";
import { Tool, ToolContent } from "@/components/ai-elements/tool";
import { CodeBlock } from "@/components/ai-elements/code-block";
import { Badge } from "@/app/components/ui/badge";
import { CollapsibleTrigger } from "@/app/components/ui/collapsible";
import { cn } from "@/lib/utils";
import { MessageContentWithAssets } from "./asset-renderer";

interface ToolCallInfo {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

const TOOL_META: Record<string, { icon: ReactNode; label: string; color: string }> = {
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
  personality_append: {
    icon: <Brain size={11} />,
    label: "Personality",
    color: "text-fuchsia-500 dark:text-fuchsia-400",
  },
  chart_generate: {
    icon: <BarChart3 size={11} />,
    label: "Chart",
    color: "text-cyan-500 dark:text-cyan-400",
  },
  mermaid_render: {
    icon: <GitGraph size={11} />,
    label: "Diagram",
    color: "text-orange-500 dark:text-orange-400",
  },
  conversation_rename: {
    icon: <Pencil size={11} />,
    label: "Rename",
    color: "text-slate-500 dark:text-slate-400",
  },
  skill_read: {
    icon: <FileText size={11} />,
    label: "Skill",
    color: "text-indigo-500 dark:text-indigo-400",
  },
};

function parseToolResult(content: string): {
  ok: boolean;
  result: unknown;
  error?: string;
  code?: string;
} {
  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === "object" && "ok" in parsed) {
      if (parsed.ok === false) {
        return {
          ok: false,
          result: null,
          error: (parsed as Record<string, unknown>).error as string | undefined,
          code: (parsed as Record<string, unknown>).code as string | undefined,
        };
      }
      let result = (parsed as Record<string, unknown>).result;
      if (typeof result === "string") {
        try {
          result = JSON.parse(result);
        } catch {
          /* keep as string */
        }
      }
      return { ok: true, result };
    }
    return { ok: true, result: parsed };
  } catch {
    return { ok: true, result: content };
  }
}

function unwrapResult(content: string): unknown {
  return parseToolResult(content).result;
}

export function getToolIcon(name: string): ReactNode {
  return TOOL_META[name]?.icon ?? <WrenchIcon size={11} />;
}

export function getToolLabel(name: string): string {
  return TOOL_META[name]?.label ?? name;
}

export function getToolColor(name: string): string {
  return TOOL_META[name]?.color ?? "text-indigo-500 dark:text-indigo-400";
}

function toolArgsPreview(name: string, args: Record<string, unknown>): string {
  if (name === "personality_write") return `/${(args.name as string) ?? "?"}`;
  if (name === "chart_generate") return (args.type as string) ?? "chart";
  if (name === "mermaid_render") {
    const def = (args.definition as string) ?? "";
    const firstLine = def.split("\n")[0]?.trim() ?? "";
    return firstLine.length > 50 ? `${firstLine.slice(0, 47)}\u2026` : firstLine;
  }
  const path = args.path ?? args.from ?? null;
  if (path && typeof path === "string") {
    const label = path.split("/").pop() ?? path;
    if (name === "memory_move") return `${label} \u2192 ${(args.to as string) ?? "?"}`;
    return label;
  }
  return "";
}

function inferResultKind(content: string): { kind: string; data?: Record<string, unknown> } {
  const { ok, result } = parseToolResult(content);
  if (!ok) return { kind: "error" };

  const rv = result as unknown;
  if (typeof rv === "object" && rv !== null && "action" in (rv as object)) {
    return {
      kind: (rv as Record<string, unknown>).action as string,
      data: rv as Record<string, unknown>,
    };
  }

  if (Array.isArray(rv) && rv.length > 0) {
    const first = rv[0] as Record<string, unknown>;
    if (first && typeof first === "object") {
      if ("path" in first && "updated_at" in first && !("content" in first)) {
        return { kind: "memory_list" };
      }
      if ("path" in first && "content" in first) {
        return { kind: "memory_search" };
      }
    }
  }

  if (rv === "saved") return { kind: "write" };
  if (rv === "appended") return { kind: "append" };
  if (rv === "deleted") return { kind: "delete" };
  if (rv === "moved") return { kind: "move" };
  if (typeof rv === "string" && (rv.startsWith("(no memory") || rv.startsWith("renamed"))) {
    return { kind: rv.startsWith("renamed") ? "rename" : "not_found" };
  }

  return { kind: "generic" };
}

interface ToolMessageAdapterProps {
  message: Message;
  call?: ToolCallInfo;
}

export default function ToolMessageAdapter({ message, call }: ToolMessageAdapterProps) {
  const { ok, error } = parseToolResult(message.content);
  const isError = !ok;
  const inferred = inferResultKind(message.content);
  const kind = inferred.kind;
  const data = inferred.data;

  const toolName = call?.name ?? (isError ? "error" : "unknown");
  const meta = TOOL_META[toolName];
  const icon = meta?.icon ?? (isError ? <AlertTriangle size={11} /> : <WrenchIcon size={11} />);
  const label = meta?.label ?? toolName;
  const color = isError
    ? "text-red-500 dark:text-red-400"
    : (meta?.color ?? "text-indigo-500 dark:text-indigo-400");
  const preview = call ? toolArgsPreview(call.name, call.arguments) : undefined;

  const statusBadge = isError ? (
    <Badge className="gap-1.5 rounded-full text-xs" variant="secondary">
      <XCircleIcon className="size-4 text-red-600" />
      Error
    </Badge>
  ) : (
    <Badge className="gap-1.5 rounded-full text-xs" variant="secondary">
      <CheckCircleIcon className="size-4 text-green-600" />
      Completed
    </Badge>
  );

  return (
    <Tool defaultOpen={false}>
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-4 p-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className={cn("flex shrink-0 items-center", color)}>{icon}</span>
          <span className="font-medium text-sm truncate">{label}</span>
          {preview && (
            <span className="truncate text-muted-foreground text-xs">\u00b7 {preview}</span>
          )}
          {statusBadge}
        </div>
        <ChevronDownIcon className="size-4 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-180" />
      </CollapsibleTrigger>
      <ToolContent>
        {call && (
          <div className="space-y-2 overflow-hidden">
            <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
              Parameters
            </h4>
            <div className="rounded-md bg-muted/50">
              <CodeBlock code={JSON.stringify(call.arguments, null, 2)} language="json" />
            </div>
          </div>
        )}

        {isError ? (
          <div className="space-y-2">
            <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
              Error
            </h4>
            <div className="overflow-x-auto rounded-md bg-destructive/10 p-3 text-xs text-destructive">
              <p>{error ?? "Unknown error"}</p>
            </div>
          </div>
        ) : kind === "chart" && data ? (
          <ChartOutput data={data} />
        ) : kind === "mermaid" ? (
          <MermaidOutput data={data} />
        ) : kind === "memory_list" || kind === "memory_search" ? (
          <MemoryListOutput content={message.content} kind={kind} />
        ) : kind === "move" || kind === "memory_move" ? (
          <MoveOutput data={data} call={call} />
        ) : kind === "delete" || kind === "memory_delete" ? (
          <DeleteOutput data={data} content={message.content} />
        ) : kind === "read" ||
          kind === "memory_read" ||
          kind === "write" ||
          kind === "memory_write" ||
          kind === "append" ||
          kind === "memory_append" ||
          kind === "personality_write" ||
          kind === "personality_append" ? (
          <ReadWriteOutput data={data} call={call} content={message.content} kind={kind} />
        ) : (
          <GenericOutput content={message.content} />
        )}
      </ToolContent>
    </Tool>
  );
}

function ChartOutput({ data }: { data: Record<string, unknown> }) {
  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      <div className="rounded-md bg-muted/50 p-3 text-xs">
        <div className="flex flex-wrap items-center gap-2 text-muted-foreground">
          <span className="text-[10px] uppercase">Chart</span>
          <span className="rounded bg-background px-1.5 py-0.5 text-xs text-foreground">
            {(data.chart_type as string) ?? "?"}
          </span>
          {data.title ? (
            <span className="rounded bg-background px-1.5 py-0.5 text-xs text-foreground">
              {String(data.title)}
            </span>
          ) : null}
          {data.labels ? (
            <span className="text-[10px]">{(data.labels as unknown[]).length} points</span>
          ) : null}
        </div>
        <p className="mt-1 text-[10px] text-muted-foreground italic">
          Rendered inline in the response above
        </p>
      </div>
    </div>
  );
}

function MermaidOutput({ data }: { data?: Record<string, unknown> }) {
  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      <div className="rounded-md bg-muted/50 p-3 text-xs">
        <div className="flex flex-wrap items-center gap-2 text-muted-foreground">
          <span className="text-[10px] uppercase">Diagram</span>
          {data?.theme && data.theme !== "default" ? (
            <span className="rounded bg-background px-1.5 py-0.5 text-xs text-foreground">
              {String(data.theme)}
            </span>
          ) : null}
        </div>
        <p className="mt-1 text-[10px] text-muted-foreground italic">
          Rendered inline in the response above
        </p>
      </div>
    </div>
  );
}

function MemoryListOutput({ content, kind }: { content: string; kind: string }) {
  let files: { path: string }[] = [];
  try {
    const parsed = unwrapResult(content);
    files = (typeof parsed === "string" ? JSON.parse(parsed) : parsed) as { path: string }[];
  } catch {
    /* empty */
  }

  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
        {kind === "memory_list" ? "Files" : "Search Results"}
      </h4>
      <div className="flex flex-wrap gap-2">
        {files.length === 0 ? (
          <span className="text-muted-foreground italic text-xs">No results</span>
        ) : (
          files.map((f, i) => (
            <div
              key={`${f.path}-${i}`}
              className="flex items-center gap-1.5 rounded border bg-background px-2 py-1 text-xs"
            >
              <FileText size={12} className="text-muted-foreground" />
              <span>{f.path}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function MoveOutput({ data, call }: { data?: Record<string, unknown>; call?: ToolCallInfo }) {
  const from = data?.from ?? call?.arguments.from ?? "?";
  const to = data?.to ?? call?.arguments.to ?? "?";
  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      <div className="rounded-md bg-muted/50 p-3">
        <div className="flex flex-col gap-2 text-xs">
          <div className="flex items-center gap-2">
            <span className="text-[10px] uppercase text-muted-foreground w-12">From</span>
            <span className="rounded bg-background px-1.5 py-0.5 font-mono">{String(from)}</span>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-[10px] uppercase text-muted-foreground w-12">To</span>
            <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 font-mono text-emerald-700 dark:text-emerald-300">
              {String(to)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Git-style diff helpers
// ---------------------------------------------------------------------------

const DIFF_CONTEXT = 3;

type DiffLine =
  | { type: "added" | "removed" | "context"; text: string }
  | { type: "collapsed"; count: number };

function buildDiffLines(oldText: string, newText: string): DiffLine[] {
  const changes = diffLines(oldText, newText);

  // Flatten to per-line items
  const flat: Array<{ type: "added" | "removed" | "context"; text: string }> = [];
  for (const change of changes) {
    const rawLines = change.value.split("\n");
    if (rawLines[rawLines.length - 1] === "") rawLines.pop();
    for (const text of rawLines) {
      flat.push({ type: change.added ? "added" : change.removed ? "removed" : "context", text });
    }
  }

  // Collapse long unchanged runs to keep the view scannable
  const result: DiffLine[] = [];
  let i = 0;
  while (i < flat.length) {
    if (flat[i].type !== "context") {
      result.push(flat[i++]);
      continue;
    }
    let j = i;
    while (j < flat.length && flat[j].type === "context") j++;
    const run = flat.slice(i, j);
    const isFirst = i === 0;
    const isLast = j === flat.length;
    const showHead = isFirst ? 0 : DIFF_CONTEXT;
    const showTail = isLast ? 0 : DIFF_CONTEXT;
    if (run.length <= showHead + showTail) {
      for (const l of run) result.push(l);
    } else {
      for (let k = 0; k < showHead; k++) result.push(run[k]);
      result.push({ type: "collapsed", count: run.length - showHead - showTail });
      for (let k = run.length - showTail; k < run.length; k++) result.push(run[k]);
    }
    i = j;
  }
  return result;
}

function DiffView({ oldContent, newContent }: { oldContent: string; newContent: string }) {
  const lines = useMemo(() => buildDiffLines(oldContent, newContent), [oldContent, newContent]);
  const hasChanges = lines.some((l) => l.type === "added" || l.type === "removed");

  if (!hasChanges) {
    return (
      <pre className="font-mono whitespace-pre-wrap rounded bg-background p-2 text-xs text-foreground">
        {newContent}
      </pre>
    );
  }

  return (
    <div className="overflow-hidden rounded border border-border/40 font-mono text-xs">
      {lines.map((line, i) => {
        if (line.type === "collapsed") {
          return (
            <div
              key={i}
              className="select-none bg-muted/20 px-2 py-0.5 text-center text-muted-foreground/60"
            >
              ⋯ {line.count} unchanged {line.count === 1 ? "line" : "lines"} ⋯
            </div>
          );
        }
        if (line.type === "removed") {
          return (
            <div key={i} className="flex min-w-0 bg-red-500/10">
              <span className="w-4 shrink-0 select-none text-center text-red-400/70">−</span>
              <span className="flex-1 whitespace-pre-wrap break-all px-1 text-red-700 dark:text-red-300/90">
                {line.text}
              </span>
            </div>
          );
        }
        if (line.type === "added") {
          return (
            <div key={i} className="flex min-w-0 bg-emerald-500/10">
              <span className="w-4 shrink-0 select-none text-center text-emerald-500/70">+</span>
              <span className="flex-1 whitespace-pre-wrap break-all px-1 text-emerald-700 dark:text-emerald-300">
                {line.text}
              </span>
            </div>
          );
        }
        return (
          <div key={i} className="flex min-w-0">
            <span className="w-4 shrink-0 select-none text-center text-muted-foreground/30"> </span>
            <span className="flex-1 whitespace-pre-wrap break-all px-1 text-foreground/70">
              {line.text}
            </span>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tool output components
// ---------------------------------------------------------------------------

function DeleteOutput({ data, content }: { data?: Record<string, unknown>; content: string }) {
  const oldContent = (data?.old_content ?? unwrapResult(content)) as string;
  const path = data?.path as string;

  const lines = useMemo(() => {
    if (!oldContent || typeof oldContent !== "string") return [];
    const rawLines = oldContent.split("\n");
    if (rawLines[rawLines.length - 1] === "") rawLines.pop();
    return rawLines;
  }, [oldContent]);

  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      <div className="rounded-md bg-muted/50 p-3 text-xs">
        {path && (
          <p className="mb-2">
            <span className="text-muted-foreground">Deleted </span>
            <span className="font-mono">{path}</span>
          </p>
        )}
        {lines.length > 0 && (
          <div className="max-h-[200px] overflow-y-auto overflow-hidden rounded border border-border/40 font-mono">
            {lines.map((text, i) => (
              <div key={i} className="flex min-w-0 bg-red-500/10">
                <span className="w-4 shrink-0 select-none text-center text-red-400/70">−</span>
                <span className="flex-1 whitespace-pre-wrap break-all px-1 text-red-700 dark:text-red-300/90">
                  {text}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ReadWriteOutput({
  data,
  call,
  content,
  kind,
}: {
  data?: Record<string, unknown>;
  call?: ToolCallInfo;
  content: string;
  kind: string;
}) {
  const displayContent = (data?.content ?? unwrapResult(content)) as string;
  const oldContent = data?.old_content as string | undefined;
  const path = (data?.path ?? call?.arguments.path) as string | undefined;
  const isWrite = kind === "write" || kind === "memory_write" || kind === "personality_write";
  const isAppend = kind === "append" || kind === "memory_append" || kind === "personality_append";
  const actionLabel = isWrite ? "Saved" : isAppend ? "Appended" : "Content";

  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      <div className="rounded-md bg-muted/50 p-3 text-xs">
        {(isWrite || isAppend) && path && (
          <p className="mb-2">
            <span className="text-muted-foreground">{actionLabel} </span>
            <span className="font-mono">{path}</span>
          </p>
        )}
        {oldContent &&
        typeof oldContent === "string" &&
        displayContent &&
        typeof displayContent === "string" ? (
          <DiffView oldContent={oldContent} newContent={displayContent} />
        ) : (
          displayContent &&
          typeof displayContent === "string" && (
            <pre className="font-mono whitespace-pre-wrap rounded bg-background p-2 text-foreground">
              {displayContent}
            </pre>
          )
        )}
      </div>
    </div>
  );
}

function GenericOutput({ content }: { content: string }) {
  const result = unwrapResult(content);
  const display = typeof result === "string" ? result : JSON.stringify(result, null, 2);
  const isAsset =
    typeof result === "string" && /^data:(image|audio|video)\/[^;]+;base64,/.test(result);

  return (
    <div className="space-y-2">
      <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">Result</h4>
      {isAsset ? (
        <div className="rounded-md bg-muted/50 p-3">
          <MessageContentWithAssets>{display}</MessageContentWithAssets>
        </div>
      ) : (
        <div className="rounded-md bg-muted/50">
          <CodeBlock code={display} language="json" />
        </div>
      )}
    </div>
  );
}
