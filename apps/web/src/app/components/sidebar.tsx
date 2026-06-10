"use client";

import { useEffect, useRef, useState } from "react";
import {
  LogOut,
  Menu,
  MessageSquarePlus,
  Pencil,
  Search,
  Settings,
  Shield,
  Trash2,
  X,
} from "lucide-react";
import useSWR, { useSWRConfig } from "swr";
import { deleteConversation, listConversations, renameConversation } from "@/lib/api";
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

const DAY_MS = 86_400_000;
const GROUP_ORDER = ["Today", "Yesterday", "Previous 7 days", "Previous 30 days", "Older"] as const;

function groupLabel(updatedAt: string, startOfToday: number): (typeof GROUP_ORDER)[number] {
  const t = new Date(updatedAt).getTime();
  if (Number.isNaN(t) || t >= startOfToday) return "Today";
  if (t >= startOfToday - DAY_MS) return "Yesterday";
  if (t >= startOfToday - 7 * DAY_MS) return "Previous 7 days";
  if (t >= startOfToday - 30 * DAY_MS) return "Previous 30 days";
  return "Older";
}

function groupConversations(conversations: ConversationSummary[]) {
  const startOfToday = new Date().setHours(0, 0, 0, 0);
  const groups = new Map<string, ConversationSummary[]>();
  for (const conv of conversations) {
    const label = groupLabel(conv.updated_at, startOfToday);
    const bucket = groups.get(label);
    if (bucket) bucket.push(conv);
    else groups.set(label, [conv]);
  }
  return GROUP_ORDER.filter((label) => groups.has(label)).map(
    (label) => [label, groups.get(label)!] as const,
  );
}

export default function Sidebar({ conversationId, onSelect }: Props) {
  const { accessToken, currentUser, logout } = useAuth();
  const router = useRouter();
  const pathname = usePathname();
  const { mutate } = useSWRConfig();
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");
  const [shortcutHint, setShortcutHint] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setShortcutHint(/Mac|iPhone|iPad/.test(navigator.userAgent) ? "⌘K" : "Ctrl K");
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (window.matchMedia("(max-width: 767px)").matches) setMobileOpen(true);
        requestAnimationFrame(() => searchInputRef.current?.focus());
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const { data: conversations = [] } = useSWR<ConversationSummary[]>(
    accessToken ? ["/api/conversations", accessToken] : null,
    ([, token]) => listConversations(token as string),
    { refreshInterval: 10_000 },
  );

  const filteredConversations = search.trim()
    ? conversations.filter((conv) =>
        (conv.title || "Untitled").toLowerCase().includes(search.trim().toLowerCase()),
      )
    : conversations;

  // Guards against double-commit: closing the editor (Enter/Escape) can fire the
  // input's blur handler with a stale closure before React re-renders.
  const renameSettledRef = useRef(false);

  function startRename(conv: ConversationSummary) {
    renameSettledRef.current = false;
    setEditingId(conv.id);
    setEditingTitle(conv.title || "Untitled");
  }

  function cancelRename() {
    renameSettledRef.current = true;
    setEditingId(null);
  }

  async function commitRename() {
    if (renameSettledRef.current) return;
    renameSettledRef.current = true;
    const id = editingId;
    const title = editingTitle.trim();
    setEditingId(null);
    if (!accessToken || !id || !title) return;
    const current = conversations.find((c) => c.id === id);
    if (!current || title === (current.title || "Untitled")) return;
    try {
      await renameConversation(id, title, accessToken);
    } finally {
      await mutate(["/api/conversations", accessToken]);
    }
  }

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
        <BrandMark compact />
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

      {conversations.length > 0 && (
        <div className="relative px-3 pb-2">
          <Search
            size={13}
            className="pointer-events-none absolute left-6 top-1/2 -translate-y-1/2 text-sidebar-foreground/35"
          />
          <input
            ref={searchInputRef}
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search conversations…"
            className="w-full rounded-xl border border-sidebar-foreground/10 bg-sidebar-foreground/5 py-2 pr-12 pl-8 text-sm text-sidebar-foreground outline-none transition placeholder:text-sidebar-foreground/35 focus:border-sidebar-foreground/20 focus:bg-sidebar-foreground/10"
          />
          {shortcutHint && (
            <kbd className="pointer-events-none absolute right-6 top-1/2 hidden -translate-y-1/2 rounded border border-sidebar-foreground/15 px-1.5 py-0.5 font-sans text-[10px] text-sidebar-foreground/35 md:block">
              {shortcutHint}
            </kbd>
          )}
        </div>
      )}

      <nav className="flex-1 space-y-1 overflow-y-auto px-3 pb-3 pt-1">
        {conversations.length === 0 && (
          <p className="px-3 py-3 text-xs leading-5 text-sidebar-foreground/40">
            Your recent conversations will appear here.
          </p>
        )}
        {conversations.length > 0 && filteredConversations.length === 0 && (
          <p className="px-3 py-3 text-xs leading-5 text-sidebar-foreground/40">
            No conversations match “{search.trim()}”.
          </p>
        )}
        {groupConversations(filteredConversations).map(([label, group]) => (
          <div key={label} className="pt-3 first:pt-1">
            <p className="px-2 pb-2 text-[10px] font-semibold uppercase tracking-[0.18em] text-sidebar-foreground/40">
              {label}
            </p>
            <div className="space-y-1">
              {group.map((conv) => {
                const active = conv.id === conversationId;
                const editing = editingId === conv.id;
                return (
                  <div
                    key={conv.id}
                    className={`group flex items-center rounded-xl transition ${
                      active
                        ? "bg-sidebar-foreground/10 text-sidebar-foreground"
                        : "text-sidebar-foreground/60 hover:bg-sidebar-foreground/[0.055] hover:text-sidebar-foreground/85"
                    }`}
                  >
                    {editing ? (
                      <input
                        autoFocus
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            void commitRename();
                          } else if (e.key === "Escape") {
                            cancelRename();
                          }
                        }}
                        onBlur={() => void commitRename()}
                        aria-label="Conversation title"
                        className="m-1 min-w-0 flex-1 rounded-lg border border-sidebar-foreground/20 bg-sidebar-foreground/5 px-2 py-1.5 text-sm text-sidebar-foreground outline-none focus:border-sidebar-foreground/35"
                      />
                    ) : (
                      <>
                        <button
                          onClick={() => {
                            setMobileOpen(false);
                            onSelect(conv.id);
                          }}
                          className="min-w-0 flex-1 px-3 py-2.5 text-left text-sm"
                        >
                          <span className="block truncate leading-snug">
                            {conv.title || "Untitled"}
                          </span>
                        </button>
                        <button
                          onClick={() => startRename(conv)}
                          className="shrink-0 rounded-lg p-1.5 text-sidebar-foreground/40 opacity-70 transition hover:bg-sidebar-foreground/10 hover:text-sidebar-foreground/85 md:opacity-0 md:group-hover:opacity-100"
                          aria-label="Rename conversation"
                        >
                          <Pencil size={13} />
                        </button>
                        <button
                          onClick={() => setDeleteTarget(conv.id)}
                          className="mr-1.5 shrink-0 rounded-lg p-1.5 text-sidebar-foreground/40 opacity-70 transition hover:bg-red-500/10 hover:text-red-300 md:opacity-0 md:group-hover:opacity-100"
                          aria-label="Delete conversation"
                        >
                          <Trash2 size={13} />
                        </button>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ))}
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
