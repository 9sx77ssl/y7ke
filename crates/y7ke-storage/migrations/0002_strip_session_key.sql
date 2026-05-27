-- Migration 0002: session keys moved to on-demand static DH derivation.
-- Recreate sessions table without the key columns; existing key material is discarded.
CREATE TABLE sessions_v2 (
    peer_y7_id    TEXT PRIMARY KEY,
    established_at INTEGER NOT NULL,
    last_used_at   INTEGER NOT NULL
);

INSERT INTO sessions_v2 (peer_y7_id, established_at, last_used_at)
SELECT peer_y7_id, established_at, last_used_at FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_v2 RENAME TO sessions;
