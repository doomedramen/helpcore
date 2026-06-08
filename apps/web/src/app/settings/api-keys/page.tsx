"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import { toast } from "sonner";
import { Copy, Trash2 } from "lucide-react";
import AppShell from "@/app/components/app-shell";
import PageHeader from "@/app/components/page-header";
import SettingsNav from "@/app/components/settings-nav";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { createApiKey, listApiKeys, revokeApiKey } from "@/lib/api";
import type { ApiKeyInfo, CreateApiKeyResponse } from "@/lib/types";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/app/components/ui/dialog";

export default function ApiKeysPage() {
  const { accessToken } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<CreateApiKeyResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyInfo | null>(null);

  const { data } = useSWR(
    accessToken ? ["/api/auth/api-keys", accessToken] : null,
    ([, t]) => listApiKeys(t),
    { revalidateOnFocus: false },
  );
  const keys: ApiKeyInfo[] = data?.keys ?? [];

  // No auth guard needed here — AuthGate (in the root layout) only renders
  // this page once a session is confirmed.

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

  async function confirmRevoke() {
    if (!accessToken || !revokeTarget) return;
    const id = revokeTarget.id;
    setRevokeTarget(null);
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
    toast.success("Copied to clipboard");
  }

  return (
    <AppShell
      conversationId={null}
      onSelectConversation={(id) => router.push(id ? `/chat/?id=${id}` : "/chat/")}
      mainClassName="overflow-y-auto"
    >
      <div className="mx-auto max-w-5xl px-4 py-7 sm:px-6 sm:py-10">
        <PageHeader
          breadcrumb="Settings"
          title="API keys"
          description="API keys let CLI tools and scripts authenticate without a password."
        />
        <SettingsNav />
        <div className="mt-6">
          {/* Create new key */}
          <div className="surface-card mb-6 p-4 sm:p-5">
            <h2 className="text-sm font-semibold text-slate-900 dark:text-white mb-3">
              Create new key
            </h2>
            <div className="flex flex-col gap-2 sm:flex-row">
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate();
                }}
                placeholder="Key name (e.g. laptop CLI)"
                className="field-input flex-1"
              />
              <button
                onClick={handleCreate}
                disabled={creating || !newName.trim()}
                className="primary-action"
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
              <button
                onClick={() => setCreated(null)}
                className="mt-3 text-xs text-green-700 dark:text-green-400 underline"
              >
                Dismiss
              </button>
            </div>
          )}

          {error && (
            <div className="mb-4">
              <StatusMessage type="error" message={error} />
            </div>
          )}

          {/* Key list */}
          <div className="surface-card divide-y divide-slate-100 overflow-hidden dark:divide-slate-800">
            {keys.length === 0 ? (
              <div className="px-5 py-8 text-center text-sm text-slate-400">No API keys yet.</div>
            ) : (
              keys.map((key) => (
                <div
                  key={key.id}
                  className="flex items-start justify-between gap-4 px-4 py-4 sm:items-center sm:px-5"
                >
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
                      {key.name}
                    </p>
                    <p className="mt-1 text-xs leading-5 text-slate-400">
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
                    onClick={() => setRevokeTarget(key)}
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

      <Dialog
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Revoke API key</DialogTitle>
            <DialogDescription>
              Revoke {revokeTarget?.name}? Any apps using it will stop working.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button
              onClick={() => setRevokeTarget(null)}
              className="secondary-action min-h-9 px-3 py-1.5"
            >
              Cancel
            </button>
            <button
              onClick={confirmRevoke}
              className="inline-flex min-h-9 items-center justify-center rounded-xl bg-red-600 px-3 py-1.5 text-sm font-semibold text-white transition hover:bg-red-500"
            >
              Revoke
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </AppShell>
  );
}
