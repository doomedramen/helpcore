'use client';

import { LogOut, MessageSquarePlus, Trash2 } from 'lucide-react';
import useSWR, { useSWRConfig } from 'swr';
import { deleteConversation, listConversations } from '@/lib/api';
import { useAuth } from '@/context/auth';
import type { ConversationSummary } from '@/lib/types';
import { useRouter } from 'next/navigation';

interface Props {
  conversationId: string | null;
  onSelect: (id: string | null) => void;
}

export default function Sidebar({ conversationId, onSelect }: Props) {
  const { accessToken, logout } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const { data: conversations = [] } = useSWR<ConversationSummary[]>(
    accessToken ? ['/api/conversations', accessToken] : null,
    ([, token]) => listConversations(token as string),
    { refreshInterval: 10_000 },
  );

  async function handleDelete(e: React.MouseEvent, id: string) {
    e.stopPropagation();
    if (!accessToken) return;
    await deleteConversation(id, accessToken);
    await mutate(['/api/conversations', accessToken]);
    if (conversationId === id) onSelect(null);
  }

  async function handleLogout() {
    await logout();
    router.replace('/login/');
  }

  function handleNew() {
    onSelect(null);
    router.replace('/chat/', { scroll: false });
  }

  return (
    <aside className="flex flex-col w-64 shrink-0 bg-slate-950 text-slate-300 h-full">
      {/* Logo */}
      <div className="px-4 pt-5 pb-3">
        <span className="text-base font-semibold text-white tracking-tight">helpcore</span>
      </div>

      {/* New chat */}
      <div className="px-3 pb-3">
        <button
          onClick={handleNew}
          className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-slate-300 hover:bg-slate-800 hover:text-white transition-colors"
        >
          <MessageSquarePlus size={16} className="shrink-0" />
          New chat
        </button>
      </div>

      {/* Divider */}
      <div className="mx-3 border-t border-slate-800 mb-2" />

      {/* Conversation list */}
      <nav className="flex-1 overflow-y-auto px-3 space-y-0.5 pb-2">
        {conversations.map(conv => (
          <button
            key={conv.id}
            onClick={() => onSelect(conv.id)}
            className={`group flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors ${
              conv.id === conversationId
                ? 'bg-slate-800 text-white'
                : 'text-slate-400 hover:bg-slate-900 hover:text-slate-200'
            }`}
          >
            <span className="truncate flex-1 leading-snug">{conv.title || 'Untitled'}</span>
            <span
              role="button"
              tabIndex={0}
              onClick={e => handleDelete(e as unknown as React.MouseEvent, conv.id)}
              onKeyDown={e => e.key === 'Enter' && handleDelete(e as unknown as React.MouseEvent, conv.id)}
              className="shrink-0 opacity-0 group-hover:opacity-100 p-0.5 rounded hover:text-red-400 transition-opacity"
              aria-label="Delete conversation"
            >
              <Trash2 size={13} />
            </span>
          </button>
        ))}
      </nav>

      {/* Footer */}
      <div className="mx-3 border-t border-slate-800 mt-2 pt-2 pb-3">
        <button
          onClick={handleLogout}
          className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-slate-400 hover:bg-slate-900 hover:text-slate-200 transition-colors"
        >
          <LogOut size={15} className="shrink-0" />
          Sign out
        </button>
      </div>
    </aside>
  );
}
