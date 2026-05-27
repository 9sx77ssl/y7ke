//! Composition root: `AppHandle` owns the storage layer, the libp2p swarm,
//! the local identity, and the broadcast channel of `AppEvent`s.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::broadcast;
use y7ke_core::crypto::SigningKey;
use y7ke_core::error::{AppError, Result};
use y7ke_core::{
    AppEvent, ConnectionKind, ContactStatus, ConversationId, MessageId, MessageStatus,
    RequestResolution, Y7Id,
};
use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, peer_id_from_y7, spawn_swarm, NetHandle,
};
use y7ke_storage::dao::contacts::NewContact;
use y7ke_storage::dao::messages::NewMessage;
use y7ke_storage::dao::requests::{NewRequest, RequestDirection};
use y7ke_storage::{Db, DbConfig};

use crate::views::{ContactView, MessageView, RequestView};
use crate::{event_loop, handshake, identity, messaging};

/// Capacity of the AppEvent broadcast channel. Should comfortably exceed the
/// number of events a moderate burst of activity produces in a couple of
/// seconds.
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// All the configuration AppHandle needs.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub db: DbConfig,
}

impl AppConfig {
    pub fn default_for_app() -> Result<Self> {
        Ok(Self {
            db: DbConfig::default_for_app()?,
        })
    }

    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        Self {
            db: DbConfig::in_dir(dir),
        }
    }
}

/// Shared state owned by `AppHandle` and the background event loop.
pub(crate) struct AppInner {
    pub db: Db,
    pub net: NetHandle,
    pub me: SigningKey,
    pub my_pubkey: [u8; 32],
    pub my_y7_id: Y7Id,
}

/// The single public handle the Tauri shell holds.
pub struct AppHandle {
    pub(crate) inner: Arc<AppInner>,
    pub(crate) event_tx: broadcast::Sender<AppEvent>,
}

impl AppHandle {
    /// Open the database, ensure identity exists, spawn the libp2p swarm,
    /// and launch the background event loop. Returns when the runtime is
    /// fully wired up.
    pub async fn boot(config: AppConfig) -> Result<Self> {
        let db = Db::open(config.db).await?;
        let local = identity::ensure(&db).await?;
        let my_pubkey = local.signing_key.verifying_key().to_bytes();
        let my_y7_id = local.y7_id;
        let secret = local.signing_key.to_bytes();

        let keypair = libp2p_keypair_from_y7_secret(&secret)?;
        let swarm = build_swarm(keypair)?;
        let net = spawn_swarm(swarm);
        let event_rx_for_loop = net.try_clone_event_rx();

        let inner = Arc::new(AppInner {
            db,
            net,
            me: local.signing_key,
            my_pubkey,
            my_y7_id,
        });

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        // Spawn background event loop.
        let loop_inner = Arc::clone(&inner);
        let loop_event_tx = event_tx.clone();
        tokio::spawn(async move {
            event_loop::run(loop_inner, loop_event_tx, event_rx_for_loop).await;
        });

        // IdentityReady is best-effort: with no subscribers it returns Err
        // (broadcast tx with zero receivers) which we ignore.
        let _ = event_tx.send(AppEvent::IdentityReady {
            y7_id: my_y7_id.to_uri(),
        });

        tracing::info!(y7_id = %my_y7_id, "y7ke-app booted");

        Ok(Self { inner, event_tx })
    }

    pub fn my_y7_id(&self) -> &Y7Id {
        &self.inner.my_y7_id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.event_tx.subscribe()
    }

    // ------------------------------------------------------------
    // V1 capability 1 — identity is exposed through `my_y7_id`.
    // V1 capability 2 — add contact by key.
    // ------------------------------------------------------------

    pub async fn send_contact_request(&self, peer: Y7Id, greeting: Option<String>) -> Result<()> {
        if peer == self.inner.my_y7_id {
            return Err(AppError::invalid_input("cannot add yourself as a contact"));
        }

        // Always store the outgoing request locally so the UI shows it
        // regardless of whether the peer is currently reachable.
        self.inner
            .db
            .requests()
            .insert(NewRequest {
                direction: RequestDirection::Outgoing,
                peer_y7_id: peer,
                initial_text: greeting.clone(),
            })
            .await?;
        self.inner
            .db
            .contacts()
            .insert(NewContact {
                y7_id: peer,
                ed25519_pub: *peer.pubkey(),
                nickname: None,
                status: ContactStatus::PendingOut,
            })
            .await?;

        // Try the handshake. If it fails the user retries; V1 does not auto-retry.
        let _ = self.inner.net.dial(peer).await;
        let peer_id = peer_id_from_y7(&peer);
        let (req, eph) = handshake::open_initiator(
            &self.inner.me,
            &self.inner.my_pubkey,
            peer.pubkey(),
            greeting,
        );
        let resp = self.inner.net.send_handshake(peer_id, req).await?;
        let session_key =
            handshake::finalize_initiator(eph, &self.inner.my_pubkey, peer.pubkey(), &resp)?;
        self.inner.db.sessions().upsert(&peer, session_key).await?;
        Ok(())
    }

    // ------------------------------------------------------------
    // V1 capability 3 — accept / reject a pending incoming request.
    // ------------------------------------------------------------

    pub async fn accept_request(&self, id: i64) -> Result<()> {
        let pending = self.find_request(id).await?;
        self.inner
            .db
            .requests()
            .resolve(id, RequestResolution::Accepted)
            .await?;
        self.inner
            .db
            .contacts()
            .update_status(&pending.peer_y7_id, ContactStatus::Accepted)
            .await?;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: pending.peer_y7_id.to_uri(),
            resolution: RequestResolution::Accepted,
        });
        let _ = self.event_tx.send(AppEvent::ContactAdded {
            y7_id: pending.peer_y7_id.to_uri(),
        });
        Ok(())
    }

    pub async fn reject_request(&self, id: i64) -> Result<()> {
        let pending = self.find_request(id).await?;
        self.inner
            .db
            .requests()
            .resolve(id, RequestResolution::Rejected)
            .await?;
        self.inner
            .db
            .contacts()
            .update_status(&pending.peer_y7_id, ContactStatus::Blocked)
            .await?;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: pending.peer_y7_id.to_uri(),
            resolution: RequestResolution::Rejected,
        });
        Ok(())
    }

    async fn find_request(&self, id: i64) -> Result<y7ke_storage::dao::requests::Request> {
        self.inner
            .db
            .requests()
            .list_pending(None)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(AppError::NotFound)
    }

    // ------------------------------------------------------------
    // V1 capability 4 — open chat (list contacts + list messages).
    // ------------------------------------------------------------

    pub async fn list_contacts(&self) -> Result<Vec<ContactView>> {
        let rows = self.inner.db.contacts().list().await?;
        Ok(rows
            .into_iter()
            .map(|c| ContactView {
                y7_id: c.y7_id.to_uri(),
                nickname: c.nickname,
                status: c.status,
                added_at: c.added_at,
                presence: ConnectionKind::Offline,
            })
            .collect())
    }

    pub async fn list_pending_requests(&self) -> Result<Vec<RequestView>> {
        let rows = self.inner.db.requests().list_pending(None).await?;
        Ok(rows
            .into_iter()
            .map(|r| RequestView {
                id: r.id,
                direction: match r.direction {
                    RequestDirection::Incoming => "incoming".into(),
                    RequestDirection::Outgoing => "outgoing".into(),
                },
                peer_y7_id: r.peer_y7_id.to_uri(),
                initial_text: r.initial_text,
                created_at: r.created_at,
            })
            .collect())
    }

    /// List messages exchanged with `peer`. The conversation ID is derived
    /// internally from the sorted pubkeys.
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
                    // Verify + decrypt for the UI.
                    let verifying = y7ke_core::crypto::VerifyingKey::from_bytes(&m.sender_pub)?;
                    let envelope = y7ke_net::protocol::MessageEnvelope {
                        message_id: *m.message_id.as_bytes(),
                        sender_pub: m.sender_pub,
                        timestamp_ms: m.timestamp_ms,
                        nonce: m.payload_nonce,
                        ciphertext: m.payload_enc.clone(),
                        sig: m.sig,
                    };
                    messaging::open_envelope(&envelope, &verifying, &s.session_key)
                        .unwrap_or_else(|_| "<decryption failed>".into())
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

    // ------------------------------------------------------------
    // V1 capability 5 — send encrypted message.
    // V1 capability 6 — persist (handled in `insert` below).
    // V1 capability 7 — offline retry (handled in event loop on
    //                   PeerDiscovered / ConnectionEstablished).
    // ------------------------------------------------------------

    pub async fn send_message(&self, to: Y7Id, text: String) -> Result<MessageId> {
        if to == self.inner.my_y7_id {
            return Err(AppError::invalid_input("cannot message yourself"));
        }
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

        let peer_id = peer_id_from_y7(&to);
        let req = y7ke_net::protocol::MsgReq { envelope };
        match self.inner.net.send_msg(peer_id, req).await {
            Ok(resp) if resp.ack => {
                self.inner
                    .db
                    .messages()
                    .update_status(&message_id, MessageStatus::Sent)
                    .await?;
                let _ = self.event_tx.send(AppEvent::MessageStatusChanged {
                    message_id: message_id.to_string(),
                    status: MessageStatus::Sent,
                });
            }
            _ => {
                // Could not push live — enqueue for retry on next reconnect.
                let next = event_loop::next_retry_at(0);
                self.inner
                    .db
                    .sync_queue()
                    .enqueue(&message_id, &to, next)
                    .await?;
            }
        }

        // Also locally surface the message immediately so the user sees their own send.
        let _ = self.event_tx.send(AppEvent::MessageReceived {
            conversation_id: conversation_id.to_hex(),
            message_id: message_id.to_string(),
            sender_y7_id: self.inner.my_y7_id.to_uri(),
            timestamp_ms,
            text,
        });

        Ok(message_id)
    }
}
