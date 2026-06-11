"use client";

import { useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, CircleHelp, CornerDownLeft, ExternalLink } from "lucide-react";
import type {
  ConfigField,
  InteractionQuestion,
  PendingInteraction,
  SecretStatus,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/app/components/ui/tooltip";

interface Props {
  interaction: PendingInteraction;
  submitting: boolean;
  error: string;
  onSubmit: (response: Record<string, unknown>) => void;
  onDismiss: () => void;
}

interface DraftAnswer {
  optionIds?: string[];
  custom?: string;
}

/** Codex-style composer for durable assistant questions and plugin workflows. */
export default function InteractionComposer({
  interaction,
  submitting,
  error,
  onSubmit,
  onDismiss,
}: Props) {
  const pages = useMemo(() => interactionPages(interaction), [interaction]);
  const [index, setIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<string, DraftAnswer>>(() =>
    initialAnswers(interaction),
  );
  const [config, setConfig] = useState<Record<string, unknown>>(() => initialConfig(interaction));

  useEffect(() => {
    setIndex(0);
    setAnswers(initialAnswers(interaction));
    setConfig(initialConfig(interaction));
  }, [interaction]);

  const page = pages[Math.min(index, pages.length - 1)];
  const canContinue = page ? pageComplete(page, answers, config) : false;

  function advance() {
    if (!canContinue || submitting) return;
    if (index < pages.length - 1) {
      setIndex((current) => current + 1);
      return;
    }
    onSubmit(buildResponse(interaction, answers, config));
  }

  function advanceWithAnswer(answer: DraftAnswer) {
    if (submitting) return;
    const nextAnswers = { ...answers, [page.id]: answer };
    setAnswers(nextAnswers);
    if (index < pages.length - 1) {
      setIndex((current) => current + 1);
      return;
    }
    onSubmit(buildResponse(interaction, nextAnswers, config));
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onDismiss();
        return;
      }
      const target = event.target as HTMLElement | null;
      const typing =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.tagName === "SELECT";
      if (!typing && page?.kind === "choice" && /^[1-3]$/.test(event.key)) {
        const option = page.options[Number(event.key) - 1];
        if (option) {
          event.preventDefault();
          if (page.selectionMode === "single") {
            advanceWithAnswer({ optionIds: [option.id], custom: "" });
          } else {
            setAnswers((current) => {
              const selected = current[page.id]?.optionIds ?? [];
              const optionIds = selected.includes(option.id)
                ? selected.filter((id) => id !== option.id)
                : [...selected, option.id];
              return {
                ...current,
                [page.id]: { optionIds, custom: current[page.id]?.custom },
              };
            });
          }
        }
      }
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        advance();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  if (!page) return null;

  return (
    <div className="rounded-[2rem] border bg-card p-4 text-card-foreground shadow-xl sm:p-5">
      <div className="flex items-start justify-between gap-4 px-1">
        <div>
          <p className="text-base font-medium sm:text-lg">{page.title}</p>
          {page.subtitle && <p className="mt-1 text-sm text-muted-foreground">{page.subtitle}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-2 text-sm text-muted-foreground">
          <button
            type="button"
            onClick={() => setIndex((current) => Math.max(0, current - 1))}
            disabled={index === 0}
            className="rounded-md p-1 transition hover:bg-muted disabled:opacity-30"
            aria-label="Previous question"
          >
            <ChevronLeft className="size-4" />
          </button>
          <span className="tabular-nums">
            {index + 1} of {pages.length}
          </span>
          <button
            type="button"
            onClick={() => {
              if (canContinue) {
                setIndex((current) => Math.min(pages.length - 1, current + 1));
              }
            }}
            disabled={index === pages.length - 1 || !canContinue}
            className="rounded-md p-1 transition hover:bg-muted disabled:opacity-30"
            aria-label="Next question"
          >
            <ChevronRight className="size-4" />
          </button>
        </div>
      </div>

      <div className="mt-5">
        {page.kind === "choice" ? (
          <ChoicePage
            page={page}
            answer={answers[page.id]}
            onChange={(answer) => setAnswers((current) => ({ ...current, [page.id]: answer }))}
            onSelectSingle={advanceWithAnswer}
          />
        ) : page.kind === "text" ? (
          <TextPage
            value={answers[page.id]?.custom ?? ""}
            onChange={(custom) => setAnswers((current) => ({ ...current, [page.id]: { custom } }))}
          />
        ) : (
          <ConfigPage
            field={page.field}
            value={config[page.field.key]}
            secretConfigured={page.secretConfigured}
            onChange={(value) => setConfig((current) => ({ ...current, [page.field.key]: value }))}
          />
        )}
      </div>

      {interaction.kind === "plugin_approval" && interaction.plugin.setup_guide && (
        <a
          href={interaction.plugin.setup_guide}
          target="_blank"
          rel="noreferrer"
          className="mt-3 inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground"
        >
          Open setup guide <ExternalLink className="size-3" />
        </a>
      )}

      {(error || (interaction.kind === "plugin_config" && interaction.error)) && (
        <p className="mt-4 rounded-xl bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error || (interaction.kind === "plugin_config" ? interaction.error : "")}
        </p>
      )}

      <div className="mt-5 flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={onDismiss}
          disabled={submitting}
          className="text-sm text-muted-foreground transition hover:text-foreground disabled:opacity-50"
        >
          Dismiss <kbd className="ml-1 rounded-md bg-muted px-1.5 py-0.5">ESC</kbd>
        </button>
        <button
          type="button"
          onClick={advance}
          disabled={!canContinue || submitting}
          className="inline-flex items-center gap-2 rounded-full bg-primary px-5 py-2 text-sm font-medium text-primary-foreground transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {submitting ? "Working..." : index === pages.length - 1 ? "Continue" : "Next"}
          <CornerDownLeft className="size-4" />
        </button>
      </div>
    </div>
  );
}

interface ChoicePageData {
  kind: "choice";
  id: string;
  title: string;
  subtitle?: string;
  options: { id: string; label: string; description: string }[];
  selectionMode: "single" | "multi";
  allowCustom: boolean;
}

interface TextPageData {
  kind: "text";
  id: string;
  title: string;
  subtitle?: string;
}

interface ConfigPageData {
  kind: "config";
  id: string;
  title: string;
  subtitle?: string;
  field: ConfigField;
  secretConfigured: boolean;
}

type Page = ChoicePageData | TextPageData | ConfigPageData;

function interactionPages(interaction: PendingInteraction): Page[] {
  if (interaction.kind === "questions") {
    return interaction.questions.map(questionPage);
  }
  if (interaction.kind === "plugin_approval") {
    const action =
      interaction.plugin.action === "enable"
        ? "Enable"
        : interaction.plugin.action === "configure_enable"
          ? "Configure and enable"
          : "Install, configure, and enable";
    const details = [
      interaction.plugin.permissions.length
        ? `Permissions: ${interaction.plugin.permissions.join(", ")}`
        : "No special permissions",
      interaction.plugin.allowed_hosts.length
        ? `Hosts: ${interaction.plugin.allowed_hosts.join(", ")}`
        : null,
    ]
      .filter(Boolean)
      .join(" · ");
    return [
      {
        kind: "choice",
        id: "plugin_approval",
        title: `${action} ${interaction.plugin.name}?`,
        subtitle: `${interaction.plugin.rationale}${details ? ` · ${details}` : ""}`,
        options: [
          {
            id: "approve",
            label: action,
            description: "Approve the guided plugin setup for this account.",
          },
          {
            id: "decline",
            label: "Not now",
            description: "Continue without changing the plugin.",
          },
        ],
        selectionMode: "single",
        allowCustom: false,
      },
    ];
  }
  return interaction.fields.map((field) => {
    const current = interaction.current_values[field.key];
    return {
      kind: "config",
      id: field.key,
      title: field.label,
      subtitle: field.hint,
      field,
      secretConfigured: isSecretStatus(current) && current.configured,
    };
  });
}

function questionPage(question: InteractionQuestion): ChoicePageData | TextPageData {
  if (question.question_type === "text") {
    return {
      kind: "text",
      id: question.id,
      title: question.question,
      subtitle: question.header,
    };
  }
  return {
    kind: "choice",
    id: question.id,
    title: question.question,
    subtitle: question.header,
    options: question.options,
    selectionMode: question.question_type === "multi_select" ? "multi" : "single",
    allowCustom: true,
  };
}

function ChoicePage({
  page,
  answer,
  onChange,
  onSelectSingle,
}: {
  page: ChoicePageData;
  answer?: DraftAnswer;
  onChange: (answer: DraftAnswer) => void;
  onSelectSingle: (answer: DraftAnswer) => void;
}) {
  return (
    <TooltipProvider>
      <div className="space-y-2">
        {page.options.map((option, optionIndex) => {
          const selected = answer?.optionIds?.includes(option.id) ?? false;
          return (
            <div
              key={option.id}
              className={cn(
                "flex w-full items-center gap-2 rounded-2xl px-3 py-2 transition",
                selected ? "bg-muted text-foreground" : "text-muted-foreground hover:bg-muted/60",
              )}
            >
              <button
                type="button"
                aria-label={option.label}
                aria-pressed={selected}
                onClick={() => {
                  const selectedIds = answer?.optionIds ?? [];
                  if (page.selectionMode === "single") {
                    onSelectSingle({ optionIds: [option.id], custom: "" });
                    return;
                  }
                  onChange({
                    optionIds: selectedIds.includes(option.id)
                      ? selectedIds.filter((id) => id !== option.id)
                      : [...selectedIds, option.id],
                    custom: answer?.custom,
                  });
                }}
                className="flex min-w-0 flex-1 items-center gap-3 py-1 text-left"
              >
                <span
                  className={cn(
                    "flex size-8 shrink-0 items-center justify-center border text-sm",
                    page.selectionMode === "multi" ? "rounded-lg" : "rounded-full",
                    selected && "border-primary bg-primary text-primary-foreground",
                  )}
                >
                  {optionIndex + 1}
                </span>
                <span className="truncate font-medium">{option.label}</span>
              </button>
              <Tooltip>
                <TooltipTrigger
                  render={
                    <button
                      type="button"
                      className="rounded-full p-1 text-muted-foreground hover:bg-background hover:text-foreground"
                      aria-label={`More information about ${option.label}`}
                    >
                      <CircleHelp className="size-4" />
                    </button>
                  }
                />
                <TooltipContent>{option.description}</TooltipContent>
              </Tooltip>
            </div>
          );
        })}
        {page.allowCustom && (
          <input
            type="text"
            value={answer?.custom ?? ""}
            onChange={(event) =>
              onChange({
                optionIds: page.selectionMode === "single" ? [] : answer?.optionIds,
                custom: event.target.value,
              })
            }
            placeholder="Something else"
            aria-label={`Custom answer for ${page.title}`}
            className="field-input mt-3"
          />
        )}
      </div>
    </TooltipProvider>
  );
}

function TextPage({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <textarea
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder="Type your answer"
      rows={3}
      autoFocus
      className="field-input min-h-24 resize-y"
    />
  );
}

function ConfigPage({
  field,
  value,
  secretConfigured,
  onChange,
}: {
  field: ConfigField;
  value: unknown;
  secretConfigured: boolean;
  onChange: (value: unknown) => void;
}) {
  if (field.type === "select") {
    return (
      <select
        value={String(value ?? "")}
        onChange={(event) => onChange(event.target.value)}
        className="field-input"
      >
        {!field.required && <option value="">Select an option</option>}
        {field.options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    );
  }
  if (field.type === "boolean") {
    return (
      <label className="flex items-center gap-3 rounded-2xl bg-muted px-4 py-3">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="text-sm">Enabled</span>
      </label>
    );
  }
  return (
    <input
      type={field.type === "secret" ? "password" : field.type === "url" ? "url" : field.type}
      min={field.min}
      max={field.max}
      value={String(value ?? "")}
      onChange={(event) =>
        onChange(
          field.type === "number" && event.target.value !== ""
            ? Number(event.target.value)
            : event.target.value,
        )
      }
      placeholder={
        field.type === "secret" && secretConfigured
          ? "Configured - leave blank to keep"
          : (field.default ?? "")
      }
      autoComplete={field.type === "secret" ? "new-password" : undefined}
      className="field-input"
    />
  );
}

function initialConfig(interaction: PendingInteraction): Record<string, unknown> {
  if (interaction.kind !== "plugin_config") return {};
  const draft: Record<string, unknown> = {};
  for (const field of interaction.fields) {
    const current = interaction.current_values[field.key];
    draft[field.key] =
      field.type === "secret" ? "" : current !== undefined ? current : (field.default ?? "");
  }
  return draft;
}

function initialAnswers(interaction: PendingInteraction): Record<string, DraftAnswer> {
  if (interaction.kind === "questions") {
    return Object.fromEntries(
      interaction.questions.map((question) => [
        question.id,
        question.question_type === "single_select"
          ? { optionIds: question.options[0] ? [question.options[0].id] : [] }
          : question.question_type === "multi_select"
            ? { optionIds: [] }
            : { custom: "" },
      ]),
    );
  }
  if (interaction.kind === "plugin_approval") {
    return { plugin_approval: { optionIds: ["approve"] } };
  }
  return {};
}

function pageComplete(
  page: Page,
  answers: Record<string, DraftAnswer>,
  config: Record<string, unknown>,
): boolean {
  if (page.kind === "choice") {
    const answer = answers[page.id];
    return Boolean(answer?.optionIds?.length || answer?.custom?.trim());
  }
  if (page.kind === "text") {
    return Boolean(answers[page.id]?.custom?.trim());
  }
  const value = config[page.field.key];
  if (!page.field.required) return true;
  if (page.field.type === "boolean") return typeof value === "boolean";
  if (page.field.type === "secret" && page.secretConfigured && !value) return true;
  return value !== null && value !== undefined && String(value).trim() !== "";
}

function buildResponse(
  interaction: PendingInteraction,
  answers: Record<string, DraftAnswer>,
  config: Record<string, unknown>,
): Record<string, unknown> {
  if (interaction.kind === "questions") {
    return {
      kind: "questions",
      answers: interaction.questions.map((question) => ({
        question_id: question.id,
        option_ids: answers[question.id]?.optionIds ?? [],
        custom_response: answers[question.id]?.custom?.trim() || null,
      })),
    };
  }
  if (interaction.kind === "plugin_approval") {
    return {
      kind: "plugin_approval",
      approved: answers.plugin_approval?.optionIds?.includes("approve") ?? false,
    };
  }
  const values: Record<string, unknown> = {};
  for (const field of interaction.fields) {
    const value = config[field.key];
    if (field.type === "secret" && value === "") continue;
    values[field.key] = value;
  }
  return { kind: "plugin_config", values };
}

function isSecretStatus(value: unknown): value is SecretStatus {
  return typeof value === "object" && value !== null && "configured" in value;
}
