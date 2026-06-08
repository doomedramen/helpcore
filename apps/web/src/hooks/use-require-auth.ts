"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/context/auth";
import type { CurrentUser } from "@/lib/types";

interface UseRequireAuthOptions {
  /**
   * If set, the page additionally requires `currentUser.role` to equal this
   * value. Authenticated users with the wrong role are sent to
   * `roleRedirectPath` instead of `loginPath`.
   */
  role?: CurrentUser["role"];
  /** Where unauthenticated visitors are sent. Defaults to "/login/". */
  loginPath?: string;
  /** Where authenticated-but-wrong-role users are sent. Defaults to "/chat/". */
  roleRedirectPath?: string;
  /**
   * When `false`, the hook performs no checks and no redirects, and `ready`
   * is always `true`. Defaults to `true`.
   *
   * This exists for {@link AuthGate}: it must call this hook unconditionally
   * on every route (rules of hooks), but should only actually *enforce*
   * auth on routes that aren't in its public allowlist (e.g. not on
   * `/login/`, where redirecting a signed-out visitor to `/login/` would be
   * a pointless no-op, and on `/setup/`, where it would be an active bug —
   * there is by definition no session yet).
   */
  enabled?: boolean;
}

interface UseRequireAuthResult {
  accessToken: string | null;
  currentUser: CurrentUser | null;
  isLoading: boolean;
  /**
   * True once auth has resolved, the user is signed in, and (if `role` was
   * given) holds the right role — i.e. it's safe to render the protected
   * page body. False while auth is still loading or a redirect is in flight,
   * so callers should render a loading state (or `null`) until this flips.
   */
  ready: boolean;
}

/**
 * Centralises the "redirect to /login if signed out (and optionally to a
 * fallback page if signed in with the wrong role)" guard.
 *
 * Important: the redirect MUST happen in an effect, not during render —
 * calling `router.replace` directly in the component body triggers "Cannot
 * update a component while rendering a different component" and can fire on
 * every re-render before navigation completes. This hook owns that effect so
 * callers can't get it wrong (this used to be hand-rolled, inconsistently,
 * on every protected page — see git history for the bugs that caused).
 *
 * Most pages don't need to call this directly any more — {@link AuthGate}
 * (mounted once, in the root layout) already redirects signed-out visitors
 * away from every route that isn't on its public allowlist, so a page that
 * renders at all is guaranteed to have a session. Reach for this hook only
 * when a page has *additional*, page-specific requirements on top of "is
 * signed in" — the main case being a role check:
 *
 *   const { ready } = useRequireAuth({ role: "admin" });
 *   if (!ready) return <Loading />; // brief redirect-in-flight state
 *   ... render the admin-only page
 *
 * (`AuthGate` itself is the other caller — see the `enabled` option.)
 */
export function useRequireAuth(options: UseRequireAuthOptions = {}): UseRequireAuthResult {
  const { role, loginPath = "/login/", roleRedirectPath = "/chat/", enabled = true } = options;
  const { accessToken, currentUser, isLoading } = useAuth();
  const router = useRouter();

  const roleMismatch = enabled && role !== undefined && currentUser?.role !== role;

  useEffect(() => {
    if (!enabled || isLoading) return;
    if (!accessToken) {
      router.replace(loginPath);
    } else if (roleMismatch) {
      router.replace(roleRedirectPath);
    }
  }, [enabled, accessToken, isLoading, roleMismatch, loginPath, roleRedirectPath, router]);

  return {
    accessToken,
    currentUser,
    isLoading,
    ready: !enabled || (!isLoading && !!accessToken && !roleMismatch),
  };
}
