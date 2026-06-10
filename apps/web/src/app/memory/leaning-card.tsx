"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { useAuth } from "@/context/auth";
import { updateMe } from "@/lib/api";

const LEANING_OPTIONS = [
  {
    value: "off",
    label: "Don't use memory",
    description: "The AI will not store or recall anything about you.",
  },
  { value: "light", label: "Light", description: "Only save things when you explicitly ask." },
  {
    value: "moderate",
    label: "Moderate",
    description: "Balance — save useful context, skip trivia.",
  },
  {
    value: "heavy",
    label: "Heavy",
    description: "Proactively remember everything it learns about you.",
  },
];

export default function MemoryLeaningCard() {
  const { accessToken, currentUser } = useAuth();
  const [leaning, setLeaning] = useState(currentUser?.memory_leaning ?? "moderate");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (currentUser?.memory_leaning) setLeaning(currentUser.memory_leaning);
  }, [currentUser?.memory_leaning]);

  async function handleChange(value: string) {
    setLeaning(value);
    if (!accessToken) return;
    setSaving(true);
    try {
      await updateMe({ memory_leaning: value }, accessToken);
      toast.success("Saved");
    } catch {
      setLeaning(currentUser?.memory_leaning ?? "moderate");
      toast.error("Could not save memory leaning");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="surface-card p-4 sm:p-5">
      <h3 className="mb-2 text-sm font-semibold text-slate-900 dark:text-white">Memory leaning</h3>
      <p className="mb-4 text-xs leading-5 text-slate-500 dark:text-slate-400">
        Controls how heavily the AI leans into storing and recalling information it learns about
        you.
      </p>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        {LEANING_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            onClick={() => handleChange(opt.value)}
            disabled={saving}
            className={`rounded-xl border px-3 py-3 text-left transition ${
              leaning === opt.value
                ? "border-indigo-200 bg-indigo-50 ring-1 ring-indigo-200 dark:border-indigo-800 dark:bg-indigo-950/40 dark:ring-indigo-800"
                : "border-slate-200/80 bg-white/60 hover:border-slate-300 dark:border-slate-800 dark:bg-slate-900/60 dark:hover:border-slate-700"
            }`}
          >
            <span className="block text-sm font-medium text-slate-900 dark:text-white">
              {opt.label}
            </span>
            <span className="mt-0.5 block text-xs leading-4 text-slate-400 dark:text-slate-500">
              {opt.description}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
