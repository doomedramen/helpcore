# helpcore Web UI

## Stack

- **Next.js 16** (App Router) with static export
- **Tailwind CSS 4** with CSS variables theming
- **shadcn/ui v4** (`@base-ui/react` primitives, NOT Radix)
- **AI Elements** — 48 chat UI components (`src/components/ai-elements/`)
- **SWR** for data fetching with token-based cache keys
- **Serwist** for PWA/service worker (precaching, runtime caching, offline fallback)
- **React 19** (client components)

## Getting Started

```bash
cd apps/web
npm install
npm run dev          # → localhost:3001 (proxies /api/* to Rust server)
```

Build for production:

```bash
NEXT_EXPORT=true npm run build   # static export to out/
```

The Rust server embeds the `out/` directory when built with `HELPCORE_EMBED_WEB_DIR=apps/web/out`.

## Project Structure

```
src/
├── app/                          # Next.js App Router pages & layouts
│   ├── layout.tsx                # Root layout (providers, metadata, PWA head tags)
│   ├── globals.css               # Tailwind imports, CSS variables, theme config
│   ├── manifest.ts               # PWA manifest (icons, shortcuts, display mode)
│   ├── sw.ts                     # Serwist service worker (runtime caching strategies)
│   ├── page.tsx                  # Root page (redirects to /chat or /login)
│   ├── ~offline/                 # Offline fallback page
│   ├── chat/                     # Chat interface (main conversation view)
│   ├── login/                    # Login / registration page
│   ├── setup/                    # Initial admin setup wizard
│   ├── settings/                 # User settings (profile, API keys)
│   ├── memory/                   # Memory file editor
│   ├── personality/              # Personality file editor
│   ├── plugins/                  # Plugin manager (install, configure, enable)
│   ├── admin/                    # Admin panel (users, config, provider grants)
│   ├── change-password/          # Force password change page
│   ├── components/               # App-specific components
│   │   ├── ui/                   # shadcn/ui primitives (Button, Dialog, Select, etc.)
│   │   ├── admin/                # Admin-specific components (config form)
│   │   ├── app-shell.tsx         # Main authenticated layout (sidebar + content)
│   │   ├── auth-shell.tsx        # Unauthenticated layout (centered card)
│   │   ├── chat-window.tsx       # Core chat UI with SSE streaming
│   │   ├── sidebar.tsx           # Conversation list sidebar
│   │   ├── plugin-manager.tsx    # Plugin install/configure UI
│   │   ├── tool-adapter.tsx      # Bridge AI Elements tool rendering to MCP tools
│   │   ├── content-renderers.tsx # Streamdown content type handlers
│   │   ├── asset-renderer.tsx    # Mermaid, chart, code rendering
│   │   ├── streamdown-code-block-handlers.tsx
│   │   ├── theme-toggle.tsx      # Light/dark/system theme switcher
│   │   ├── brand-mark.tsx        # App logo/branding
│   │   └── ...
│   └── serwist/                  # Serwist SW route
├── components/
│   └── ai-elements/              # 48 pre-built AI chat components
│       ├── conversation.tsx, message.tsx, prompt-input.tsx
│       ├── tool.tsx, code-block.tsx, reasoning.tsx
│       ├── chain-of-thought.tsx, plan.tsx, task.tsx
│       ├── artifact.tsx, web-preview.tsx, sandbox.tsx
│       └── ... (agent, attachments, canvas, commit, etc.)
├── context/
│   ├── auth.tsx                  # Auth context (login, logout, token refresh, SWR bridge)
│   ├── theme.tsx                 # Theme re-export (delegates to next-themes)
│   └── theme-provider.tsx        # next-themes provider config
├── hooks/
│   └── use-mobile.ts             # useIsMobile() — 768px breakpoint
└── lib/
    ├── api.ts                    # API client (REST + SSE streaming)
    ├── types.ts                  # TypeScript types for all API responses
    └── utils.ts                  # cn() helper (clsx + tailwind-merge)
```

## Path Aliases

All `@/*` paths resolve to `./src/*` (configured in `tsconfig.json`):

| Alias                        | Location                                            |
| ---------------------------- | --------------------------------------------------- |
| `@/app/components/ui/*`      | `src/app/components/ui/*` — shadcn/ui primitives    |
| `@/components/ai-elements/*` | `src/components/ai-elements/*` — AI chat components |
| `@/lib/api.ts`               | `src/lib/api.ts` — API client                       |
| `@/lib/types.ts`             | `src/lib/types.ts` — TypeScript types               |
| `@/lib/utils.ts`             | `src/lib/utils.ts` — `cn()` helper                  |
| `@/context/auth`             | `src/context/auth.tsx` — Auth context               |
| `@/context/theme`            | `src/context/theme.tsx` — Theme context             |

## Key Components

### Providers (root layout)

- **`SerwistProvider`** — Wraps the app for PWA service worker registration (`swUrl="/serwist/sw.js"`)
- **`ThemeProvider`** — `next-themes` with `class` strategy, `system` default, persisted to `helpcore-theme`
- **`AuthProvider`** — Token management (login, logout, refresh), user state, SWR `onError` 401 recovery
- **`Toaster`** (sonner) — Toast notifications

### Auth (`context/auth.tsx`)

- JWT access + refresh token flow stored in localStorage (`helpcore_refresh` key)
- `AuthProvider` wraps children in an `SWRConfig` that auto-refreshes on 401 errors
- Single in-flight refresh deduplication (`refreshInFlightRef`) — prevents race conditions when multiple stale SWR requests fail at once
- SWR cache keys are `[url, accessToken]`, so refreshing the token automatically re-fetches all data
- Exposes `useAuth()` hook: `accessToken`, `currentUser`, `isLoading`, `login()`, `logout()`, `refreshAccessToken()`, `forcePasswordChange`

### Theme (`context/theme-provider.tsx`)

- Delegates to `next-themes` with `attribute="class"`, `defaultTheme="system"`, `storageKey="helpcore-theme"`
- Dark mode triggers via `@custom-variant dark (&:where(.dark, .dark *))` in `globals.css`

### Chat (`app/components/chat-window.tsx`)

- Core chat UI: message list with auto-scroll, prompt input, SSE streaming
- Uses AI Elements components (`Conversation`, `Message`, `PromptInput`)
- Tool calls rendered via `tool-adapter.tsx` bridging to AI Elements `Tool` components
- Content rendered through `streamdown` with custom code-block handlers

## API Client (`lib/api.ts`)

### REST endpoints

All REST calls go through `req<T>(path, init, token?)` which:

- Prepends `/api` to all paths
- Adds `Content-Type: application/json` and `Authorization: Bearer <token>` headers
- Parses JSON responses, returns `undefined` for 204 No Content
- Throws `ApiError` with parsed server message on non-OK responses

Endpoints organized by section:

| Section           | Functions                                                                                                                                                                          |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Auth**          | `login()`, `logout()`, `refresh()`, `getCurrentUser()`, `updateMe()`, `changePassword()`                                                                                           |
| **Setup**         | `setupStatus()`, `setupAdmin()`                                                                                                                                                    |
| **Conversations** | `listConversations()`, `getMessages()`, `deleteConversation()`, `cancelGeneration()`                                                                                               |
| **Chat (SSE)**    | `chat()`, `retryMessage()`, `consumeChatStream()`                                                                                                                                  |
| **Personality**   | `getPersonality()`, `putPersonality()`                                                                                                                                             |
| **Memory**        | `listMemory()`, `getMemoryFile()`, `putMemoryFile()`, `deleteMemoryFile()`                                                                                                         |
| **API Keys**      | `listApiKeys()`, `createApiKey()`, `revokeApiKey()`                                                                                                                                |
| **Admin**         | `listAdminUsers()`, `createAdminUser()`, `updateAdminUser()`, `resetAdminUserPassword()`, `listProviderGrants()`, `setProviderGrants()`, `getAdminConfig()`, `updateAdminConfig()` |
| **Plugins**       | `listPlugins()`, `listPluginStore()`, `setPluginEnabled()`, `installPlugin()`, `updatePlugin()`, `rollbackPlugin()`, `uninstallPlugin()`, `configurePlugin()`                      |
| **TTS**           | `tts()` — returns audio Blob                                                                                                                                                       |
| **Feedback**      | `submitFeedback()` — best-effort, silently ignores failures                                                                                                                        |

### SSE streaming protocol

`consumeChatStream()` is the core streaming function used by both `chat()` and `retryMessage()`. It:

1. Sends a POST to the chat endpoint with `{ message, conversation_id, provider_id }`
2. Reads the response body as a ReadableStream
3. Splits the stream into SSE events (double-newline delimited)
4. Dispatches typed handlers per event:

| SSE Event     | Handler        | Data                                               |
| ------------- | -------------- | -------------------------------------------------- |
| `started`     | `onStarted`    | `{ conversation_id, user_message_id, message_id }` |
| `chunk`       | `onChunk`      | `{ delta: string }`                                |
| `tool_call`   | `onToolCall`   | `{ id, name, arguments }`                          |
| `tool_result` | `onToolResult` | `{ id, name, result }`                             |
| `done`        | `onDone`       | `{ conversation_id, message_id }`                  |
| `error`       | (throws)       | `{ message: string }`                              |

Messages are **not** streamed via SSE — they are polled via SWR after the stream completes.

## Styling

### CSS variables (`globals.css`)

All colors are CSS custom properties in both `:root` (light) and `.dark` (dark) blocks, using `oklch()` color space. Key variables:

- `--background` / `--foreground` — page body
- `--card` / `--card-foreground` — card surfaces
- `--primary` / `--primary-foreground` — accent actions (indigo)
- `--secondary` / `--secondary-foreground` — secondary surfaces
- `--muted` / `--muted-foreground` — subdued text
- `--accent` / `--accent-foreground` — accent highlights (teal/green)
- `--destructive` / `--destructive-foreground` — error states
- `--border`, `--input`, `--ring` — form element borders
- `--sidebar-*` — sidebar-specific variables
- `--chart-1` through `--chart-5` — chart palette
- `--radius` — border radius base (0.625rem)

### Tailwind conventions

- Colors mapped in `@theme inline` block — use `bg-background`, `text-foreground`, `bg-primary`, etc.
- Custom variants: `dark:` qualifier triggers via `.dark` class on `<html>`
- Component layer classes: `.surface-card`, `.field-label`, `.field-input`, `.primary-action`, `.secondary-action`, `.icon-button`
- Base layer: global border-colors, code inline/block styles, font-sans default

### `cn()` helper

`cn()` in `lib/utils.ts` merges classes with `clsx` and resolves Tailwind conflicts with `tailwind-merge`. Always use `cn()` instead of string interpolation or template literals for conditional classes.

## Component Conventions

- **`"use client"`** — All components under `src/app/components/` are client components unless purely server-rendered. Mark explicitly at the top of each file.
- **`cn()` for class merging** — Always use `cn()` from `@/lib/utils` for combining Tailwind classes. Never hardcode colors — use CSS variable-backed utility classes.
- **`render` prop, NOT `asChild`** — `@base-ui/react` triggers use `<Trigger render={<Button ... />} />`, not `<Trigger asChild><Button ... /></Trigger>`.
- **Dialog `onOpenChange` wrapping** — base-ui provides `(open: boolean, eventDetails: ...)`. Always wrap: `(open) => fn(open)`, never pass the handler directly.
- **DropdownMenu `onSelect` casting** — base-ui's custom event type is incompatible with plain `Event`. Cast handlers with `as any` when needed.
- **HoverCard no openDelay/closeDelay** — base-ui `PreviewCard` doesn't support these props. Spread props and drop unsupported ones.

When re-installing AI Elements components from the registry, re-apply these base-ui compatibility fixes and run:

```bash
npm run format
npx tsc --noEmit
```

## PWA

### Service worker (`sw.ts`)

Serwist service worker with:

- **Precaching** — Next.js static assets via `self.__SW_MANIFEST`
- **`skipWaiting` + `clientsClaim`** — immediate activation
- **Navigation preload** — faster page loads
- **Runtime caching strategies:**
  - `/api/*` → `NetworkFirst` (network with 10s timeout, falls back to cache)
  - Documents → `StaleWhileRevalidate` (serve cached, refresh in background)
  - Default cache — standard asset caching from `@serwist/turbopack/worker`
- **Offline fallback** — `/~offline` page served when a document request fails while offline

### Manifest (`manifest.ts`)

```json
{
  "display": "standalone",
  "theme_color": "#4f46e5",
  "background_color": "#0f172a",
  "icons": [48–512px, including maskable variants],
  "shortcuts": ["New Chat", "Memory"]
}
```

### iOS PWA support

Apple touch icons, splash screens for all iPhone/iPad sizes, and `mobile-web-app-capable` meta tag configured in `layout.tsx`.

## Quality Gates

Managed by lefthook (see root `AGENTS.md`):

| Gate         | Command                        | When       |
| ------------ | ------------------------------ | ---------- |
| Lint         | `npm run lint` (oxlint)        | pre-commit |
| Format check | `npm run format:check` (oxfmt) | pre-commit |
| TypeScript   | `npx tsc --noEmit`             | pre-push   |

Auto-fix formatting: `npm run format`
