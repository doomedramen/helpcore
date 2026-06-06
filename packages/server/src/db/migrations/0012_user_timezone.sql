-- timezone was already present in 0001_users.sql.
-- Add created_by FK for admin accountability on member accounts.
ALTER TABLE users ADD COLUMN created_by TEXT REFERENCES users(id) ON DELETE SET NULL;
