'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR, { useSWRConfig } from 'swr';
import { FilePlus, Trash2 } from 'lucide-react';
import Sidebar from '@/components/sidebar';
import { useAuth } from '@/context/auth';
import { deleteMemoryFile, getMemoryFile, listMemory, putMemoryFile } from '@/lib/api';
import type { MemoryEntry } from '@/lib/types';

export default function MemoryPage() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const [selected, setSelected] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [creating, setCreating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [draft, setDraft] = useState('');
  const [isDirty, setIsDirty] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  const { data: fileList } = useSWR<MemoryEntry[]>(
    accessToken ? ['/api/memory', accessToken] : null,
    ([, t]) => listMemory(t as string).then(r => r.files),
    { refreshInterval: 30_000 },
  );

  const { data: fileContent, mutate: mutateFile } = useSWR(
    accessToken && selected ? [`/api/memory/${selected}`, accessToken] : null,
    ([, t]) => getMemoryFile(selected!, t as string),
  );

  if (isLoading) {
    return <div className="flex h-screen items-center justify-center text-sm text-slate-400">Loading…</div>;
  }
  if (!accessToken) {
    router.replace('/login/');
    return null;
  }

  function handleSelect(path: string) {
    setSelected(path);
    setIsDirty(false);
    setDraft('');
    setError(null);
    setSavedAt(null);
  }

  function currentContent(): string {
    if (isDirty) return draft;
    return fileContent?.content ?? '';
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
      setError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!accessToken || !selected) return;
    if (!confirm(`Delete ${selected}? This cannot be undone.`)) return;
    setDeleting(true);
    try {
      await deleteMemoryFile(selected, accessToken);
      setSelected(null);
      setIsDirty(false);
      setDraft('');
      await mutate(['/api/memory', accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Delete failed');
    } finally {
      setDeleting(false);
    }
  }

  async function handleCreate() {
    const name = newName.trim().replace(/\s+/g, '-').replace(/[^a-zA-Z0-9_.-]/g, '');
    if (!name || !accessToken) return;
    const path = name.endsWith('.md') ? name : `${name}.md`;
    setCreating(true);
    try {
      await putMemoryFile(path, '', accessToken);
      await mutate(['/api/memory', accessToken]);
      setNewName('');
      handleSelect(path);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create failed');
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={null}
        onSelect={id => router.push(id ? `/chat/?id=${id}` : '/chat/')}
      />
      <main className="flex-1 flex overflow-hidden">
        {/* File list */}
        <aside className="w-56 shrink-0 border-r border-slate-200 dark:border-slate-800 flex flex-col bg-white dark:bg-slate-900">
          <div className="px-4 pt-5 pb-3">
            <p className="text-xs font-semibold text-slate-400 uppercase tracking-wider">Memory files</p>
          </div>

          <nav className="flex-1 overflow-y-auto px-2 pb-2 space-y-0.5">
            {(fileList ?? []).map(entry => (
              <button
                key={entry.path}
                onClick={() => handleSelect(entry.path)}
                className={`flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                  selected === entry.path
                    ? 'bg-violet-50 dark:bg-violet-950/50 text-violet-700 dark:text-violet-300'
                    : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'
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
                onChange={e => setNewName(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') handleCreate(); }}
                placeholder="new-file.md"
                className="flex-1 min-w-0 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-1.5 text-xs text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-violet-500"
              />
              <button
                onClick={handleCreate}
                disabled={creating || !newName.trim()}
                title="Create file"
                className="rounded-lg p-1.5 bg-violet-600 text-white hover:bg-violet-500 disabled:opacity-50 transition-colors"
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
              <div className="flex items-center justify-between px-6 pt-5 pb-3 border-b border-slate-200 dark:border-slate-800">
                <div>
                  <h1 className="text-base font-semibold text-slate-900 dark:text-white">{selected}</h1>
                  {savedAt && !error && (
                    <p className="text-xs text-slate-400 mt-0.5">Saved at {savedAt}</p>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={handleDelete}
                    disabled={deleting}
                    className="rounded-lg p-1.5 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/30 transition-colors"
                    title="Delete file"
                  >
                    <Trash2 size={15} />
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={saving || !isDirty}
                    className="rounded-lg px-3 py-1.5 text-sm font-medium bg-violet-600 text-white hover:bg-violet-500 disabled:opacity-50 transition-colors"
                  >
                    {saving ? 'Saving…' : 'Save'}
                  </button>
                </div>
              </div>

              {error && (
                <div className="mx-6 mt-4 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
                  {error}
                </div>
              )}

              <textarea
                className="flex-1 p-6 font-mono text-sm text-slate-900 dark:text-slate-100 bg-white dark:bg-slate-900 resize-none focus:outline-none"
                value={currentContent()}
                onChange={e => handleChange(e.target.value)}
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
      </main>
    </div>
  );
}
