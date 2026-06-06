"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import { Copy, Trash2 } from "lucide-react";
import Sidebar from "@/app/components/sidebar";
import SettingsNav from "@/app/components/settings-nav";
import { useAuth } from "@/context/auth";
import { createApiKey, listApiKeys, revokeApiKey } from "@/lib/api";
import type { ApiKeyInfo, CreateApiKeyResponse } from "@/lib/types";

export default function ApiKeysPage() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<CreateApiKeyResponse | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { data } = useSWR(
    accessToken ? ["/api/auth/api-keys", accessToken] : null,
    ([, t]) => listApiKeys(t),
    { revalidateOnFocus: false },
  );
  const keys: ApiKeyInfo[] = data?.keys ?? [];

  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center text-sm text-slate-400">
        Loading…
      </div>
    );
  }
  if (!accessToken) {
    router.replace("/login/");
    return null;
  }

  async function handleCreate() {
    const name = newName.trim();
    if (!name || !accessToken) return;
    setCreating(true);
    setError(null);
    try {
      const result = await createApiKey({ name }, accessToken);
      setCreated(result);
      setNewName("");
      await mutate(["/api/auth/api-keys", accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Creation failed");
    } finally {
      setCreating(false);
    }
  }

  async function handleRevoke(id: string) {
    if (!accessToken) return;
    if (!confirm("Revoke this API key? Any apps using it will stop working.")) return;
    try {
      await revokeApiKey(id, accessToken);
      await mutate(["/api/auth/api-keys", accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Revoke failed");
    }
  }

  async function handleCopy() {
    if (!created) return;
    await navigator.clipboard.writeText(created.key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={null}
        onSelect={(id) => router.push(id ? `/chat/?id=${id}` : "/chat/")}
      />
      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-2xl px-6 py-8">
          <div className="mb-6">
            <p className="text-sm font-medium text-slate-500 dark:text-slate-400">Settings</p>
            <h1 className="mt-1 text-2xl font-semibold text-slate-900 dark:text-white">API keys</h1>
            <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">
              API keys let CLI tools and scripts authenticate without a password.
            </p>
          </div>
          <SettingsNav />
          <div className="mt-6">
            {/* Create new key */}
            <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 p-5 mb-6">
              <h2 className="text-sm font-semibold text-slate-900 dark:text-white mb-3">
                Create new key
              </h2>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreate();
                  }}
                  placeholder="Key name (e.g. laptop CLI)"
                  className="flex-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-3 py-2 text-sm text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
                <button
                  onClick={handleCreate}
                  disabled={creating || !newName.trim()}
                  className="rounded-lg px-4 py-2 text-sm font-medium bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
                >
                  {creating ? "Creating…" : "Create"}
                </button>
              </div>
            </div>

            {/* One-time reveal after creation */}
            {created && (
              <div className="rounded-xl border border-green-200 dark:border-green-800 bg-green-50 dark:bg-green-950/40 p-5 mb-6">
                <p className="text-sm font-semibold text-green-800 dark:text-green-300 mb-2">
                  Key created — copy it now. It won't be shown again.
                </p>
                <div className="flex items-center gap-2">
                  <code className="flex-1 rounded-lg bg-white dark:bg-slate-900 border border-green-200 dark:border-green-800 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 break-all select-all">
                    {created.key}
                  </code>
                  <button
                    onClick={handleCopy}
                    className="shrink-0 rounded-lg p-2 text-green-700 dark:text-green-400 hover:bg-green-100 dark:hover:bg-green-900/30 transition-colors"
                    title="Copy"
                  >
                    <Copy size={15} />
                  </button>
                </div>
                {copied && (
                  <p className="mt-2 text-xs text-green-600 dark:text-green-400">Copied!</p>
                )}
                <button
                  onClick={() => setCreated(null)}
                  className="mt-3 text-xs text-green-700 dark:text-green-400 underline"
                >
                  Dismiss
                </button>
              </div>
            )}

            {error && (
              <div className="mb-4 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
                {error}
              </div>
            )}

            {/* Key list */}
            <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 divide-y divide-slate-100 dark:divide-slate-800">
              {keys.length === 0 ? (
                <div className="px-5 py-8 text-center text-sm text-slate-400">No API keys yet.</div>
              ) : (
                keys.map((key) => (
                  <div key={key.id} className="flex items-center justify-between px-5 py-4 gap-4">
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
                        {key.name}
                      </p>
                      <p className="text-xs text-slate-400 mt-0.5">
                        <code>{key.key_prefix}…</code>
                        {" · "}
                        Created {new Date(key.created_at).toLocaleDateString()}
                        {key.last_used_at && (
                          <> · Last used {new Date(key.last_used_at).toLocaleDateString()}</>
                        )}
                        {key.expires_at && (
                          <> · Expires {new Date(key.expires_at).toLocaleDateString()}</>
                        )}
                      </p>
                    </div>
                    <button
                      onClick={() => handleRevoke(key.id)}
                      className="shrink-0 rounded-lg p-1.5 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/30 transition-colors"
                      title="Revoke"
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
