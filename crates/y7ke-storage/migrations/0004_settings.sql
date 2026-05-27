CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    payload_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
-- Seed with defaults so .get() always returns Some.
INSERT OR IGNORE INTO settings (id, payload_json, updated_at) VALUES (1, '{"dial_modes":{"lan":true,"internet":true,"relay":true,"p2p":false},"extra_bootstraps":[]}', 0);
