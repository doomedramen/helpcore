"use client";

import { FormEvent, KeyboardEvent, useRef, useEffect } from "react";
import { SendHorizontal, Sparkles, Square } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/app/components/ui/select";
import type { ProviderInfo } from "@/lib/types";

interface Props {
  onSend: (message: string) => void;
  onStop?: () => void;
  disabled?: boolean;
  active?: boolean;
  value: string;
  onChange: (value: string) => void;
  providers: ProviderInfo[];
  selectedProviderId: string;
  onProviderChange: (providerId: string) => void;
}

export default function ChatInput({
  onSend,
  onStop,
  disabled,
  active,
  value,
  onChange,
  providers,
  selectedProviderId,
  onProviderChange,
}: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [value]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  function submit() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    onChange("");
  }

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    submit();
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="surface-card overflow-hidden transition focus-within:border-indigo-300 focus-within:shadow-[0_20px_50px_-28px_rgba(79,70,229,0.45)] dark:focus-within:border-indigo-800"
    >
      <div className="flex items-end gap-2 px-2.5 pt-2.5">
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={disabled}
          rows={1}
          placeholder={
            providers.length === 0 ? "Configure a chat provider to begin." : "Message helpcore…"
          }
          autoFocus
          className="min-h-12 flex-1 resize-none overflow-hidden bg-transparent px-2 py-2.5 text-[15px] leading-relaxed text-slate-950 placeholder-slate-400 focus:outline-none disabled:opacity-50 dark:text-slate-100 dark:placeholder-slate-600"
        />
        <div className="flex shrink-0 gap-1.5 pb-1.5">
          {active && (
            <button
              type="button"
              onClick={onStop}
              className="flex size-10 items-center justify-center rounded-xl bg-slate-800 text-white transition hover:bg-red-600 focus:outline-none focus:ring-4 focus:ring-red-500/20"
              aria-label="Stop generation"
            >
              <Square size={15} fill="currentColor" />
            </button>
          )}
          <button
            type="submit"
            disabled={disabled || !value.trim()}
            className="flex size-10 items-center justify-center rounded-xl bg-indigo-600 text-white shadow-sm shadow-indigo-950/20 transition hover:bg-indigo-500 focus:outline-none focus:ring-4 focus:ring-indigo-500/20 disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-400 disabled:shadow-none dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:disabled:bg-slate-800 dark:disabled:text-slate-600"
            aria-label="Send message"
          >
            <SendHorizontal size={16} />
          </button>
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-slate-100 px-3 py-2 dark:border-slate-800">
        <Sparkles size={13} className="shrink-0 text-indigo-500 dark:text-indigo-400" />
        <Select
          value={selectedProviderId}
          onValueChange={(nextValue) => onProviderChange(nextValue ?? "")}
          disabled={providers.length === 0}
        >
          <SelectTrigger
            size="sm"
            className="h-7 max-w-[13rem] border-0 bg-transparent px-1.5 text-xs shadow-none focus-visible:ring-0"
          >
            <SelectValue placeholder="No chat provider">
              {providers.find((provider) => provider.id === selectedProviderId)?.name ??
                selectedProviderId}
            </SelectValue>
          </SelectTrigger>
          <SelectContent side="top">
            {providers.map((provider) => (
              <SelectItem key={provider.id} value={provider.id}>
                {provider.name} · {provider.default_model}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="ml-auto hidden text-[11px] text-slate-400 sm:block dark:text-slate-600">
          Enter to send · Shift + Enter for a new line
        </span>
      </div>
    </form>
  );
}
