import Link from "next/link";
import { FileQuestion } from "lucide-react";

export default function NotFound() {
  return (
    <div className="flex h-screen flex-col items-center justify-center gap-3 px-6 text-center">
      <FileQuestion size={28} className="text-slate-400 dark:text-slate-500" />
      <p className="text-sm font-medium text-slate-900 dark:text-white">Page not found</p>
      <p className="max-w-sm text-xs leading-5 text-slate-500 dark:text-slate-400">
        The page you're looking for doesn't exist or may have moved.
      </p>
      <Link href="/" className="primary-action mt-1 min-h-9 px-3 py-1.5">
        Back to helpcore
      </Link>
    </div>
  );
}
