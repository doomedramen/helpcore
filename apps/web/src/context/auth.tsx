"use client";

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { SWRConfig } from "swr";
import * as api from "@/lib/api";
import type { CurrentUser } from "@/lib/types";

interface AuthContextValue {
  accessToken: string | null;
  currentUser: CurrentUser | null;
  isLoading: boolean;
  forcePasswordChange: boolean;
  login: (email: string, password: string) => Promise<{ forcePasswordChange: boolean }>;
  logout: () => Promise<void>;
  refreshAccessToken: () => Promise<string | null>;
  clearForcePasswordChange: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const REFRESH_KEY = "helpcore_refresh";

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [currentUser, setCurrentUser] = useState<CurrentUser | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [forcePasswordChange, setForcePasswordChange] = useState(false);

  const loadCurrentUser = useCallback(async (token: string) => {
    const user = await api.getCurrentUser(token);
    setCurrentUser(user);
    return user;
  }, []);

  // Refresh tokens are single-use and rotated server-side, so concurrent
  // refreshes (e.g. several stale SWR requests failing at once) must share
  // one in-flight request — otherwise the second call reuses an
  // already-rotated token and gets logged out.
  const refreshInFlightRef = useRef<Promise<string | null> | null>(null);

  const refreshAccessToken = useCallback((): Promise<string | null> => {
    if (refreshInFlightRef.current) return refreshInFlightRef.current;

    const run = async (): Promise<string | null> => {
      const stored = localStorage.getItem(REFRESH_KEY);
      if (!stored) {
        setCurrentUser(null);
        return null;
      }
      try {
        const { access_token, refresh_token } = await api.refresh(stored);
        localStorage.setItem(REFRESH_KEY, refresh_token);
        setAccessToken(access_token);
        await loadCurrentUser(access_token);
        return access_token;
      } catch {
        localStorage.removeItem(REFRESH_KEY);
        setAccessToken(null);
        setCurrentUser(null);
        return null;
      }
    };

    const promise = run().finally(() => {
      refreshInFlightRef.current = null;
    });
    refreshInFlightRef.current = promise;
    return promise;
  }, [loadCurrentUser]);

  useEffect(() => {
    refreshAccessToken().finally(() => setIsLoading(false));
  }, [refreshAccessToken]);

  const login = useCallback(
    async (email: string, password: string) => {
      const { access_token, refresh_token, force_password_change } = await api.login(
        email,
        password,
      );
      localStorage.setItem(REFRESH_KEY, refresh_token);
      setAccessToken(access_token);
      setForcePasswordChange(force_password_change ?? false);
      try {
        await loadCurrentUser(access_token);
      } catch (error) {
        localStorage.removeItem(REFRESH_KEY);
        setAccessToken(null);
        setCurrentUser(null);
        setForcePasswordChange(false);
        throw error;
      }
      return { forcePasswordChange: force_password_change ?? false };
    },
    [loadCurrentUser],
  );

  const logout = useCallback(async () => {
    const stored = localStorage.getItem(REFRESH_KEY);
    if (stored && accessToken) {
      try {
        await api.logout(stored, accessToken);
      } catch {}
    }
    localStorage.removeItem(REFRESH_KEY);
    setAccessToken(null);
    setCurrentUser(null);
    setForcePasswordChange(false);
  }, [accessToken]);

  const clearForcePasswordChange = useCallback(() => {
    setForcePasswordChange(false);
  }, []);

  return (
    <AuthContext.Provider
      value={{
        accessToken,
        currentUser,
        isLoading,
        forcePasswordChange,
        login,
        logout,
        refreshAccessToken,
        clearForcePasswordChange,
      }}
    >
      <StaleTokenRecovery refreshAccessToken={refreshAccessToken}>{children}</StaleTokenRecovery>
    </AuthContext.Provider>
  );
}

// SWR cache keys throughout the app are `[url, accessToken]`, so once
// `refreshAccessToken` swaps in a new token, every hook automatically
// re-fetches under its new key. This bridge is what notices a fetch failed
// with a stale token (e.g. a `revalidateOnFocus` request after the 15-minute
// access token expired while the tab was in the background) and kicks off
// that refresh — without it, the "authentication required" error from the
// expired request just sits there until a full page reload re-runs auth.
function StaleTokenRecovery({
  refreshAccessToken,
  children,
}: {
  refreshAccessToken: () => Promise<string | null>;
  children: React.ReactNode;
}) {
  return (
    <SWRConfig
      value={{
        onError: (error) => {
          if (error instanceof api.ApiError && error.status === 401) {
            void refreshAccessToken();
          }
        },
      }}
    >
      {children}
    </SWRConfig>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
