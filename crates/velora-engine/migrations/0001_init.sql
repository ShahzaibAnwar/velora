-- Velora schema v1.
-- Each download is stored as a JSON document (`data`) plus a few indexed columns used for queries.
-- New record fields are added to the JSON with serde defaults, so most changes need no migration.

CREATE TABLE IF NOT EXISTS downloads (
    id           TEXT PRIMARY KEY NOT NULL,
    status       TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER,
    data         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
CREATE INDEX IF NOT EXISTS idx_downloads_completed_at ON downloads(completed_at);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
