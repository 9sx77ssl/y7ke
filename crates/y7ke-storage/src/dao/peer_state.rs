//! Per-peer state: last-seen multiaddrs and sync high-water marks.

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{MessageId, Y7Id};

use crate::db::now_ms;

#[derive(Clone, Debug)]
pub struct PeerState {
    pub peer_y7_id: Y7Id,
    pub last_addrs_json: Option<String>,
    pub last_seen_at: Option<i64>,
    pub highest_seen_message_id: Option<MessageId>,
    pub highest_sent_message_id: Option<MessageId>,
}

pub struct PeerStateDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> PeerStateDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, peer: &Y7Id) -> Result<Option<PeerState>> {
        let row: Option<(
            String,
            Option<String>,
            Option<i64>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        )> = sqlx::query_as(
            "SELECT peer_y7_id, last_addrs_json, last_seen_at, \
                    highest_seen_message_id, highest_sent_message_id \
             FROM peer_state WHERE peer_y7_id = ?",
        )
        .bind(peer.to_uri())
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("peer_state.get: {e}")))?;

        let Some((peer_uri, addrs, last_seen, hs_in, hs_out)) = row else {
            return Ok(None);
        };
        let peer_y7_id = Y7Id::parse(&peer_uri)?;
        Ok(Some(PeerState {
            peer_y7_id,
            last_addrs_json: addrs,
            last_seen_at: last_seen,
            highest_seen_message_id: opt_msg_id(hs_in)?,
            highest_sent_message_id: opt_msg_id(hs_out)?,
        }))
    }

    /// Walk every row in the table. Used by the V2-A4 stale-relay
    /// sweep when the user moves to `LanOnly` and we need to strip
    /// cached circuit multiaddrs from `last_addrs_json`. Loads the
    /// full table into memory — fine at our scale (a single client's
    /// contacts), revisit if Y7KE ever ships group chat.
    pub async fn list_all(&self) -> Result<Vec<PeerState>> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<i64>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        )> = sqlx::query_as(
            "SELECT peer_y7_id, last_addrs_json, last_seen_at, \
                    highest_seen_message_id, highest_sent_message_id \
             FROM peer_state",
        )
        .fetch_all(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("peer_state.list_all: {e}")))?;
        rows.into_iter()
            .map(|(peer_uri, addrs, last_seen, hs_in, hs_out)| {
                let peer_y7_id = Y7Id::parse(&peer_uri)?;
                Ok(PeerState {
                    peer_y7_id,
                    last_addrs_json: addrs,
                    last_seen_at: last_seen,
                    highest_seen_message_id: opt_msg_id(hs_in)?,
                    highest_sent_message_id: opt_msg_id(hs_out)?,
                })
            })
            .collect()
    }

    pub async fn upsert_seen(&self, peer: &Y7Id, addrs_json: Option<String>) -> Result<()> {
        sqlx::query(
            "INSERT INTO peer_state (peer_y7_id, last_addrs_json, last_seen_at) \
             VALUES (?, ?, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET \
                last_addrs_json = excluded.last_addrs_json, \
                last_seen_at    = excluded.last_seen_at",
        )
        .bind(peer.to_uri())
        .bind(&addrs_json)
        .bind(now_ms())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("peer_state.upsert_seen: {e}")))?;
        Ok(())
    }

    pub async fn set_high_water_inbound(&self, peer: &Y7Id, message_id: &MessageId) -> Result<()> {
        sqlx::query(
            "INSERT INTO peer_state (peer_y7_id, highest_seen_message_id) \
             VALUES (?, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET highest_seen_message_id = excluded.highest_seen_message_id",
        )
        .bind(peer.to_uri())
        .bind(&message_id.as_bytes()[..])
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("peer_state.set_high_water_inbound: {e}")))?;
        Ok(())
    }

    pub async fn set_high_water_outbound(&self, peer: &Y7Id, message_id: &MessageId) -> Result<()> {
        sqlx::query(
            "INSERT INTO peer_state (peer_y7_id, highest_sent_message_id) \
             VALUES (?, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET highest_sent_message_id = excluded.highest_sent_message_id",
        )
        .bind(peer.to_uri())
        .bind(&message_id.as_bytes()[..])
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("peer_state.set_high_water_outbound: {e}")))?;
        Ok(())
    }
}

fn opt_msg_id(b: Option<Vec<u8>>) -> Result<Option<MessageId>> {
    match b {
        None => Ok(None),
        Some(bytes) => {
            let arr: [u8; 16] = bytes
                .try_into()
                .map_err(|_| AppError::storage("peer_state msg id: expected 16 bytes"))?;
            Ok(Some(MessageId::from_bytes(arr)))
        }
    }
}
