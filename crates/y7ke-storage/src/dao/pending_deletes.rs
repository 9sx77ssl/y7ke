//! Durable outbox for the `ChatDeleted` control. One row per peer, holding the
//! pre-sealed envelope bytes (sealed before the local wipe, so no session is
//! needed to retry). Survives `wipe_peer`; retried until the peer acks.

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::Y7Id;

#[derive(Clone, Debug)]
pub struct PendingDelete {
    pub peer_y7_id: Y7Id,
    pub envelope: Vec<u8>,
    pub attempts: i64,
}

pub struct PendingDeletesDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> PendingDeletesDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    /// Store (or replace) the sealed `ChatDeleted` envelope for `peer`.
    pub async fn enqueue(&self, peer: &Y7Id, envelope: &[u8], next_retry_at: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO pending_deletes (peer_y7_id, envelope, attempts, next_retry_at) \
             VALUES (?, ?, 0, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET \
               envelope = excluded.envelope, next_retry_at = excluded.next_retry_at",
        )
        .bind(peer.to_uri())
        .bind(envelope)
        .bind(next_retry_at)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("pending_deletes.enqueue: {e}")))?;
        Ok(())
    }

    /// The sealed envelope queued for `peer`, if any.
    pub async fn get(&self, peer: &Y7Id) -> Result<Option<PendingDelete>> {
        let row: Option<(Vec<u8>, i64)> =
            sqlx::query_as("SELECT envelope, attempts FROM pending_deletes WHERE peer_y7_id = ?")
                .bind(peer.to_uri())
                .fetch_optional(self.pool)
                .await
                .map_err(|e| AppError::storage(format!("pending_deletes.get: {e}")))?;
        Ok(row.map(|(envelope, attempts)| PendingDelete {
            peer_y7_id: *peer,
            envelope,
            attempts,
        }))
    }

    /// Rows whose `next_retry_at` is due (for a periodic sweep).
    pub async fn due(&self, now: i64, limit: i64) -> Result<Vec<PendingDelete>> {
        let rows: Vec<(String, Vec<u8>, i64)> = sqlx::query_as(
            "SELECT peer_y7_id, envelope, attempts FROM pending_deletes \
             WHERE next_retry_at <= ? ORDER BY next_retry_at ASC LIMIT ?",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("pending_deletes.due: {e}")))?;
        rows.into_iter()
            .map(|(peer, envelope, attempts)| {
                Ok(PendingDelete {
                    peer_y7_id: Y7Id::parse(&peer)?,
                    envelope,
                    attempts,
                })
            })
            .collect()
    }

    pub async fn bump(&self, peer: &Y7Id, attempts: i64, next_retry_at: i64) -> Result<()> {
        sqlx::query(
            "UPDATE pending_deletes SET attempts = ?, next_retry_at = ? WHERE peer_y7_id = ?",
        )
        .bind(attempts)
        .bind(next_retry_at)
        .bind(peer.to_uri())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("pending_deletes.bump: {e}")))?;
        Ok(())
    }

    pub async fn remove(&self, peer: &Y7Id) -> Result<()> {
        sqlx::query("DELETE FROM pending_deletes WHERE peer_y7_id = ?")
            .bind(peer.to_uri())
            .execute(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("pending_deletes.remove: {e}")))?;
        Ok(())
    }
}
