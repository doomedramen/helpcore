'use client';

import { ExternalLink, Package, RefreshCw } from 'lucide-react';
import useSWR from 'swr';
import { listPlugins, listPluginStore, setPluginEnabled } from '@/lib/api';

export default function PluginManager({ accessToken }: { accessToken: string }) {
  const {
    data: installed = [],
    error: installedError,
    mutate: refreshInstalled,
  } = useSWR(
    ['/api/plugins', accessToken],
    ([, token]) => listPlugins(token),
  );
  const {
    data: store,
    error: storeError,
    isLoading: storeLoading,
    mutate: refreshStore,
  } = useSWR(
    ['/api/plugins/store', accessToken],
    ([, token]) => listPluginStore(token),
  );

  async function togglePlugin(id: string, enabled: boolean) {
    await setPluginEnabled(id, enabled, accessToken);
    await Promise.all([refreshInstalled(), refreshStore()]);
  }

  return (
    <div className="space-y-6">
      <section className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div className="mb-5">
          <h2 className="text-base font-semibold text-slate-900 dark:text-white">Installed</h2>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            Plugins currently registered for your admin account.
          </p>
        </div>

        {installedError && <ErrorMessage message={installedError.message} />}
        {!installedError && installed.length === 0 && (
          <EmptyState message="No plugins are installed." />
        )}
        <div className="grid gap-3 xl:grid-cols-2">
          {installed.map(plugin => (
            <article
              key={plugin.id}
              className="rounded-xl border border-slate-200 p-4 dark:border-slate-800"
            >
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <Package size={16} className="shrink-0 text-slate-400" />
                    <h3 className="truncate font-medium text-slate-900 dark:text-slate-100">
                      {plugin.name}
                    </h3>
                  </div>
                  <p className="mt-2 text-sm leading-relaxed text-slate-500 dark:text-slate-400">
                    {plugin.description || 'No description provided.'}
                  </p>
                </div>
                <label className="relative inline-flex shrink-0 cursor-pointer items-center">
                  <input
                    type="checkbox"
                    checked={plugin.enabled}
                    onChange={event => togglePlugin(plugin.id, event.target.checked)}
                    className="peer sr-only"
                    aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
                  />
                  <span className="h-6 w-11 rounded-full bg-slate-200 transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-white after:shadow after:transition-transform peer-checked:bg-blue-600 peer-checked:after:translate-x-5 dark:bg-slate-700" />
                </label>
              </div>
              <PluginMeta
                tier={plugin.tier}
                version={plugin.version}
                permissions={plugin.permissions}
              />
            </article>
          ))}
        </div>
      </section>

      <section className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-white">Plugin store</h2>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              {store?.registry_url ?? 'Loading the configured registry…'}
            </p>
          </div>
          <button
            type="button"
            onClick={() => refreshStore()}
            className="rounded-lg border border-slate-300 p-2 text-slate-500 transition-colors hover:bg-slate-50 hover:text-slate-900 dark:border-slate-700 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white"
            aria-label="Refresh plugin store"
          >
            <RefreshCw size={16} className={storeLoading ? 'animate-spin' : ''} />
          </button>
        </div>

        {storeError && (
          <ErrorMessage message={`Could not load the plugin registry: ${storeError.message}`} />
        )}
        {!storeError && storeLoading && <EmptyState message="Loading plugin catalog…" />}
        {!storeError && !storeLoading && store?.plugins.length === 0 && (
          <EmptyState message="The configured registry does not contain any plugins yet." />
        )}
        <div className="grid gap-3 xl:grid-cols-2">
          {store?.plugins.map(plugin => (
            <article
              key={plugin.id}
              className="rounded-xl border border-slate-200 p-4 dark:border-slate-800"
            >
              <div className="flex items-start justify-between gap-4">
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="font-medium text-slate-900 dark:text-slate-100">{plugin.name}</h3>
                    {plugin.installed && <Badge label={plugin.enabled ? 'Enabled' : 'Installed'} />}
                    {plugin.blocked && <Badge label="Blocked" danger />}
                  </div>
                  <p className="mt-1 text-xs text-slate-400 dark:text-slate-500">
                    by {plugin.author}
                  </p>
                </div>
                <a
                  href={plugin.setup_guide ?? plugin.homepage}
                  target="_blank"
                  rel="noreferrer"
                  className="rounded-lg p-2 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800 dark:hover:text-white"
                  aria-label={`Open ${plugin.name} documentation`}
                >
                  <ExternalLink size={15} />
                </a>
              </div>
              <p className="mt-3 text-sm leading-relaxed text-slate-500 dark:text-slate-400">
                {plugin.description}
              </p>
              <PluginMeta
                tier={plugin.tier}
                version={plugin.version}
                permissions={plugin.permissions}
              />
            </article>
          ))}
        </div>
      </section>

      <p className="px-1 text-xs leading-relaxed text-slate-400 dark:text-slate-500">
        Store installation is not exposed yet because the core does not currently download,
        verify, and activate registry plugins. This page reports the real registry and installed
        state without marking unavailable execution paths as installed.
      </p>
    </div>
  );
}

function PluginMeta({
  tier,
  version,
  permissions,
}: {
  tier: string;
  version: string;
  permissions: string[];
}) {
  return (
    <div className="mt-4 flex flex-wrap gap-2 text-xs text-slate-500 dark:text-slate-400">
      <span className="rounded-md bg-slate-100 px-2 py-1 dark:bg-slate-800">{tier}</span>
      <span className="rounded-md bg-slate-100 px-2 py-1 dark:bg-slate-800">v{version}</span>
      {permissions.map(permission => (
        <span key={permission} className="rounded-md bg-slate-100 px-2 py-1 dark:bg-slate-800">
          {permission.replaceAll('_', ' ')}
        </span>
      ))}
    </div>
  );
}

function Badge({ label, danger = false }: { label: string; danger?: boolean }) {
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${
      danger
        ? 'bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300'
        : 'bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300'
    }`}>
      {label}
    </span>
  );
}

function EmptyState({ message }: { message: string }) {
  return (
    <div className="rounded-xl border border-dashed border-slate-300 px-4 py-8 text-center text-sm text-slate-500 dark:border-slate-700 dark:text-slate-400">
      {message}
    </div>
  );
}

function ErrorMessage({ message }: { message: string }) {
  return (
    <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
      {message}
    </div>
  );
}
