"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import useSWR from "swr";
import AppShell from "@/app/components/app-shell";
import PageHeader from "@/app/components/page-header";
import SettingsNav from "@/app/components/settings-nav";
import { useAuth } from "@/context/auth";
import { listMemory } from "@/lib/api";
import type { MemoryEntry } from "@/lib/types";
import MemoryLeaningCard from "./leaning-card";
import MemoryFileList from "./file-list";
import MemoryEditor from "./editor";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/app/components/ui/dialog";

export default function MemoryPage() {
  const { accessToken } = useAuth();
  const router = useRouter();
  const [selected, setSelected] = useState<string | null>(null);
  const [editorDirty, setEditorDirty] = useState(false);
  // File the user tried to switch to while the editor had unsaved changes.
  const [discardTarget, setDiscardTarget] = useState<string | null>(null);

  const { data: files = [] } = useSWR<MemoryEntry[]>(
    accessToken ? ["/api/memory", accessToken] : null,
    ([, t]) => listMemory(t as string).then((r) => r.files),
    { refreshInterval: 30_000 },
  );

  // No auth guard needed here — AuthGate (in the root layout) only renders
  // this page once a session is confirmed.

  function handleSelect(path: string) {
    if (path === selected) return;
    if (editorDirty) {
      setDiscardTarget(path);
      return;
    }
    setSelected(path);
  }

  function confirmDiscard() {
    if (discardTarget) {
      setSelected(discardTarget);
      setEditorDirty(false);
    }
    setDiscardTarget(null);
  }

  const selectedEntry = files.find((f) => f.path === selected);

  return (
    <AppShell
      conversationId={null}
      onSelectConversation={(id) => router.push(id ? `/chat/?id=${id}` : "/chat/")}
      mainClassName="overflow-y-auto"
    >
      <div className="mx-auto max-w-5xl px-4 py-7 sm:px-6 sm:py-10">
        <PageHeader
          breadcrumb="Settings"
          title="Memory"
          description="Edit memory files the assistant can reference."
        />

        <SettingsNav />

        <div className="mt-6 space-y-4">
          <MemoryLeaningCard />

          <div className="surface-card flex flex-col overflow-hidden p-0 md:flex-row">
            <MemoryFileList
              files={files}
              selected={selected}
              dirty={editorDirty}
              onSelect={handleSelect}
            />
            <div className="min-w-0 flex-1">
              {selected ? (
                <MemoryEditor
                  key={selected}
                  path={selected}
                  updatedAt={selectedEntry?.updated_at}
                  onDirtyChange={setEditorDirty}
                  onDeleted={() => {
                    setSelected(null);
                    setEditorDirty(false);
                  }}
                />
              ) : (
                <div className="flex min-h-[30vh] items-center justify-center p-6 text-sm text-slate-400 md:min-h-[50vh]">
                  Select a file or create a new one.
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      <Dialog
        open={discardTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDiscardTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Discard unsaved changes?</DialogTitle>
            <DialogDescription>
              {selected} has unsaved changes. Switching files will discard them.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button
              onClick={() => setDiscardTarget(null)}
              className="secondary-action min-h-9 px-3 py-1.5"
            >
              Keep editing
            </button>
            <button
              onClick={confirmDiscard}
              className="inline-flex min-h-9 items-center justify-center rounded-xl bg-red-600 px-3 py-1.5 text-sm font-semibold text-white transition hover:bg-red-500"
            >
              Discard changes
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </AppShell>
  );
}
