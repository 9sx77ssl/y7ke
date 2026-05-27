//! Address book entries. Only public information; nothing encrypted at rest.

use sqlx::SqlitePool;
use std::str::FromStr;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{ContactStatus, Y7Id};

use crate::db::now_ms;

#[derive(Clone, Debug)]
pub struct Contact {
    pub y7_id: Y7Id,
    pub ed25519_pub: [u8; 32],
    pub nickname: Option<String>,
    pub added_at: i64,
    pub status: ContactStatus,
}

pub struct NewContact {
    pub y7_id: Y7Id,
    pub ed25519_pub: [u8; 32],
    pub nickname: Option<String>,
    pub status: ContactStatus,
}

pub struct ContactsDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> ContactsDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, c: NewContact) -> Result<Contact> {
        let added_at = now_ms();
        sqlx::query(
            "INSERT INTO contacts (y7_id, ed25519_pub, nickname, added_at, status) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(y7_id) DO UPDATE SET \
                ed25519_pub = excluded.ed25519_pub, \
                nickname    = excluded.nickname, \
                status      = excluded.status",
        )
        .bind(c.y7_id.to_uri())
        .bind(&c.ed25519_pub[..])
        .bind(&c.nickname)
        .bind(added_at)
        .bind(c.status.as_str())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("contacts.insert: {e}")))?;

        Ok(Contact {
            y7_id: c.y7_id,
            ed25519_pub: c.ed25519_pub,
            nickname: c.nickname,
            added_at,
            status: c.status,
        })
    }

    pub async fn update_status(&self, y7_id: &Y7Id, status: ContactStatus) -> Result<()> {
        let affected = sqlx::query("UPDATE contacts SET status = ? WHERE y7_id = ?")
            .bind(status.as_str())
            .bind(y7_id.to_uri())
            .execute(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("contacts.update_status: {e}")))?
            .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn get(&self, y7_id: &Y7Id) -> Result<Option<Contact>> {
        let row: Option<(String, Vec<u8>, Option<String>, i64, String)> = sqlx::query_as(
            "SELECT y7_id, ed25519_pub, nickname, added_at, status \
             FROM contacts WHERE y7_id = ?",
        )
        .bind(y7_id.to_uri())
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("contacts.get: {e}")))?;

        row.map(parse_row).transpose()
    }

    pub async fn list(&self) -> Result<Vec<Contact>> {
        let rows: Vec<(String, Vec<u8>, Option<String>, i64, String)> = sqlx::query_as(
            "SELECT y7_id, ed25519_pub, nickname, added_at, status \
             FROM contacts WHERE status != 'removed' ORDER BY added_at ASC",
        )
        .fetch_all(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("contacts.list: {e}")))?;

        rows.into_iter().map(parse_row).collect()
    }
}

fn parse_row(row: (String, Vec<u8>, Option<String>, i64, String)) -> Result<Contact> {
    let (y7_uri, ed_pub, nickname, added_at, status_str) = row;
    let y7_id = Y7Id::parse(&y7_uri)?;
    let ed25519_pub: [u8; 32] = ed_pub
        .try_into()
        .map_err(|_| AppError::storage("contacts.ed25519_pub: expected 32 bytes"))?;
    let status = ContactStatus::from_str(&status_str)?;
    Ok(Contact {
        y7_id,
        ed25519_pub,
        nickname,
        added_at,
        status,
    })
}
