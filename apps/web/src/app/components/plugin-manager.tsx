"use client";

import { useState } from "react";
import {
  ExternalLink,
  MoreHorizontal,
  RefreshCw,
  Search,
  Settings2,
  Trash2,
  Undo2,
} from "lucide-react";
import useSWR from "swr";
import {
  configurePlugin,
  installPlugin,
  listPlugins,
  listPluginStore,
  rollbackPlugin,
  setPluginEnabled,
  uninstallPlugin,
  updatePlugin,
} from "@/lib/api";
import type {
  ConfigField,
  ConfigValues,
  PluginInfo,
  PluginStoreItem,
  SecretStatus,
} from "@/lib/types";

type Tab = "installed" | "browse";

export default function PluginManager({ accessToken }: { accessToken: string }) {
  const [tab, setTab] = useState<Tab>("installed");
  const [working, setWorking] = useState<string | null>(null);
  const [actionError, setActionError] = useState("");
  const [configuring, setConfiguring] = useState<string | null>(null);
  const [storeSearch, setStoreSearch] = useState("");

  const {
    data: installed = [],
    error: installedError,
    mutate: refreshInstalled,
  } = useSWR(["/api/plugins", accessToken], ([, token]) =>
    listPlugins(token).then((r) => r.plugins),
  );

  const {
    data: store,
    error: storeError,
    isLoading: storeLoading,
    isValidating: storeValidating,
    mutate: refreshStore,
  } = useSWR(["/api/plugins/store", accessToken], ([, token]) => listPluginStore(token));

  async function act(key: string, operation: () => Promise<void>) {
    setWorking(key);
    setActionError("");
    try {
      await operation();
      await Promise.all([refreshInstalled(), refreshStore()]);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Could not update the plugin.");
    } finally {
      setWorking(null);
    }
  }

  const attentionCount = installed.filter(
    (p) => p.update_available || (!p.configured && p.config_schema.length > 0),
  ).length;

  const filteredStore = store?.plugins.filter((p) => {
    if (!storeSearch) return true;
    const q = storeSearch.toLowerCase();
    return p.name.toLowerCase().includes(q) || p.description.toLowerCase().includes(q);
  });

  return (
    <div className="space-y-4">
      <div className="flex border-b border-slate-200 dark:border-slate-800">
        <TabButton active={tab === "installed"} onClick={() => setTab("installed")}>
          Installed
          {installed.length > 0 && (
            <span
              className={`ml-2 rounded-full px-1.5 py-0.5 text-xs tabular-nums ${
                attentionCount > 0
                  ? "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
                  : "bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400"
              }`}
            >
              {attentionCount > 0 ? `${attentionCount}` : installed.length}
            </span>
          )}
        </TabButton>
        <TabButton active={tab === "browse"} onClick={() => setTab("browse")}>
          Browse
        </TabButton>
      </div>

      {(installedError || actionError) && (
        <ErrorMessage message={actionError || installedError.message} />
      )}

      {tab === "installed" &&
        (installed.length === 0 ? (
          <div className="rounded-xl border border-dashed border-slate-300 px-4 py-10 text-center dark:border-slate-700">
            <p className="text-sm text-slate-500 dark:text-slate-400">No plugins installed.</p>
            <button
              type="button"
              onClick={() => setTab("browse")}
              className="mt-3 text-sm font-medium text-blue-600 hover:underline dark:text-blue-400"
            >
              Browse the plugin store →
            </button>
          </div>
        ) : (
          <div className="overflow-hidden rounded-xl border border-slate-200 dark:border-slate-800">
            {installed.map((plugin, i) => (
              <div key={plugin.id}>
                {i > 0 && <div className="border-t border-slate-200 dark:border-slate-800" />}
                <InstalledRow
                  plugin={plugin}
                  configuring={configuring === plugin.id}
                  working={working}
                  onToggle={(checked) =>
                    act(`enable:${plugin.id}`, () =>
                      setPluginEnabled(plugin.id, checked, accessToken),
                    )
                  }
                  onConfigure={() => setConfiguring(configuring === plugin.id ? null : plugin.id)}
                  onUpdate={() =>
                    act(`update:${plugin.id}`, () =>
                      updatePlugin(plugin.id, plugin.permissions, accessToken),
                    )
                  }
                  onRollback={() =>
                    act(`rollback:${plugin.id}`, () => rollbackPlugin(plugin.id, accessToken))
                  }
                  onUninstall={() => {
                    if (
                      window.confirm(`Uninstall ${plugin.name} and remove your saved versions?`)
                    ) {
                      void act(`uninstall:${plugin.id}`, () =>
                        uninstallPlugin(plugin.id, accessToken),
                      );
                    }
                  }}
                  onSaveConfig={(values) =>
                    act(`configure:${plugin.id}`, async () => {
                      await configurePlugin(plugin.id, values, accessToken);
                      setConfiguring(null);
                    })
                  }
                  onCancelConfig={() => setConfiguring(null)}
                />
              </div>
            ))}
          </div>
        ))}

      {tab === "browse" && (
        <div className="space-y-3">
          <div className="flex gap-2">
            <div className="relative flex-1">
              <Search
                size={14}
                className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400"
              />
              <input
                type="search"
                value={storeSearch}
                onChange={(e) => setStoreSearch(e.target.value)}
                placeholder="Search plugins…"
                className="w-full rounded-lg border border-slate-300 bg-white py-2 pl-9 pr-3 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
              />
            </div>
            <button
              type="button"
              onClick={() => refreshStore()}
              className="rounded-lg border border-slate-300 p-2 text-slate-500 transition-colors hover:bg-slate-50 hover:text-slate-900 dark:border-slate-700 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white"
              aria-label="Refresh plugin store"
            >
              <RefreshCw size={15} className={storeValidating ? "animate-spin" : ""} />
            </button>
          </div>

          {storeError && (
            <ErrorMessage message={`Could not load the plugin registry: ${storeError.message}`} />
          )}
          {!storeError && storeLoading && (
            <p className="py-8 text-center text-sm text-slate-400 dark:text-slate-500">Loading…</p>
          )}
          {!storeError && !storeLoading && filteredStore?.length === 0 && (
            <p className="py-8 text-center text-sm text-slate-400 dark:text-slate-500">
              {storeSearch ? "No plugins match your search." : "No plugins available."}
            </p>
          )}

          {filteredStore && filteredStore.length > 0 && (
            <div className="overflow-hidden rounded-xl border border-slate-200 dark:border-slate-800">
              {filteredStore.map((plugin, i) => (
                <div key={plugin.id}>
                  {i > 0 && <div className="border-t border-slate-200 dark:border-slate-800" />}
                  <StoreRow
                    plugin={plugin}
                    working={working}
                    onInstall={() =>
                      act(`install:${plugin.id}`, () =>
                        installPlugin(plugin.id, plugin.permissions, accessToken),
                      )
                    }
                    onUpdate={() =>
                      act(`update:${plugin.id}`, () =>
                        updatePlugin(plugin.id, plugin.permissions, accessToken),
                      )
                    }
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function InstalledRow({
  plugin,
  configuring,
  working,
  onToggle,
  onConfigure,
  onUpdate,
  onRollback,
  onUninstall,
  onSaveConfig,
  onCancelConfig,
}: {
  plugin: PluginInfo;
  configuring: boolean;
  working: string | null;
  onToggle: (checked: boolean) => void;
  onConfigure: () => void;
  onUpdate: () => void;
  onRollback: () => void;
  onUninstall: () => void;
  onSaveConfig: (values: Record<string, unknown>) => void;
  onCancelConfig: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const needsConfig = !plugin.configured && plugin.config_schema.length > 0;

  return (
    <div className="bg-white dark:bg-slate-900">
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium text-slate-900 dark:text-slate-100">{plugin.name}</span>
            {plugin.blocked && <StatusBadge label="Blocked" variant="danger" />}
            {needsConfig && <StatusBadge label="Needs config" variant="warning" />}
            {plugin.update_available && <StatusBadge label="Update available" variant="info" />}
          </div>
          <p className="mt-0.5 text-xs text-slate-400 dark:text-slate-500">
            {plugin.tier} · v{plugin.active_version}
            {!plugin.user_managed && " · server managed"}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {plugin.user_managed && plugin.config_schema.length > 0 && (
            <button
              type="button"
              onClick={onConfigure}
              className={`rounded-lg p-1.5 transition-colors ${
                configuring
                  ? "bg-slate-100 text-slate-900 dark:bg-slate-800 dark:text-white"
                  : "text-slate-400 hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800 dark:hover:text-white"
              }`}
              title="Configure"
            >
              <Settings2 size={15} />
            </button>
          )}
          {plugin.update_available && plugin.user_managed && (
            <button
              type="button"
              onClick={onUpdate}
              disabled={working !== null || plugin.blocked}
              className="rounded-lg bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700 transition-colors hover:bg-blue-100 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-blue-950/50 dark:text-blue-300 dark:hover:bg-blue-900/50"
            >
              {working === `update:${plugin.id}` ? "Updating…" : "Update"}
            </button>
          )}
          {plugin.user_managed && (
            <div className="relative">
              <button
                type="button"
                onClick={() => setMenuOpen((v) => !v)}
                className="rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-900 dark:hover:bg-slate-800 dark:hover:text-white"
                title="More options"
              >
                <MoreHorizontal size={15} />
              </button>
              {menuOpen && (
                <>
                  <div className="fixed inset-0 z-10" onClick={() => setMenuOpen(false)} />
                  <div className="absolute right-0 top-full z-20 mt-1 min-w-[160px] overflow-hidden rounded-lg border border-slate-200 bg-white py-1 shadow-lg dark:border-slate-700 dark:bg-slate-900">
                    {plugin.previous_version && (
                      <button
                        type="button"
                        onClick={() => {
                          setMenuOpen(false);
                          onRollback();
                        }}
                        disabled={working !== null || plugin.blocked}
                        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50 dark:text-slate-300 dark:hover:bg-slate-800"
                      >
                        <Undo2 size={12} />
                        Roll back to v{plugin.previous_version}
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => {
                        setMenuOpen(false);
                        onUninstall();
                      }}
                      disabled={working !== null}
                      className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-red-600 hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50 dark:text-red-400 dark:hover:bg-red-950/30"
                    >
                      <Trash2 size={12} />
                      Uninstall
                    </button>
                  </div>
                </>
              )}
            </div>
          )}
          <label className="relative ml-1 inline-flex shrink-0 cursor-pointer items-center">
            <input
              type="checkbox"
              checked={plugin.enabled}
              disabled={
                working !== null || plugin.blocked || !plugin.configured || !plugin.user_managed
              }
              onChange={(e) => onToggle(e.target.checked)}
              className="peer sr-only"
              aria-label={`${plugin.enabled ? "Disable" : "Enable"} ${plugin.name}`}
            />
            <span className="h-5 w-9 rounded-full bg-slate-200 transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-4 after:w-4 after:rounded-full after:bg-white after:shadow after:transition-transform peer-checked:bg-blue-600 peer-checked:after:translate-x-4 peer-disabled:cursor-not-allowed peer-disabled:opacity-50 dark:bg-slate-700" />
          </label>
        </div>
      </div>

      {configuring && plugin.user_managed && (
        <div className="border-t border-slate-200 bg-slate-50 px-4 pb-4 pt-3 dark:border-slate-800 dark:bg-slate-950/50">
          <PluginConfigForm
            schema={plugin.config_schema}
            currentValues={plugin.config_values}
            saving={working === `configure:${plugin.id}`}
            onSave={onSaveConfig}
            onCancel={onCancelConfig}
          />
        </div>
      )}
    </div>
  );
}

function StoreRow({
  plugin,
  working,
  onInstall,
  onUpdate,
}: {
  plugin: PluginStoreItem;
  working: string | null;
  onInstall: () => void;
  onUpdate: () => void;
}) {
  return (
    <div className="flex items-center gap-4 bg-white px-4 py-3 dark:bg-slate-900">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span className="font-medium text-slate-900 dark:text-slate-100">{plugin.name}</span>
          <span className="text-xs text-slate-400 dark:text-slate-500">by {plugin.author}</span>
          {plugin.installed && !plugin.update_available && (
            <StatusBadge label={plugin.enabled ? "Enabled" : "Installed"} variant="neutral" />
          )}
          {plugin.blocked && <StatusBadge label="Blocked" variant="danger" />}
        </div>
        <p className="mt-0.5 line-clamp-1 text-xs text-slate-500 dark:text-slate-400">
          {plugin.description}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        {(plugin.setup_guide ?? plugin.homepage) && (
          <a
            href={plugin.setup_guide ?? plugin.homepage}
            target="_blank"
            rel="noreferrer"
            className="rounded-lg p-1.5 text-slate-400 transition-colors hover:text-slate-600 dark:hover:text-slate-200"
            aria-label={`${plugin.name} documentation`}
          >
            <ExternalLink size={14} />
          </a>
        )}
        {!plugin.installed && (
          <button
            type="button"
            disabled={working !== null || !plugin.installable}
            onClick={onInstall}
            className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {plugin.blocked
              ? "Blocked"
              : !plugin.installable
                ? plugin.tier === "bridge"
                  ? "Bridge service"
                  : "Unavailable"
                : working === `install:${plugin.id}`
                  ? "Installing…"
                  : "Install"}
          </button>
        )}
        {plugin.installed && plugin.update_available && (
          <button
            type="button"
            disabled={working !== null || plugin.blocked}
            onClick={onUpdate}
            className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {working === `update:${plugin.id}` ? "Updating…" : `Update to ${plugin.version}`}
          </button>
        )}
        {plugin.installed && !plugin.update_available && (
          <span className="text-xs text-slate-400 dark:text-slate-500">
            v{plugin.active_version}
          </span>
        )}
      </div>
    </div>
  );
}

function isSecretStatus(value: unknown): value is SecretStatus {
  return typeof value === "object" && value !== null && "configured" in value;
}

function PluginConfigForm({
  schema,
  currentValues,
  saving,
  onSave,
  onCancel,
}: {
  schema: ConfigField[];
  currentValues: ConfigValues;
  saving: boolean;
  onSave: (values: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState<Record<string, unknown>>(() => {
    const initial: Record<string, unknown> = {};
    for (const field of schema) {
      if (field.type === "secret") {
        initial[field.key] = "";
      } else {
        initial[field.key] = currentValues[field.key] ?? field.default ?? "";
      }
    }
    return initial;
  });

  function set(key: string, value: unknown) {
    setDraft((prev) => ({ ...prev, [key]: value }));
  }

  return (
    <div className="space-y-3">
      {schema.map((field) => {
        const current = currentValues[field.key];
        const secretConfigured = isSecretStatus(current) && current.configured;

        return (
          <label
            key={field.key}
            className="block text-xs font-medium text-slate-600 dark:text-slate-300"
          >
            <span>
              {field.label}
              {field.required && <span className="ml-1 text-red-500">*</span>}
            </span>
            {field.hint && (
              <span className="ml-2 font-normal text-slate-400 dark:text-slate-500">
                {field.hint}
              </span>
            )}
            {field.type === "select" ? (
              <select
                value={String(draft[field.key] ?? "")}
                onChange={(e) => set(field.key, e.target.value)}
                className={inputClass}
              >
                {!field.required && <option value="">— select —</option>}
                {field.options.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>
            ) : field.type === "boolean" ? (
              <input
                type="checkbox"
                checked={Boolean(draft[field.key])}
                onChange={(e) => set(field.key, e.target.checked)}
                className="ml-2"
              />
            ) : field.type === "number" ? (
              <input
                type="number"
                min={field.min}
                max={field.max}
                value={String(draft[field.key] ?? "")}
                onChange={(e) =>
                  set(field.key, e.target.value === "" ? "" : Number(e.target.value))
                }
                className={inputClass}
              />
            ) : field.type === "secret" ? (
              <input
                type="password"
                value={String(draft[field.key] ?? "")}
                onChange={(e) => set(field.key, e.target.value)}
                placeholder={
                  secretConfigured ? "Configured — leave blank to keep" : "Not configured"
                }
                className={inputClass}
                autoComplete="new-password"
              />
            ) : (
              <input
                type={field.type === "url" ? "url" : "text"}
                value={String(draft[field.key] ?? "")}
                onChange={(e) => set(field.key, e.target.value)}
                className={inputClass}
              />
            )}
          </label>
        );
      })}

      {schema.some((f) => f.type === "secret") && (
        <p className="text-xs text-slate-400 dark:text-slate-500">
          Secret fields are encrypted at rest and never returned by the API.
        </p>
      )}

      <div className="flex gap-2 pt-1">
        <button
          type="button"
          onClick={() => onSave(draft)}
          disabled={saving}
          className="rounded-lg bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saving ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className="rounded-lg border border-slate-300 px-2.5 py-1.5 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1 border-b-2 px-4 pb-2.5 pt-1 text-sm font-medium transition-colors ${
        active
          ? "border-blue-600 text-blue-600 dark:border-blue-400 dark:text-blue-400"
          : "border-transparent text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
      }`}
    >
      {children}
    </button>
  );
}

function StatusBadge({
  label,
  variant,
}: {
  label: string;
  variant: "danger" | "warning" | "info" | "neutral";
}) {
  const styles = {
    danger: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300",
    warning: "bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300",
    info: "bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300",
    neutral: "bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300",
  };
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${styles[variant]}`}>
      {label}
    </span>
  );
}

function ErrorMessage({ message }: { message: string }) {
  return (
    <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
      {message}
    </div>
  );
}

const inputClass =
  "mt-1.5 block min-h-10 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100";
