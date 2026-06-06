"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const LINKS = [
  { href: "/plugins/", label: "Plugins" },
  { href: "/personality/", label: "Personality" },
  { href: "/memory/", label: "Memory" },
  { href: "/settings/api-keys/", label: "API keys" },
];

export default function SettingsNav() {
  const pathname = usePathname();
  return (
    <nav className="-mb-px flex border-b border-slate-200 dark:border-slate-800">
      {LINKS.map((link) => {
        const active = pathname.startsWith(link.href.replace(/\/$/, ""));
        return (
          <Link
            key={link.href}
            href={link.href}
            className={`border-b-2 px-4 pb-2.5 pt-1 text-sm font-medium transition-colors ${
              active
                ? "border-blue-600 text-blue-600 dark:border-blue-400 dark:text-blue-400"
                : "border-transparent text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
            }`}
          >
            {link.label}
          </Link>
        );
      })}
    </nav>
  );
}
