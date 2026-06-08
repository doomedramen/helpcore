import { cn } from "@/lib/utils";

interface BrandMarkProps {
  className?: string;
  compact?: boolean;
  inverse?: boolean;
  markOnly?: boolean;
}

function Mark() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="size-full"
    >
      <path d="M12 6V2H8" />
      <path d="M15 11v2" />
      <path d="M2 12h2" />
      <path d="M20 12h2" />
      <path d="M20 16a2 2 0 0 1-2 2H8.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 4 20.286V8a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2z" />
      <path d="M9 11v2" />
    </svg>
  );
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
        className={cn(
          "shrink-0",
          compact ? "size-[18px]" : "size-[22px]",
          inverse ? "text-white" : "text-slate-950 dark:text-white",
        )}
      >
        <Mark />
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
