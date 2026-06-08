import BrandMark from "@/app/components/brand-mark";

export default function OfflinePage() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-slate-950 px-6">
      <BrandMark className="mb-4" inverse />
      <h1 className="text-xl font-semibold text-slate-100">You&apos;re offline</h1>
      <p className="max-w-sm text-center text-sm text-slate-400">
        Check your connection and try again. Previously loaded pages may still be available.
      </p>
    </div>
  );
}
