'use client';

import { FormEvent, Suspense, useEffect, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { ApiError, setupAdmin, setupStatus } from '@/lib/api';
import { useAuth } from '@/context/auth';

function SetupWizard() {
  const params = useSearchParams();
  const router = useRouter();
  const { login } = useAuth();

  const token = params.get('token') ?? '';

  const [available, setAvailable] = useState<boolean | null>(null);
  const [email, setEmail] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setupStatus()
      .then(s => setAvailable(s.setup_required))
      .catch(() => setAvailable(false));
  }, []);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (password !== confirm) {
      setError('Passwords do not match.');
      return;
    }
    setError('');
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
      router.replace('/chat/');
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Setup failed.');
    } finally {
      setLoading(false);
    }
  }

  if (available === null) {
    return <div className="text-sm text-slate-500">Loading…</div>;
  }

  if (!available) {
    return (
      <div className="text-center space-y-3">
        <p className="text-sm text-slate-600">Setup has already been completed.</p>
        <a href="/login/" className="text-sm text-blue-600 hover:underline">Go to sign in</a>
      </div>
    );
  }

  if (!token) {
    return (
      <p className="text-sm text-slate-600 text-center">
        Missing setup token. Use the URL printed by the server on first run.
      </p>
    );
  }

  return (
    <form onSubmit={onSubmit} className="bg-white rounded-xl border border-slate-200 shadow-sm p-6 space-y-4">
      {error && (
        <div className="rounded-lg bg-red-50 border border-red-200 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      <div className="space-y-1">
        <label htmlFor="email" className="block text-sm font-medium text-slate-700">Email</label>
        <input
          id="email"
          type="email"
          autoComplete="email"
          required
          value={email}
          onChange={e => setEmail(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="admin@example.com"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="display-name" className="block text-sm font-medium text-slate-700">
          Display name <span className="text-slate-400 font-normal">(optional)</span>
        </label>
        <input
          id="display-name"
          type="text"
          autoComplete="name"
          value={displayName}
          onChange={e => setDisplayName(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="Your name"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="password" className="block text-sm font-medium text-slate-700">Password</label>
        <input
          id="password"
          type="password"
          autoComplete="new-password"
          required
          minLength={8}
          value={password}
          onChange={e => setPassword(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="At least 8 characters"
        />
      </div>

      <div className="space-y-1">
        <label htmlFor="confirm" className="block text-sm font-medium text-slate-700">Confirm password</label>
        <input
          id="confirm"
          type="password"
          autoComplete="new-password"
          required
          value={confirm}
          onChange={e => setConfirm(e.target.value)}
          className="block w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="••••••••"
        />
      </div>

      <button
        type="submit"
        disabled={loading}
        className="w-full rounded-lg bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        {loading ? 'Creating account…' : 'Create admin account'}
      </button>
    </form>
  );
}

export default function SetupPage() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-50">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-2xl font-semibold text-slate-900">helpcore</h1>
          <p className="mt-1 text-sm text-slate-500">Create your admin account</p>
        </div>
        <Suspense fallback={<div className="text-sm text-slate-500 text-center">Loading…</div>}>
          <SetupWizard />
        </Suspense>
      </div>
    </div>
  );
}
