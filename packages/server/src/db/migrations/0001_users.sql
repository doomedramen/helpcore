CREATE TABLE IF NOT EXISTS users (
    id                    TEXT    NOT NULL PRIMARY KEY,
    email                 TEXT    NOT NULL UNIQUE,
    password_hash         TEXT    NOT NULL,
    display_name          TEXT,
    role                  TEXT    NOT NULL DEFAULT 'member',
    status                TEXT    NOT NULL DEFAULT 'active',
    force_password_change INTEGER NOT NULL DEFAULT 0,
    timezone              TEXT,
    created_at            TEXT    NOT NULL,
    updated_at            TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS users_email  ON users(email);
CREATE INDEX IF NOT EXISTS users_status ON users(status);
