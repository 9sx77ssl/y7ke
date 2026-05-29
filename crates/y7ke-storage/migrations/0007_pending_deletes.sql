-- Durable outbox for the ChatDeleted control.
--
-- delete_contact wipes the peer's session locally, so it must seal the
-- ChatDeleted envelope BEFORE the wipe and stash the sealed bytes here. This
-- table is deliberately NOT cleared by wipe_peer, so the deletion survives the
-- local wipe and can be retried until the peer acks (i.e. delivered the next
-- time they come online), fulfilling the "wiped on both sides" promise even
-- when the peer was offline at delete time.
CREATE TABLE IF NOT EXISTS pending_deletes (
    peer_y7_id    TEXT    NOT NULL PRIMARY KEY,
    envelope      BLOB    NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER NOT NULL
);
