//! Handshake-completion tracking. Session keys are derived on demand from
//! static identity DH — nothing is stored here beyond "handshake done".

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::Y7Id;

use crate::db::now_ms;

#[derive(Clone)]
pub struct Session {
    pub peer_y7_id: Y7Id,
    pub established_at: i64,
    pub last_used_at: i64,
}

pub struct SessionsDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> SessionsDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, peer: &Y7Id) -> Result<()> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO sessions (peer_y7_id, established_at, last_used_at) \
             VALUES (?, ?, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET last_used_at = excluded.last_used_at",
        )
        .bind(peer.to_uri())
        .bind(now)
        .bind(now)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sessions.upsert: {e}")))?;
        Ok(())
    }

    pub async fn get(&self, peer: &Y7Id) -> Result<Option<Session>> {
        let row: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT peer_y7_id, established_at, last_used_at \
             FROM sessions WHERE peer_y7_id = ?",
        )
        .bind(peer.to_uri())
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sessions.get: {e}")))?;

        let Some((uri, established_at, last_used_at)) = row else {
            return Ok(None);
        };
        let peer_y7_id = Y7Id::parse(&uri)?;
        Ok(Some(Session {
            peer_y7_id,
            established_at,
            last_used_at,
        }))
    }

    pub async fn touch(&self, peer: &Y7Id) -> Result<()> {
        sqlx::query("UPDATE sessions SET last_used_at = ? WHERE peer_y7_id = ?")
            .bind(now_ms())
            .bind(peer.to_uri())
            .execute(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("sessions.touch: {e}")))?;
        Ok(())
    }
}
