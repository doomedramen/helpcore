'use client';

import { useState } from 'react';
import { ExternalLink, Package, RefreshCw, Settings2, Trash2, Undo2 } from 'lucide-react';
import useSWR from 'swr';
import {
  configurePlugin,
  installPlugin,
  listPlugins,
  listPluginStore,
  rollbackPlugin,
  setPluginEnabled,
  uninstallPlugin,
  updatePlugin,
} from '@/lib/api';
import type { PluginInfo } from '@/lib/types';

export default function PluginManager({ accessToken }: { accessToken: string }) {
  const [working, setWorking] = useState<string | null>(null);
  const [actionError, setActionError] = useState('');
  const [configuring, setConfiguring] = useState<string | null>(null);
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

  async function act(key: string, operation: () => Promise<void>) {
    setWorking(key);
    setActionError('');
    try {
      await operation();
      await Promise.all([refreshInstalled(), refreshStore()]);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : 'Could not update the plugin.');
    } finally {
      setWorking(null);
    }
  }

  return (
    <div className="space-y-6">
      {(installedError || actionError) && (
        <ErrorMessage message={actionError || installedError.message} />
      )}

      <section className={sectionClass}>
        <div className="mb-5">
          <h2 className="text-base font-semibold text-slate-900 dark:text-white">Installed</h2>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            These plugins and versions belong only to your account.
          </p>
        </div>

        {!installedError && installed.length === 0 && (
          <EmptyState message="You have not installed any plugins." />
        )}
        <div className="grid gap-3 xl:grid-cols-2">
          {installed.map(plugin => (
            <article key={plugin.id} className={cardClass}>
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <Package size={16} className="shrink-0 text-slate-400" />
                    <h3 className="font-medium text-slate-900 dark:text-slate-100">
                      {plugin.name}
                    </h3>
                    {plugin.blocked && <Badge label="Blocked" danger />}
                    {plugin.update_available && <Badge label="Update available" />}
                  </div>
                  <p className="mt-2 text-sm leading-relaxed text-slate-500 dark:text-slate-400">
                    {plugin.description || 'No description provided.'}
                  </p>
                </div>
                <label className="relative inline-flex shrink-0 cursor-pointer items-center">
                  <input
                    type="checkbox"
                    checked={plugin.enabled}
                    disabled={working !== null || plugin.blocked || !plugin.configured || !plugin.user_managed}
                    onChange={event => act(
                      `enable:${plugin.id}`,
                      () => setPluginEnabled(plugin.id, event.target.checked, accessToken),
                    )}
                    className="peer sr-only"
                    aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
                  />
                  <span className="h-6 w-11 rounded-full bg-slate-200 transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-white after:shadow after:transition-transform peer-checked:bg-blue-600 peer-checked:after:translate-x-5 peer-disabled:cursor-not-allowed peer-disabled:opacity-50 dark:bg-slate-700" />
                </label>
              </div>

              <PluginMeta
                tier={plugin.tier}
                version={plugin.active_version}
                permissions={plugin.permissions}
              />

              {plugin.tier === 'bridge' && !plugin.configured && (
                <p className="mt-3 text-xs text-amber-700 dark:text-amber-300">
                  Configure and health-check the bridge before enabling it.
                </p>
              )}
              {!plugin.user_managed && (
                <p className="mt-3 text-xs text-slate-500 dark:text-slate-400">
                  This plugin is provisioned by the server administrator.
                </p>
              )}

              {plugin.user_managed && configuring === plugin.id ? (
                <BridgeConfigForm
                  plugin={plugin}
                  saving={working === `configure:${plugin.id}`}
                  onCancel={() => setConfiguring(null)}
                  onSave={(endpoint, settings, secrets) => act(
                    `configure:${plugin.id}`,
                    async () => {
                      await configurePlugin(
                        plugin.id,
                        { endpoint, settings, secrets },
                        accessToken,
                      );
                      setConfiguring(null);
                    },
                  )}
                />
              ) : plugin.user_managed ? (
                <div className="mt-4 flex flex-wrap gap-2 border-t border-slate-200 pt-3 dark:border-slate-800">
                  {plugin.tier === 'bridge' && (
                    <ActionButton
                      label="Configure"
                      icon={<Settings2 size={14} />}
                      onClick={() => setConfiguring(plugin.id)}
                      disabled={working !== null}
                    />
                  )}
                  {plugin.update_available && (
                    <ActionButton
                      label="Update"
                      onClick={() => act(
                        `update:${plugin.id}`,
                        () => updatePlugin(plugin.id, plugin.permissions, accessToken),
                      )}
                      disabled={working !== null || plugin.blocked}
                    />
                  )}
                  {plugin.previous_version && (
                    <ActionButton
                      label={`Roll back to ${plugin.previous_version}`}
                      icon={<Undo2 size={14} />}
                      onClick={() => act(
                        `rollback:${plugin.id}`,
                        () => rollbackPlugin(plugin.id, accessToken),
                      )}
                      disabled={working !== null || plugin.blocked}
                    />
                  )}
                  <ActionButton
                    label="Uninstall"
                    icon={<Trash2 size={14} />}
                    danger
                    onClick={() => {
                      if (window.confirm(`Uninstall ${plugin.name} and remove your saved versions?`)) {
                        void act(
                          `uninstall:${plugin.id}`,
                          () => uninstallPlugin(plugin.id, accessToken),
                        );
                      }
                    }}
                    disabled={working !== null}
                  />
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className={sectionClass}>
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
            <article key={plugin.id} className={cardClass}>
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
              <div className="mt-4 border-t border-slate-200 pt-3 dark:border-slate-800">
                {!plugin.installed && (
                  <div className="flex items-center gap-3">
                    <button
                      type="button"
                      disabled={working !== null || !plugin.installable}
                      onClick={() => act(
                        `install:${plugin.id}`,
                        () => installPlugin(plugin.id, plugin.permissions, accessToken),
                      )}
                      className={primaryButtonClass}
                    >
                      {plugin.blocked
                        ? 'Blocked by server'
                        : plugin.installable
                          ? working === `install:${plugin.id}` ? 'Installing…' : 'Install'
                          : plugin.tier === 'bridge'
                            ? 'Bridge service'
                            : 'Package unavailable'}
                    </button>
                    {!plugin.installable && plugin.tier === 'bridge' && (
                      <span className="text-xs text-slate-400 dark:text-slate-500">
                        Deployed separately by admin.
                        {plugin.setup_guide && (
                          <a href={plugin.setup_guide} target="_blank" rel="noreferrer" className="ml-1 text-blue-500 hover:underline">
                            Setup guide
                          </a>
                        )}
                      </span>
                    )}
                  </div>
                )}
                {plugin.installed && plugin.update_available && (
                  <button
                    type="button"
                    disabled={working !== null || plugin.blocked}
                    onClick={() => act(
                      `update:${plugin.id}`,
                      () => updatePlugin(plugin.id, plugin.permissions, accessToken),
                    )}
                    className={primaryButtonClass}
                  >
                    {working === `update:${plugin.id}` ? 'Updating…' : `Update to ${plugin.version}`}
                  </button>
                )}
                {plugin.installed && !plugin.update_available && (
                  <p className="text-xs text-slate-400 dark:text-slate-500">
                    Version {plugin.active_version} is installed for your account.
                  </p>
                )}
              </div>
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

function BridgeConfigForm({
  plugin,
  saving,
  onSave,
  onCancel,
}: {
  plugin: PluginInfo;
  saving: boolean;
  onSave: (endpoint: string, settings: unknown, secrets: unknown) => void;
  onCancel: () => void;
}) {
  const [endpoint, setEndpoint] = useState(plugin.endpoint ?? '');
  const [settings, setSettings] = useState('{}');
  const [secrets, setSecrets] = useState('{}');
  const [error, setError] = useState('');

  function submit() {
    try {
      onSave(endpoint, JSON.parse(settings), JSON.parse(secrets));
    } catch {
      setError('Settings and secrets must be valid JSON objects.');
    }
  }

  return (
    <div className="mt-4 space-y-3 border-t border-slate-200 pt-4 dark:border-slate-800">
      {error && <ErrorMessage message={error} />}
      <label className="block text-xs font-medium text-slate-600 dark:text-slate-300">
        Bridge endpoint
        <input
          type="url"
          value={endpoint}
          onChange={event => setEndpoint(event.target.value)}
          placeholder="http://home-assistant-bridge:8765"
          className={inputClass}
        />
      </label>
      <label className="block text-xs font-medium text-slate-600 dark:text-slate-300">
        Settings JSON
        <textarea value={settings} onChange={event => setSettings(event.target.value)} className={inputClass} />
      </label>
      <label className="block text-xs font-medium text-slate-600 dark:text-slate-300">
        Secrets JSON
        <textarea
          value={secrets}
          onChange={event => setSecrets(event.target.value)}
          className={inputClass}
          placeholder='{"token":"…"}'
        />
      </label>
      <p className="text-xs text-slate-400 dark:text-slate-500">
        Secrets are encrypted at rest and are never returned by the API.
      </p>
      <div className="flex gap-2">
        <button type="button" onClick={submit} disabled={saving || !endpoint} className={primaryButtonClass}>
          {saving ? 'Checking…' : 'Save and health-check'}
        </button>
        <ActionButton label="Cancel" onClick={onCancel} disabled={saving} />
      </div>
    </div>
  );
}

function ActionButton({
  label,
  icon,
  danger = false,
  disabled,
  onClick,
}: {
  label: string;
  icon?: React.ReactNode;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
        danger
          ? 'border-red-200 text-red-600 hover:bg-red-50 dark:border-red-900 dark:text-red-400 dark:hover:bg-red-950/50'
          : 'border-slate-300 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800'
      }`}
    >
      {icon}
      {label}
    </button>
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

const sectionClass = 'rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900';
const cardClass = 'rounded-xl border border-slate-200 p-4 dark:border-slate-800';
const primaryButtonClass = 'rounded-lg bg-blue-600 px-3 py-2 text-xs font-semibold text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50';
const inputClass = 'mt-1.5 block min-h-10 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100';
