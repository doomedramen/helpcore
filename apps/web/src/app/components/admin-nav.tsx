"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const LINKS = [
  { href: "/admin/", label: "Server" },
  { href: "/admin/users/", label: "Users" },
];

export default function AdminNav() {
  const pathname = usePathname();
  return (
    <nav className="flex w-full gap-1 overflow-x-auto rounded-xl border border-slate-200/80 bg-white/60 p-1 shadow-sm backdrop-blur dark:border-slate-800 dark:bg-slate-900/60 sm:w-fit">
      {LINKS.map((link) => {
        const active = pathname.startsWith(link.href.replace(/\/$/, ""));
        return (
          <Link
            key={link.href}
            href={link.href}
            className={`shrink-0 rounded-lg px-3 py-2 text-sm font-medium transition ${
              active
                ? "bg-slate-950 text-white shadow-sm dark:bg-white dark:text-slate-950"
                : "text-slate-500 hover:bg-white hover:text-slate-950 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white"
            }`}
          >
            {link.label}
          </Link>
        );
      })}
    </nav>
  );
}
