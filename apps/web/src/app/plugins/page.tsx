"use client";

import { useRouter } from "next/navigation";
import PluginManager from "@/app/components/plugin-manager";
import PageHeader from "@/app/components/page-header";
import AppShell from "@/app/components/app-shell";
import SettingsNav from "@/app/components/settings-nav";
import { useAuth } from "@/context/auth";

export default function PluginsPage() {
  const { accessToken } = useAuth();
  const router = useRouter();

  // AuthGate guarantees a session is present once this page renders; this
  // check exists purely to narrow `accessToken` from `string | null` to
  // `string` for PluginManager below (it should never actually return null).
  if (!accessToken) return null;

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
