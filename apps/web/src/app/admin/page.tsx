'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import useSWR from 'swr';
import ConfigForm from '@/app/components/admin/config-form';
import Sidebar from '@/app/components/sidebar';
import { useAuth } from '@/context/auth';
import { getAdminConfig } from '@/lib/api';

export default function AdminPage() {
  const { accessToken, currentUser, isLoading } = useAuth();
  const router = useRouter();
  const {
    data: config,
    error,
    mutate,
  } = useSWR(
    accessToken && currentUser?.role === 'admin' ? ['/api/admin/config', accessToken] : null,
    ([, token]) => getAdminConfig(token),
    {
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
    },
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
              Configure server-wide settings, providers, registry policy, and plugin blacklist.
            </p>
          </div>

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
        </div>
      </main>
    </div>
  );
}
