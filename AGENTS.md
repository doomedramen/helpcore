# AGENTS.md

Project conventions and quality gates for AI agents working on helpcore.

## Project structure

```
apps/web/          Next.js frontend (React 19, Tailwind CSS 4, shadcn/ui v4 @base-ui/react)
packages/api/      Rust – shared API types
packages/cli/      Rust – CLI binary (hc)
packages/server/   Rust – Axum server, SSE chat, MCP-style tools
```

## Pre-commit (must pass before `git commit`)

All run in parallel. Everything must pass — if any fails, fix before committing.

| Check | Command | Covers |
|---|---|---|
| Rust fmt | `cargo fmt -- --check` | `*.rs` |
| Web lint | `npm run lint` (oxlint) | `apps/web/**/*.{ts,tsx}` |
| Web format | `npm run format:check` (oxfmt) | `apps/web/**/*.{ts,tsx}` |

If format check fails, run `npm run format` to auto-fix.

## Pre-push (must pass before `git push`)

| Check | Command |
|---|---|
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` |
| Rust tests | `cargo test --workspace` |
| Rust audit | `cargo audit` |
| TypeScript | `npx tsc --noEmit` (run from `apps/web/`) |

## Web frontend conventions

### Stack
- **Next.js 16** (App Router, RSC enabled)
- **Tailwind CSS 4** (`@tailwindcss/postcss`, CSS variables mode)
- **shadcn/ui v4** (`@base-ui/react` primitives, NOT Radix)
- **AI Elements** (`src/components/ai-elements/`) — component library for chat UI

### Key path aliases (from tsconfig `@/*` → `./src/*`)
- `@/app/components/ui/*` — shadcn/ui primitives (Button, Collapsible, Select, etc.)
- `@/components/ai-elements/*` — AI Elements components (Message, Conversation, PromptInput, etc.)
- `@/lib/api.ts` — API client (SSE streaming, auth, CRUD)
- `@/lib/types.ts` — TypeScript types (Message, ConversationSummary, etc.)
- `@/context/auth.tsx` — Auth context
- `@/context/theme.tsx` — Theme context

### AI Elements usage
- Components live in `src/components/ai-elements/` (48 components installed via CLI)
- Imports use shadcn v4 primitives from `@/app/components/ui/*`, NOT Radix
- Backend uses custom SSE protocol (not Vercel AI SDK protocol) — messages are polled via SWR
- No AI Gateway dependency
- key dependencies: `ai` (types only), `streamdown`, `use-stick-to-bottom`, `motion`, `@radix-ui/react-use-controllable-state`

### Component conventions
- All new components in `src/app/components/` are `"use client"` unless they're purely server
- Use `cn()` from `@/lib/utils` for class merging
- Use CSS variables from `globals.css`, never hardcode colors

### Before modifying ai-elements components
The installed components in `src/components/ai-elements/` had to be patched for `@base-ui/react` compatibility:
- HoverCard: `openDelay`/`closeDelay` → `delay` (not supported in base-ui, just spread props)
- DropdownMenu `onSelect`: event type is `BaseUIEvent`, not plain `Event` — use `as any` for handlers
- CollapsibleTrigger with `asChild` Button: use `render` prop instead
- Dialog `onOpenChange`: base-ui provides `(open: boolean, eventDetails: ...)` — wrap with `(open) => fn(open)`

If re-installing components from the registry, re-apply these fixes.

## Build & run

```bash
# Web dev server (proxies /api/* to Rust server)
cd apps/web && npm run dev          # → localhost:3001

# Rust server (needs config.toml)
cargo run -p helpcore-server

# Full build (web + Rust)
make build
```

## Re-adding AI Elements components

```bash
cd apps/web
npx ai-elements@latest add <component-name>
# then re-apply base-ui compat fixes and run:
npm run format
npx tsc --noEmit
```
