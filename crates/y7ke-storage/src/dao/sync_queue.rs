//! Outbox of messages awaiting confirmation. Drained by the retry driver.

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{MessageId, Y7Id};

#[derive(Clone, Debug)]
pub struct SyncQueueEntry {
    pub message_id: MessageId,
    pub target_peer_y7_id: Y7Id,
    pub attempts: i64,
    pub next_retry_at: i64,
}

pub struct SyncQueueDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> SyncQueueDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(
        &self,
        message_id: &MessageId,
        target: &Y7Id,
        next_retry_at: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_queue (message_id, target_peer_y7_id, attempts, next_retry_at) \
             VALUES (?, ?, 0, ?) \
             ON CONFLICT(message_id, target_peer_y7_id) DO UPDATE SET \
               next_retry_at = excluded.next_retry_at",
        )
        .bind(&message_id.as_bytes()[..])
        .bind(target.to_uri())
        .bind(next_retry_at)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sync_queue.enqueue: {e}")))?;
        Ok(())
    }

    pub async fn due(&self, now: i64, limit: i64) -> Result<Vec<SyncQueueEntry>> {
        let rows: Vec<(Vec<u8>, String, i64, i64)> = sqlx::query_as(
            "SELECT message_id, target_peer_y7_id, attempts, next_retry_at \
             FROM sync_queue WHERE next_retry_at <= ? ORDER BY next_retry_at ASC LIMIT ?",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sync_queue.due: {e}")))?;

        rows.into_iter()
            .map(|(mid, target, attempts, next)| {
                let bytes: [u8; 16] = mid
                    .try_into()
                    .map_err(|_| AppError::storage("sync_queue.message_id: expected 16 bytes"))?;
                Ok(SyncQueueEntry {
                    message_id: MessageId::from_bytes(bytes),
                    target_peer_y7_id: Y7Id::parse(&target)?,
                    attempts,
                    next_retry_at: next,
                })
            })
            .collect()
    }

    pub async fn bump(
        &self,
        message_id: &MessageId,
        target: &Y7Id,
        attempts: i64,
        next_retry_at: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE sync_queue SET attempts = ?, next_retry_at = ? \
             WHERE message_id = ? AND target_peer_y7_id = ?",
        )
        .bind(attempts)
        .bind(next_retry_at)
        .bind(&message_id.as_bytes()[..])
        .bind(target.to_uri())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sync_queue.bump: {e}")))?;
        Ok(())
    }

    pub async fn remove(&self, message_id: &MessageId, target: &Y7Id) -> Result<()> {
        sqlx::query("DELETE FROM sync_queue WHERE message_id = ? AND target_peer_y7_id = ?")
            .bind(&message_id.as_bytes()[..])
            .bind(target.to_uri())
            .execute(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("sync_queue.remove: {e}")))?;
        Ok(())
    }
}
