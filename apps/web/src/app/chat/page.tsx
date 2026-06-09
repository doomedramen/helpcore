"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import ChatWindow from "@/app/components/chat-window";
import AppShell from "@/app/components/app-shell";
import StreamdownCodeBlockHandlers from "@/app/components/streamdown-code-block-handlers";

function ChatApp() {
  const router = useRouter();
  const params = useSearchParams();
  const rawId = params.get("id");
  const [conversationId, setConversationId] = useState<string | null>(rawId || null);

  // Sync conversationId with URL param
  useEffect(() => {
    setConversationId(params.get("id") || null);
  }, [params]);

  // No auth guard needed here — AuthGate (in the root layout) only renders
  // this page once a session is confirmed.
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
        onConversationNotFound={() => {
          setConversationId(null);
          router.replace("/chat/", { scroll: false });
        }}
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
