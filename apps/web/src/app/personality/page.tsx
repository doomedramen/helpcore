"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import { toast } from "sonner";
import AppShell from "@/app/components/app-shell";
import PageHeader from "@/app/components/page-header";
import SettingsNav from "@/app/components/settings-nav";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { getPersonality, putPersonality } from "@/lib/api";

const TABS = [
  {
    key: "soul",
    label: "Soul",
    hint: "How the assistant behaves — tone, values, communication style.",
  },
  { key: "identity", label: "Identity", hint: "Background and persona of the assistant." },
  {
    key: "user",
    label: "About me",
    hint: "Facts about you that the assistant should always know.",
  },
] as const;

type TabKey = (typeof TABS)[number]["key"];

export default function PersonalityPage() {
  const { accessToken } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const [activeTab, setActiveTab] = useState<TabKey>("soul");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const key = (tab: TabKey) => (accessToken ? [`/api/personality/${tab}`, accessToken] : null);

  const { data: soul } = useSWR(key("soul"), ([, t]) => getPersonality("soul", t as string));
  const { data: identity } = useSWR(key("identity"), ([, t]) =>
    getPersonality("identity", t as string),
  );
  const { data: user } = useSWR(key("user"), ([, t]) => getPersonality("user", t as string));

  const dataMap = { soul, identity, user };
  const [draft, setDraft] = useState<Record<TabKey, string>>({ soul: "", identity: "", user: "" });
  const [edited, setEdited] = useState<Record<TabKey, boolean>>({
    soul: false,
    identity: false,
    user: false,
  });

  // No auth guard needed here — AuthGate (in the root layout) only renders
  // this page once a session is confirmed.

  function currentContent(tab: TabKey): string {
    if (edited[tab]) return draft[tab];
    return dataMap[tab]?.content ?? "";
  }

  const showUpdatedAt = (tab: TabKey) => {
    if (edited[tab]) return null;
    const file = dataMap[tab];
    if (!file?.updated_at) return null;
    const date = new Date(file.updated_at);
    return (
      <span className="text-xs text-slate-400 dark:text-slate-500">
        Last updated {date.toLocaleString()}
      </span>
    );
  };

  function handleChange(tab: TabKey, value: string) {
    setDraft((d) => ({ ...d, [tab]: value }));
    setEdited((e) => ({ ...e, [tab]: true }));
    setSaveError(null);
  }

  async function handleSave(tab: TabKey) {
    if (!accessToken) return;
    setSaving(true);
    setSaveError(null);
    try {
      await putPersonality(tab, currentContent(tab), accessToken);
      await mutate(key(tab));
      setEdited((e) => ({ ...e, [tab]: false }));
      toast.success("Saved");
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
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
          title="Personality"
          description="Edit the three personality files that shape how the assistant talks and understands you."
        />

        <SettingsNav />

        <div className="mt-6">
          {/* Inner tabs */}
          <div className="flex gap-1 rounded-xl bg-slate-200/55 p-1 dark:bg-slate-900/70">
            {TABS.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setActiveTab(tab.key)}
                className={`flex flex-1 items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium transition sm:flex-none ${
                  activeTab === tab.key
                    ? "bg-white text-slate-950 shadow-sm dark:bg-slate-800 dark:text-white"
                    : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                }`}
              >
                {tab.label}
                {edited[tab.key] && (
                  <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-400" />
                )}
              </button>
            ))}
          </div>

          {TABS.map(
            (tab) =>
              activeTab === tab.key && (
                <div key={tab.key} className="surface-card mt-4 p-4 sm:p-5">
                  <p className="mb-3 text-sm text-slate-500 dark:text-slate-400">
                    {tab.hint}
                    <br />
                    {showUpdatedAt(tab.key)}
                  </p>

                  {saveError && (
                    <div className="mb-4">
                      <StatusMessage type="error" message={saveError} />
                    </div>
                  )}

                  <textarea
                    className="field-input min-h-[45vh] resize-y font-mono leading-6"
                    value={currentContent(tab.key)}
                    onChange={(e) => handleChange(tab.key, e.target.value)}
                    placeholder={`Write ${tab.label.toLowerCase()} content here…`}
                    spellCheck={false}
                  />

                  <div className="mt-3 flex justify-end">
                    <button
                      onClick={() => handleSave(tab.key)}
                      disabled={saving || !edited[tab.key]}
                      className="primary-action"
                    >
                      {saving ? "Saving…" : "Save"}
                    </button>
                  </div>
                </div>
              ),
          )}
        </div>
      </div>
    </AppShell>
  );
}
