# Security

This document describes the security measures currently implemented in helpcore.

---

## Authentication

- Passwords are hashed with **Argon2** (the memory-hard hash, OWASP-recommended).
- Session tokens: access tokens expire after 15 minutes, refresh tokens after 30 days.
- Token refresh rotates the refresh token (old one is revoked).
- API keys use the `hc_` prefix, stored as hashes — the raw key is shown once at creation.
- Plugin bridge tokens use the `hcp_` prefix, scoped to a specific plugin.
- Setup tokens are one-time use, expire after 15 minutes.

---

## Data isolation

- Each user has an isolated data partition in the database.
- No user can access another user's conversations, memory files, or plugin data.
- Plugin installs are per-user — plugins installed by one user do not apply to others.
- Memory file paths are sanitised to prevent traversal attacks.

---

## Plugin security

### Path security

- Plugin packages are extracted to versioned directories under the user's data directory.
- Archives containing symlinks, nested paths (`../`), unexpected files, or manifest
  mismatches are rejected.
- File permissions are set to `0o700` (owner-only) for plugin directories and
  `0o600` for sensitive files.

### Bridge tokens

- Bridge plugins authenticate with scoped bearer tokens.
- Tokens are bound to a specific plugin ID and permission set.
- The core validates the token before proxying any request.

### Permission enforcement

- Plugins declare required permissions in their manifest.
- Permissions are presented to the user at install time for approval.
- WASM plugins cannot exceed their declared permissions: every host function that
  reaches outside the plugin's sandbox (`http-request`, `data-read`, `data-write`,
  `secret-read`, ...) checks the plugin's approved permission set before acting.
- Bridge plugins are further constrained by `allowed_hosts` in their manifest.

### Plugin secrets

- Plugin secrets are encrypted at rest using XChaCha20-Poly1305 (via the `crypto_secretbox`
  construction from libsodium through the `aes-gcm` crate-equivalent API).
- Secrets are decrypted on read and never stored in plaintext in the database.
- Decrypted secrets are kept separate from the plugin's regular configuration inside
  the WASM sandbox and are only reachable through `secret-read`, which requires the
  `read_secrets` permission — a plugin cannot read its own secrets unless the user
  approved that permission at install time.

---

## Filesystem permissions

- Config file: `0o600` (owner read/write only).
- Config directory: `0o700` (owner only).
- Data directories: `0o700` for user-scoped directories.
- The Docker entrypoint repairs ownership and permissions before dropping to UID 100.

---

## Deployment

- TLS termination expected at the reverse proxy (not handled by helpcore itself).
- Rate limiting is expected at the proxy level.
- No secrets are stored in the database — API keys and credentials live only in the
  server config file (`config.toml`, mode `0600`).

---

## What is not yet implemented

- Audit log (event-based logging for auth/admin actions).
- Prometheus metrics endpoint.
- Structured prompt injection defence layer.
- Landlock/seccomp sandboxing for WASM runtime.
