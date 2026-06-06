'use client';

import { Monitor, Moon, Sun } from 'lucide-react';
import { useTheme } from 'next-themes';

interface Props {
  className?: string;
  variant?: 'icon' | 'segmented';
}

const OPTIONS = [
  { value: 'light', icon: <Sun size={14} />, label: 'Light' },
  { value: 'dark', icon: <Moon size={14} />, label: 'Dark' },
  { value: 'system', icon: <Monitor size={14} />, label: 'System' },
] as const;

export default function ThemeToggle({ className = '', variant = 'icon' }: Props) {
  const { theme, resolvedTheme, setTheme } = useTheme();

  if (variant === 'segmented') {
    return (
      <div className={`flex items-center justify-between ${className}`}>
        <span className="text-sm text-slate-400">Theme</span>
        <div className="flex rounded-lg bg-slate-800 p-0.5 gap-0.5">
          {OPTIONS.map(opt => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setTheme(opt.value)}
              aria-label={opt.label}
              title={opt.label}
              className={`rounded-md p-1.5 transition-colors ${
                theme === opt.value
                  ? 'bg-slate-600 text-white'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {opt.icon}
            </button>
          ))}
        </div>
      </div>
    );
  }

  const isDark = resolvedTheme === 'dark';
  const label = isDark ? 'Use light mode' : 'Use dark mode';

  return (
    <button
      type="button"
      onClick={() => setTheme(isDark ? 'light' : 'dark')}
      className={className}
      aria-label={label}
      title={label}
    >
      {isDark ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );
}
