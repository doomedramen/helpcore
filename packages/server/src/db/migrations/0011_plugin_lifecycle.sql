ALTER TABLE plugin_installs ADD COLUMN manifest TEXT;
ALTER TABLE plugin_installs ADD COLUMN tools TEXT NOT NULL DEFAULT '{"tools":[]}';
ALTER TABLE plugin_installs ADD COLUMN previous_version TEXT;
ALTER TABLE plugin_installs ADD COLUMN secrets TEXT;
ALTER TABLE plugin_installs ADD COLUMN updated_at TEXT;

UPDATE plugin_installs
SET manifest = (
        SELECT plugins.manifest FROM plugins WHERE plugins.id = plugin_installs.plugin_id
    ),
    updated_at = installed_at
WHERE manifest IS NULL;
