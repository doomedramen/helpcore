'use client';

import { FormEvent, KeyboardEvent, useRef, useEffect } from 'react';
import { SendHorizontal, Square } from 'lucide-react';

interface Props {
  onSend: (message: string) => void;
  onStop?: () => void;
  disabled?: boolean;
  active?: boolean;
  value: string;
  onChange: (value: string) => void;
}

export default function ChatInput({ onSend, onStop, disabled, active, value, onChange }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [value]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  function submit() {
    const trimmed = value.trim();
    if (!trimmed || disabled || active) return;
    onSend(trimmed);
    onChange('');
  }

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    submit();
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex items-end gap-2 rounded-2xl border border-slate-200 bg-white/90 p-2 shadow-lg shadow-slate-900/5 backdrop-blur transition-colors focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/15 dark:border-slate-700 dark:bg-slate-900/90 dark:shadow-black/20"
    >
      <textarea
        ref={textareaRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        rows={1}
        placeholder="Message helpcore…"
        className="min-h-9 flex-1 resize-none overflow-hidden bg-transparent px-2.5 py-2 text-sm leading-relaxed text-slate-900 placeholder-slate-400 focus:outline-none disabled:opacity-50 dark:text-slate-100 dark:placeholder-slate-500"
      />
      {active ? (
        <button
          type="button"
          onClick={onStop}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-slate-700 text-white transition-colors hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 dark:focus:ring-offset-slate-900"
          aria-label="Stop generation"
        >
          <Square size={16} fill="currentColor" />
        </button>
      ) : (
        <button
          type="submit"
          disabled={disabled || !value.trim()}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-blue-600 text-white transition-colors hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:bg-slate-100 disabled:text-slate-400 dark:focus:ring-offset-slate-900 dark:disabled:bg-slate-800 dark:disabled:text-slate-600"
          aria-label="Send message"
        >
          <SendHorizontal size={16} />
        </button>
      )}
    </form>
  );
}
