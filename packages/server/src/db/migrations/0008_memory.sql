-- Per-user personality (soul, identity, user profile).
CREATE TABLE IF NOT EXISTS user_personality (
    user_id    TEXT NOT NULL,
    name       TEXT NOT NULL,   -- 'soul' | 'identity' | 'user'
    content    TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, name),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Per-user memory files (canonical store; filesystem sync deferred to Phase 3).
CREATE TABLE IF NOT EXISTS memory_files (
    id         INTEGER PRIMARY KEY,
    user_id    TEXT    NOT NULL,
    path       TEXT    NOT NULL,   -- relative path, e.g. "notes.md" or "home/devices.md"
    content    TEXT    NOT NULL,
    updated_at TEXT    NOT NULL,
    UNIQUE(user_id, path),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- FTS5 content table for full-text search over memory file content and path.
-- Kept in sync with memory_files automatically via the triggers below.
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    path,
    content='memory_files',
    content_rowid='id'
);

-- Triggers to keep the FTS5 index current whenever memory_files changes.
-- Required because FTS5 content= tables do not auto-update on DML.

CREATE TRIGGER IF NOT EXISTS memory_files_ai
AFTER INSERT ON memory_files BEGIN
    INSERT INTO memory_fts(rowid, content, path) VALUES (new.id, new.content, new.path);
END;

CREATE TRIGGER IF NOT EXISTS memory_files_ad
AFTER DELETE ON memory_files BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, path)
    VALUES ('delete', old.id, old.content, old.path);
END;

CREATE TRIGGER IF NOT EXISTS memory_files_au
AFTER UPDATE ON memory_files BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, path)
    VALUES ('delete', old.id, old.content, old.path);
    INSERT INTO memory_fts(rowid, content, path) VALUES (new.id, new.content, new.path);
END;
