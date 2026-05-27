-- Y7KE schema v1. All BLOB columns hold raw bytes; column-encrypted blobs
-- are noted with `_enc`. Companion `_nonce` columns store the 12-byte
-- ChaCha20-Poly1305 nonce.

CREATE TABLE users (
    -- Exactly one local user.
    id                 INTEGER PRIMARY KEY CHECK (id = 1),
    y7_id              TEXT    NOT NULL UNIQUE,           -- 'y7:<base58>'
    ed25519_pub        BLOB    NOT NULL,                  -- 32 bytes
    ed25519_priv_enc   BLOB    NOT NULL,                  -- ChaCha20-Poly1305(dek, ...)
    ed25519_priv_nonce BLOB    NOT NULL,                  -- 12 bytes
    created_at         INTEGER NOT NULL                   -- ms since Unix epoch
);

CREATE TABLE contacts (
    y7_id       TEXT    PRIMARY KEY,
    ed25519_pub BLOB    NOT NULL,
    nickname    TEXT,
    added_at    INTEGER NOT NULL,
    status      TEXT    NOT NULL CHECK (status IN ('accepted','pending_out','pending_in','blocked','removed'))
);
CREATE INDEX idx_contacts_status ON contacts(status);

CREATE TABLE requests (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    direction           TEXT    NOT NULL CHECK (direction IN ('incoming','outgoing')),
    peer_y7_id          TEXT    NOT NULL,
    initial_text_enc    BLOB,                            -- optional encrypted greeting
    initial_text_nonce  BLOB,
    created_at          INTEGER NOT NULL,
    resolved_at         INTEGER,
    resolution          TEXT    CHECK (resolution IN ('accepted','rejected','cancelled'))
);
CREATE INDEX idx_requests_peer       ON requests(peer_y7_id);
CREATE INDEX idx_requests_unresolved ON requests(resolved_at) WHERE resolved_at IS NULL;

CREATE TABLE messages (
    message_id      BLOB    PRIMARY KEY,                  -- UUIDv7, 16 bytes
    conversation_id BLOB    NOT NULL,                     -- blake3(sort(pubA,pubB))[..16]
    sender_pub      BLOB    NOT NULL,                     -- Ed25519 pubkey of sender
    recipient_pub   BLOB    NOT NULL,                     -- Ed25519 pubkey of recipient
    timestamp_ms    INTEGER NOT NULL,                     -- ms since Unix epoch
    status          INTEGER NOT NULL,                     -- MessageStatus discriminant
    payload_enc     BLOB    NOT NULL,                     -- ChaCha20-Poly1305(session_key, plaintext)
    payload_nonce   BLOB    NOT NULL,                     -- 12 bytes
    sig             BLOB    NOT NULL,                     -- Ed25519(sender_priv, message_id||timestamp_ms||payload_enc)
    inserted_at     INTEGER NOT NULL
);
CREATE INDEX idx_messages_conv_ts ON messages(conversation_id, timestamp_ms);
CREATE INDEX idx_messages_status  ON messages(status);

CREATE TABLE sessions (
    peer_y7_id          TEXT    PRIMARY KEY,
    shared_secret_enc   BLOB    NOT NULL,                  -- ChaCha20-Poly1305(dek, x25519_shared_secret)
    shared_secret_nonce BLOB    NOT NULL,                  -- 12 bytes
    established_at      INTEGER NOT NULL,
    last_used_at        INTEGER NOT NULL
);

CREATE TABLE keys (
    -- Future-proofing for ephemeral / per-message keys.
    key_id         TEXT    PRIMARY KEY,
    purpose        TEXT    NOT NULL,
    material_enc   BLOB    NOT NULL,
    material_nonce BLOB    NOT NULL,
    created_at     INTEGER NOT NULL
);

CREATE TABLE sync_queue (
    message_id        BLOB    NOT NULL,                    -- references messages.message_id
    target_peer_y7_id TEXT    NOT NULL,
    attempts          INTEGER NOT NULL DEFAULT 0,
    next_retry_at     INTEGER NOT NULL,
    PRIMARY KEY (message_id, target_peer_y7_id)
);
CREATE INDEX idx_sync_queue_retry ON sync_queue(next_retry_at);

CREATE TABLE peer_state (
    peer_y7_id               TEXT    PRIMARY KEY,
    last_addrs_json          TEXT,
    last_seen_at             INTEGER,
    highest_seen_message_id  BLOB,                        -- UUIDv7 high-water mark inbound
    highest_sent_message_id  BLOB                         -- UUIDv7 high-water mark outbound
);
