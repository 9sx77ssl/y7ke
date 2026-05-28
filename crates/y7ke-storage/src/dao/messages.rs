//! Encrypted messages. `payload_enc` is opaque ciphertext encrypted with the
//! session key (held in `sessions`); this DAO never touches plaintexts.

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{ConversationId, MessageId, MessageStatus};

use crate::db::now_ms;

#[derive(Clone, Debug)]
pub struct Message {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    pub sender_pub: [u8; 32],
    pub recipient_pub: [u8; 32],
    pub timestamp_ms: i64,
    pub status: MessageStatus,
    pub payload_enc: Vec<u8>,
    pub payload_nonce: [u8; 12],
    pub sig: [u8; 64],
    pub inserted_at: i64,
}

pub struct NewMessage {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    pub sender_pub: [u8; 32],
    pub recipient_pub: [u8; 32],
    pub timestamp_ms: i64,
    pub status: MessageStatus,
    pub payload_enc: Vec<u8>,
    pub payload_nonce: [u8; 12],
    pub sig: [u8; 64],
}

pub struct MessagesDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> MessagesDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    /// INSERT OR IGNORE — silently skips if the message already exists (dedup
    /// across both peers). Returns whether a row was inserted.
    pub async fn insert(&self, m: NewMessage) -> Result<bool> {
        let affected = sqlx::query(
            "INSERT OR IGNORE INTO messages \
             (message_id, conversation_id, sender_pub, recipient_pub, timestamp_ms, \
              status, payload_enc, payload_nonce, sig, inserted_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&m.message_id.as_bytes()[..])
        .bind(&m.conversation_id.as_bytes()[..])
        .bind(&m.sender_pub[..])
        .bind(&m.recipient_pub[..])
        .bind(m.timestamp_ms)
        .bind(m.status.as_i64())
        .bind(&m.payload_enc)
        .bind(&m.payload_nonce[..])
        .bind(&m.sig[..])
        .bind(now_ms())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("messages.insert: {e}")))?
        .rows_affected();
        Ok(affected > 0)
    }

    pub async fn update_status(&self, id: &MessageId, status: MessageStatus) -> Result<()> {
        sqlx::query("UPDATE messages SET status = ? WHERE message_id = ?")
            .bind(status.as_i64())
            .bind(&id.as_bytes()[..])
            .execute(self.pool)
            .await
            .map_err(|e| AppError::storage(format!("messages.update_status: {e}")))?;
        Ok(())
    }

    /// Fetch a single message by id. Used by the outbound-queue drain so
    /// it doesn't have to page the whole conversation (which strands a
    /// queued message older than the page when a conversation is large).
    pub async fn get(&self, message_id: &MessageId) -> Result<Option<Message>> {
        let row: Option<RawMessage> = sqlx::query_as::<_, RawMessage>(
            "SELECT message_id, conversation_id, sender_pub, recipient_pub, timestamp_ms, \
                    status, payload_enc, payload_nonce, sig, inserted_at \
             FROM messages WHERE message_id = ?",
        )
        .bind(&message_id.as_bytes()[..])
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("messages.get: {e}")))?;
        row.map(RawMessage::decode).transpose()
    }

    pub async fn list_for_conversation(
        &self,
        conv: &ConversationId,
        limit: i64,
    ) -> Result<Vec<Message>> {
        let rows: Vec<RawMessage> = sqlx::query_as::<_, RawMessage>(
            "SELECT message_id, conversation_id, sender_pub, recipient_pub, timestamp_ms, \
                    status, payload_enc, payload_nonce, sig, inserted_at \
             FROM messages WHERE conversation_id = ? ORDER BY timestamp_ms ASC LIMIT ?",
        )
        .bind(&conv.as_bytes()[..])
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("messages.list: {e}")))?;

        rows.into_iter().map(RawMessage::decode).collect()
    }

    pub async fn highest_inbound(
        &self,
        conv: &ConversationId,
        recipient_pub: &[u8; 32],
    ) -> Result<Option<MessageId>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT message_id FROM messages \
             WHERE conversation_id = ? AND recipient_pub = ? \
             ORDER BY message_id DESC LIMIT 1",
        )
        .bind(&conv.as_bytes()[..])
        .bind(&recipient_pub[..])
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("messages.highest_inbound: {e}")))?;
        row.map(|(b,)| try_msg_id(b)).transpose()
    }

    pub async fn highest_outbound(
        &self,
        conv: &ConversationId,
        sender_pub: &[u8; 32],
    ) -> Result<Option<MessageId>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT message_id FROM messages \
             WHERE conversation_id = ? AND sender_pub = ? \
             ORDER BY message_id DESC LIMIT 1",
        )
        .bind(&conv.as_bytes()[..])
        .bind(&sender_pub[..])
        .fetch_optional(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("messages.highest_outbound: {e}")))?;
        row.map(|(b,)| try_msg_id(b)).transpose()
    }

    /// Pull messages with `message_id > since` ordered ascending, up to `limit`.
    /// Outbound-sync page: messages in `conv` that WE (`sender_pub`) sent,
    /// after `since`, ascending, capped at `limit`. The `sender_pub` filter
    /// lives in the query (not the caller) so a full page reliably means
    /// "more of OUR rows exist" — the sync responder derives `has_more`
    /// from this length, and the puller never gets a spuriously-empty page
    /// (which would truncate the reconcile when a conversation interleaves
    /// both directions).
    pub async fn pull_outbound_after(
        &self,
        conv: &ConversationId,
        sender_pub: &[u8; 32],
        since: Option<MessageId>,
        limit: i64,
    ) -> Result<Vec<Message>> {
        let conv_bytes = conv.as_bytes().to_vec();
        let sender = sender_pub.to_vec();
        let rows: Vec<RawMessage> =
            match since {
                Some(s) => sqlx::query_as::<_, RawMessage>(
                    "SELECT message_id, conversation_id, sender_pub, recipient_pub, timestamp_ms, \
                        status, payload_enc, payload_nonce, sig, inserted_at \
                 FROM messages WHERE conversation_id = ? AND sender_pub = ? AND message_id > ? \
                 ORDER BY message_id ASC LIMIT ?",
                )
                .bind(conv_bytes)
                .bind(sender)
                .bind(s.as_bytes().to_vec())
                .bind(limit)
                .fetch_all(self.pool)
                .await,
                None => sqlx::query_as::<_, RawMessage>(
                    "SELECT message_id, conversation_id, sender_pub, recipient_pub, timestamp_ms, \
                        status, payload_enc, payload_nonce, sig, inserted_at \
                 FROM messages WHERE conversation_id = ? AND sender_pub = ? \
                 ORDER BY message_id ASC LIMIT ?",
                )
                .bind(conv_bytes)
                .bind(sender)
                .bind(limit)
                .fetch_all(self.pool)
                .await,
            }
            .map_err(|e| AppError::storage(format!("messages.pull_outbound_after: {e}")))?;

        rows.into_iter().map(RawMessage::decode).collect()
    }
}

#[derive(sqlx::FromRow)]
struct RawMessage {
    message_id: Vec<u8>,
    conversation_id: Vec<u8>,
    sender_pub: Vec<u8>,
    recipient_pub: Vec<u8>,
    timestamp_ms: i64,
    status: i64,
    payload_enc: Vec<u8>,
    payload_nonce: Vec<u8>,
    sig: Vec<u8>,
    inserted_at: i64,
}

impl RawMessage {
    fn decode(self) -> Result<Message> {
        Ok(Message {
            message_id: try_msg_id(self.message_id)?,
            conversation_id: try_conv_id(self.conversation_id)?,
            sender_pub: try_pubkey(self.sender_pub, "messages.sender_pub")?,
            recipient_pub: try_pubkey(self.recipient_pub, "messages.recipient_pub")?,
            timestamp_ms: self.timestamp_ms,
            status: MessageStatus::from_i64(self.status).ok_or_else(|| {
                AppError::storage(format!("messages.status: invalid {}", self.status))
            })?,
            payload_enc: self.payload_enc,
            payload_nonce: try_nonce(self.payload_nonce)?,
            sig: try_sig(self.sig)?,
            inserted_at: self.inserted_at,
        })
    }
}

fn try_msg_id(b: Vec<u8>) -> Result<MessageId> {
    let bytes: [u8; 16] = b
        .try_into()
        .map_err(|_| AppError::storage("messages.message_id: expected 16 bytes"))?;
    Ok(MessageId::from_bytes(bytes))
}

fn try_conv_id(b: Vec<u8>) -> Result<ConversationId> {
    let bytes: [u8; 16] = b
        .try_into()
        .map_err(|_| AppError::storage("messages.conversation_id: expected 16 bytes"))?;
    Ok(ConversationId(bytes))
}

fn try_pubkey(b: Vec<u8>, ctx: &'static str) -> Result<[u8; 32]> {
    b.try_into()
        .map_err(|_| AppError::storage(format!("{ctx}: expected 32 bytes")))
}

fn try_nonce(b: Vec<u8>) -> Result<[u8; 12]> {
    b.try_into()
        .map_err(|_| AppError::storage("messages.payload_nonce: expected 12 bytes"))
}

fn try_sig(b: Vec<u8>) -> Result<[u8; 64]> {
    b.try_into()
        .map_err(|_| AppError::storage("messages.sig: expected 64 bytes"))
}
