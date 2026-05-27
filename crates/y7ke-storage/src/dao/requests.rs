//! Pending contact requests. Greeting text (if any) is encrypted at rest.

use sqlx::SqlitePool;
use y7ke_core::crypto::SymmetricKey;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{RequestResolution, Y7Id};

use crate::db::now_ms;
use crate::field_crypto;

#[derive(Clone, Debug)]
pub struct Request {
    pub id: i64,
    pub direction: RequestDirection,
    pub peer_y7_id: Y7Id,
    pub initial_text: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolution: Option<RequestResolution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestDirection {
    Incoming,
    Outgoing,
}

impl RequestDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "incoming" => Self::Incoming,
            "outgoing" => Self::Outgoing,
            other => {
                return Err(AppError::storage(format!(
                    "requests.direction: unexpected {other:?}"
                )));
            }
        })
    }
}

pub struct NewRequest {
    pub direction: RequestDirection,
    pub peer_y7_id: Y7Id,
    pub initial_text: Option<String>,
}

pub struct RequestsDao<'db> {
    pool: &'db SqlitePool,
    dek: &'db SymmetricKey,
}

impl<'db> RequestsDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool, dek: &'db SymmetricKey) -> Self {
        Self { pool, dek }
    }

    pub async fn insert(&self, new: NewRequest) -> Result<Request> {
        let (initial_text_enc, initial_text_nonce) = match &new.initial_text {
            Some(text) => {
                let (ct, nonce) = field_crypto::seal(
                    self.dek,
                    text.as_bytes(),
                    new.peer_y7_id.to_uri().as_bytes(),
                )?;
                (Some(ct), Some(nonce))
            }
            None => (None, None),
        };
        let created_at = now_ms();

        let id: (i64,) = sqlx::query_as(
            "INSERT INTO requests (direction, peer_y7_id, initial_text_enc, initial_text_nonce, created_at) \
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(new.direction.as_str())
        .bind(new.peer_y7_id.to_uri())
        .bind(initial_text_enc.as_deref())
        .bind(initial_text_nonce.as_deref())
        .bind(created_at)
        .fetch_one(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("requests.insert: {e}")))?;

        Ok(Request {
            id: id.0,
            direction: new.direction,
            peer_y7_id: new.peer_y7_id,
            initial_text: new.initial_text,
            created_at,
            resolved_at: None,
            resolution: None,
        })
    }

    pub async fn list_pending(&self, direction: Option<RequestDirection>) -> Result<Vec<Request>> {
        let rows: Vec<RawRequest> = match direction {
            None => sqlx::query_as::<_, RawRequest>(
                "SELECT id, direction, peer_y7_id, initial_text_enc, initial_text_nonce, \
                        created_at, resolved_at, resolution \
                 FROM requests WHERE resolved_at IS NULL ORDER BY created_at ASC",
            )
            .fetch_all(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("requests.list_pending: {e}")))?,

            Some(dir) => sqlx::query_as::<_, RawRequest>(
                "SELECT id, direction, peer_y7_id, initial_text_enc, initial_text_nonce, \
                        created_at, resolved_at, resolution \
                 FROM requests WHERE resolved_at IS NULL AND direction = ? ORDER BY created_at ASC",
            )
            .bind(dir.as_str())
            .fetch_all(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("requests.list_pending: {e}")))?,
        };

        rows.into_iter().map(|r| r.decode(self.dek)).collect()
    }

    pub async fn resolve(&self, id: i64, resolution: RequestResolution) -> Result<()> {
        let resolution_str = match resolution {
            RequestResolution::Accepted => "accepted",
            RequestResolution::Rejected => "rejected",
            RequestResolution::Cancelled => "cancelled",
        };

        let affected = sqlx::query(
            "UPDATE requests SET resolved_at = ?, resolution = ? WHERE id = ? AND resolved_at IS NULL",
        )
        .bind(now_ms())
        .bind(resolution_str)
        .bind(id)
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("requests.resolve: {e}")))?
        .rows_affected();

        if affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RawRequest {
    id: i64,
    direction: String,
    peer_y7_id: String,
    initial_text_enc: Option<Vec<u8>>,
    initial_text_nonce: Option<Vec<u8>>,
    created_at: i64,
    resolved_at: Option<i64>,
    resolution: Option<String>,
}

impl RawRequest {
    fn decode(self, dek: &SymmetricKey) -> Result<Request> {
        let direction = RequestDirection::parse(&self.direction)?;
        let peer_y7_id = Y7Id::parse(&self.peer_y7_id)?;
        let initial_text =
            match (self.initial_text_enc, self.initial_text_nonce) {
                (Some(ct), Some(nonce)) => {
                    let pt = field_crypto::open(dek, &nonce, &ct, peer_y7_id.to_uri().as_bytes())?;
                    Some(String::from_utf8(pt).map_err(|e| {
                        AppError::storage(format!("requests.initial_text decode: {e}"))
                    })?)
                }
                _ => None,
            };
        let resolution = self
            .resolution
            .as_deref()
            .map(|s| match s {
                "accepted" => Ok(RequestResolution::Accepted),
                "rejected" => Ok(RequestResolution::Rejected),
                "cancelled" => Ok(RequestResolution::Cancelled),
                other => Err(AppError::storage(format!(
                    "requests.resolution: unexpected {other:?}"
                ))),
            })
            .transpose()?;

        Ok(Request {
            id: self.id,
            direction,
            peer_y7_id,
            initial_text,
            created_at: self.created_at,
            resolved_at: self.resolved_at,
            resolution,
        })
    }
}
