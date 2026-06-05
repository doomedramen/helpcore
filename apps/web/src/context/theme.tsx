'use client';

import { createContext, useContext, useEffect, useState } from 'react';

type Theme = 'light' | 'dark';

interface ThemeContextValue {
  theme: Theme | null;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme | null>(null);

  useEffect(() => {
    setTheme(document.documentElement.classList.contains('dark') ? 'dark' : 'light');
  }, []);

  function toggleTheme() {
    const root = document.documentElement;
    const nextTheme: Theme = root.classList.contains('dark') ? 'light' : 'dark';

    root.classList.toggle('dark', nextTheme === 'dark');
    root.style.colorScheme = nextTheme;
    localStorage.setItem('helpcore-theme', nextTheme);
    setTheme(nextTheme);
  }

  return (
    <ThemeContext.Provider value={{ theme, toggleTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error('useTheme must be used within ThemeProvider');
  return context;
}
