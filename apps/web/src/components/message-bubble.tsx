'use client';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import type { Message } from '@/lib/types';

import 'highlight.js/styles/github.css';

interface Props {
  message: Message;
  onRetry?: (messageId: string) => void;
  retrying?: boolean;
}

export default function MessageBubble({ message, onRetry, retrying = false }: Props) {
  const isUser = message.role === 'user';

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded-2xl rounded-tr-sm bg-blue-600 px-4 py-2.5 text-sm text-white shadow-sm">
          <p className="whitespace-pre-wrap leading-relaxed">{message.content}</p>
        </div>
      </div>
    );
  }

  const active = message.status === 'pending' || message.status === 'streaming';
  const retryable = message.status === 'failed' || message.status === 'interrupted';

  return (
    <div className="flex justify-start">
      <div className="max-w-[85%] rounded-2xl rounded-tl-sm border border-slate-200 bg-white px-4 py-3 text-sm text-slate-900 shadow-sm dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100">
        {active && !message.content ? (
          <span className="flex gap-1 items-center py-0.5">
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:0ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:150ms]" />
            <span className="w-1.5 h-1.5 rounded-full bg-slate-400 animate-bounce [animation-delay:300ms]" />
          </span>
        ) : (
          <div className="prose prose-sm max-w-none">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeHighlight]}
            >
              {message.content}
            </ReactMarkdown>
          </div>
        )}
        {active && message.content && (
          <p className="mt-2 text-xs text-slate-400 dark:text-slate-500">Still working…</p>
        )}
        {retryable && (
          <div className="mt-3 border-t border-slate-200 pt-3 dark:border-slate-700">
            <p className="text-xs text-red-600 dark:text-red-400">
              {message.error || 'This response did not finish.'}
            </p>
            {onRetry && (
              <button
                type="button"
                onClick={() => onRetry(message.id)}
                disabled={retrying}
                className="mt-2 rounded-lg border border-slate-300 px-2.5 py-1.5 text-xs font-medium text-slate-700 transition-colors hover:bg-slate-50 disabled:cursor-wait disabled:opacity-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
              >
                {retrying ? 'Retrying…' : 'Retry response'}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
