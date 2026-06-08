"use client";

import { useEffect } from "react";
import { CircleAlert } from "lucide-react";

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <div className="flex h-screen flex-col items-center justify-center gap-3 px-6 text-center">
      <CircleAlert size={28} className="text-red-500" />
      <p className="text-sm font-medium text-slate-900 dark:text-white">Something went wrong.</p>
      <p className="max-w-sm text-xs leading-5 text-slate-500 dark:text-slate-400">
        {error.message || "An unexpected error occurred."}
      </p>
      <button onClick={reset} className="primary-action mt-1 min-h-9 px-3 py-1.5">
        Try again
      </button>
    </div>
  );
}
