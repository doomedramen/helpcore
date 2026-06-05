# helpcore — Auth & Users

---

## Auth mechanism

Password-based auth. Passwords hashed with **argon2**.

No OAuth, no passkeys in initial scope. API keys for programmatic access
(see below).

---

## User roles

| Role | Capabilities |
|---|---|
| `admin` | Create users, deactivate users, delete users, reset passwords |
| `member` | Own data only — conversations, plugins, settings |

Admin has **zero access to any user's conversations or data**. No API endpoint
exists for this. This is an architectural constraint enforced at the application
layer, not just policy. The audit log records admin actions (who created/deleted/
reset which user) but never conversation content.

---

## User states

| State | Meaning |
|---|---|
| Active | Normal access |
| Deactivated | Cannot log in; all data preserved; reversible by admin |
| Deleted | Hard delete — account and all associated data permanently removed |

Admin can transition a user to either state independently. Deactivate is the
safe default; delete is explicit and irreversible.

---

## Account creation flow

Admin creates a new account and sets a temporary password.
`force_password_change` is set to `true`.

On the user's first login:
1. Login succeeds with the temporary password
2. Server returns a `password_change_required` response
3. User must set a new password before any other API call succeeds
4. Once the new password is set, `force_password_change` is cleared and normal
   access begins

---

## First admin setup

On first run with an empty database, the server has no admin account.
Two paths are supported:

**Web path:** server prints a one-time setup URL to stdout at startup:
```
Setup required → http://localhost:3000/setup?token=<one-time-token>
```
The token expires after 15 minutes or first use. Opening the URL presents a
form to create the first admin account.

**Headless / CLI path:**
```bash
helpcore setup
```
Interactive prompt: email, password. Creates the first admin account directly.

After the first admin account exists, the setup token (if unused) is
invalidated and the setup endpoint is permanently disabled.

---

## Sessions

Web and CLI share the same session system.

**Login** returns two tokens:
- `access_token` — short-lived (15 minutes), sent on every request
- `refresh_token` — long-lived (30 days), used only to rotate access tokens

**Refresh:** when the access token expires, the client sends the refresh token
to `POST /auth/refresh`. The server returns a new access token and a new
refresh token. The old refresh token is immediately invalidated (single-use
rotation).

**Logout** revokes both tokens immediately.

**CLI** stores credentials in `~/.helpcore/credentials` (restricted to `600`
permissions). The CLI handles token refresh automatically — the user logs in
once and stays logged in.

---

## API keys

User-generated keys for programmatic access: scripts, bridges, custom
integrations.

- Named by the user (e.g. `"obsidian bridge"`, `"home scripts"`)
- Prefixed `hc_` — full key shown exactly once at creation, never retrievable again
- Only the prefix (first 8 characters after `hc_`) is stored in plaintext for
  display in the UI; the rest is stored hashed
- Optional expiry date
- Optional scoped permissions (subset of the user's own permissions)
- `last_used_at` tracked on every use

Generated via:
- Web UI
- CLI: `helpcore api-keys create --name "my scripts"`
- Chat: "create an API key called obsidian bridge"

---

## Database schema

### `users`
| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| email | TEXT | Unique |
| display_name | TEXT | |
| password_hash | TEXT | argon2 |
| role | TEXT | `admin` or `member` |
| force_password_change | BOOL | Set on admin-created accounts |
| created_by | UUID | FK → users.id; null for first admin |
| created_at | TIMESTAMP | |
| updated_at | TIMESTAMP | |
| deactivated_at | TIMESTAMP | Null = active |

### `sessions`
| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| user_id | UUID | FK → users.id |
| access_token_hash | TEXT | |
| refresh_token_hash | TEXT | |
| created_at | TIMESTAMP | |
| expires_at | TIMESTAMP | Refresh token expiry |
| refreshed_at | TIMESTAMP | Last rotation |
| last_seen_at | TIMESTAMP | Last access token use |
| user_agent | TEXT | |
| revoked_at | TIMESTAMP | Null = active |

### `api_keys`
| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| user_id | UUID | FK → users.id |
| name | TEXT | User-given label |
| key_hash | TEXT | Full key hashed |
| key_prefix | TEXT | First 8 chars, plaintext, for UI display |
| permissions | JSON | Null = full user permissions |
| created_at | TIMESTAMP | |
| last_used_at | TIMESTAMP | |
| expires_at | TIMESTAMP | Null = no expiry |
| revoked_at | TIMESTAMP | Null = active |
