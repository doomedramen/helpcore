"use client";

import { FormEvent, Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ApiError, setupAdmin, setupStatus } from "@/lib/api";
import { useAuth } from "@/context/auth";
import ThemeToggle from "@/app/components/theme-toggle";

function SetupWizard() {
  const params = useSearchParams();
  const router = useRouter();
  const { login } = useAuth();

  const token = params.get("token") ?? "";

  const [available, setAvailable] = useState<boolean | null>(null);
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setupStatus()
      .then((s) => setAvailable(s.setup_required))
      .catch(() => setAvailable(false));
  }, []);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (password !== confirm) {
      setError("Passwords do not match.");
      return;
    }
    setError("");
    setLoading(true);
    try {
      await setupAdmin({
        token,
        email,
        password,
        display_name: displayName || undefined,
      });
      // Log in with the new credentials so we have a session
      await login(email, password);
      router.replace("/chat/");
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Setup failed.");
    } finally {
      setLoading(false);
    }
  }

  if (available === null) {
    return <div className="text-sm text-slate-500 dark:text-slate-400">Loading…</div>;
  }

  if (!available) {
    return (
      <div className="text-center space-y-3">
        <p className="text-sm text-slate-600 dark:text-slate-300">
          Setup has already been completed.
        </p>
        <a href="/login/" className="text-sm text-blue-600 hover:underline dark:text-blue-400">
          Go to sign in
        </a>
      </div>
    );
  }

  if (!token) {
    return (
      <p className="text-center text-sm text-slate-600 dark:text-slate-300">
        Missing setup token. Use the URL printed by the server on first run.
      </p>
    );
  }

  return (
    <form
      onSubmit={onSubmit}
      className="space-y-4 rounded-xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900"
    >
      {error && (
        <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300">
          {error}
        </div>
      )}

      <div className="space-y-1">
        <label
          htmlFor="email"
          className="block text-sm font-medium text-slate-700 dark:text-slate-300"
        >
          Email
        </label>
        <input
          id="email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500"
          placeholder="admin@example.com"
        />
      </div>

      <div className="space-y-1">
        <label
          htmlFor="display-name"
          className="block text-sm font-medium text-slate-700 dark:text-slate-300"
        >
          Display name{" "}
          <span className="font-normal text-slate-400 dark:text-slate-500">(optional)</span>
        </label>
        <input
          id="display-name"
          type="text"
          autoComplete="name"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500"
          placeholder="Your name"
        />
      </div>

      <div className="space-y-1">
        <label
          htmlFor="password"
          className="block text-sm font-medium text-slate-700 dark:text-slate-300"
        >
          Password
        </label>
        <input
          id="password"
          type="password"
          autoComplete="new-password"
          required
          minLength={8}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500"
          placeholder="At least 8 characters"
        />
      </div>

      <div className="space-y-1">
        <label
          htmlFor="confirm"
          className="block text-sm font-medium text-slate-700 dark:text-slate-300"
        >
          Confirm password
        </label>
        <input
          id="confirm"
          type="password"
          autoComplete="new-password"
          required
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:placeholder-slate-500"
          placeholder="••••••••"
        />
      </div>

      <button
        type="submit"
        disabled={loading}
        className="w-full rounded-lg bg-blue-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 dark:focus:ring-offset-slate-900"
      >
        {loading ? "Creating account…" : "Create admin account"}
      </button>
    </form>
  );
}

export default function SetupPage() {
  return (
    <div className="relative flex min-h-screen items-center justify-center bg-slate-50 px-4 dark:bg-slate-950">
      <ThemeToggle className="absolute right-4 top-4 flex h-9 w-9 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-200 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-white" />
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-2xl font-semibold text-slate-900 dark:text-white">helpcore</h1>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            Create your admin account
          </p>
        </div>
        <Suspense
          fallback={
            <div className="text-center text-sm text-slate-500 dark:text-slate-400">Loading…</div>
          }
        >
          <SetupWizard />
        </Suspense>
      </div>
    </div>
  );
}
