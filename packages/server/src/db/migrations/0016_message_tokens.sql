ALTER TABLE messages ADD COLUMN input_tokens INTEGER;
ALTER TABLE messages ADD COLUMN output_tokens INTEGER;
ALTER TABLE messages ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE messages ADD COLUMN cache_write_tokens INTEGER;
