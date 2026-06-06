"use client";

import { FormEvent, KeyboardEvent, useRef, useEffect } from "react";
import { SendHorizontal, Square } from "lucide-react";
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
    <div className="space-y-2">
      <div className="flex items-center gap-2 px-1">
        <label className="text-xs font-medium text-slate-500 dark:text-slate-400">Provider</label>
        <Select
          value={selectedProviderId}
          onValueChange={(value) => onProviderChange(value ?? "")}
          disabled={providers.length === 0}
        >
          <SelectTrigger size="sm" className="max-w-full text-xs">
            <SelectValue placeholder="No chat providers available" />
          </SelectTrigger>
          <SelectContent side="top">
            {providers.map((provider) => (
              <SelectItem key={provider.id} value={provider.id}>
                {provider.name} · {provider.default_model}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <form
        onSubmit={handleSubmit}
        className="flex items-end gap-2 rounded-2xl border border-slate-200 bg-white/90 p-2 shadow-lg shadow-slate-900/5 backdrop-blur transition-colors focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/15 dark:border-slate-700 dark:bg-slate-900/90 dark:shadow-black/20"
      >
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
          className="min-h-9 flex-1 resize-none overflow-hidden bg-transparent px-2.5 py-2 text-sm leading-relaxed text-slate-900 placeholder-slate-400 focus:outline-none disabled:opacity-50 dark:text-slate-100 dark:placeholder-slate-500"
        />
        <div className="flex shrink-0 gap-1.5">
          {active && (
            <button
              type="button"
              onClick={onStop}
              className="flex h-9 w-9 items-center justify-center rounded-xl bg-slate-700 text-white transition-colors hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 dark:focus:ring-offset-slate-900"
              aria-label="Stop generation"
            >
              <Square size={16} fill="currentColor" />
            </button>
          )}
          <button
            type="submit"
            disabled={disabled || !value.trim()}
            className="flex h-9 w-9 items-center justify-center rounded-xl bg-blue-600 text-white transition-colors hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-400 dark:focus:ring-offset-slate-900 dark:disabled:bg-slate-800 dark:disabled:text-slate-600"
            aria-label="Send message"
          >
            <SendHorizontal size={16} />
          </button>
        </div>
      </form>
    </div>
  );
}
