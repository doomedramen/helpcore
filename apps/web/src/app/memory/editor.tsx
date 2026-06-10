"use client";

import { useEffect, useState } from "react";
import useSWR, { useSWRConfig } from "swr";
import { toast } from "sonner";
import { Trash2 } from "lucide-react";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { deleteMemoryFile, getMemoryFile, putMemoryFile } from "@/lib/api";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/app/components/ui/dialog";

interface Props {
  path: string;
  updatedAt?: string;
  onDirtyChange: (dirty: boolean) => void;
  onDeleted: () => void;
}

/**
 * Editor for a single memory file. Mount with `key={path}` so all draft
 * state resets when the selected file changes.
 */
export default function MemoryEditor({ path, updatedAt, onDirtyChange, onDeleted }: Props) {
  const { accessToken } = useAuth();
  const { mutate } = useSWRConfig();
  const { data: file, mutate: mutateFile } = useSWR(
    accessToken ? [`/api/memory/${path}`, accessToken] : null,
    ([, t]) => getMemoryFile(path, t as string),
  );

  // null = no local edits; the server copy is authoritative.
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const dirty = draft !== null;
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const content = draft ?? file?.content ?? "";

  async function handleSave() {
    if (!accessToken || !dirty || saving) return;
    setSaving(true);
    setError(null);
    try {
      await putMemoryFile(path, content, accessToken);
      await mutateFile();
      setDraft(null);
      void mutate(["/api/memory", accessToken]);
      toast.success("Saved");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!accessToken) return;
    setConfirmingDelete(false);
    setDeleting(true);
    try {
      await deleteMemoryFile(path, accessToken);
      void mutate(["/api/memory", accessToken]);
      onDeleted();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
      setDeleting(false);
    }
  }

  return (
    <div className="flex h-full flex-col p-4 sm:p-5">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate text-sm font-semibold text-slate-900 dark:text-white">{path}</h2>
          {updatedAt && (
            <p className="mt-0.5 text-xs text-slate-400 dark:text-slate-500">
              Last updated {new Date(updatedAt).toLocaleString()}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            onClick={() => setConfirmingDelete(true)}
            disabled={deleting}
            className="icon-button size-9 hover:text-red-500 dark:hover:text-red-400"
            title="Delete file"
            aria-label="Delete file"
          >
            <Trash2 size={15} />
          </button>
          <button
            onClick={handleSave}
            disabled={saving || !dirty}
            className="primary-action min-h-9 px-3 py-1.5"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      {error && (
        <div className="mb-3">
          <StatusMessage type="error" message={error} />
        </div>
      )}

      <textarea
        className="field-input min-h-[50vh] flex-1 resize-y font-mono text-sm leading-6"
        value={content}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
            e.preventDefault();
            void handleSave();
          }
        }}
        disabled={!file}
        spellCheck={false}
        placeholder="Write markdown content here…"
      />

      <Dialog
        open={confirmingDelete}
        onOpenChange={(open) => {
          if (!open) setConfirmingDelete(false);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete memory file</DialogTitle>
            <DialogDescription>Delete {path}? This cannot be undone.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button
              onClick={() => setConfirmingDelete(false)}
              className="secondary-action min-h-9 px-3 py-1.5"
            >
              Cancel
            </button>
            <button
              onClick={handleDelete}
              className="inline-flex min-h-9 items-center justify-center rounded-xl bg-red-600 px-3 py-1.5 text-sm font-semibold text-white transition hover:bg-red-500"
            >
              Delete
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
