"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useAuth } from "@/context/auth";
import ChatWindow from "@/app/components/chat-window";
import AppShell from "@/app/components/app-shell";
import StreamdownCodeBlockHandlers from "@/app/components/streamdown-code-block-handlers";

function ChatApp() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();
  const params = useSearchParams();
  const [conversationId, setConversationId] = useState<string | null>(params.get("id"));

  // Sync conversationId with URL param
  useEffect(() => {
    setConversationId(params.get("id"));
  }, [params]);

  useEffect(() => {
    if (!isLoading && !accessToken) {
      router.replace("/login/");
    }
  }, [accessToken, isLoading, router]);

  if (isLoading) {
    return (
      <div className="app-canvas flex h-dvh items-center justify-center">
        <div className="text-sm text-slate-400 dark:text-slate-500">Loading…</div>
      </div>
    );
  }

  if (!accessToken) return null;

  return (
    <AppShell
      conversationId={conversationId}
      onSelectConversation={(id) => {
        setConversationId(id);
        const url = id ? `/chat/?id=${id}` : "/chat/";
        router.replace(url, { scroll: false });
      }}
      mainClassName="flex flex-col"
    >
      <StreamdownCodeBlockHandlers />
      <ChatWindow
        conversationId={conversationId}
        onConversationCreated={(id) => setConversationId(id)}
      />
    </AppShell>
  );
}

export default function ChatPage() {
  return (
    <Suspense>
      <ChatApp />
    </Suspense>
  );
}
