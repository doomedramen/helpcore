"use client";

import { FormEvent, useState } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/context/auth";
import { changePassword } from "@/lib/api";
import AuthShell from "@/app/components/auth-shell";

export default function ChangePasswordPage() {
  const { accessToken, clearForcePasswordChange } = useAuth();
  const router = useRouter();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (next !== confirm) {
      setError("Passwords do not match.");
      return;
    }
    if (next.length < 8) {
      setError("New password must be at least 8 characters.");
      return;
    }
    setError("");
    setSaving(true);
    try {
      await changePassword({ current_password: current, new_password: next }, accessToken!);
      clearForcePasswordChange();
      router.replace("/chat/");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not change password.");
    } finally {
      setSaving(false);
    }
  }

  if (isLoading || !ready || !accessToken) return null;

  return (
    <AuthShell
      eyebrow="Account security"
      title="Choose a new password"
      description="Your temporary password needs to be replaced before you can continue."
    >
      <form onSubmit={onSubmit} className="surface-card space-y-4 p-5 sm:p-6">
        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300">
            {error}
          </div>
        )}

        <div className="space-y-1">
          <label className="field-label">Temporary password</label>
          <input
            type="password"
            required
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            autoComplete="current-password"
            className="field-input"
          />
        </div>

        <div className="space-y-1">
          <label className="field-label">New password</label>
          <input
            type="password"
            required
            value={next}
            onChange={(e) => setNext(e.target.value)}
            autoComplete="new-password"
            className="field-input"
          />
        </div>

        <div className="space-y-1">
          <label className="field-label">Confirm new password</label>
          <input
            type="password"
            required
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="new-password"
            className="field-input"
          />
        </div>

        <button type="submit" disabled={saving} className="primary-action w-full">
          {saving ? "Saving…" : "Set new password"}
        </button>
      </form>
    </AuthShell>
  );
}
