'use client';

import { Moon, Sun } from 'lucide-react';
import { useTheme } from '@/context/theme';

interface Props {
  className?: string;
  showLabel?: boolean;
}

export default function ThemeToggle({ className = '', showLabel = false }: Props) {
  const { theme, toggleTheme } = useTheme();
  const isDark = theme === 'dark';
  const label = isDark ? 'Use light mode' : 'Use dark mode';

  return (
    <button
      type="button"
      onClick={toggleTheme}
      className={className}
      aria-label={label}
      title={label}
    >
      <span className={theme === null ? 'opacity-0' : ''}>
        {isDark ? <Sun size={16} /> : <Moon size={16} />}
      </span>
      {showLabel && <span>{isDark ? 'Light mode' : 'Dark mode'}</span>}
    </button>
  );
}
