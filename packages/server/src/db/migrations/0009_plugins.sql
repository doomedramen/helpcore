-- Registry cache: metadata for plugins the server knows about.
CREATE TABLE IF NOT EXISTS plugins (
    id         TEXT NOT NULL PRIMARY KEY,   -- e.g. "home-assistant-bridge"
    name       TEXT NOT NULL,
    version    TEXT NOT NULL,
    manifest   TEXT NOT NULL,              -- full manifest.toml as JSON
    tier       TEXT NOT NULL,              -- 'wasm' | 'bridge'
    source_url TEXT,                       -- NULL for local-path plugins
    cached_at  TEXT NOT NULL
);

-- Per-user plugin install state.
CREATE TABLE IF NOT EXISTS plugin_installs (
    id           TEXT NOT NULL PRIMARY KEY,
    user_id      TEXT NOT NULL,
    plugin_id    TEXT NOT NULL,
    version      TEXT NOT NULL,
    permissions  TEXT NOT NULL DEFAULT '[]',  -- JSON array of granted permissions
    enabled      INTEGER NOT NULL DEFAULT 1,
    config       TEXT NOT NULL DEFAULT '{}',  -- plugin-specific JSON config
    skill_md     TEXT,                        -- cached skill fragment (from skill.md)
    installed_at TEXT NOT NULL,
    UNIQUE(user_id, plugin_id),
    FOREIGN KEY (user_id)    REFERENCES users(id)   ON DELETE CASCADE,
    FOREIGN KEY (plugin_id)  REFERENCES plugins(id) ON DELETE CASCADE
);

-- Scoped bearer tokens — Tier 2 bridge plugins use these to call the core API.
CREATE TABLE IF NOT EXISTS plugin_tokens (
    id          TEXT NOT NULL PRIMARY KEY,
    user_id     TEXT NOT NULL,
    plugin_id   TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    permissions TEXT NOT NULL DEFAULT '[]',  -- JSON, subset of install permissions
    created_at  TEXT NOT NULL,
    revoked_at  TEXT,                        -- NULL = active
    FOREIGN KEY (user_id)   REFERENCES users(id)             ON DELETE CASCADE,
    FOREIGN KEY (plugin_id) REFERENCES plugins(id)           ON DELETE CASCADE
);
