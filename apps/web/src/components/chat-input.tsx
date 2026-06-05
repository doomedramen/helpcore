'use client';

import { FormEvent, KeyboardEvent, useRef, useEffect } from 'react';
import { SendHorizontal } from 'lucide-react';

interface Props {
  onSend: (message: string) => void;
  disabled?: boolean;
  value: string;
  onChange: (value: string) => void;
}

export default function ChatInput({ onSend, disabled, value, onChange }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
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
    if (!trimmed || disabled) return;
    onSend(trimmed);
    onChange('');
  }

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    submit();
  }

  return (
    <form onSubmit={handleSubmit} className="flex items-end gap-2 border-t border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-950">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        rows={1}
        placeholder="Message helpcore… (Enter to send, Shift+Enter for newline)"
        className="flex-1 resize-none overflow-hidden rounded-xl border border-slate-300 bg-white px-3.5 py-2.5 text-sm leading-relaxed text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:opacity-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100 dark:placeholder-slate-500"
      />
      <button
        type="submit"
        disabled={disabled || !value.trim()}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-blue-600 text-white transition-colors hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-40 dark:focus:ring-offset-slate-950"
        aria-label="Send message"
      >
        <SendHorizontal size={16} />
      </button>
    </form>
  );
}
