CREATE TABLE IF NOT EXISTS setup_tokens (
    token      TEXT NOT NULL PRIMARY KEY,
    expires_at TEXT NOT NULL,
    used_at    TEXT
);
