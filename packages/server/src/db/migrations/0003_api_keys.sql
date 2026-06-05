CREATE TABLE IF NOT EXISTS api_keys (
    id           TEXT NOT NULL PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    key_prefix   TEXT NOT NULL,
    key_hash     TEXT NOT NULL UNIQUE,
    scopes       TEXT,
    expires_at   TEXT,
    created_at   TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at   TEXT
);

CREATE INDEX IF NOT EXISTS api_keys_hash ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS api_keys_user ON api_keys(user_id);
