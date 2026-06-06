"use client";

import { createContext, useCallback, useContext, useEffect, useState } from "react";
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

  const refreshAccessToken = useCallback(async (): Promise<string | null> => {
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
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
