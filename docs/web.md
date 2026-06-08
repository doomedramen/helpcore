# Web frontend

## Overview

The helpcore web UI is a Next.js 16 frontend using the App Router. In production, it's statically exported and embedded in the Rust server binary — the API and web UI are both served from port 3000. In development, the Next.js dev server runs on port 3001 and proxies `/api/*` requests to the Rust server on port 3000 (no auth).

## Stack

| Layer | Technology |
|-------|-----------|
| Framework | Next.js 16 (App Router) |
| Styling | Tailwind CSS 4 (`@tailwindcss/postcss`, CSS variables mode) |
| UI primitives | shadcn/ui v4 (`@base-ui/react` primitives, NOT Radix) |
| Chat UI | AI Elements (48 components in `src/components/ai-elements/`) |
| Data fetching | SWR for message polling |
| Service worker | Serwist (PWA) |
| Markdown rendering | streamdown |
| Animation | motion |
| Icons | lucide-react |

## Development

```bash
cd apps/web

# Install dependencies
npm ci

# Start dev server on port 3001
npm run dev

# Lint
npm run lint           # oxlint

# Format check
npm run format:check   # oxfmt --check

# Auto-format
npm run format         # oxfmt --write

# Type check
npx tsc --noEmit
```

The dev server proxies `/api/*` to `http://localhost:3000/api/*` (or `HELPCORE_URL` if set), so the Rust server must be running separately.

## Project structure

```
apps/web/src/
  app/                    Next.js App Router pages and layouts
    admin/                Admin settings, users, providers
    chat/                 Chat interface (main feature)
    login/                Login page
    memory/               Memory file manager
    personality/          Soul and identity editor
    plugins/              Plugin store and management
    settings/             User settings
    setup/                First-time admin setup wizard
    ~offline/             Offline fallback page
    globals.css           Global styles and CSS variables
    layout.tsx            Root layout
    manifest.ts           PWA manifest
    sw.ts                 Service worker registration
  components/
    ai-elements/          48 AI Elements components (Message, Conversation, PromptInput, etc.)
  context/
    auth.tsx              Auth context (token management, refresh)
    theme-provider.tsx    Theme provider (next-themes)
    theme.tsx             Theme context
  hooks/
    use-mobile.ts         Mobile detection hook
    use-require-auth.ts   Auth guard hook
  lib/
    api.ts                API client (SSE streaming, REST, auth)
    types.ts              TypeScript type definitions
    utils.ts              cn() helper (clsx + tailwind-merge)
```

## AI Elements

AI Elements is a library of 48 React components for building chat interfaces. It is **not** a chatbot framework — it provides primitives (message bubbles, tool displays, prompt inputs, etc.) that you compose into your own UI.

Key components:

- **Conversation** — scrollable message list with auto-scroll
- **Message** — individual message bubble with role, content, and actions
- **PromptInput** — text input with attachments, voice, and model selector
- **Tool** — tool call/result display (invocation and response)
- **Reasoning** — collapsible reasoning chain display
- **Sources** — source citations
- **CodeBlock** — syntax-highlighted code blocks
- **Suggestion** — suggested follow-up prompts

The backend uses a custom SSE protocol (not Vercel AI SDK protocol). Messages are polled via SWR.

Key deps: `ai` (types only), `streamdown`, `use-stick-to-bottom`, `motion`, `@radix-ui/react-use-controllable-state`.

## API client

`src/lib/api.ts` handles all communication with the Rust server:

- **REST** for auth, CRUD, admin, plugins — JSON request/response with Bearer token auth
- **SSE streaming** for chat (`POST /api/chat`) — `text/event-stream` with events: `started`, `chunk`, `tool_call`, `tool_result`, `done`, `error`

**Auth flow**: Login returns an access token + refresh token. The access token is short-lived and auto-refreshed by the auth context. Tokens are sent as `Authorization: Bearer <token>` headers.

`ApiError` is thrown on non-2xx responses with the server's error message.

## Styling

- **Tailwind CSS 4** with `@tailwindcss/postcss`
- **CSS variables** in `globals.css` define colors, border radius, and sidebar theming
- **Dark mode** via `next-themes` — `.dark` class toggles the dark variable set
- Use `cn()` from `@/lib/utils` (clsx + tailwind-merge) for conditional class merging
- Never hardcode colors — always use Tailwind utility classes or CSS variables

## Component conventions

- Components in `src/app/components/` are `"use client"` unless purely server-renderable
- Use `cn()` from `@/lib/utils` for class merging
- Import from `@/...` path aliases (maps to `./src/...`)

## @base-ui/react compatibility

The ai-elements components were patched for `@base-ui/react` (NOT Radix). Key differences to maintain:

- **`render` prop instead of `asChild`**: `<Trigger render={<Button />} />`, NOT `<Trigger><Button /></Trigger>`
- **Dialog `onOpenChange`**: base-ui provides `(open: boolean, eventDetails)`. Always wrap: `(open) => fn(open)`, never pass the handler directly.
- **DropdownMenu `onSelect`**: event type is base-ui's custom event — cast handlers with `as any` when needed.
- **HoverCard**: base-ui PreviewCard has no `openDelay`/`closeDelay` — spread props and drop unsupported ones.

When re-installing ai-elements components, re-apply these fixes and run:
```bash
npm run format
npx tsc --noEmit
```

## PWA

Serwist provides the service worker for offline support and installability. Configuration is in `next.config.ts` via `withSerwist`. The PWA manifest is at `src/app/manifest.ts`.

## Building for production

```bash
# Full build (static export + Rust server with web embedded)
make build
# Equivalent to:
# cd apps/web && npm ci
# cd apps/web && NEXT_EXPORT=true npm run build
# HELPCORE_EMBED_WEB_DIR=apps/web/out cargo build --release -p helpcore-server -p helpcore-cli

# Headless build (Rust only, no web UI)
make build-headless
```

When `NEXT_EXPORT=true` is set, Next.js produces a static export to `apps/web/out/`. The Rust server embeds this directory at build time (via `HELPCORE_EMBED_WEB_DIR`) and serves it alongside the API on port 3000.
