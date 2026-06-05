'use client';

import { useEffect, useState } from 'react';
import { Plug, Server } from 'lucide-react';
import { useRouter } from 'next/navigation';
import useSWR from 'swr';
import ConfigForm from '@/components/admin/config-form';
import PluginManager from '@/components/admin/plugin-manager';
import Sidebar from '@/components/sidebar';
import { useAuth } from '@/context/auth';
import { getAdminConfig } from '@/lib/api';

type Tab = 'configuration' | 'plugins';

export default function AdminPage() {
  const { accessToken, currentUser, isLoading } = useAuth();
  const router = useRouter();
  const [tab, setTab] = useState<Tab>('configuration');

  const {
    data: config,
    error,
    mutate,
  } = useSWR(
    accessToken && currentUser?.role === 'admin' ? ['/api/admin/config', accessToken] : null,
    ([, token]) => getAdminConfig(token),
  );

  useEffect(() => {
    if (isLoading) return;
    if (!accessToken) router.replace('/login/');
    else if (currentUser?.role !== 'admin') router.replace('/chat/');
  }, [accessToken, currentUser, isLoading, router]);

  if (isLoading || !accessToken || currentUser?.role !== 'admin') {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-50 text-sm text-slate-400 dark:bg-slate-950 dark:text-slate-500">
        Loading…
      </div>
    );
  }

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={null}
        onSelect={id => router.push(id ? `/chat/?id=${id}` : '/chat/')}
      />
      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl px-6 py-8">
          <div className="mb-7">
            <p className="text-sm font-medium text-blue-600 dark:text-blue-400">Administration</p>
            <h1 className="mt-1 text-2xl font-semibold text-slate-900 dark:text-white">
              Server settings
            </h1>
            <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">
              Configure this helpcore server and inspect its plugin catalog.
            </p>
          </div>

          <div className="mb-6 flex gap-1 rounded-xl bg-slate-200/70 p-1 dark:bg-slate-900">
            <TabButton
              active={tab === 'configuration'}
              icon={<Server size={16} />}
              label="Configuration"
              onClick={() => setTab('configuration')}
            />
            <TabButton
              active={tab === 'plugins'}
              icon={<Plug size={16} />}
              label="Plugins"
              onClick={() => setTab('plugins')}
            />
          </div>

          {tab === 'configuration' && (
            <>
              {error && (
                <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
                  {error.message}
                </div>
              )}
              {!error && !config && (
                <div className="py-16 text-center text-sm text-slate-400">Loading configuration…</div>
              )}
              {config && (
                <ConfigForm
                  accessToken={accessToken}
                  config={config}
                  onSaved={updated => mutate(updated, false)}
                />
              )}
            </>
          )}

          {tab === 'plugins' && <PluginManager accessToken={accessToken} />}
        </div>
      </main>
    </div>
  );
}

function TabButton({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex flex-1 items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
        active
          ? 'bg-white text-slate-900 shadow-sm dark:bg-slate-800 dark:text-white'
          : 'text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white'
      }`}
    >
      {icon}
      {label}
    </button>
  );
}
