"use client";

import { useEffect, useRef, useState } from "react";
import { useForm, useFieldArray } from "react-hook-form";
import { ChevronDown, ChevronRight, Plus, Save, Trash2 } from "lucide-react";
import { updateAdminConfig } from "@/lib/api";
import type {
  AdminConfig,
  AdminConfigUpdate,
  AdminProviderUpdate,
  ProviderRole,
  ProviderType,
} from "@/lib/types";

interface Props {
  accessToken: string;
  config: AdminConfig;
  onSaved: (config: AdminConfig) => void;
}

const providerTypes: Array<{ value: ProviderType; label: string }> = [
  { value: "ollama", label: "Ollama" },
  { value: "openai", label: "OpenAI (ChatGPT models)" },
  { value: "anthropic", label: "Anthropic Claude" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "openai_compatible", label: "OpenAI-compatible" },
];

const providerRoles: Array<{ value: ProviderRole; label: string }> = [
  { value: "chat", label: "Chat" },
  { value: "code", label: "Code" },
  { value: "image_gen", label: "Image generation" },
  { value: "video_gen", label: "Video generation" },
  { value: "embeddings", label: "Embeddings" },
];

function draftFromConfig(config: AdminConfig): AdminConfigUpdate {
  return {
    server: { ...config.server },
    logging_level: config.logging_level,
    registry_url: config.registry_url,
    plugin_blacklist: [...config.plugin_blacklist],
    providers: config.providers.map(({ api_key_configured: _apiKeyConfigured, ...provider }) => ({
      ...provider,
      api_key: null,
      clear_api_key: false,
    })),
  };
}

const hostedProviderTypes = new Set<ProviderType>(["openai", "anthropic", "deepseek"]);

function defaultUrl(providerType: ProviderType): string | null {
  switch (providerType) {
    case "ollama":
      return "http://localhost:11434";
    case "openai_compatible":
      return "http://localhost:1234/v1";
    default:
      return null;
  }
}

function validateProviders(
  providers: AdminProviderUpdate[],
  existingProviders: AdminConfig["providers"],
): string {
  const ids = new Set<string>();
  for (const provider of providers) {
    const id = provider.id.trim();
    if (!id) return "Every provider needs an ID.";
    if (ids.has(id)) return `Provider IDs must be unique: ${id}`;
    ids.add(id);
    if (!provider.name.trim()) return `Provider ${id} needs a name.`;
    if (!provider.default_model.trim()) return `Provider ${id} needs a default model.`;
    if (
      (provider.provider_type === "ollama" || provider.provider_type === "openai_compatible") &&
      !provider.url?.trim()
    ) {
      return `Provider ${id} needs a base URL.`;
    }
    if (provider.url?.trim()) {
      try {
        const url = new URL(provider.url);
        if (url.protocol !== "http:" && url.protocol !== "https:") {
          return `Provider ${id} base URL must use http or https.`;
        }
      } catch {
        return `Provider ${id} base URL is invalid.`;
      }
    }
    if (hostedProviderTypes.has(provider.provider_type)) {
      const existing = existingProviders.find((item) => item.id === id);
      const hasSavedKey = existing?.api_key_configured && !provider.clear_api_key;
      const hasNewKey = Boolean(provider.api_key?.trim());
      if (!hasSavedKey && !hasNewKey) return `Provider ${id} needs an API key.`;
    }
    if (provider.roles.length === 0) return `Provider ${id} needs at least one role.`;
    if (provider.num_ctx !== null && provider.num_ctx <= 0) {
      return `Provider ${id} context tokens must be greater than zero.`;
    }
    if (provider.num_predict !== null && provider.num_predict <= 0) {
      return `Provider ${id} maximum response tokens must be greater than zero.`;
    }
  }
  return "";
}

export default function ConfigForm({ accessToken, config, onSaved }: Props) {
  const {
    control,
    register,
    reset,
    watch,
    getValues,
    setValue,
    formState: { isDirty },
  } = useForm<AdminConfigUpdate>({
    defaultValues: draftFromConfig(config),
  });
  const { fields, append, remove } = useFieldArray({ control, name: "providers" });

  const [blacklist, setBlacklist] = useState(config.plugin_blacklist.join("\n"));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [savedConfig, setSavedConfig] = useState<AdminConfig | null>(null);
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set());
  const prevFieldsLength = useRef(fields.length);

  useEffect(() => {
    if (fields.length > prevFieldsLength.current) {
      const newField = fields[fields.length - 1];
      setExpandedProviders((prev) => new Set([...prev, newField.id]));
    }
    prevFieldsLength.current = fields.length;
  }, [fields]);

  function toggleProvider(fieldId: string) {
    setExpandedProviders((prev) => {
      const next = new Set(prev);
      if (next.has(fieldId)) next.delete(fieldId);
      else next.add(fieldId);
      return next;
    });
  }

  useEffect(() => {
    if (isDirty) return;
    reset(draftFromConfig(config));
    setBlacklist(config.plugin_blacklist.join("\n"));
  }, [config, isDirty, reset]);

  const contextTokenDefaults: Record<ProviderType, number> = {
    ollama: 8192,
    openai: 128000,
    anthropic: 200000,
    deepseek: 128000,
    openai_compatible: 8192,
  };

  const predictTokenDefaults: Record<ProviderType, number | null> = {
    ollama: null,
    openai: null,
    anthropic: 4096,
    deepseek: null,
    openai_compatible: null,
  };

  function addProvider() {
    const providerType = "ollama" as ProviderType;
    append({
      id: crypto.randomUUID(),
      name: "New provider",
      provider_type: providerType,
      api_key: null,
      clear_api_key: false,
      url: "http://localhost:11434",
      default_model: "",
      roles: ["chat"] as ProviderRole[],
      num_ctx: contextTokenDefaults[providerType],
      num_predict: predictTokenDefaults[providerType],
    });
  }

  async function save() {
    const data = getValues();
    const validationError = validateProviders(data.providers, config.providers);
    if (validationError) {
      setError(validationError);
      return;
    }
    setSaving(true);
    setError("");
    setSavedConfig(null);
    try {
      const updated = await updateAdminConfig(
        {
          ...data,
          plugin_blacklist: blacklist
            .split(/[\n,]/)
            .map((item) => item.trim())
            .filter(Boolean),
        },
        accessToken,
      );
      reset(draftFromConfig(updated));
      setBlacklist(updated.plugin_blacklist.join("\n"));
      onSaved(updated);
      setSavedConfig(updated);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save configuration.");
    } finally {
      setSaving(false);
    }
  }

  const values = watch();

  return (
    <div className="space-y-6">
      {savedConfig && (
        <div
          className={`rounded-xl border px-4 py-3 text-sm ${
            savedConfig.restart_required
              ? "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200"
              : "border-green-200 bg-green-50 text-green-800 dark:border-green-900 dark:bg-green-950/40 dark:text-green-200"
          }`}
        >
          {savedConfig.restart_required
            ? "Configuration saved. Provider changes are active now; restart helpcore to apply port or logging changes."
            : "Configuration saved. Provider changes are active now."}
        </div>
      )}
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
          {error}
        </div>
      )}
      {!config.config_writable && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
          Configuration is read-only. {config.config_writability_error}
        </div>
      )}

      <Section title="Server" description={`Saved to ${config.config_path}`}>
        <div className="grid gap-4 md:grid-cols-2">
          <Field label="Display name">
            <input {...register("server.name")} className={inputClass} />
          </Field>
          <Field
            label="Public URL"
            hint="How clients reach this server. Includes protocol, hostname, and port."
          >
            <input type="url" {...register("server.url")} className={inputClass} />
          </Field>
          <Field
            label="Server port"
            hint="The local port the server binds to. Can differ from the public URL port behind a reverse proxy."
          >
            <input
              type="number"
              min={1}
              max={65535}
              {...register("server.port", { valueAsNumber: true })}
              className={inputClass}
            />
          </Field>
          <Field label="Log level">
            <select {...register("logging_level")} className={inputClass}>
              {["trace", "debug", "info", "warn", "error"].map((level) => (
                <option key={level} value={level}>
                  {level}
                </option>
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
            <input type="url" {...register("registry_url")} className={inputClass} />
          </Field>
          <Field
            label="Blacklisted plugin IDs"
            hint="One ID per line. Blocked plugins remain visible in the catalog."
          >
            <textarea
              rows={4}
              value={blacklist}
              onChange={(event) => setBlacklist(event.target.value)}
              placeholder="untrusted-plugin"
              className={`${inputClass} resize-y`}
            />
          </Field>
        </div>
      </Section>

      <Section
        title="Providers"
        description="Provider changes are validated and applied immediately when saved."
        action={
          <button
            type="button"
            onClick={addProvider}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
          >
            <Plus size={15} />
            Add provider
          </button>
        }
      >
        <div className="space-y-4">
          {fields.length === 0 && (
            <p className="rounded-xl border border-dashed border-slate-300 px-4 py-8 text-center text-sm text-slate-500 dark:border-slate-700 dark:text-slate-400">
              No providers configured. Chat will be unavailable until one is added.
            </p>
          )}
          {fields.map((field, index) => {
            const providerValues = values.providers?.[index];
            const providerId = providerValues?.id ?? "";
            const existing = config.providers.find((item) => item.id === providerId);
            const isExpanded = expandedProviders.has(field.id);
            const activeRoles = providerValues?.roles ?? [];
            const roleLabels = providerRoles
              .filter((r) => activeRoles.includes(r.value))
              .map((r) => r.label);
            const typeLabel =
              providerTypes.find((t) => t.value === providerValues?.provider_type)?.label ?? "";

            return (
              <div
                key={field.id}
                className="overflow-hidden rounded-xl border border-slate-200 dark:border-slate-800"
              >
                {/* Collapsed header — always visible */}
                <button
                  type="button"
                  onClick={() => toggleProvider(field.id)}
                  className="flex w-full items-center gap-3 bg-white px-4 py-3 text-left transition-colors hover:bg-slate-50 dark:bg-slate-900 dark:hover:bg-slate-800/60"
                >
                  {isExpanded ? (
                    <ChevronDown size={15} className="shrink-0 text-slate-400" />
                  ) : (
                    <ChevronRight size={15} className="shrink-0 text-slate-400" />
                  )}
                  <div className="min-w-0 flex-1">
                    <span className="font-medium text-slate-900 dark:text-slate-100">
                      {providerValues?.name || "Unnamed provider"}
                    </span>
                    <span className="ml-2 text-xs text-slate-400 dark:text-slate-500">
                      {typeLabel}
                      {roleLabels.length > 0 ? ` · ${roleLabels.join(", ")}` : ""}
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      remove(index);
                    }}
                    className="shrink-0 rounded-lg p-1.5 text-slate-400 transition-colors hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/50 dark:hover:text-red-400"
                    aria-label={`Remove ${providerValues?.name || "provider"}`}
                  >
                    <Trash2 size={15} />
                  </button>
                </button>

                {/* Expanded fields */}
                {isExpanded && (
                  <div className="border-t border-slate-200 bg-slate-50/60 p-4 dark:border-slate-800 dark:bg-slate-950/50">
                    <div className="grid gap-4 md:grid-cols-2">
                      <input type="hidden" {...register(`providers.${index}.id`)} />
                      <Field label="Name">
                        <input {...register(`providers.${index}.name`)} className={inputClass} />
                      </Field>
                      <Field label="Type">
                        <select
                          {...register(`providers.${index}.provider_type`, {
                            onChange: (e) => {
                              const providerType = e.target.value as ProviderType;
                              setValue(`providers.${index}.url`, defaultUrl(providerType));
                              setValue(
                                `providers.${index}.num_ctx`,
                                contextTokenDefaults[providerType],
                              );
                              setValue(
                                `providers.${index}.num_predict`,
                                predictTokenDefaults[providerType],
                              );
                            },
                          })}
                          className={inputClass}
                        >
                          {providerTypes.map((type) => (
                            <option key={type.value} value={type.value}>
                              {type.label}
                            </option>
                          ))}
                        </select>
                      </Field>
                      <Field label="Default model">
                        <input
                          {...register(`providers.${index}.default_model`)}
                          className={inputClass}
                        />
                      </Field>
                      <Field
                        label="Base URL"
                        hint={
                          providerValues?.provider_type === "ollama"
                            ? "Required. URL of the Ollama server."
                            : providerValues?.provider_type === "openai_compatible"
                              ? "Required. Include the API version prefix when needed."
                              : "Optional. Leave blank to use the official API."
                        }
                      >
                        <input
                          type="url"
                          {...register(`providers.${index}.url`, {
                            setValueAs: (v) => (v === "" ? null : v),
                          })}
                          placeholder={
                            defaultUrl(providerValues?.provider_type ?? "ollama") ??
                            "Official API endpoint"
                          }
                          className={inputClass}
                        />
                      </Field>
                      <Field
                        label="API key"
                        hint={
                          existing?.api_key_configured
                            ? "A key is saved. Leave blank to keep it."
                            : hostedProviderTypes.has(providerValues?.provider_type ?? "ollama")
                              ? "Required. Stored only in the server config file."
                              : "Optional. Stored only in the server config file."
                        }
                      >
                        <input
                          type="password"
                          {...register(`providers.${index}.api_key`, {
                            setValueAs: (v) => (v === "" ? null : v),
                          })}
                          placeholder={
                            existing?.api_key_configured ? "Configured" : "Not configured"
                          }
                          className={inputClass}
                        />
                        {existing?.api_key_configured && (
                          <label className="mt-2 flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                            <input
                              type="checkbox"
                              checked={providerValues?.clear_api_key ?? false}
                              onChange={(e) => {
                                if (e.target.checked) {
                                  setValue(`providers.${index}.clear_api_key`, true);
                                  setValue(`providers.${index}.api_key`, null);
                                } else {
                                  setValue(`providers.${index}.clear_api_key`, false);
                                }
                              }}
                            />
                            Remove saved key
                          </label>
                        )}
                      </Field>
                      <Field
                        label="Context tokens"
                        hint={
                          providerValues?.num_ctx
                            ? undefined
                            : `Defaults: Ollama 8192, OpenAI/DeepSeek 128000, Anthropic 200000. Set lower for small models to save RAM.`
                        }
                      >
                        <input
                          type="number"
                          min={1}
                          {...register(`providers.${index}.num_ctx`, {
                            setValueAs: (v) => (v === "" ? null : Number(v)),
                          })}
                          placeholder={
                            providerValues?.provider_type === "anthropic"
                              ? "200000"
                              : providerValues?.provider_type === "ollama"
                                ? "8192"
                                : "128000"
                          }
                          className={inputClass}
                        />
                      </Field>
                      <Field
                        label="Maximum response tokens"
                        hint={
                          providerValues?.num_predict
                            ? undefined
                            : "Limits how many tokens the model can generate per response. Leave blank for provider default."
                        }
                      >
                        <input
                          type="number"
                          min={1}
                          {...register(`providers.${index}.num_predict`, {
                            setValueAs: (v) => (v === "" ? null : Number(v)),
                          })}
                          placeholder="2048"
                          className={inputClass}
                        />
                      </Field>
                    </div>

                    <div className="mt-4">
                      <span className="text-sm font-medium text-slate-700 dark:text-slate-300">
                        Roles
                      </span>
                      <div className="mt-2 flex flex-wrap gap-3">
                        {providerRoles.map((role) => (
                          <label
                            key={role.value}
                            className="flex items-center gap-2 text-sm text-slate-600 dark:text-slate-300"
                          >
                            <input
                              type="checkbox"
                              checked={providerValues?.roles?.includes(role.value) ?? false}
                              onChange={() => {
                                const current =
                                  (getValues(`providers.${index}.roles`) as ProviderRole[]) || [];
                                if (current.includes(role.value)) {
                                  setValue(
                                    `providers.${index}.roles`,
                                    current.filter((r) => r !== role.value),
                                    { shouldDirty: true },
                                  );
                                } else {
                                  setValue(`providers.${index}.roles`, [...current, role.value], {
                                    shouldDirty: true,
                                  });
                                }
                              }}
                            />
                            {role.label}
                          </label>
                        ))}
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </Section>

      <div className="flex justify-end">
        <button
          type="button"
          onClick={save}
          disabled={saving || !config.config_writable}
          className="inline-flex items-center gap-2 rounded-lg bg-blue-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Save size={16} />
          {saving ? "Saving…" : "Save configuration"}
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

const inputClass =
  "block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500";
