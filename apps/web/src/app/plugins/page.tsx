"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import PluginManager from "@/app/components/plugin-manager";
import PageHeader from "@/app/components/page-header";
import AppShell from "@/app/components/app-shell";
import SettingsNav from "@/app/components/settings-nav";
import { useAuth } from "@/context/auth";

export default function PluginsPage() {
  const { accessToken, isLoading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (!isLoading && !accessToken) router.replace("/login/");
  }, [accessToken, isLoading, router]);

  if (isLoading || !accessToken) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-50 text-sm text-slate-400 dark:bg-slate-950 dark:text-slate-500">
        Loading…
      </div>
    );
  }

  return (
    <AppShell
      conversationId={null}
      onSelectConversation={(id) => router.push(id ? `/chat/?id=${id}` : "/chat/")}
      mainClassName="overflow-y-auto"
    >
      <div className="mx-auto max-w-5xl px-4 py-7 sm:px-6 sm:py-10">
        <PageHeader
          breadcrumb="Settings"
          title="Plugins"
          description="Install and manage isolated plugin versions for your account."
        />
        <SettingsNav />
        <div className="mt-6">
          <PluginManager accessToken={accessToken} />
        </div>
      </div>
    </AppShell>
  );
}
