'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import useSWR, { useSWRConfig } from 'swr';
import Sidebar from '@/app/components/sidebar';
import { useAuth } from '@/context/auth';
import {
  createAdminUser,
  listAdminUsers,
  resetAdminUserPassword,
  updateAdminUser,
} from '@/lib/api';
import type { AdminUserSummary } from '@/lib/types';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';

export default function AdminUsersPage() {
  const { accessToken, currentUser, isLoading } = useAuth();
  const router = useRouter();
  const { mutate } = useSWRConfig();

  const [error, setError] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [newEmail, setNewEmail] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [newDisplay, setNewDisplay] = useState('');
  const [creating, setCreating] = useState(false);

  const [resetTarget, setResetTarget] = useState<AdminUserSummary | null>(null);
  const [newPass, setNewPass] = useState('');
  const [resetting, setResetting] = useState(false);

  const { data } = useSWR(
    accessToken && currentUser?.role === 'admin' ? ['/api/admin/users', accessToken] : null,
    ([, t]) => listAdminUsers(t),
    { revalidateOnFocus: false },
  );
  const users: AdminUserSummary[] = data?.users ?? [];

  if (isLoading) {
    return <div className="flex h-screen items-center justify-center text-sm text-slate-400">Loading…</div>;
  }
  if (!accessToken || currentUser?.role !== 'admin') {
    router.replace(accessToken ? '/chat/' : '/login/');
    return null;
  }

  async function handleCreate() {
    if (!accessToken) return;
    setCreating(true);
    setError(null);
    try {
      await createAdminUser(
        { email: newEmail.trim(), password: newPassword, display_name: newDisplay.trim() || undefined },
        accessToken,
      );
      setNewEmail(''); setNewPassword(''); setNewDisplay('');
      setShowCreate(false);
      await mutate(['/api/admin/users', accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create failed');
    } finally {
      setCreating(false);
    }
  }

  async function handleToggleStatus(user: AdminUserSummary) {
    if (!accessToken) return;
    const next = user.status === 'active' ? 'deactivated' : 'active';
    try {
      await updateAdminUser(user.id, { status: next }, accessToken);
      await mutate(['/api/admin/users', accessToken]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Update failed');
    }
  }

  async function handleResetPassword() {
    if (!accessToken || !resetTarget) return;
    if (newPass.length < 8) { setError('Password must be at least 8 characters'); return; }
    setResetting(true);
    setError(null);
    try {
      await resetAdminUserPassword(resetTarget.id, newPass, accessToken);
      setResetTarget(null);
      setNewPass('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Reset failed');
    } finally {
      setResetting(false);
    }
  }

  return (
    <div className="flex h-screen overflow-hidden bg-slate-50 dark:bg-slate-950">
      <Sidebar
        conversationId={null}
        onSelect={id => router.push(id ? `/chat/?id=${id}` : '/chat/')}
      />
      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-4xl px-6 py-8">
          <div className="mb-7 flex items-start justify-between">
            <div>
              <p className="text-sm font-medium text-blue-600 dark:text-blue-400">Administration</p>
              <h1 className="mt-1 text-2xl font-semibold text-slate-900 dark:text-white">Users</h1>
              <p className="mt-2 text-sm text-slate-500 dark:text-slate-400">
                Manage member accounts and reset passwords.
              </p>
            </div>
            <button
              onClick={() => { setError(null); setShowCreate(true); }}
              className="rounded-lg px-4 py-2 text-sm font-medium bg-blue-600 text-white hover:bg-blue-500 transition-colors"
            >
              + New user
            </button>
          </div>

          {error && (
            <div className="mb-4 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
              {error}
            </div>
          )}

          {/* User table */}
          <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 overflow-hidden">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-100 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/50">
                  <th className="text-left px-5 py-3 text-xs font-semibold text-slate-400 uppercase tracking-wider">User</th>
                  <th className="text-left px-5 py-3 text-xs font-semibold text-slate-400 uppercase tracking-wider">Role</th>
                  <th className="text-left px-5 py-3 text-xs font-semibold text-slate-400 uppercase tracking-wider">Status</th>
                  <th className="text-left px-5 py-3 text-xs font-semibold text-slate-400 uppercase tracking-wider">Joined</th>
                  <th className="px-5 py-3" />
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
                {users.map(user => (
                  <tr key={user.id}>
                    <td className="px-5 py-4">
                      <p className="font-medium text-slate-900 dark:text-white">{user.display_name ?? user.email}</p>
                      {user.display_name && (
                        <p className="text-xs text-slate-400">{user.email}</p>
                      )}
                    </td>
                    <td className="px-5 py-4 text-slate-500 dark:text-slate-400 capitalize">{user.role}</td>
                    <td className="px-5 py-4">
                      <span className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${
                        user.status === 'active'
                          ? 'bg-green-50 text-green-700 dark:bg-green-950/50 dark:text-green-400'
                          : 'bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400'
                      }`}>
                        {user.status}
                      </span>
                    </td>
                    <td className="px-5 py-4 text-slate-400">
                      {new Date(user.created_at).toLocaleDateString()}
                    </td>
                    <td className="px-5 py-4">
                      <div className="flex items-center justify-end gap-3">
                        <button
                          onClick={() => { setResetTarget(user); setError(null); }}
                          className="text-xs text-blue-600 dark:text-blue-400 hover:underline"
                        >
                          Reset password
                        </button>
                        {user.id !== currentUser?.id && (
                          <button
                            onClick={() => handleToggleStatus(user)}
                            className={`text-xs ${
                              user.status === 'active'
                                ? 'text-amber-600 dark:text-amber-400'
                                : 'text-green-600 dark:text-green-400'
                            } hover:underline`}
                          >
                            {user.status === 'active' ? 'Deactivate' : 'Activate'}
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
                {users.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-5 py-8 text-center text-sm text-slate-400">
                      No users yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </main>

      {/* Create user dialog */}
      <Dialog open={showCreate} onOpenChange={open => { if (!open) { setShowCreate(false); setNewEmail(''); setNewPassword(''); setNewDisplay(''); } }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create member account</DialogTitle>
            <DialogDescription>
              The user will be asked to set a new password on first login.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1">Email</label>
              <input
                type="email"
                value={newEmail}
                onChange={e => setNewEmail(e.target.value)}
                autoFocus
                className={inputClass}
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1">Temporary password</label>
              <input
                type="password"
                value={newPassword}
                onChange={e => setNewPassword(e.target.value)}
                className={inputClass}
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1">Display name <span className="font-normal text-slate-500">(optional)</span></label>
              <input
                type="text"
                value={newDisplay}
                onChange={e => setNewDisplay(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') handleCreate(); }}
                className={inputClass}
              />
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={() => setShowCreate(false)}
              className="rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={creating || !newEmail.trim() || newPassword.length < 8}
              className="rounded-lg px-4 py-1.5 text-sm font-medium bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
            >
              {creating ? 'Creating…' : 'Create'}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Reset password dialog */}
      <Dialog open={resetTarget !== null} onOpenChange={open => { if (!open) { setResetTarget(null); setNewPass(''); } }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset password</DialogTitle>
            <DialogDescription>
              Set a new temporary password for <strong className="text-slate-200">{resetTarget?.email}</strong>. They will be asked to change it on next login.
            </DialogDescription>
          </DialogHeader>
          <input
            type="password"
            value={newPass}
            onChange={e => setNewPass(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') handleResetPassword(); }}
            placeholder="New password (min 8 chars)"
            autoFocus
            className={inputClass}
          />
          <DialogFooter>
            <button
              onClick={() => { setResetTarget(null); setNewPass(''); }}
              className="rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleResetPassword}
              disabled={resetting || newPass.length < 8}
              className="rounded-lg px-4 py-1.5 text-sm font-medium bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
            >
              {resetting ? 'Saving…' : 'Set password'}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

const inputClass = 'w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-sm text-slate-100 placeholder-slate-500 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500';
