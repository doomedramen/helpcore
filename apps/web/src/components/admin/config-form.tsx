'use client';

import { useEffect, useState } from 'react';
import { Plus, Save, Trash2 } from 'lucide-react';
import { updateAdminConfig } from '@/lib/api';
import type {
  AdminConfig,
  AdminConfigUpdate,
  AdminProviderUpdate,
  ProviderRole,
  ProviderType,
} from '@/lib/types';

interface Props {
  accessToken: string;
  config: AdminConfig;
  onSaved: (config: AdminConfig) => void;
}

const providerTypes: Array<{ value: ProviderType; label: string }> = [
  { value: 'ollama', label: 'Ollama' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'openai_compatible', label: 'OpenAI-compatible' },
];

const providerRoles: Array<{ value: ProviderRole; label: string }> = [
  { value: 'chat', label: 'Chat' },
  { value: 'code', label: 'Code' },
  { value: 'image_gen', label: 'Image generation' },
  { value: 'video_gen', label: 'Video generation' },
  { value: 'embeddings', label: 'Embeddings' },
];

function draftFromConfig(config: AdminConfig): AdminConfigUpdate {
  return {
    server: { ...config.server },
    logging_level: config.logging_level,
    registry_url: config.registry_url,
    plugin_blacklist: [...config.plugin_blacklist],
    providers: config.providers.map(provider => ({
      ...provider,
      api_key: null,
      clear_api_key: false,
    })),
  };
}

export default function ConfigForm({ accessToken, config, onSaved }: Props) {
  const [draft, setDraft] = useState<AdminConfigUpdate>(() => draftFromConfig(config));
  const [blacklist, setBlacklist] = useState(config.plugin_blacklist.join('\n'));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setDraft(draftFromConfig(config));
    setBlacklist(config.plugin_blacklist.join('\n'));
  }, [config]);

  function updateProvider(index: number, patch: Partial<AdminProviderUpdate>) {
    setDraft(current => ({
      ...current,
      providers: current.providers.map((provider, providerIndex) => (
        providerIndex === index ? { ...provider, ...patch } : provider
      )),
    }));
  }

  function addProvider() {
    setDraft(current => ({
      ...current,
      providers: [
        ...current.providers,
        {
          id: `provider-${current.providers.length + 1}`,
          name: 'New provider',
          provider_type: 'ollama',
          api_key: null,
          clear_api_key: false,
          url: 'http://localhost:11434',
          default_model: '',
          roles: ['chat'],
          num_ctx: null,
          num_predict: null,
        },
      ],
    }));
  }

  async function save() {
    setSaving(true);
    setError('');
    setSaved(false);
    try {
      const updated = await updateAdminConfig({
        ...draft,
        plugin_blacklist: blacklist
          .split(/[\n,]/)
          .map(item => item.trim())
          .filter(Boolean),
      }, accessToken);
      onSaved(updated);
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not save configuration.');
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-6">
      {saved && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
          Configuration saved. Restart helpcore to apply server and provider changes.
        </div>
      )}
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
          {error}
        </div>
      )}

      <Section title="Server" description={`Saved to ${config.config_path}`}>
        <div className="grid gap-4 md:grid-cols-2">
          <Field label="Display name">
            <input
              value={draft.server.name}
              onChange={event => setDraft(current => ({
                ...current,
                server: { ...current.server, name: event.target.value },
              }))}
              className={inputClass}
            />
          </Field>
          <Field label="Public URL">
            <input
              type="url"
              value={draft.server.url}
              onChange={event => setDraft(current => ({
                ...current,
                server: { ...current.server, url: event.target.value },
              }))}
              className={inputClass}
            />
          </Field>
          <Field label="Port">
            <input
              type="number"
              min={1}
              max={65535}
              value={draft.server.port}
              onChange={event => setDraft(current => ({
                ...current,
                server: { ...current.server, port: Number(event.target.value) },
              }))}
              className={inputClass}
            />
          </Field>
          <Field label="Log level">
            <select
              value={draft.logging_level}
              onChange={event => setDraft(current => ({
                ...current,
                logging_level: event.target.value,
              }))}
              className={inputClass}
            >
              {['trace', 'debug', 'info', 'warn', 'error'].map(level => (
                <option key={level} value={level}>{level}</option>
              ))}
            </select>
          </Field>
        </div>
      </Section>

      <Section
        title="Plugin registry"
        description="The catalog URL and server-wide install blacklist."
      >
        <div className="space-y-4">
          <Field label="Registry URL">
            <input
              type="url"
              value={draft.registry_url}
              onChange={event => setDraft(current => ({
                ...current,
                registry_url: event.target.value,
              }))}
              className={inputClass}
            />
          </Field>
          <Field label="Blacklisted plugin IDs" hint="One ID per line. Blocked plugins remain visible in the catalog.">
            <textarea
              rows={4}
              value={blacklist}
              onChange={event => setBlacklist(event.target.value)}
              placeholder="untrusted-plugin"
              className={`${inputClass} resize-y`}
            />
          </Field>
        </div>
      </Section>

      <Section
        title="Providers"
        description="Provider changes are validated now and loaded after a server restart."
        action={(
          <button
            type="button"
            onClick={addProvider}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
          >
            <Plus size={15} />
            Add provider
          </button>
        )}
      >
        <div className="space-y-4">
          {draft.providers.length === 0 && (
            <p className="rounded-xl border border-dashed border-slate-300 px-4 py-8 text-center text-sm text-slate-500 dark:border-slate-700 dark:text-slate-400">
              No providers configured. Chat will be unavailable until one is added.
            </p>
          )}
          {draft.providers.map((provider, index) => {
            const existing = config.providers.find(item => item.id === provider.id);
            return (
              <div
                key={`${provider.id}-${index}`}
                className="rounded-xl border border-slate-200 bg-slate-50/60 p-4 dark:border-slate-800 dark:bg-slate-950/50"
              >
                <div className="mb-4 flex items-start justify-between gap-4">
                  <div>
                    <h3 className="font-medium text-slate-900 dark:text-slate-100">
                      {provider.name || 'Unnamed provider'}
                    </h3>
                    <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">
                      {provider.id || 'Provider ID required'}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => setDraft(current => ({
                      ...current,
                      providers: current.providers.filter((_, itemIndex) => itemIndex !== index),
                    }))}
                    className="rounded-lg p-2 text-slate-400 transition-colors hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/50 dark:hover:text-red-400"
                    aria-label={`Remove ${provider.name || 'provider'}`}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>

                <div className="grid gap-4 md:grid-cols-2">
                  <Field label="ID">
                    <input
                      value={provider.id}
                      onChange={event => updateProvider(index, { id: event.target.value })}
                      className={inputClass}
                    />
                  </Field>
                  <Field label="Name">
                    <input
                      value={provider.name}
                      onChange={event => updateProvider(index, { name: event.target.value })}
                      className={inputClass}
                    />
                  </Field>
                  <Field label="Type">
                    <select
                      value={provider.provider_type}
                      onChange={event => updateProvider(index, {
                        provider_type: event.target.value as ProviderType,
                      })}
                      className={inputClass}
                    >
                      {providerTypes.map(type => (
                        <option key={type.value} value={type.value}>{type.label}</option>
                      ))}
                    </select>
                  </Field>
                  <Field label="Default model">
                    <input
                      value={provider.default_model}
                      onChange={event => updateProvider(index, { default_model: event.target.value })}
                      className={inputClass}
                    />
                  </Field>
                  <Field label="Base URL" hint="Required for Ollama and OpenAI-compatible providers.">
                    <input
                      type="url"
                      value={provider.url ?? ''}
                      onChange={event => updateProvider(index, { url: event.target.value || null })}
                      placeholder="http://localhost:11434"
                      className={inputClass}
                    />
                  </Field>
                  <Field
                    label="API key"
                    hint={existing?.api_key_configured ? 'A key is saved. Leave blank to keep it.' : 'Stored only in the server config file.'}
                  >
                    <input
                      type="password"
                      value={provider.api_key ?? ''}
                      onChange={event => updateProvider(index, {
                        api_key: event.target.value || null,
                        clear_api_key: false,
                      })}
                      placeholder={existing?.api_key_configured ? 'Configured' : 'Not configured'}
                      className={inputClass}
                    />
                    {existing?.api_key_configured && (
                      <label className="mt-2 flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                        <input
                          type="checkbox"
                          checked={provider.clear_api_key ?? false}
                          onChange={event => updateProvider(index, {
                            clear_api_key: event.target.checked,
                            api_key: null,
                          })}
                        />
                        Remove saved key
                      </label>
                    )}
                  </Field>
                  <Field label="Context tokens">
                    <input
                      type="number"
                      min={1}
                      value={provider.num_ctx ?? ''}
                      onChange={event => updateProvider(index, {
                        num_ctx: event.target.value ? Number(event.target.value) : null,
                      })}
                      placeholder="8192"
                      className={inputClass}
                    />
                  </Field>
                  <Field label="Maximum response tokens">
                    <input
                      type="number"
                      min={1}
                      value={provider.num_predict ?? ''}
                      onChange={event => updateProvider(index, {
                        num_predict: event.target.value ? Number(event.target.value) : null,
                      })}
                      placeholder="2048"
                      className={inputClass}
                    />
                  </Field>
                </div>

                <div className="mt-4">
                  <span className="text-sm font-medium text-slate-700 dark:text-slate-300">Roles</span>
                  <div className="mt-2 flex flex-wrap gap-3">
                    {providerRoles.map(role => (
                      <label
                        key={role.value}
                        className="flex items-center gap-2 text-sm text-slate-600 dark:text-slate-300"
                      >
                        <input
                          type="checkbox"
                          checked={provider.roles.includes(role.value)}
                          onChange={event => updateProvider(index, {
                            roles: event.target.checked
                              ? [...provider.roles, role.value]
                              : provider.roles.filter(item => item !== role.value),
                          })}
                        />
                        {role.label}
                      </label>
                    ))}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </Section>

      <div className="flex justify-end">
        <button
          type="button"
          onClick={save}
          disabled={saving}
          className="inline-flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Save size={16} />
          {saving ? 'Saving…' : 'Save configuration'}
        </button>
      </div>
    </div>
  );
}

function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <div className="mb-5 flex items-start justify-between gap-4">
        <div>
          <h2 className="text-base font-semibold text-slate-900 dark:text-white">{title}</h2>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">{description}</p>
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="text-sm font-medium text-slate-700 dark:text-slate-300">{label}</span>
      {hint && <span className="ml-2 text-xs text-slate-400 dark:text-slate-500">{hint}</span>}
      <div className="mt-1.5">{children}</div>
    </label>
  );
}

const inputClass = 'block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500';
