"use client";

import { useState } from "react";
import { FilePlus } from "lucide-react";
import { toast } from "sonner";
import { useSWRConfig } from "swr";
import { useAuth } from "@/context/auth";
import { putMemoryFile } from "@/lib/api";
import type { MemoryEntry } from "@/lib/types";

interface Props {
  files: MemoryEntry[];
  selected: string | null;
  dirty: boolean;
  onSelect: (path: string) => void;
}

function relativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "";
  const mins = Math.round((Date.now() - t) / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(iso).toLocaleDateString();
}

export default function MemoryFileList({ files, selected, dirty, onSelect }: Props) {
  const { accessToken } = useAuth();
  const { mutate } = useSWRConfig();
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);

  async function handleCreate() {
    const name = newName
      .trim()
      .replace(/\s+/g, "-")
      .replace(/[^a-zA-Z0-9_.-]/g, "");
    if (!name || !accessToken) return;
    const path = name.endsWith(".md") ? name : `${name}.md`;
    setCreating(true);
    try {
      await putMemoryFile(path, "", accessToken);
      await mutate(["/api/memory", accessToken]);
      setNewName("");
      onSelect(path);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Could not create file");
    } finally {
      setCreating(false);
    }
  }

  const createForm = (
    <div className="flex gap-1.5">
      <input
        type="text"
        value={newName}
        onChange={(e) => setNewName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void handleCreate();
        }}
        placeholder="new-file.md"
        className="field-input min-h-9 min-w-0 flex-1 px-2.5 py-1.5 text-xs"
      />
      <button
        onClick={handleCreate}
        disabled={creating || !newName.trim()}
        title="Create file"
        aria-label="Create memory file"
        className="primary-action min-h-9 px-2.5 py-1.5"
      >
        <FilePlus size={14} />
      </button>
    </div>
  );

  return (
    <>
      {/* Mobile: dropdown + create */}
      <div className="space-y-2 border-b border-slate-200/70 p-3 dark:border-slate-800 md:hidden">
        <select
          value={selected ?? ""}
          onChange={(e) => {
            if (e.target.value) onSelect(e.target.value);
          }}
          className="field-input"
        >
          <option value="">Select a memory file</option>
          {files.map((entry) => (
            <option key={entry.path} value={entry.path}>
              {entry.path}
            </option>
          ))}
        </select>
        {createForm}
      </div>

      {/* Desktop: file list + create */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-slate-200/70 dark:border-slate-800 md:flex">
        <p className="px-4 pb-2 pt-4 text-xs font-semibold uppercase tracking-wider text-slate-400">
          Memory files
        </p>
        <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 pb-2">
          {files.map((entry) => (
            <button
              key={entry.path}
              onClick={() => onSelect(entry.path)}
              className={`w-full rounded-lg px-3 py-2 text-left transition-colors ${
                selected === entry.path
                  ? "bg-indigo-50 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300"
                  : "text-slate-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
              }`}
            >
              <span className="flex items-center gap-1.5 text-sm">
                <span className="truncate">{entry.path}</span>
                {dirty && selected === entry.path && (
                  <span
                    className="inline-block size-1.5 shrink-0 rounded-full bg-amber-400"
                    title="Unsaved changes"
                  />
                )}
              </span>
              <span className="block text-[11px] text-slate-400 dark:text-slate-500">
                {relativeTime(entry.updated_at)}
              </span>
            </button>
          ))}
          {files.length === 0 && (
            <p className="px-3 py-4 text-xs text-slate-400">No memory files yet.</p>
          )}
        </nav>
        <div className="border-t border-slate-200/70 p-3 dark:border-slate-800">{createForm}</div>
      </aside>
    </>
  );
}
