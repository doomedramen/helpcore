"use client";

import { usePathname } from "next/navigation";
import { useRequireAuth } from "@/hooks/use-require-auth";

/**
 * Top-level routes that render without a session — the *only* pages that
 * intentionally skip the gate below. Matched by exact path or as a prefix
 * (so e.g. nested routes under `/login/` would also be public).
 *
 *   "/login/"    — used to *obtain* a session; gating it would be a no-op
 *                  at best and a redirect loop at worst
 *   "/setup/"    — first-admin bootstrap; runs before any account exists,
 *                  so there is by definition no session to require
 *   "/~offline/" — PWA offline fallback, rendered by the service worker
 *                  with no network (and therefore no auth) available
 *
 * "/" is handled separately below — it's a tiny redirector that sends you
 * to /chat/ or /login/ depending on whether you're signed in, so it must
 * render for both signed-in and signed-out visitors.
 *
 * Keep this in sync with `app/`: any new top-level page that must be
 * reachable while signed out belongs here. Everything else is gated.
 */
const PUBLIC_ROUTES = ["/login/", "/setup/", "/~offline/"];

function isPublicRoute(pathname: string): boolean {
  if (pathname === "/") return true;
  return PUBLIC_ROUTES.some((route) => pathname === route || pathname.startsWith(route));
}

/**
 * App-wide auth gate. Mounted once in the root layout (inside
 * `AuthProvider`), wrapping every route — this is the single place that
 * decides whether the current page requires a session, and redirects
 * signed-out visitors to `/login/` before any page-level code runs.
 *
 * Replaces the old pattern where every protected page hand-rolled (and
 * sometimes botched — see git history) its own "redirect if signed out"
 * `useEffect`. Protected pages can now assume `useAuth().accessToken` is
 * present the instant they render; they don't need their own loading or
 * redirect guards for plain "is signed in" checks any more.
 *
 * Admin-only pages still layer their own `useRequireAuth({ role: "admin" })`
 * on top — *which* session is valid for a route is page-specific in a way
 * that doesn't belong in a global allowlist, so that stays local.
 */
export function AuthGate({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const requiresAuth = !isPublicRoute(pathname);

  // Always called — hooks can't be conditional — but inert (no redirect, no
  // gating) on public routes via `enabled`.
  const { ready } = useRequireAuth({ enabled: requiresAuth });

  if (!requiresAuth || ready) {
    return <>{children}</>;
  }

  return (
    <div className="flex h-screen items-center justify-center text-sm text-slate-400">Loading…</div>
  );
}
