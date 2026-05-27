//! Background task: drains `NetEvent`s from the swarm, performs storage
//! operations, and emits user-visible `AppEvent`s on the broadcast channel.

use std::sync::Arc;

use tokio::sync::broadcast;
use y7ke_core::crypto::VerifyingKey;
use y7ke_core::error::Result;
use y7ke_core::{
    AppError, AppEvent, ConnectionKind, ContactStatus, ConversationId, MessageStatus, Y7Id,
};
use y7ke_net::protocol::{ConversationDigest, MessageEnvelope, MsgResp, SyncReq, SyncResp};
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
                // V2-A1: persist the addr list so future dials can take
                // the cache fast path instead of waiting on Kad lookup.
                if !addrs.is_empty() {
                    let strs: Vec<String> = addrs.iter().map(|a| a.to_string()).collect();
                    if let Ok(json) = serde_json::to_string(&strs) {
                        let _ = inner.db.peer_state().upsert_seen(&y7, Some(json)).await;
                    }
                }
                drain_queue_for_peer(inner, event_tx, &y7, peer).await?;
                spawn_kick_sync(inner.clone(), event_tx.clone(), y7, peer);
            } else {
                tracing::warn!(%peer, "peer discovered without recoverable Y7Id");
            }
            Ok(())
        }
        NetEvent::ConnectionEstablished { peer, kind } => {
            if let Some(y7) = y7ke_net::y7_id_from_peer_id(&peer) {
                inner.presence.write().await.insert(y7, kind);
                let _ = event_tx.send(AppEvent::PresenceChanged {
                    y7_id: y7.to_uri(),
                    connection: kind,
                });
                drain_queue_for_peer(inner, event_tx, &y7, peer).await?;
                spawn_kick_sync(inner.clone(), event_tx.clone(), y7, peer);
            } else {
                tracing::warn!(%peer, "connection established with non-Ed25519 peer (V1 should never see this)");
            }
            Ok(())
        }
        NetEvent::ConnectionClosed { peer } => {
            if let Some(y7) = y7ke_net::y7_id_from_peer_id(&peer) {
                inner
                    .presence
                    .write()
                    .await
                    .insert(y7, ConnectionKind::Offline);
                tracing::debug!(%y7, "connection closed → presence offline");
                let _ = event_tx.send(AppEvent::PresenceChanged {
                    y7_id: y7.to_uri(),
                    connection: ConnectionKind::Offline,
                });
            } else {
                tracing::debug!(%peer, "connection closed for non-Ed25519 peer");
            }
            Ok(())
        }
        NetEvent::HandshakeReceived {
            peer,
            request,
            channel,
        } => handle_handshake(inner, event_tx, peer, request, channel).await,
        NetEvent::MsgReceived {
            peer,
            request,
            channel,
        } => handle_msg(inner, event_tx, peer, request.envelope, channel).await,
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
    libp2p_peer: PeerId,
    request: y7ke_net::protocol::HandshakeReq,
    channel: y7ke_net::handle::TakeOnce<
        libp2p::request_response::ResponseChannel<y7ke_net::protocol::HandshakeResp>,
    >,
) -> Result<()> {
    if !inner.rate_limiter.allow_handshake(libp2p_peer).await {
        tracing::warn!(%libp2p_peer, "handshake rate-limited; refusing");
        let reject = handshake::reject_response(&inner.me, &inner.my_pubkey, &request);
        inner.net.respond_handshake_take(channel, reject).await?;
        return Ok(());
    }

    // M4: bind libp2p PeerId to the claimed initiator Ed25519 pubkey. The
    // Noise handshake already proved ownership of the libp2p key, and the
    // application signature inside the request proves ownership of
    // initiator_ed25519_pub. If these point to two different identities the
    // peer is misbehaving — refuse without touching storage.
    let claimed_id = y7ke_core::Y7Id::from_pubkey(request.initiator_ed25519_pub);
    let expected_peer = y7ke_net::peer_id_from_y7(&claimed_id)?;
    if expected_peer != libp2p_peer {
        tracing::warn!(
            connection_peer = %libp2p_peer,
            claimed = %claimed_id,
            "rejecting handshake — claimed identity does not match connection PeerId"
        );
        let reject = handshake::reject_response(&inner.me, &inner.my_pubkey, &request);
        inner.net.respond_handshake_take(channel, reject).await?;
        return Ok(());
    }

    let greeting = request.greeting.clone();

    // H1 backstop: if we already have a session for this peer, refuse to
    // overwrite it. Send `accept = false`; the initiator's
    // `finalize_initiator` rejects and keeps its own session intact.
    if inner.db.sessions().get(&claimed_id).await?.is_some() {
        tracing::debug!(%claimed_id, "handshake from peer with existing session — rejecting to preserve session keys");
        let reject = handshake::reject_response(&inner.me, &inner.my_pubkey, &request);
        inner.net.respond_handshake_take(channel, reject).await?;
        return Ok(());
    }

    let (resp, _session_key, initiator_y7) =
        handshake::respond(&inner.me, &inner.my_pubkey, &request)?;
    debug_assert_eq!(initiator_y7, claimed_id, "respond() must derive same Y7Id");

    inner.db.sessions().upsert(&initiator_y7).await?;

    // Upsert pending-in contact (only if new).
    let existing = inner.db.contacts().get(&initiator_y7).await?;
    let was_new_contact = existing.is_none();
    if was_new_contact {
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

    // H3: only insert a request row if no pending one already exists for
    // this peer. Otherwise a replayed HandshakeReq would spam the UI.
    let already_pending = inner
        .db
        .requests()
        .list_pending(Some(RequestDirection::Incoming))
        .await?
        .into_iter()
        .any(|r| r.peer_y7_id == initiator_y7);

    if !already_pending {
        inner
            .db
            .requests()
            .insert(NewRequest {
                direction: RequestDirection::Incoming,
                peer_y7_id: initiator_y7,
                initial_text: greeting.clone(),
            })
            .await?;
    }

    // Respond on the wire.
    inner.net.respond_handshake_take(channel, resp).await?;

    if was_new_contact && !already_pending {
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
    libp2p_peer: PeerId,
    envelope: MessageEnvelope,
    channel: y7ke_net::handle::TakeOnce<libp2p::request_response::ResponseChannel<MsgResp>>,
) -> Result<()> {
    if !inner.rate_limiter.allow_msg(libp2p_peer).await {
        tracing::warn!(%libp2p_peer, "msg rate-limited; refusing");
        inner
            .net
            .respond_msg_take(channel, MsgResp { ack: false })
            .await?;
        return Ok(());
    }

    // M2: cap ciphertext size. ChaCha20-Poly1305 adds a 16-byte tag, so allow
    // MAX_MESSAGE_BYTES + 16 + some slack.
    if envelope.ciphertext.len() > crate::app::MAX_MESSAGE_BYTES + 256 {
        tracing::warn!(
            size = envelope.ciphertext.len(),
            "rejecting oversized inbound message"
        );
        inner
            .net
            .respond_msg_take(channel, MsgResp { ack: false })
            .await?;
        return Ok(());
    }

    let sender_y7 = Y7Id::from_pubkey(envelope.sender_pub);

    // M4: verify the connection's PeerId matches the claimed sender pubkey.
    let expected_peer = y7ke_net::peer_id_from_y7(&sender_y7)?;
    if expected_peer != libp2p_peer {
        tracing::warn!(
            connection_peer = %libp2p_peer,
            claimed = %sender_y7,
            "rejecting message — sender does not match connection PeerId"
        );
        inner
            .net
            .respond_msg_take(channel, MsgResp { ack: false })
            .await?;
        return Ok(());
    }

    // Need a session — established by an earlier handshake.
    if inner.db.sessions().get(&sender_y7).await?.is_none() {
        return Err(AppError::network(format!("no session for {sender_y7}")));
    }
    let conversation_id = ConversationId::between(&sender_y7, &inner.my_y7_id);
    let conv_key =
        messaging::derive_conv_key(&inner.me, &envelope.sender_pub, conversation_id.as_bytes())?;
    let verifying = VerifyingKey::from_bytes(&envelope.sender_pub)?;
    let kind = messaging::open_envelope(&envelope, &verifying, &conv_key)?;

    // Control payloads don't land in `messages` — dispatch inline.
    let text = match kind {
        messaging::PlaintextKind::Text(t) => t,
        messaging::PlaintextKind::Control(ctrl) => {
            handle_control(inner, event_tx, sender_y7, ctrl).await?;
            inner
                .net
                .respond_msg_take(channel, MsgResp { ack: true })
                .await?;
            return Ok(());
        }
    };

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

    // Auto-promote — only for OUR outgoing requests. If we initiated the
    // request (our contact = pending_out) and the peer just sent us a
    // message, that's the practical signal they accepted us; resolve the
    // outgoing request and promote the contact.
    //
    // We deliberately do NOT promote pending_in. That status means the peer
    // initiated the handshake and we haven't manually accepted yet — letting
    // their messages auto-promote us would bypass the user's accept/reject
    // gate. Messages from pending_in peers are still stored (the user can
    // review them by clicking the pending contact in the sidebar) but the
    // contact stays gated until accept_request is called.
    if let Some(contact) = inner.db.contacts().get(&sender_y7).await? {
        if matches!(contact.status, y7ke_core::ContactStatus::PendingOut) {
            inner
                .db
                .contacts()
                .update_status(&sender_y7, y7ke_core::ContactStatus::Accepted)
                .await?;
            for r in inner.db.requests().list_pending(None).await? {
                if r.peer_y7_id == sender_y7 {
                    let _ = inner
                        .db
                        .requests()
                        .resolve(r.id, y7ke_core::RequestResolution::Accepted)
                        .await;
                    let _ = event_tx.send(AppEvent::RequestResolved {
                        y7_id: sender_y7.to_uri(),
                        resolution: y7ke_core::RequestResolution::Accepted,
                    });
                }
            }
            let _ = event_tx.send(AppEvent::ContactAdded {
                y7_id: sender_y7.to_uri(),
            });
        }
    }

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
    peer: PeerId,
    request: SyncReq,
    channel: y7ke_net::handle::TakeOnce<libp2p::request_response::ResponseChannel<SyncResp>>,
) -> Result<()> {
    if !inner.rate_limiter.allow_sync(peer).await {
        tracing::warn!(%peer, "sync rate-limited; refusing");
        let resp = empty_sync_resp_for(&request);
        inner.net.respond_sync_take(channel, resp).await?;
        return Ok(());
    }

    // H2: identify the requester so we can scope conversation-pulls to a
    // single (self, requester) pair. Anyone else who has guessed at a
    // ConversationId must not get messages back.
    let requester_y7 = match y7ke_net::y7_id_from_peer_id(&peer) {
        Some(y7) => y7,
        None => {
            tracing::warn!(%peer, "sync request from non-Ed25519 peer; refusing");
            let resp = empty_sync_resp_for(&request);
            inner.net.respond_sync_take(channel, resp).await?;
            return Ok(());
        }
    };

    let resp = match request {
        SyncReq::Header { conversations } => {
            // For each conversation the requester listed, return our own
            // outbound/inbound HWMs scoped to (self, requester). Anything else
            // is silently dropped.
            let expected = ConversationId::between(&inner.my_y7_id, &requester_y7);
            let mut ours = Vec::new();
            for cd in &conversations {
                if cd.conversation_id != *expected.as_bytes() {
                    continue;
                }
                let my_outbound = inner
                    .db
                    .messages()
                    .highest_outbound(&expected, &inner.my_pubkey)
                    .await?;
                let my_inbound = inner
                    .db
                    .messages()
                    .highest_inbound(&expected, &inner.my_pubkey)
                    .await?;
                ours.push(ConversationDigest {
                    conversation_id: cd.conversation_id,
                    highest_outbound_msg_id: my_outbound.map(|m| *m.as_bytes()),
                    highest_inbound_msg_id: my_inbound.map(|m| *m.as_bytes()),
                });
            }
            SyncResp::HeaderAck { ours }
        }
        SyncReq::Pull {
            conversation_id,
            since,
            limit,
        } => {
            // H2: only return messages for the (self, requester) conversation.
            let expected = ConversationId::between(&inner.my_y7_id, &requester_y7);
            if expected.0 != conversation_id {
                tracing::warn!(
                    requester = %requester_y7,
                    "sync Pull for non-participating conversation; refusing"
                );
                SyncResp::Pull {
                    envelopes: Vec::new(),
                    has_more: false,
                }
            } else {
                let since_id = since.map(y7ke_core::MessageId::from_bytes);
                let rows = inner
                    .db
                    .messages()
                    .pull_after(&expected, since_id, limit as i64)
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
        }
        SyncReq::Ack {
            conversation_id,
            confirmed_ids,
        } => {
            // H2: same scoping rule for Ack — only honor for the
            // (self, requester) conversation, and only for messages addressed
            // to the requester.
            let expected = ConversationId::between(&inner.my_y7_id, &requester_y7);
            if expected.0 != conversation_id {
                tracing::warn!(
                    requester = %requester_y7,
                    "sync Ack for non-participating conversation; ignoring"
                );
            } else {
                for mid in confirmed_ids {
                    let id = y7ke_core::MessageId::from_bytes(mid);
                    let _ = inner
                        .db
                        .messages()
                        .update_status(&id, MessageStatus::Synced)
                        .await;
                }
            }
            SyncResp::Ack
        }
    };
    inner.net.respond_sync_take(channel, resp).await?;
    Ok(())
}

fn empty_sync_resp_for(req: &SyncReq) -> SyncResp {
    match req {
        SyncReq::Header { .. } => SyncResp::HeaderAck { ours: Vec::new() },
        SyncReq::Pull { .. } => SyncResp::Pull {
            envelopes: Vec::new(),
            has_more: false,
        },
        SyncReq::Ack { .. } => SyncResp::Ack,
    }
}

/// On peer reconnect, retry any outbound messages we have queued for them.
/// Successful sends drop the row from `sync_queue` and update
/// `messages.status` to `Synced` (peer acked → both sides hold the row).
async fn drain_queue_for_peer(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    peer_y7: &Y7Id,
    peer_id: PeerId,
) -> Result<()> {
    let due = inner.db.sync_queue().due(i64::MAX, 256).await?;
    for entry in due {
        if &entry.target_peer_y7_id != peer_y7 {
            continue;
        }
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
                // M1: peer acked → both sides hold it → Synced (not Sent).
                inner
                    .db
                    .messages()
                    .update_status(&message.message_id, MessageStatus::Delivered)
                    .await?;
                inner
                    .db
                    .sync_queue()
                    .remove(&message.message_id, peer_y7)
                    .await?;
                let _ = event_tx.send(AppEvent::MessageStatusChanged {
                    message_id: message.message_id.to_string(),
                    status: MessageStatus::Delivered,
                });
            }
            Ok(_) | Err(_) => {
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

// 50 × 64 = 3200 envelopes max per reconnect.
const SYNC_PULL_LIMIT: u16 = 50;
const SYNC_MAX_PULL_PAGES: u8 = 64;

/// Detach kick_sync to a tokio task so the event loop never blocks on
/// send_sync (otherwise both peers can deadlock waiting for each other).
fn spawn_kick_sync(
    inner: Arc<AppInner>,
    event_tx: broadcast::Sender<AppEvent>,
    peer_y7: Y7Id,
    peer_id: PeerId,
) {
    tokio::spawn(async move {
        if let Err(e) = kick_sync_for_peer(&inner, &event_tx, &peer_y7, peer_id).await {
            tracing::debug!(%peer_y7, error = %e, "kick_sync_for_peer failed");
        }
    });
}

/// Initiator-side `/y7ke/sync/1.0.0`: Header → Pull loop → Ack.
async fn kick_sync_for_peer(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    peer_y7: &Y7Id,
    peer_id: PeerId,
) -> Result<()> {
    let Some(contact) = inner.db.contacts().get(peer_y7).await? else {
        return Ok(());
    };
    if contact.status != ContactStatus::Accepted {
        return Ok(());
    }

    let conv = ConversationId::between(&inner.my_y7_id, peer_y7);
    let my_outbound = inner
        .db
        .messages()
        .highest_outbound(&conv, &inner.my_pubkey)
        .await?;
    let my_inbound = inner
        .db
        .messages()
        .highest_inbound(&conv, &inner.my_pubkey)
        .await?;
    let my_digest = ConversationDigest {
        conversation_id: *conv.as_bytes(),
        highest_outbound_msg_id: my_outbound.map(|m| *m.as_bytes()),
        highest_inbound_msg_id: my_inbound.map(|m| *m.as_bytes()),
    };

    let header_resp = match inner
        .net
        .send_sync(
            peer_id,
            SyncReq::Header {
                conversations: vec![my_digest],
            },
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(%peer_y7, error = %e, "sync Header send failed");
            return Ok(());
        }
    };

    let their = match header_resp {
        SyncResp::HeaderAck { ours } => ours
            .into_iter()
            .find(|d| d.conversation_id == *conv.as_bytes()),
        other => {
            tracing::warn!(?other, "unexpected response to sync Header");
            return Ok(());
        }
    };

    let Some(their) = their else {
        return Ok(());
    };

    // UUIDv7 byte-order matches chronological + SQL ORDER BY.
    let need_pull = match (their.highest_outbound_msg_id, my_inbound) {
        (Some(theirs_out), Some(my_in)) => theirs_out > *my_in.as_bytes(),
        (Some(_), None) => true,
        (None, _) => false,
    };
    if !need_pull {
        return Ok(());
    }

    let mut since = my_inbound.map(|m| *m.as_bytes());
    let mut newly_persisted: Vec<[u8; 16]> = Vec::new();
    for _ in 0..SYNC_MAX_PULL_PAGES {
        let resp = match inner
            .net
            .send_sync(
                peer_id,
                SyncReq::Pull {
                    conversation_id: *conv.as_bytes(),
                    since,
                    limit: SYNC_PULL_LIMIT,
                },
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(%peer_y7, error = %e, "sync Pull send failed mid-loop");
                break;
            }
        };
        let (envelopes, has_more) = match resp {
            SyncResp::Pull {
                envelopes,
                has_more,
            } => (envelopes, has_more),
            other => {
                tracing::warn!(?other, "unexpected response to sync Pull");
                break;
            }
        };
        if envelopes.is_empty() {
            break;
        }
        let last_id = envelopes.last().map(|e| e.message_id);
        for env in envelopes {
            match ingest_synced_envelope(inner, event_tx, peer_y7, &env).await {
                Ok(true) => newly_persisted.push(env.message_id),
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "ingest_synced_envelope failed"),
            }
        }
        if !has_more {
            break;
        }
        since = last_id;
    }

    if !newly_persisted.is_empty() {
        let _ = inner
            .net
            .send_sync(
                peer_id,
                SyncReq::Ack {
                    conversation_id: *conv.as_bytes(),
                    confirmed_ids: newly_persisted,
                },
            )
            .await;
    }

    Ok(())
}

/// Verify sig, decrypt, INSERT OR IGNORE; emit MessageReceived on new row.
async fn ingest_synced_envelope(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    expected_sender: &Y7Id,
    envelope: &MessageEnvelope,
) -> Result<bool> {
    // M2 cap.
    if envelope.ciphertext.len() > crate::app::MAX_MESSAGE_BYTES + 256 {
        tracing::warn!(
            size = envelope.ciphertext.len(),
            "oversized synced envelope"
        );
        return Ok(false);
    }

    let sender_y7 = Y7Id::from_pubkey(envelope.sender_pub);
    if &sender_y7 != expected_sender {
        tracing::warn!(claimed = %sender_y7, expected = %expected_sender, "synced envelope signed by wrong key");
        return Ok(false);
    }

    if inner.db.sessions().get(&sender_y7).await?.is_none() {
        return Err(AppError::network(format!("no session for {sender_y7}")));
    }
    let conversation_id = ConversationId::between(&sender_y7, &inner.my_y7_id);
    let conv_key =
        messaging::derive_conv_key(&inner.me, &envelope.sender_pub, conversation_id.as_bytes())?;
    let verifying = VerifyingKey::from_bytes(&envelope.sender_pub)?;
    let kind = messaging::open_envelope(envelope, &verifying, &conv_key)?;
    let text = match kind {
        messaging::PlaintextKind::Text(t) => t,
        messaging::PlaintextKind::Control(_) => {
            tracing::debug!("control payload arrived via sync; ignoring");
            return Ok(false);
        }
    };
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

    if inserted {
        let _ = event_tx.send(AppEvent::MessageReceived {
            conversation_id: conversation_id.to_hex(),
            message_id: y7ke_core::MessageId::from_bytes(envelope.message_id).to_string(),
            sender_y7_id: sender_y7.to_uri(),
            timestamp_ms: envelope.timestamp_ms,
            text,
        });
    }
    Ok(inserted)
}

/// Exponential backoff capped at 1 hour: `min(2^attempts * 30s, 1h)`.
pub fn next_retry_at(attempts: i64) -> i64 {
    let now = y7ke_storage::now_ms();
    let secs = (1i64 << attempts.min(7)) * 30; // 30s, 60s, 120s, 240s, 480s, ...
    let capped = secs.min(3600);
    now + capped * 1000
}

/// Apply a control payload received from `sender`.
async fn handle_control(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    sender: Y7Id,
    ctrl: messaging::ControlPayload,
) -> Result<()> {
    tracing::info!(%sender, ?ctrl, "control received");
    match ctrl {
        messaging::ControlPayload::RejectedRequest => {
            for r in inner.db.requests().list_pending(None).await? {
                if r.peer_y7_id == sender {
                    let _ = inner
                        .db
                        .requests()
                        .resolve(r.id, y7ke_core::RequestResolution::Rejected)
                        .await;
                }
            }
            inner
                .db
                .contacts()
                .update_status(&sender, y7ke_core::ContactStatus::Blocked)
                .await
                .ok();
            let _ = event_tx.send(AppEvent::RequestResolved {
                y7_id: sender.to_uri(),
                resolution: y7ke_core::RequestResolution::Rejected,
            });
        }
        messaging::ControlPayload::AcceptedRequest => {
            for r in inner.db.requests().list_pending(None).await? {
                if r.peer_y7_id == sender {
                    let _ = inner
                        .db
                        .requests()
                        .resolve(r.id, y7ke_core::RequestResolution::Accepted)
                        .await;
                }
            }
            inner
                .db
                .contacts()
                .update_status(&sender, y7ke_core::ContactStatus::Accepted)
                .await
                .ok();
            let _ = event_tx.send(AppEvent::RequestResolved {
                y7_id: sender.to_uri(),
                resolution: y7ke_core::RequestResolution::Accepted,
            });
            let _ = event_tx.send(AppEvent::ContactAdded {
                y7_id: sender.to_uri(),
            });
        }
        messaging::ControlPayload::ChatDeleted => {
            // Peer wiped the conversation; mirror locally and tell UI to refresh.
            wipe_conversation(inner, &sender).await?;
            let _ = event_tx.send(AppEvent::ContactRemoved {
                y7_id: sender.to_uri(),
            });
        }
    }
    Ok(())
}

/// Wipe local state for `peer` via the storage DAO.
pub(crate) async fn wipe_conversation(inner: &Arc<AppInner>, peer: &Y7Id) -> Result<()> {
    let conv = ConversationId::between(&inner.my_y7_id, peer);
    inner.db.wipe_peer(peer, &conv).await
}
