//! The local-user row. Always `id = 1`; at most one row.

use sqlx::SqlitePool;
use y7ke_core::crypto::SymmetricKey;
use y7ke_core::error::{AppError, Result};
use y7ke_core::Y7Id;

use crate::db::now_ms;
use crate::field_crypto;

#[derive(Clone)]
pub struct User {
    pub y7_id: Y7Id,
    pub ed25519_pub: [u8; 32],
    pub ed25519_priv: [u8; 32],
    pub created_at: i64,
}

pub struct NewUser {
    pub y7_id: Y7Id,
    pub ed25519_pub: [u8; 32],
    pub ed25519_priv: [u8; 32],
}

pub struct UsersDao<'db> {
    pool: &'db SqlitePool,
    dek: &'db SymmetricKey,
}

impl<'db> UsersDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool, dek: &'db SymmetricKey) -> Self {
        Self { pool, dek }
    }

    pub async fn get(&self) -> Result<Option<User>> {
        let row: Option<(String, Vec<u8>, Vec<u8>, Vec<u8>, i64)> = sqlx::query_as(
            "SELECT y7_id, ed25519_pub, ed25519_priv_enc, ed25519_priv_nonce, created_at \
             FROM users WHERE id = 1",
        )
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("users.get: {e}")))?;

        let Some((y7_id_uri, pub_bytes, priv_enc, priv_nonce, created_at)) = row else {
            return Ok(None);
        };

        let y7_id = Y7Id::parse(&y7_id_uri)?;
        let ed25519_pub: [u8; 32] = pub_bytes
            .try_into()
            .map_err(|_| AppError::storage("users.ed25519_pub: expected 32 bytes"))?;
        let plaintext = field_crypto::open(self.dek, &priv_nonce, &priv_enc, &ed25519_pub)?;
        let ed25519_priv: [u8; 32] = plaintext
            .try_into()
            .map_err(|_| AppError::storage("users.ed25519_priv: expected 32 bytes plaintext"))?;

        Ok(Some(User {
            y7_id,
            ed25519_pub,
            ed25519_priv,
            created_at,
        }))
    }

    pub async fn insert(&self, new: NewUser) -> Result<User> {
        let (priv_enc, priv_nonce) =
            field_crypto::seal(self.dek, &new.ed25519_priv, &new.ed25519_pub)?;
        let created_at = now_ms();

        sqlx::query(
            "INSERT INTO users (id, y7_id, ed25519_pub, ed25519_priv_enc, ed25519_priv_nonce, created_at) \
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(new.y7_id.to_uri())
        .bind(&new.ed25519_pub[..])
        .bind(&priv_enc)
        .bind(&priv_nonce)
        .bind(created_at)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("users.insert: {e}")))?;

        Ok(User {
            y7_id: new.y7_id,
            ed25519_pub: new.ed25519_pub,
            ed25519_priv: new.ed25519_priv,
            created_at,
        })
    }
}
