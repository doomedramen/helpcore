import BrandMark from "@/app/components/brand-mark";
import ThemeToggle from "@/app/components/theme-toggle";

interface AuthShellProps {
  eyebrow?: string;
  title: string;
  description: string;
  children: React.ReactNode;
}

export default function AuthShell({ eyebrow, title, description, children }: AuthShellProps) {
  return (
    <div className="app-canvas relative min-h-dvh overflow-hidden">
      <ThemeToggle className="icon-button absolute right-4 top-4 z-20" />

      <div className="relative mx-auto grid min-h-dvh max-w-6xl items-stretch lg:grid-cols-[1.05fr_0.95fr] lg:p-5">
        <section className="relative hidden overflow-hidden rounded-[2rem] bg-[#11131d] p-10 text-white shadow-2xl shadow-indigo-950/20 lg:flex lg:flex-col lg:justify-between">
          <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_right,rgba(99,102,241,0.35),transparent_38%),radial-gradient(circle_at_bottom_left,rgba(20,184,166,0.2),transparent_34%)]" />
          <div className="relative">
            <BrandMark inverse />
          </div>

          <div className="relative max-w-md">
            <div className="mb-7 flex items-center gap-3">
              <div className="grid size-14 place-items-center rounded-2xl border border-white/10 bg-white/5 backdrop-blur">
                <div className="size-5 rounded-full border-2 border-indigo-300 shadow-[0_0_0_7px_rgba(99,102,241,0.12),0_0_0_14px_rgba(45,212,191,0.06)]" />
              </div>
              <span className="text-xs font-medium uppercase tracking-[0.22em] text-indigo-200">
                Your private workspace
              </span>
            </div>
            <h2 className="text-4xl font-semibold leading-[1.08] tracking-[-0.045em]">
              One quiet place for the things you ask, remember, and build.
            </h2>
            <p className="mt-5 max-w-sm text-sm leading-6 text-slate-400">
              A focused assistant with a memory you control, designed to stay useful without getting
              in your way.
            </p>
          </div>

          <p className="relative text-xs text-slate-500">Private by default. Useful by design.</p>
        </section>

        <section className="flex min-h-dvh items-center px-4 py-16 sm:px-8 lg:min-h-0 lg:px-14">
          <div className="mx-auto w-full max-w-md">
            <BrandMark className="mb-10 lg:hidden" />
            <div className="mb-7">
              {eyebrow && (
                <p className="mb-2 text-xs font-semibold uppercase tracking-[0.18em] text-indigo-600 dark:text-indigo-400">
                  {eyebrow}
                </p>
              )}
              <h1 className="text-3xl font-semibold tracking-[-0.04em] text-slate-950 dark:text-white">
                {title}
              </h1>
              <p className="mt-2 text-sm leading-6 text-slate-500 dark:text-slate-400">
                {description}
              </p>
            </div>
            {children}
          </div>
        </section>
      </div>
    </div>
  );
}
