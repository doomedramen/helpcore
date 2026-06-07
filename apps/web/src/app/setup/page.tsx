"use client";

import { FormEvent, Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ApiError, setupAdmin, setupStatus } from "@/lib/api";
import { useAuth } from "@/context/auth";
import AuthShell from "@/app/components/auth-shell";

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
    <form onSubmit={onSubmit} className="surface-card space-y-4 p-5 sm:p-6">
      {error && (
        <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300">
          {error}
        </div>
      )}

      <div className="space-y-1">
        <label htmlFor="email" className="field-label">
          Email
        </label>
        <input
          id="email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className="field-input"
          placeholder="admin@example.com"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="display-name" className="field-label">
          Display name{" "}
          <span className="font-normal text-slate-400 dark:text-slate-500">(optional)</span>
        </label>
        <input
          id="display-name"
          type="text"
          autoComplete="name"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          className="field-input"
          placeholder="Your name"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="password" className="field-label">
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
          className="field-input"
          placeholder="At least 8 characters"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="confirm" className="field-label">
          Confirm password
        </label>
        <input
          id="confirm"
          type="password"
          autoComplete="new-password"
          required
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          className="field-input"
          placeholder="••••••••"
        />
      </div>

      <button type="submit" disabled={loading} className="primary-action w-full">
        {loading ? "Creating account…" : "Create admin account"}
      </button>
    </form>
  );
}

export default function SetupPage() {
  return (
    <AuthShell
      eyebrow="First run"
      title="Create your workspace"
      description="Set up the first administrator account. You can invite and manage other members later."
    >
      <Suspense
        fallback={
          <div className="text-center text-sm text-slate-500 dark:text-slate-400">Loading…</div>
        }
      >
        <SetupWizard />
      </Suspense>
    </AuthShell>
  );
}
