import { cn } from "@/lib/utils";

interface BrandMarkProps {
  className?: string;
  compact?: boolean;
  inverse?: boolean;
  markOnly?: boolean;
}

export default function BrandMark({
  className,
  compact = false,
  inverse = false,
  markOnly = false,
}: BrandMarkProps) {
  return (
    <div className={cn("inline-flex items-center gap-2.5", className)}>
      <span
        aria-hidden="true"
        className={cn(
          "relative grid shrink-0 place-items-center overflow-hidden rounded-[0.85rem] bg-gradient-to-br from-indigo-500 via-indigo-500 to-teal-400 shadow-[0_8px_24px_-10px_rgba(79,70,229,0.9)]",
          compact ? "size-8" : "size-10",
        )}
      >
        <span className="absolute -right-2 -top-2 size-6 rounded-full bg-white/20 blur-sm" />
        <span
          className={cn(
            "relative rounded-full border-2 border-white/90",
            compact ? "size-3.5" : "size-4",
          )}
        >
          <span className="absolute left-1/2 top-1/2 size-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white" />
        </span>
      </span>
      {!markOnly && (
        <span
          className={cn(
            "font-semibold tracking-[-0.035em]",
            compact ? "text-base" : "text-lg",
            inverse ? "text-white" : "text-slate-950 dark:text-white",
          )}
        >
          helpcore
        </span>
      )}
    </div>
  );
}
