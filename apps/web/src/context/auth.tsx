'use client';

import { createContext, useCallback, useContext, useEffect, useState } from 'react';
import * as api from '@/lib/api';

interface AuthContextValue {
  accessToken: string | null;
  isLoading: boolean;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  refreshAccessToken: () => Promise<string | null>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const REFRESH_KEY = 'helpcore_refresh';

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const refreshAccessToken = useCallback(async (): Promise<string | null> => {
    const stored = localStorage.getItem(REFRESH_KEY);
    if (!stored) return null;
    try {
      const { access_token, refresh_token } = await api.refresh(stored);
      localStorage.setItem(REFRESH_KEY, refresh_token);
      setAccessToken(access_token);
      return access_token;
    } catch {
      localStorage.removeItem(REFRESH_KEY);
      setAccessToken(null);
      return null;
    }
  }, []);

  useEffect(() => {
    refreshAccessToken().finally(() => setIsLoading(false));
  }, [refreshAccessToken]);

  const login = useCallback(async (email: string, password: string) => {
    const { access_token, refresh_token } = await api.login(email, password);
    localStorage.setItem(REFRESH_KEY, refresh_token);
    setAccessToken(access_token);
  }, []);

  const logout = useCallback(async () => {
    const stored = localStorage.getItem(REFRESH_KEY);
    if (stored && accessToken) {
      try { await api.logout(stored, accessToken); } catch {}
    }
    localStorage.removeItem(REFRESH_KEY);
    setAccessToken(null);
  }, [accessToken]);

  return (
    <AuthContext.Provider value={{ accessToken, isLoading, login, logout, refreshAccessToken }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}
