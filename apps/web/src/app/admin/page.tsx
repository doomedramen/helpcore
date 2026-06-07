"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import useSWR from "swr";
import AdminNav from "@/app/components/admin-nav";
import AppShell from "@/app/components/app-shell";
import ConfigForm from "@/app/components/admin/config-form";
import PageHeader from "@/app/components/page-header";
import StatusMessage from "@/app/components/status-message";
import { useAuth } from "@/context/auth";
import { getAdminConfig } from "@/lib/api";

export default function AdminPage() {
  const { accessToken, currentUser, isLoading } = useAuth();
  const router = useRouter();
  const {
    data: config,
    error,
    mutate,
  } = useSWR(
    accessToken && currentUser?.role === "admin" ? ["/api/admin/config", accessToken] : null,
    ([, token]) => getAdminConfig(token),
    {
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
    },
  );

  useEffect(() => {
    if (isLoading) return;
    if (!accessToken) router.replace("/login/");
    else if (currentUser?.role !== "admin") router.replace("/chat/");
  }, [accessToken, currentUser, isLoading, router]);

  if (isLoading || !accessToken || currentUser?.role !== "admin") {
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
          breadcrumb="Administration"
          title="Server settings"
          description="Configure server-wide settings, providers, registry policy, and plugin blacklist."
          accent
        />

        <AdminNav />

        <div className="mt-6">
          {error && <StatusMessage type="error" message={error.message} />}
          {!error && !config && (
            <div className="py-16 text-center text-sm text-slate-400">Loading configuration…</div>
          )}
          {config && (
            <ConfigForm
              accessToken={accessToken}
              config={config}
              onSaved={(updated) => mutate(updated, false)}
            />
          )}
        </div>
      </div>
    </AppShell>
  );
}
