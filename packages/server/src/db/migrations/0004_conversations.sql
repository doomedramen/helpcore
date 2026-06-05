CREATE TABLE IF NOT EXISTS conversations (
    id            TEXT    NOT NULL PRIMARY KEY,
    user_id       TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title         TEXT    NOT NULL,
    provider_id   TEXT,
    model         TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS conversations_user ON conversations(user_id, updated_at DESC);
