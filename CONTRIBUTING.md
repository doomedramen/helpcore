# Contributing to helpcore

## Development setup

```bash
git clone https://github.com/DoomedRamen/helpcore.git
cd helpcore

# First-time setup (required for the server to start)
cp config.toml.example config.toml

# Install dependencies
# - Rust 1.87+
# - Node.js 22+
cd apps/web && npm ci
```

## Project structure

```
helpcore/
  apps/web/                Next.js frontend (React 19, Tailwind CSS 4, shadcn/ui v4)
  packages/
    api/                   Rust — shared API types
    cli/                   Rust — CLI (bins: `helpcore` and `hc`)
    server/                Rust — Axum server, SSE chat, MCP-style tools
    skill-validate/        Rust — CI tool for skill brief validation
  docs/                    Documentation
  plugins/                 First-party plugin examples
```

## Development workflow

```bash
# Web dev server (proxies /api/* to Rust server, no auth)
cd apps/web && npm run dev          # → localhost:3001

# Rust server (needs config.toml)
cargo run -p helpcore-server        # defaults to port 3000

# Rust server headless (API only, no web UI)
cargo run -p helpcore-server -- --headless
```

## Pre-commit checks

Managed by lefthook. All run in parallel. Everything must pass before `git commit`.

| Check | Command | What it covers |
|-------|---------|---------------|
| Rust format | `cargo fmt -- --check` | All `*.rs` files |
| Web lint | `npm run lint` (oxlint) | `apps/web/**/*.{ts,tsx}` |
| Web format | `npm run format:check` (oxfmt) | `apps/web/**/*.{ts,tsx}` |

If format checks fail, run `npm run format` (from `apps/web/`) or `cargo fmt` to auto-fix.

## Pre-push checks

Managed by lefthook. All run in parallel before `git push`.

| Check | Command | Notes |
|-------|---------|-------|
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` | Zero-warning tolerance |
| Rust tests | `cargo test --workspace` | Server tests use in-process axum |
| Rust audit | `cargo audit` | Dependency vulnerability scan |
| TypeScript | `npx tsc --noEmit` | Run from `apps/web/` |

## Code style

- **Rust**: `cargo fmt` (standard Rust formatting)
- **TypeScript / TSX**: `npm run format` (oxfmt, same as `npm run format:check` but auto-fixes)
- **Components**: Use `cn()` from `@/lib/utils` for class merging, CSS variables from `globals.css`, never hardcode colors
- **Client components**: Mark with `"use client"` directive

## Pull request process

1. Run all pre-push checks locally before opening a PR:
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --workspace
   cargo audit
   cd apps/web && npx tsc --noEmit
   ```
2. Ensure `cargo fmt` and `npm run format:check` pass.
3. CI also runs `cargo run --bin skill-validate` and a production web build (`NEXT_EXPORT=true npm run build`).

## Testing

```bash
# All Rust tests
cargo test --workspace

# Single crate
cargo test -p helpcore-server

# Server tests use in-process axum (no real network).
```

## License

helpcore is licensed under the [PolyForm Noncommercial License 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0).
