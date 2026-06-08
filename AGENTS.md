# AGENTS.md

Project conventions and quality gates for AI agents working on helpcore.

## Project structure

```
apps/web/              Next.js frontend (React 19, Tailwind CSS 4, shadcn/ui v4 @base-ui/react)
packages/api/          Rust – shared API types (workspace edition 2024)
packages/cli/          Rust – CLI (bins: `helpcore` and `hc`; both delegate to `helpcore_cli::run()`)
packages/server/       Rust – Axum server, SSE chat, MCP-style tools
packages/skill-validate/  Rust – CI-only tool that validates skill brief discriminability (edition 2021, NOT 2024)
```

## Pre-commit (must pass before `git commit`)

Managed by lefthook. All run in parallel. Everything must pass — if any fails, fix before committing.

| Check | Command | Covers |
|---|---|---|
| Rust fmt | `cargo fmt -- --check` | `*.rs` |
| Web lint | `npm run lint` (oxlint) | `apps/web/**/*.{ts,tsx}` |
| Web format | `npm run format:check` (oxfmt) | `apps/web/**/*.{ts,tsx}` |

If format check fails, run `npm run format` to auto-fix.

## Pre-push (must pass before `git push`)

Managed by lefthook. All run in parallel.

| Check | Command | Notes |
|---|---|---|
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` | |
| Rust tests | `cargo test --workspace` | Server tests use in-process axum (no real network) |
| Rust audit | `cargo audit` | |
| TypeScript | `npx tsc --noEmit` | Run from `apps/web/` |

## CI extras (not in lefthook, but run in CI)

- `cargo run --bin skill-validate` — validates skill brief discriminability, produces JSON report
- macOS test (`cargo test --workspace` on macos-14 runner)
- Web production build (`npm run build` with `NEXT_EXPORT=true`)

## Build & run

### First-time setup
```bash
cp config.toml.example config.toml   # required for server to start
```

### Dev
```bash
# Web dev server (proxies /api/* to Rust server, no auth)
cd apps/web && npm run dev          # → localhost:3001

# Rust server (needs config.toml; server also serves web UI when built with embedded dir)
cargo run -p helpcore-server        # defaults to port 3000

# Rust server headless (API only, no web UI)
cargo run -p helpcore-server -- --headless
```

### Production build
```bash
# Full build: web static export + Rust server with web embedded
make build        # Runs npm ci, NEXT_EXPORT=true npm run build, then cargo build --release

# Headless build (Rust only, no web UI)
make build-headless

# Run the bundled release server
make run           # Requires apps/web/out/index.html to exist
```

Key env vars:
- `HELPCORE_CONFIG` — path to config.toml (defaults to `./config.toml`; Docker uses `/config/config.toml`)
- `HELPCORE_DATA` — data directory (Docker uses `/data`)
- `HELPCORE_EMBED_WEB_DIR` — path to Next.js `out/` directory for embedding in the Rust binary
- `NEXT_EXPORT=true` — triggers Next.js static export (used in Docker, CI, and `make build`)

## Web frontend conventions

### Stack
- **Next.js 16** (App Router, `next.config.ts` uses Serwist for service worker)
- **Tailwind CSS 4** (`@tailwindcss/postcss`, CSS variables mode)
- **shadcn/ui v4** (`@base-ui/react` primitives, NOT Radix)
- **AI Elements** (`src/components/ai-elements/`) — 48 components for chat UI

### Key path aliases (`@/*` → `./src/*`)
- `@/app/components/ui/*` — shadcn/ui primitives (Button, Collapsible, Select, etc.)
- `@/components/ai-elements/*` — AI Elements components (Message, Conversation, PromptInput, etc.)
- `@/lib/api.ts` — API client (SSE streaming, auth, CRUD)
- `@/lib/types.ts` — TypeScript types
- `@/context/auth.tsx` — Auth context
- `@/context/theme.tsx` — Theme context
- `@/lib/utils.ts` — `cn()` helper (clsx + tailwind-merge)

### AI Elements usage
- Backend uses custom SSE protocol (not Vercel AI SDK protocol) — messages are polled via SWR
- No AI Gateway dependency
- Key deps: `ai` (types only), `streamdown`, `use-stick-to-bottom`, `motion`, `@radix-ui/react-use-controllable-state`

### Component conventions
- Components in `src/app/components/` are `"use client"` unless purely server
- Use `cn()` from `@/lib/utils` for class merging
- Use CSS variables from `globals.css`, never hardcode colors

### @base-ui/react compatibility patches
The installed ai-elements components were patched for `@base-ui/react` (NOT Radix):

- **`render` prop instead of `asChild`**: base-ui triggers use `<Trigger render={<Button ... />} />`, NOT `<Trigger asChild><Button ... /></Trigger>`
- **Dialog `onOpenChange`**: base-ui provides `(open: boolean, eventDetails: ...)` — always wrap with `(open) => fn(open)`, never pass directly
- **DropdownMenu `onSelect`**: event type is base-ui's custom event, not plain `Event` — cast handlers with `as any` when needed
- **HoverCard no `openDelay`/`closeDelay`**: base-ui PreviewCard doesn't support these — just spread props and drop unsupported ones

If re-installing ai-elements components from the registry, re-apply these fixes and run:
```bash
npm run format
npx tsc --noEmit
```

## Re-adding AI Elements components

```bash
cd apps/web
npx ai-elements@latest add <component-name>
# then re-apply base-ui compat fixes and run:
npm run format
npx tsc --noEmit
```
