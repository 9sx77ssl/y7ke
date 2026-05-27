//! Established X25519 session keys, encrypted with the master DEK.

use sqlx::SqlitePool;
use y7ke_core::crypto::SymmetricKey;
use y7ke_core::error::{AppError, Result};
use y7ke_core::Y7Id;

use crate::db::now_ms;
use crate::field_crypto;

#[derive(Clone)]
pub struct Session {
    pub peer_y7_id: Y7Id,
    pub session_key: SymmetricKey,
    pub established_at: i64,
    pub last_used_at: i64,
}

pub struct SessionsDao<'db> {
    pool: &'db SqlitePool,
    dek: &'db SymmetricKey,
}

impl<'db> SessionsDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool, dek: &'db SymmetricKey) -> Self {
        Self { pool, dek }
    }

    pub async fn upsert(&self, peer: &Y7Id, session_key: SymmetricKey) -> Result<Session> {
        let (enc, nonce) =
            field_crypto::seal(self.dek, session_key.as_bytes(), peer.to_uri().as_bytes())?;
        let now = now_ms();

        sqlx::query(
            "INSERT INTO sessions (peer_y7_id, shared_secret_enc, shared_secret_nonce, established_at, last_used_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(peer_y7_id) DO UPDATE SET \
               shared_secret_enc   = excluded.shared_secret_enc, \
               shared_secret_nonce = excluded.shared_secret_nonce, \
               established_at      = excluded.established_at, \
               last_used_at        = excluded.last_used_at",
        )
        .bind(peer.to_uri())
        .bind(&enc)
        .bind(&nonce)
        .bind(now)
        .bind(now)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sessions.upsert: {e}")))?;

        Ok(Session {
            peer_y7_id: *peer,
            session_key,
            established_at: now,
            last_used_at: now,
        })
    }

    pub async fn get(&self, peer: &Y7Id) -> Result<Option<Session>> {
        let row: Option<(String, Vec<u8>, Vec<u8>, i64, i64)> = sqlx::query_as(
            "SELECT peer_y7_id, shared_secret_enc, shared_secret_nonce, established_at, last_used_at \
             FROM sessions WHERE peer_y7_id = ?",
        )
        .bind(peer.to_uri())
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("sessions.get: {e}")))?;

        let Some((peer_uri, enc, nonce, established_at, last_used_at)) = row else {
            return Ok(None);
        };
        let peer_y7_id = Y7Id::parse(&peer_uri)?;
        let pt = field_crypto::open(self.dek, &nonce, &enc, peer_y7_id.to_uri().as_bytes())?;
        let key_bytes: [u8; 32] = pt.try_into().map_err(|_| {
            AppError::storage("sessions.shared_secret: expected 32 bytes plaintext")
        })?;
        Ok(Some(Session {
            peer_y7_id,
            session_key: SymmetricKey::new(key_bytes),
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
