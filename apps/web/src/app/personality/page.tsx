'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR, { useSWRConfig } from 'swr';
import Sidebar from '@/app/components/sidebar';
import SettingsNav from '@/app/components/settings-nav';
import { useAuth } from '@/context/auth';
import { getPersonality, putPersonality } from '@/lib/api';

const TABS = [
  { key: 'soul', label: 'Soul', hint: 'How the assistant behaves — tone, values, communication style.' },
  { key: 'identity', label: 'Identity', hint: 'Background and persona of the assistant.' },
  { key: 'user', label: 'About me', hint: 'Facts about you that the assistant should always know.' },
] as const;

type TabKey = (typeof TABS)[number]['key'];

export default function PersonalityPage() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const [activeTab, setActiveTab] = useState<TabKey>('soul');
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedTab, setSavedTab] = useState<TabKey | null>(null);

  const key = (tab: TabKey) =>
    accessToken ? [`/api/personality/${tab}`, accessToken] : null;

  const { data: soul } = useSWR(key('soul'), ([, t]) => getPersonality('soul', t as string));
  const { data: identity } = useSWR(key('identity'), ([, t]) => getPersonality('identity', t as string));
  const { data: user } = useSWR(key('user'), ([, t]) => getPersonality('user', t as string));

  const dataMap = { soul, identity, user };
  const [draft, setDraft] = useState<Record<TabKey, string>>({ soul: '', identity: '', user: '' });
  const [edited, setEdited] = useState<Record<TabKey, boolean>>({ soul: false, identity: false, user: false });

  if (isLoading) {
    return <div className="flex h-screen items-center justify-center text-sm text-slate-400">Loading…</div>;
  }
  if (!accessToken) {
    router.replace('/login/');
    return null;
  }

  function currentContent(tab: TabKey): string {
    if (edited[tab]) return draft[tab];
    return dataMap[tab]?.content ?? '';
  }

  function handleChange(tab: TabKey, value: string) {
    setDraft(d => ({ ...d, [tab]: value }));
    setEdited(e => ({ ...e, [tab]: true }));
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
      setEdited(e => ({ ...e, [tab]: false }));
      setSavedTab(tab);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={null}
        onSelect={id => router.push(id ? `/chat/?id=${id}` : '/chat/')}
      />
      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-6 py-8">
          <div className="mb-6">
            <p className="text-sm font-medium text-slate-500 dark:text-slate-400">Settings</p>
            <h1 className="mt-1 text-2xl font-semibold text-slate-900 dark:text-white">Personality</h1>
            <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">
              Edit the three personality files that shape how the assistant talks and understands you.
            </p>
          </div>

          <SettingsNav />

          <div className="mt-6">
            {/* Inner tabs */}
            <div className="flex border-b border-slate-200 dark:border-slate-800">
              {TABS.map(tab => (
                <button
                  key={tab.key}
                  onClick={() => setActiveTab(tab.key)}
                  className={`flex items-center gap-1.5 border-b-2 px-4 pb-2.5 pt-1 text-sm font-medium transition-colors ${
                    activeTab === tab.key
                      ? 'border-blue-600 text-blue-600 dark:border-blue-400 dark:text-blue-400'
                      : 'border-transparent text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white'
                  }`}
                >
                  {tab.label}
                  {edited[tab.key] && (
                    <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-400" />
                  )}
                </button>
              ))}
            </div>

            {TABS.map(tab => (
              activeTab === tab.key && (
                <div key={tab.key} className="mt-5">
                  <p className="mb-3 text-sm text-slate-500 dark:text-slate-400">{tab.hint}</p>

                  {saveError && (
                    <div className="mb-4 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
                      {saveError}
                    </div>
                  )}
                  {savedTab === tab.key && !saveError && (
                    <div className="mb-4 rounded-xl border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-700 dark:border-green-900 dark:bg-green-950/50 dark:text-green-300">
                      Saved.
                    </div>
                  )}

                  <textarea
                    className="w-full h-80 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 px-4 py-3 text-sm font-mono text-slate-900 dark:text-slate-100 resize-y focus:outline-none focus:ring-2 focus:ring-blue-500"
                    value={currentContent(tab.key)}
                    onChange={e => handleChange(tab.key, e.target.value)}
                    placeholder={`Write ${tab.label.toLowerCase()} content here…`}
                    spellCheck={false}
                  />

                  <div className="mt-3 flex justify-end">
                    <button
                      onClick={() => handleSave(tab.key)}
                      disabled={saving || !edited[tab.key]}
                      className="rounded-lg px-4 py-2 text-sm font-medium bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      {saving ? 'Saving…' : 'Save'}
                    </button>
                  </div>
                </div>
              )
            ))}
          </div>
        </div>
      </main>
    </div>
  );
}
