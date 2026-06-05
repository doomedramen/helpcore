'use client';

import { Suspense, useEffect, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { useAuth } from '@/context/auth';
import Sidebar from '@/components/sidebar';
import ChatWindow from '@/components/chat-window';

function ChatApp() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();
  const params = useSearchParams();
  const [conversationId, setConversationId] = useState<string | null>(params.get('id'));

  // Sync conversationId with URL param
  useEffect(() => {
    setConversationId(params.get('id'));
  }, [params]);

  useEffect(() => {
    if (!isLoading && !accessToken) {
      router.replace('/login/');
    }
  }, [accessToken, isLoading, router]);

  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-50 dark:bg-slate-950">
        <div className="text-sm text-slate-400 dark:text-slate-500">Loading…</div>
      </div>
    );
  }

  if (!accessToken) return null;

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={conversationId}
        onSelect={id => {
          setConversationId(id);
          const url = id ? `/chat/?id=${id}` : '/chat/';
          router.replace(url, { scroll: false });
        }}
      />
      <main className="flex-1 flex flex-col overflow-hidden">
        <ChatWindow
          conversationId={conversationId}
          onConversationCreated={id => setConversationId(id)}
        />
      </main>
    </div>
  );
}

export default function ChatPage() {
  return (
    <Suspense>
      <ChatApp />
    </Suspense>
  );
}
