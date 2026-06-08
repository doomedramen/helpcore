"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import { toast } from "sonner";
import { FilePlus, Trash2 } from "lucide-react";
import AppShell from "@/app/components/app-shell";
import PageHeader from "@/app/components/page-header";
import SettingsNav from "@/app/components/settings-nav";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { deleteMemoryFile, getMemoryFile, listMemory, putMemoryFile, updateMe } from "@/lib/api";
import type { MemoryEntry } from "@/lib/types";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/app/components/ui/dialog";

const MEMORY_LEANING_OPTIONS = [
  {
    value: "off",
    label: "Don't use memory",
    description: "The AI will not store or recall anything about you.",
  },
  { value: "light", label: "Light", description: "Only save things when you explicitly ask." },
  {
    value: "moderate",
    label: "Moderate",
    description: "Balance — save useful context, skip trivia.",
  },
  {
    value: "heavy",
    label: "Heavy",
    description: "Proactively remember everything it learns about you.",
  },
];

export default function MemoryPage() {
  const { accessToken, currentUser } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const [selected, setSelected] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [isDirty, setIsDirty] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  const [memoryLeaning, setMemoryLeaning] = useState(currentUser?.memory_leaning ?? "moderate");
  const [leaningSaving, setLeaningSaving] = useState(false);

  useEffect(() => {
    if (currentUser?.memory_leaning) {
      setMemoryLeaning(currentUser.memory_leaning);
    }
  }, [currentUser?.memory_leaning]);

  const { data: fileList } = useSWR<MemoryEntry[]>(
    accessToken ? ["/api/memory", accessToken] : null,
    ([, t]) => listMemory(t as string).then((r) => r.files),
    { refreshInterval: 30_000 },
  );

  const { data: fileContent, mutate: mutateFile } = useSWR(
    accessToken && selected ? [`/api/memory/${selected}`, accessToken] : null,
    ([, t]) => getMemoryFile(selected!, t as string),
  );

  // No auth guard needed here — AuthGate (in the root layout) only renders
  // this page once a session is confirmed.

  function handleSelect(path: string) {
    setSelected(path);
    setIsDirty(false);
    setDraft("");
    setError(null);
    setSavedAt(null);
  }

  function currentContent(): string {
    if (isDirty) return draft;
    return fileContent?.content ?? "";
  }

  function handleChange(value: string) {
    setDraft(value);
    setIsDirty(true);
    setError(null);
    setSavedAt(null);
  }

  async function handleSave() {
    if (!accessToken || !selected) return;
    setSaving(true);
    setError(null);
    try {
      await putMemoryFile(selected, currentContent(), accessToken);
      await mutateFile();
      setIsDirty(false);
      setSavedAt(new Date().toLocaleTimeString());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  async function confirmDelete() {
    if (!accessToken || !selected) return;
    setDeleteConfirmOpen(false);
    setDeleting(true);
    try {
      await deleteMemoryFile(selected, accessToken);
      setSelected(null);
      setIsDirty(false);
      setDraft("");
      await mutate(["/api/memory", accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
    } finally {
      setDeleting(false);
    }
  }

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
      handleSelect(path);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Create failed");
    } finally {
      setCreating(false);
    }
  }

  async function handleLeaningChange(value: string) {
    setMemoryLeaning(value);
    if (!accessToken) return;
    setLeaningSaving(true);
    try {
      await updateMe({ memory_leaning: value }, accessToken);
      toast.success("Saved");
    } catch {
      setMemoryLeaning(currentUser?.memory_leaning ?? "moderate");
      toast.error("Could not save memory leaning");
    } finally {
      setLeaningSaving(false);
    }
  }

  return (
    <AppShell
      conversationId={null}
      onSelectConversation={(id) => router.push(id ? `/chat/?id=${id}` : "/chat/")}
      mainClassName="flex flex-col"
    >
      <div className="shrink-0 border-b border-slate-200/70 bg-white/45 backdrop-blur dark:border-slate-800 dark:bg-slate-950/25">
        <div className="px-4 pb-2 pt-5 sm:px-6">
          <PageHeader
            breadcrumb="Settings"
            title="Memory"
            description="Edit memory files the assistant can reference."
          />
        </div>
        <div className="px-4 pb-4 sm:px-6">
          <SettingsNav />
        </div>
      </div>

      <div className="mx-auto w-full max-w-5xl px-4 sm:px-6">
        <div className="surface-card p-4 sm:p-5">
          <h3 className="text-sm font-semibold text-slate-900 dark:text-white mb-2">
            Memory leaning
          </h3>
          <p className="mb-4 text-xs leading-5 text-slate-500 dark:text-slate-400">
            Controls how heavily the AI leans into storing and recalling information it learns about
            you.
          </p>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
            {MEMORY_LEANING_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                onClick={() => handleLeaningChange(opt.value)}
                disabled={leaningSaving}
                className={`rounded-xl border px-3 py-3 text-left transition ${
                  memoryLeaning === opt.value
                    ? "border-indigo-200 bg-indigo-50 ring-1 ring-indigo-200 dark:border-indigo-800 dark:bg-indigo-950/40 dark:ring-indigo-800"
                    : "border-slate-200/80 bg-white/60 hover:border-slate-300 dark:border-slate-800 dark:bg-slate-900/60 dark:hover:border-slate-700"
                }`}
              >
                <span className="block text-sm font-medium text-slate-900 dark:text-white">
                  {opt.label}
                </span>
                <span className="mt-0.5 block text-xs leading-4 text-slate-400 dark:text-slate-500">
                  {opt.description}
                </span>
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="flex flex-1 flex-col overflow-hidden md:flex-row">
        <div className="space-y-2 border-b border-slate-200/70 bg-white/55 p-3 dark:border-slate-800 dark:bg-slate-900/40 md:hidden">
          <select
            value={selected ?? ""}
            onChange={(event) => {
              if (event.target.value) handleSelect(event.target.value);
            }}
            className="field-input"
          >
            <option value="">Select a memory file</option>
            {(fileList ?? []).map((entry) => (
              <option key={entry.path} value={entry.path}>
                {entry.path}
              </option>
            ))}
          </select>
          <div className="flex gap-2">
            <input
              type="text"
              value={newName}
              onChange={(event) => setNewName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") handleCreate();
              }}
              placeholder="new-memory.md"
              className="field-input min-h-10 flex-1 py-2"
            />
            <button
              onClick={handleCreate}
              disabled={creating || !newName.trim()}
              className="primary-action min-h-10 px-3"
              aria-label="Create memory file"
            >
              <FilePlus size={15} />
              <span className="sr-only">Create</span>
            </button>
          </div>
        </div>

        <aside className="hidden w-60 shrink-0 flex-col border-r border-slate-200/70 bg-white/60 dark:border-slate-800 dark:bg-slate-900/45 md:flex">
          <div className="px-4 pt-5 pb-3">
            <p className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
              Memory files
            </p>
          </div>

          <nav className="flex-1 overflow-y-auto px-2 pb-2 space-y-0.5">
            {(fileList ?? []).map((entry) => (
              <button
                key={entry.path}
                onClick={() => handleSelect(entry.path)}
                className={`flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                  selected === entry.path
                    ? "bg-indigo-50 text-indigo-700 dark:bg-indigo-950/50 dark:text-indigo-300"
                    : "text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800"
                }`}
              >
                <span className="truncate">{entry.path}</span>
              </button>
            ))}
            {fileList?.length === 0 && (
              <p className="px-3 py-4 text-xs text-slate-400">No memory files yet.</p>
            )}
          </nav>

          {/* Create new file */}
          <div className="border-t border-slate-200 dark:border-slate-800 p-3">
            <div className="flex gap-1.5">
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate();
                }}
                placeholder="new-file.md"
                className="field-input min-h-9 min-w-0 flex-1 px-2.5 py-1.5 text-xs"
              />
              <button
                onClick={handleCreate}
                disabled={creating || !newName.trim()}
                title="Create file"
                className="primary-action min-h-9 px-2.5 py-1.5"
              >
                <FilePlus size={14} />
              </button>
            </div>
          </div>
        </aside>

        {/* Editor */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {selected ? (
            <>
              <div className="flex items-center justify-between gap-3 border-b border-slate-200/70 bg-white/35 px-4 py-3 dark:border-slate-800 dark:bg-slate-900/20 sm:px-6">
                <div>
                  <h1 className="text-base font-semibold text-slate-900 dark:text-white">
                    {selected}
                  </h1>
                  {savedAt && !error && (
                    <p className="text-xs text-slate-400 mt-0.5">Saved at {savedAt}</p>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setDeleteConfirmOpen(true)}
                    disabled={deleting}
                    className="icon-button size-9 hover:text-red-500 dark:hover:text-red-400"
                    title="Delete file"
                  >
                    <Trash2 size={15} />
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={saving || !isDirty}
                    className="primary-action min-h-9 px-3 py-1.5"
                  >
                    {saving ? "Saving…" : "Save"}
                  </button>
                </div>
              </div>

              {error && (
                <div className="mx-4 mt-4 md:mx-6">
                  <StatusMessage type="error" message={error} />
                </div>
              )}

              <textarea
                className="flex-1 resize-none bg-white/65 p-4 font-mono text-sm leading-6 text-slate-900 outline-none dark:bg-slate-900/55 dark:text-slate-100 sm:p-6"
                value={currentContent()}
                onChange={(e) => handleChange(e.target.value)}
                spellCheck={false}
                placeholder="Write markdown content here…"
              />
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-sm text-slate-400">
              Select a file or create a new one.
            </div>
          )}
        </div>
      </div>

      <Dialog
        open={deleteConfirmOpen}
        onOpenChange={(open) => {
          if (!open) setDeleteConfirmOpen(false);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete memory file</DialogTitle>
            <DialogDescription>Delete {selected}? This cannot be undone.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button
              onClick={() => setDeleteConfirmOpen(false)}
              className="secondary-action min-h-9 px-3 py-1.5"
            >
              Cancel
            </button>
            <button
              onClick={confirmDelete}
              className="inline-flex min-h-9 items-center justify-center rounded-xl bg-red-600 px-3 py-1.5 text-sm font-semibold text-white transition hover:bg-red-500"
            >
              Delete
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </AppShell>
  );
}
