CREATE TABLE IF NOT EXISTS sessions (
    id                  TEXT NOT NULL PRIMARY KEY,
    user_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    access_token_hash   TEXT NOT NULL UNIQUE,
    refresh_token_hash  TEXT NOT NULL UNIQUE,
    access_expires_at   TEXT NOT NULL,
    refresh_expires_at  TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    revoked_at          TEXT
);

CREATE INDEX IF NOT EXISTS sessions_access_token  ON sessions(access_token_hash);
CREATE INDEX IF NOT EXISTS sessions_refresh_token ON sessions(refresh_token_hash);
CREATE INDEX IF NOT EXISTS sessions_user_id       ON sessions(user_id);
