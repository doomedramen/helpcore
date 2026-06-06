'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import PluginManager from '@/components/plugin-manager';
import Sidebar from '@/components/sidebar';
import { useAuth } from '@/context/auth';

export default function PluginsPage() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (!isLoading && !accessToken) router.replace('/login/');
  }, [accessToken, isLoading, router]);

  if (isLoading || !accessToken) {
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
            <p className="text-sm font-medium text-blue-600 dark:text-blue-400">Your account</p>
            <h1 className="mt-1 text-2xl font-semibold text-slate-900 dark:text-white">Plugins</h1>
            <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">
              Install and manage isolated plugin versions for your account.
            </p>
          </div>
          <PluginManager accessToken={accessToken} />
        </div>
      </main>
    </div>
  );
}
