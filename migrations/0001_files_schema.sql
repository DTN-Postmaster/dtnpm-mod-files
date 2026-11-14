CREATE TABLE IF NOT EXISTS files_bundles (
    id TEXT PRIMARY KEY,
    source_eid TEXT NOT NULL,
    destination_eid TEXT NOT NULL,
    creation_timestamp INTEGER NOT NULL,
    lifetime_secs INTEGER NOT NULL,
    expiry_timestamp INTEGER NOT NULL,
    size INTEGER NOT NULL,
    payload_path TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    is_sticky INTEGER NOT NULL DEFAULT 0,
    is_read INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    auth_status TEXT NOT NULL DEFAULT 'unsigned'
);

CREATE INDEX IF NOT EXISTS idx_files_created ON files_bundles(creation_timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_files_expiry ON files_bundles(is_sticky, expiry_timestamp);
