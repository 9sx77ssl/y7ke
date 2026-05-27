//! Message commands: list + non-blocking send.

use std::sync::Arc;

use tokio::sync::broadcast;

use y7ke_core::error::{AppError, Result};
use y7ke_core::{AppEvent, ConversationId, MessageId, MessageStatus, Y7Id};
use y7ke_net::{peer_id_from_y7, PeerId};
use y7ke_storage::dao::messages::NewMessage;

use crate::messaging;
use crate::views::MessageView;

use super::{AppHandle, AppInner, MAX_MESSAGE_BYTES, SEND_TIMEOUT};

impl AppHandle {
    /// Decrypt + return messages for `peer`. Skips control payloads.
    pub async fn list_messages(&self, peer: Y7Id, limit: i64) -> Result<Vec<MessageView>> {
        let conv = ConversationId::between(&self.inner.my_y7_id, &peer);
        let rows = self
            .inner
            .db
            .messages()
            .list_for_conversation(&conv, limit)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for m in rows {
            let sender_y7 = Y7Id::from_pubkey(m.sender_pub);
            let session_owner = if m.sender_pub == self.inner.my_pubkey {
                Y7Id::from_pubkey(m.recipient_pub)
            } else {
                sender_y7
            };
            let session = self.inner.db.sessions().get(&session_owner).await?;
            let text = match session {
                Some(s) => {
                    let verifying = y7ke_core::crypto::VerifyingKey::from_bytes(&m.sender_pub)?;
                    let envelope = y7ke_net::protocol::MessageEnvelope {
                        message_id: *m.message_id.as_bytes(),
                        sender_pub: m.sender_pub,
                        timestamp_ms: m.timestamp_ms,
                        nonce: m.payload_nonce,
                        ciphertext: m.payload_enc.clone(),
                        sig: m.sig,
                    };
                    match messaging::open_envelope(&envelope, &verifying, &s.session_key) {
                        Ok(messaging::PlaintextKind::Text(t)) => t,
                        Ok(messaging::PlaintextKind::Control(_)) => continue,
                        Err(_) => "<decryption failed>".into(),
                    }
                }
                None => "<no session>".into(),
            };
            out.push(MessageView {
                message_id: m.message_id.to_string(),
                conversation_id: m.conversation_id.to_hex(),
                sender_y7_id: sender_y7.to_uri(),
                text,
                timestamp_ms: m.timestamp_ms,
                status: m.status.as_i64(),
                is_mine: m.sender_pub == self.inner.my_pubkey,
            });
        }
        Ok(out)
    }

    /// Drop queued outbound retries for `peer` without delivering. Test-only.
    #[doc(hidden)]
    pub async fn debug_clear_outbound_queue(&self, peer: &Y7Id) -> Result<usize> {
        let due = self.inner.db.sync_queue().due(i64::MAX, 100_000).await?;
        let mut n = 0;
        for entry in due {
            if &entry.target_peer_y7_id == peer {
                self.inner
                    .db
                    .sync_queue()
                    .remove(&entry.message_id, peer)
                    .await?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Non-blocking send: persists + spawns bg push, returns immediately.
    pub async fn send_message(&self, to: Y7Id, text: String) -> Result<MessageId> {
        if to == self.inner.my_y7_id {
            return Err(AppError::invalid_input("cannot message yourself"));
        }
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(AppError::invalid_input(format!(
                "message exceeds {MAX_MESSAGE_BYTES} bytes ({} bytes given)",
                text.len()
            )));
        }
        let peer_id = peer_id_from_y7(&to)?;
        let session = self.inner.db.sessions().get(&to).await?.ok_or_else(|| {
            AppError::invalid_input(format!(
                "no established session with {to} — add them as a contact first"
            ))
        })?;
        let (message_id, envelope, timestamp_ms) = messaging::seal_outgoing(
            &self.inner.me,
            &self.inner.my_pubkey,
            &session.session_key,
            &text,
        )?;
        let conversation_id = ConversationId::between(&self.inner.my_y7_id, &to);

        self.inner
            .db
            .messages()
            .insert(NewMessage {
                message_id,
                conversation_id,
                sender_pub: self.inner.my_pubkey,
                recipient_pub: *to.pubkey(),
                timestamp_ms,
                status: MessageStatus::Sending,
                payload_enc: envelope.ciphertext.clone(),
                payload_nonce: envelope.nonce,
                sig: envelope.sig,
            })
            .await?;

        let inner = Arc::clone(&self.inner);
        let event_tx = self.event_tx.clone();
        let req = y7ke_net::protocol::MsgReq { envelope };
        tokio::spawn(async move {
            push_one(&inner, &event_tx, message_id, to, peer_id, req).await;
        });

        Ok(message_id)
    }
}

/// Background sender: timeout-wrapped send_msg + status update + retry enqueue.
async fn push_one(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    message_id: MessageId,
    to: Y7Id,
    peer_id: PeerId,
    req: y7ke_net::protocol::MsgReq,
) {
    let result = tokio::time::timeout(SEND_TIMEOUT, inner.net.send_msg(peer_id, req)).await;
    let new_status = match result {
        Ok(Ok(resp)) if resp.ack => MessageStatus::Synced,
        other => {
            tracing::warn!(?other, %to, "send_msg failed; enqueuing");
            let next = crate::event_loop::next_retry_at(0);
            let _ = inner.db.sync_queue().enqueue(&message_id, &to, next).await;
            MessageStatus::Failed
        }
    };
    let _ = inner
        .db
        .messages()
        .update_status(&message_id, new_status)
        .await;
    let _ = event_tx.send(AppEvent::MessageStatusChanged {
        message_id: message_id.to_string(),
        status: new_status,
    });
}
