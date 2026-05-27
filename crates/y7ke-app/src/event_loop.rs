//! Background task: drains `NetEvent`s from the swarm, performs storage
//! operations, and emits user-visible `AppEvent`s on the broadcast channel.

use std::sync::Arc;

use tokio::sync::broadcast;
use y7ke_core::crypto::VerifyingKey;
use y7ke_core::error::Result;
use y7ke_core::{
    AppError, AppEvent, ConnectionKind, ContactStatus, ConversationId, MessageStatus, Y7Id,
};
use y7ke_net::protocol::{MessageEnvelope, MsgResp, SyncReq, SyncResp};
use y7ke_net::{NetEvent, PeerId};
use y7ke_storage::dao::contacts::NewContact;
use y7ke_storage::dao::messages::NewMessage;
use y7ke_storage::dao::requests::{NewRequest, RequestDirection};

use crate::app::AppInner;
use crate::{handshake, messaging};

/// Main entry point. Runs until the broadcast channel closes (i.e. the
/// swarm task has exited).
pub(crate) async fn run(
    inner: Arc<AppInner>,
    event_tx: broadcast::Sender<AppEvent>,
    mut net_rx: broadcast::Receiver<NetEvent>,
) {
    loop {
        match net_rx.recv().await {
            Ok(ev) => {
                if let Err(e) = dispatch(&inner, &event_tx, ev).await {
                    tracing::warn!(error = %e, "event loop handler failed");
                    let _ = event_tx.send(AppEvent::BackgroundError {
                        message: e.to_string(),
                    });
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("net event channel closed; event loop exiting");
                return;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "event loop lagged");
            }
        }
    }
}

async fn dispatch(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    event: NetEvent,
) -> Result<()> {
    match event {
        NetEvent::Listening { addr } => {
            tracing::info!(addr = %addr, "listening");
            Ok(())
        }
        NetEvent::PeerDiscovered { peer, addrs, y7_id } => {
            tracing::debug!(peer = %peer, addrs = ?addrs, y7_id = ?y7_id, "peer discovered");
            if let Some(y7) = y7_id {
                drain_queue_for_peer(inner, &y7, peer).await?;
            }
            Ok(())
        }
        NetEvent::ConnectionEstablished { peer, kind } => {
            if let Some(y7) = y7ke_net::y7_id_from_peer_id(&peer) {
                let _ = event_tx.send(AppEvent::PresenceChanged {
                    y7_id: y7.to_uri(),
                    connection: kind,
                });
                drain_queue_for_peer(inner, &y7, peer).await?;
            }
            Ok(())
        }
        NetEvent::ConnectionClosed { peer } => {
            if let Some(y7) = y7ke_net::y7_id_from_peer_id(&peer) {
                let _ = event_tx.send(AppEvent::PresenceChanged {
                    y7_id: y7.to_uri(),
                    connection: ConnectionKind::Offline,
                });
            }
            Ok(())
        }
        NetEvent::HandshakeReceived {
            peer: _,
            request,
            channel,
        } => handle_handshake(inner, event_tx, request, channel).await,
        NetEvent::MsgReceived {
            peer: _,
            request,
            channel,
        } => handle_msg(inner, event_tx, request.envelope, channel).await,
        NetEvent::SyncReceived {
            peer,
            request,
            channel,
        } => handle_sync(inner, peer, request, channel).await,
        NetEvent::Error { message } => {
            tracing::warn!(message = %message, "net error");
            Ok(())
        }
    }
}

async fn handle_handshake(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    request: y7ke_net::protocol::HandshakeReq,
    channel: y7ke_net::handle::TakeOnce<
        libp2p::request_response::ResponseChannel<y7ke_net::protocol::HandshakeResp>,
    >,
) -> Result<()> {
    let greeting = request.greeting.clone();
    let (resp, session_key, initiator_y7) =
        handshake::respond(&inner.me, &inner.my_pubkey, &request)?;

    // Persist session.
    inner
        .db
        .sessions()
        .upsert(&initiator_y7, session_key)
        .await?;

    // Upsert pending-in contact.
    let existing = inner.db.contacts().get(&initiator_y7).await?;
    let was_new = existing.is_none();
    if existing.is_none() {
        inner
            .db
            .contacts()
            .insert(NewContact {
                y7_id: initiator_y7,
                ed25519_pub: *initiator_y7.pubkey(),
                nickname: None,
                status: ContactStatus::PendingIn,
            })
            .await?;
    }

    // Insert a request row so the UI surfaces it.
    inner
        .db
        .requests()
        .insert(NewRequest {
            direction: RequestDirection::Incoming,
            peer_y7_id: initiator_y7,
            initial_text: greeting.clone(),
        })
        .await?;

    // Respond on the wire.
    inner.net.respond_handshake_take(channel, resp).await?;

    if was_new {
        let _ = event_tx.send(AppEvent::RequestReceived {
            y7_id: initiator_y7.to_uri(),
            greeting,
        });
    }
    Ok(())
}

async fn handle_msg(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    envelope: MessageEnvelope,
    channel: y7ke_net::handle::TakeOnce<libp2p::request_response::ResponseChannel<MsgResp>>,
) -> Result<()> {
    let sender_y7 = Y7Id::from_pubkey(envelope.sender_pub);

    // Need a session — established by an earlier handshake.
    let session = inner
        .db
        .sessions()
        .get(&sender_y7)
        .await?
        .ok_or_else(|| AppError::network(format!("no session for {sender_y7}")))?;

    // Verify + decrypt.
    let verifying = VerifyingKey::from_bytes(&envelope.sender_pub)?;
    let text = messaging::open_envelope(&envelope, &verifying, &session.session_key)?;

    let conversation_id = ConversationId::between(&sender_y7, &inner.my_y7_id);

    // Persist (INSERT OR IGNORE → dedup).
    let inserted = inner
        .db
        .messages()
        .insert(NewMessage {
            message_id: y7ke_core::MessageId::from_bytes(envelope.message_id),
            conversation_id,
            sender_pub: envelope.sender_pub,
            recipient_pub: inner.my_pubkey,
            timestamp_ms: envelope.timestamp_ms,
            status: MessageStatus::Synced,
            payload_enc: envelope.ciphertext.clone(),
            payload_nonce: envelope.nonce,
            sig: envelope.sig,
        })
        .await?;

    // Ack on the wire — always, even if dedup'd.
    inner
        .net
        .respond_msg_take(channel, MsgResp { ack: true })
        .await?;

    if inserted {
        let _ = event_tx.send(AppEvent::MessageReceived {
            conversation_id: conversation_id.to_hex(),
            message_id: y7ke_core::MessageId::from_bytes(envelope.message_id).to_string(),
            sender_y7_id: sender_y7.to_uri(),
            timestamp_ms: envelope.timestamp_ms,
            text,
        });
    }
    Ok(())
}

async fn handle_sync(
    inner: &Arc<AppInner>,
    _peer: PeerId,
    request: SyncReq,
    channel: y7ke_net::handle::TakeOnce<libp2p::request_response::ResponseChannel<SyncResp>>,
) -> Result<()> {
    let resp = match request {
        SyncReq::Header { conversations: _ } => {
            // V1 minimal sync: respond with empty digest (we use queue-based
            // retry instead of header-based reconcile).
            SyncResp::HeaderAck { ours: Vec::new() }
        }
        SyncReq::Pull {
            conversation_id,
            since,
            limit,
        } => {
            let conv = ConversationId(conversation_id);
            let since_id = since.map(y7ke_core::MessageId::from_bytes);
            let rows = inner
                .db
                .messages()
                .pull_after(&conv, since_id, limit as i64)
                .await?;
            let envelopes: Vec<MessageEnvelope> = rows
                .into_iter()
                .map(|m| MessageEnvelope {
                    message_id: *m.message_id.as_bytes(),
                    sender_pub: m.sender_pub,
                    timestamp_ms: m.timestamp_ms,
                    nonce: m.payload_nonce,
                    ciphertext: m.payload_enc,
                    sig: m.sig,
                })
                .collect();
            let has_more = envelopes.len() as u16 == limit;
            SyncResp::Pull {
                envelopes,
                has_more,
            }
        }
        SyncReq::Ack {
            conversation_id: _,
            confirmed_ids,
        } => {
            for mid in confirmed_ids {
                let id = y7ke_core::MessageId::from_bytes(mid);
                let _ = inner
                    .db
                    .messages()
                    .update_status(&id, MessageStatus::Synced)
                    .await;
            }
            SyncResp::Ack
        }
    };
    inner.net.respond_sync_take(channel, resp).await?;
    Ok(())
}

/// On peer reconnect, retry any outbound messages we have queued for them.
/// Successful sends drop the row from `sync_queue` and update `messages.status`.
async fn drain_queue_for_peer(
    inner: &Arc<AppInner>,
    peer_y7: &Y7Id,
    peer_id: PeerId,
) -> Result<()> {
    // Fetch all due entries (now + huge limit). For V1 we drain everything;
    // V2 will respect a smaller limit.
    let due = inner.db.sync_queue().due(i64::MAX, 256).await?;
    for entry in due {
        if &entry.target_peer_y7_id != peer_y7 {
            continue;
        }
        // Look up the message and re-send.
        let conv = ConversationId::between(&inner.my_y7_id, peer_y7);
        let messages = inner
            .db
            .messages()
            .list_for_conversation(&conv, 1000)
            .await?;
        let Some(message) = messages
            .into_iter()
            .find(|m| m.message_id == entry.message_id)
        else {
            // Row vanished — drop the queue entry.
            let _ = inner
                .db
                .sync_queue()
                .remove(&entry.message_id, peer_y7)
                .await;
            continue;
        };
        let envelope = MessageEnvelope {
            message_id: *message.message_id.as_bytes(),
            sender_pub: message.sender_pub,
            timestamp_ms: message.timestamp_ms,
            nonce: message.payload_nonce,
            ciphertext: message.payload_enc,
            sig: message.sig,
        };
        match inner
            .net
            .send_msg(peer_id, y7ke_net::protocol::MsgReq { envelope })
            .await
        {
            Ok(resp) if resp.ack => {
                inner
                    .db
                    .messages()
                    .update_status(&message.message_id, MessageStatus::Sent)
                    .await?;
                inner
                    .db
                    .sync_queue()
                    .remove(&message.message_id, peer_y7)
                    .await?;
            }
            Ok(_) | Err(_) => {
                // Peer refused or transport blip — bump retry.
                let next = next_retry_at(entry.attempts);
                inner
                    .db
                    .sync_queue()
                    .bump(&message.message_id, peer_y7, entry.attempts + 1, next)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Exponential backoff capped at 1 hour: `min(2^attempts * 30s, 1h)`.
pub fn next_retry_at(attempts: i64) -> i64 {
    let now = y7ke_storage::now_ms();
    let secs = (1i64 << attempts.min(7)) * 30; // 30s, 60s, 120s, 240s, 480s, ...
    let capped = secs.min(3600);
    now + capped * 1000
}
