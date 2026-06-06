CREATE TABLE IF NOT EXISTS user_provider_grants (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    granted_by  TEXT REFERENCES users(id) ON DELETE SET NULL,
    granted_at  TEXT NOT NULL,
    PRIMARY KEY (user_id, provider_id)
);
CREATE INDEX IF NOT EXISTS user_provider_grants_user ON user_provider_grants(user_id);
