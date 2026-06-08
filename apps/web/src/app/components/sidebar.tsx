"use client";

import { useState } from "react";
import { LogOut, Menu, MessageSquarePlus, Settings, Shield, Trash2, X } from "lucide-react";
import useSWR, { useSWRConfig } from "swr";
import { deleteConversation, listConversations } from "@/lib/api";
import { useAuth } from "@/context/auth";
import type { ConversationSummary } from "@/lib/types";
import { usePathname, useRouter } from "next/navigation";
import ThemeToggle from "./theme-toggle";
import BrandMark from "./brand-mark";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/app/components/ui/dialog";

interface Props {
  conversationId: string | null;
  onSelect: (id: string | null) => void;
}

function isSettingsPath(p: string) {
  return (
    p.startsWith("/settings") ||
    p.startsWith("/personality") ||
    p.startsWith("/memory") ||
    p.startsWith("/plugins")
  );
}

export default function Sidebar({ conversationId, onSelect }: Props) {
  const { accessToken, currentUser, logout } = useAuth();
  const router = useRouter();
  const pathname = usePathname();
  const { mutate } = useSWRConfig();
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [mobileOpen, setMobileOpen] = useState(false);

  const { data: conversations = [] } = useSWR<ConversationSummary[]>(
    accessToken ? ["/api/conversations", accessToken] : null,
    ([, token]) => listConversations(token as string),
    { refreshInterval: 10_000 },
  );

  async function handleDeleteConfirm() {
    if (!accessToken || !deleteTarget) return;
    const id = deleteTarget;
    setDeleteTarget(null);
    await deleteConversation(id, accessToken);
    await mutate(["/api/conversations", accessToken]);
    if (conversationId === id) onSelect(null);
  }

  async function handleLogout() {
    setMobileOpen(false);
    await logout();
    router.replace("/login/");
  }

  function handleNew() {
    setMobileOpen(false);
    onSelect(null);
    router.replace("/chat/", { scroll: false });
  }

  function handleNavigate(href: string) {
    setMobileOpen(false);
    router.push(href);
  }

  const sidebarContent = (
    <>
      <div className="flex items-center justify-between px-4 pb-5 pt-5">
        <BrandMark compact inverse />
        <button
          onClick={() => setMobileOpen(false)}
          className="rounded-xl p-2 text-sidebar-foreground/50 transition hover:bg-sidebar-foreground/5 hover:text-sidebar-foreground md:hidden"
          aria-label="Close menu"
        >
          <X size={18} />
        </button>
      </div>

      <div className="px-3 pb-4">
        <button
          onClick={handleNew}
          className="flex w-full items-center justify-center gap-2 rounded-xl bg-sidebar-foreground px-3 py-2.5 text-sm font-semibold text-sidebar shadow-lg shadow-black/10 transition hover:bg-sidebar-foreground/90"
        >
          <MessageSquarePlus size={16} className="shrink-0" />
          New chat
        </button>
      </div>

      <div className="px-5 pb-2 pt-1">
        <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-sidebar-foreground/40">
          Recent
        </p>
      </div>

      <nav className="flex-1 space-y-1 overflow-y-auto px-3 pb-3">
        {conversations.length === 0 && (
          <p className="px-3 py-3 text-xs leading-5 text-sidebar-foreground/40">
            Your recent conversations will appear here.
          </p>
        )}
        {conversations.map((conv) => {
          const active = conv.id === conversationId;
          return (
            <div
              key={conv.id}
              className={`group flex items-center rounded-xl transition ${
                active
                  ? "bg-sidebar-foreground/10 text-sidebar-foreground"
                  : "text-sidebar-foreground/60 hover:bg-sidebar-foreground/[0.055] hover:text-sidebar-foreground/85"
              }`}
            >
              <button
                onClick={() => {
                  setMobileOpen(false);
                  onSelect(conv.id);
                }}
                className="min-w-0 flex-1 px-3 py-2.5 text-left text-sm"
              >
                <span className="block truncate leading-snug">{conv.title || "Untitled"}</span>
              </button>
              <button
                onClick={() => setDeleteTarget(conv.id)}
                className="mr-1.5 shrink-0 rounded-lg p-1.5 text-sidebar-foreground/40 opacity-70 transition hover:bg-red-500/10 hover:text-red-300 md:opacity-0 md:group-hover:opacity-100"
                aria-label="Delete conversation"
              >
                <Trash2 size={13} />
              </button>
            </div>
          );
        })}
      </nav>

      <div className="mx-3 border-t border-sidebar-foreground/[0.08] px-1 pb-3 pt-3">
        <div className="mb-2 flex items-center gap-2.5 px-2">
          <div className="grid size-8 shrink-0 place-items-center rounded-full bg-gradient-to-br from-indigo-400/30 to-teal-400/20 text-xs font-semibold text-sidebar-foreground ring-1 ring-sidebar-foreground/10">
            {(currentUser?.display_name ?? currentUser?.email ?? "H").charAt(0).toUpperCase()}
          </div>
          <div className="min-w-0">
            <p className="truncate text-xs font-medium text-sidebar-foreground/85">
              {currentUser?.display_name ?? "Your workspace"}
            </p>
            <p className="truncate text-[11px] text-sidebar-foreground/40">{currentUser?.email}</p>
          </div>
        </div>

        <button
          onClick={() => handleNavigate("/settings/")}
          className={`flex w-full items-center gap-2 rounded-xl px-3 py-2 text-sm transition ${
            isSettingsPath(pathname)
              ? "bg-sidebar-foreground/10 text-sidebar-foreground"
              : "text-sidebar-foreground/50 hover:bg-sidebar-foreground/[0.055] hover:text-sidebar-foreground/85"
          }`}
        >
          <Settings size={15} className="shrink-0" />
          Settings
        </button>
        {currentUser?.role === "admin" && (
          <button
            onClick={() => handleNavigate("/admin/")}
            className={`flex w-full items-center gap-2 rounded-xl px-3 py-2 text-sm transition ${
              pathname.startsWith("/admin")
                ? "bg-sidebar-foreground/10 text-sidebar-foreground"
                : "text-sidebar-foreground/50 hover:bg-sidebar-foreground/[0.055] hover:text-sidebar-foreground/85"
            }`}
          >
            <Shield size={15} className="shrink-0" />
            Admin
          </button>
        )}
        <ThemeToggle variant="segmented" className="px-3 py-2" />
        <button
          onClick={handleLogout}
          className="flex w-full items-center gap-2 rounded-xl px-3 py-2 text-sm text-sidebar-foreground/50 transition hover:bg-sidebar-foreground/[0.055] hover:text-sidebar-foreground/85"
        >
          <LogOut size={15} className="shrink-0" />
          Sign out
        </button>
      </div>
    </>
  );

  return (
    <>
      <div className="fixed inset-x-0 top-0 z-30 flex h-14 items-center justify-between border-b border-slate-200/70 bg-white/80 px-3 backdrop-blur-xl dark:border-slate-800 dark:bg-slate-950/80 md:hidden">
        <BrandMark compact />
        <button
          onClick={() => setMobileOpen(true)}
          className="icon-button size-9"
          aria-label="Open menu"
        >
          <Menu size={18} />
        </button>
      </div>

      {mobileOpen && (
        <div
          className="fixed inset-0 z-40 bg-slate-950/60 backdrop-blur-sm md:hidden"
          onClick={() => setMobileOpen(false)}
        />
      )}

      <aside
        className={`h-full w-[17rem] shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground shadow-2xl shadow-slate-950/10 ${
          mobileOpen ? "fixed inset-y-0 left-0 z-50 flex" : "hidden"
        } md:relative md:flex`}
      >
        {sidebarContent}
      </aside>

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete conversation</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete this conversation? This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button
              onClick={() => setDeleteTarget(null)}
              className="secondary-action min-h-9 px-3 py-1.5"
            >
              Cancel
            </button>
            <button
              onClick={handleDeleteConfirm}
              className="inline-flex min-h-9 items-center justify-center rounded-xl bg-red-600 px-3 py-1.5 text-sm font-semibold text-white transition hover:bg-red-500"
            >
              Delete
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
