"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import useSWR, { useSWRConfig } from "swr";
import AppShell from "@/app/components/app-shell";
import PageHeader from "@/app/components/page-header";
import SettingsNav from "@/app/components/settings-nav";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { getPersonality, putPersonality, updateMe } from "@/lib/api";

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
  const { accessToken, isLoading, currentUser } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const [activeTab, setActiveTab] = useState<TabKey>("soul");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedTab, setSavedTab] = useState<TabKey | null>(null);

  const [memoryLeaning, setMemoryLeaning] = useState(currentUser?.memory_leaning ?? "moderate");
  const [leaningSaving, setLeaningSaving] = useState(false);
  const [leaningSaved, setLeaningSaved] = useState(false);

  useEffect(() => {
    if (currentUser?.memory_leaning) {
      setMemoryLeaning(currentUser.memory_leaning);
    }
  }, [currentUser?.memory_leaning]);

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
    setSavedTab(null);
  }

  async function handleSave(tab: TabKey) {
    if (!accessToken) return;
    setSaving(true);
    setSaveError(null);
    try {
      await putPersonality(tab, currentContent(tab), accessToken);
      await mutate(key(tab));
      setEdited((e) => ({ ...e, [tab]: false }));
      setSavedTab(tab);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  async function handleLeaningChange(value: string) {
    setMemoryLeaning(value);
    if (!accessToken) return;
    setLeaningSaving(true);
    setLeaningSaved(false);
    try {
      await updateMe({ memory_leaning: value }, accessToken);
      setLeaningSaved(true);
      setTimeout(() => setLeaningSaved(false), 2000);
    } catch {
      // revert on failure
      setMemoryLeaning(currentUser?.memory_leaning ?? "moderate");
    } finally {
      setLeaningSaving(false);
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

        <div className="surface-card mt-6 p-4 sm:p-5">
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
          {leaningSaved && (
            <p className="mt-2 text-xs text-green-600 dark:text-green-400">Saved.</p>
          )}
        </div>

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
                  {savedTab === tab.key && !saveError && (
                    <div className="mb-4">
                      <StatusMessage type="success" message="Saved." />
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
