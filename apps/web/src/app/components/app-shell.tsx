"use client";

import Sidebar from "@/app/components/sidebar";
import { cn } from "@/lib/utils";

interface AppShellProps {
  children: React.ReactNode;
  conversationId: string | null;
  onSelectConversation: (id: string | null) => void;
  mainClassName?: string;
}

export default function AppShell({
  children,
  conversationId,
  onSelectConversation,
  mainClassName,
}: AppShellProps) {
  return (
    <div className="app-canvas flex h-dvh overflow-hidden">
      <Sidebar conversationId={conversationId} onSelect={onSelectConversation} />
      <main className={cn("min-w-0 flex-1 overflow-hidden pt-14 md:pt-0", mainClassName)}>
        {children}
      </main>
    </div>
  );
}
