import { Spinner } from "@/app/components/ui/spinner";

export default function Loading() {
  return (
    <div className="flex h-screen items-center justify-center gap-2 text-sm text-slate-400 dark:text-slate-500">
      <Spinner />
      Loading…
    </div>
  );
}
