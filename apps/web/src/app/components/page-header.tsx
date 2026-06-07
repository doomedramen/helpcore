interface PageHeaderProps {
  breadcrumb: string;
  title: string;
  description?: string;
  accent?: boolean;
  className?: string;
}

export default function PageHeader({
  breadcrumb,
  title,
  description,
  accent,
  className,
}: PageHeaderProps) {
  return (
    <div className={className ?? "mb-6"}>
      <p
        className={
          accent
            ? "text-xs font-semibold uppercase tracking-[0.16em] text-teal-600 dark:text-teal-400"
            : "text-xs font-semibold uppercase tracking-[0.16em] text-indigo-600 dark:text-indigo-400"
        }
      >
        {breadcrumb}
      </p>
      <h1 className="mt-2 text-2xl font-semibold tracking-[-0.035em] text-slate-950 sm:text-3xl dark:text-white">
        {title}
      </h1>
      {description && (
        <p className="mt-2 max-w-2xl text-sm leading-6 text-slate-500 dark:text-slate-400">
          {description}
        </p>
      )}
    </div>
  );
}
