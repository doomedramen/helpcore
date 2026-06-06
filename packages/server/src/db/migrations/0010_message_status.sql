ALTER TABLE messages ADD COLUMN status TEXT NOT NULL DEFAULT 'complete';
ALTER TABLE messages ADD COLUMN error TEXT;
ALTER TABLE messages ADD COLUMN updated_at TEXT;

UPDATE messages SET updated_at = created_at WHERE updated_at IS NULL;

CREATE INDEX IF NOT EXISTS messages_active_generation
ON messages(conversation_id, status)
WHERE role = 'assistant' AND status IN ('pending', 'streaming');
