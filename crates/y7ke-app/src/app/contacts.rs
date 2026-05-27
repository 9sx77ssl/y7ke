//! Contact + request commands on `AppHandle`.

use y7ke_core::error::{AppError, Result};
use y7ke_core::{AppEvent, ContactStatus, RequestResolution, Y7Id};
use y7ke_net::peer_id_from_y7;
use y7ke_storage::dao::contacts::NewContact;
use y7ke_storage::dao::requests::{NewRequest, RequestDirection};

use crate::messaging;
use crate::views::{ContactView, RequestView};

use super::{AppHandle, SEND_TIMEOUT};

impl AppHandle {
    pub async fn send_contact_request(&self, peer: Y7Id, greeting: Option<String>) -> Result<()> {
        if peer == self.inner.my_y7_id {
            return Err(AppError::invalid_input("cannot add yourself as a contact"));
        }
        let peer_id = peer_id_from_y7(&peer)?;

        // H1: idempotent. Existing session means we already handshook.
        if self.inner.db.sessions().get(&peer).await?.is_some() {
            if self.inner.db.contacts().get(&peer).await?.is_none() {
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
            }
            return Ok(());
        }

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

        if let Err(e) = self.inner.net.dial(peer).await {
            tracing::warn!(error = %e, %peer, "dial failed; proceeding to handshake");
        }
        let (req, eph) = crate::handshake::open_initiator(
            &self.inner.me,
            &self.inner.my_pubkey,
            peer.pubkey(),
            greeting,
        );
        let resp = self.inner.net.send_handshake(peer_id, req).await?;
        let session_key =
            crate::handshake::finalize_initiator(eph, &self.inner.my_pubkey, peer.pubkey(), &resp)?;
        self.inner.db.sessions().upsert(&peer, session_key).await?;
        Ok(())
    }

    pub async fn accept_request(&self, id: i64) -> Result<()> {
        let pending = self.find_request(id).await?;
        let peer = pending.peer_y7_id;
        self.inner
            .db
            .requests()
            .resolve(id, RequestResolution::Accepted)
            .await?;
        self.inner
            .db
            .contacts()
            .update_status(&peer, ContactStatus::Accepted)
            .await?;
        // Tell the initiator they got accepted; without this they only learn
        // about it via the first inbound message (B-fix from earlier turn).
        self.send_control(&peer, messaging::ControlPayload::AcceptedRequest)
            .await;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: peer.to_uri(),
            resolution: RequestResolution::Accepted,
        });
        let _ = self.event_tx.send(AppEvent::ContactAdded {
            y7_id: peer.to_uri(),
        });
        Ok(())
    }

    pub async fn reject_request(&self, id: i64) -> Result<()> {
        let pending = self.find_request(id).await?;
        let peer = pending.peer_y7_id;
        self.inner
            .db
            .requests()
            .resolve(id, RequestResolution::Rejected)
            .await?;
        self.inner
            .db
            .contacts()
            .update_status(&peer, ContactStatus::Blocked)
            .await?;
        self.send_control(&peer, messaging::ControlPayload::RejectedRequest)
            .await;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: peer.to_uri(),
            resolution: RequestResolution::Rejected,
        });
        Ok(())
    }

    /// Cancel a pending OUTGOING request. Local-only.
    pub async fn cancel_request(&self, id: i64) -> Result<()> {
        let pending = self.find_request(id).await?;
        if pending.direction != RequestDirection::Outgoing {
            return Err(AppError::invalid_input(
                "only outgoing requests can be cancelled",
            ));
        }
        self.inner
            .db
            .requests()
            .resolve(id, RequestResolution::Cancelled)
            .await?;
        self.inner
            .db
            .contacts()
            .update_status(&pending.peer_y7_id, ContactStatus::Removed)
            .await?;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: pending.peer_y7_id.to_uri(),
            resolution: RequestResolution::Cancelled,
        });
        Ok(())
    }

    /// Wipe a conversation locally + notify the peer.
    pub async fn delete_contact(&self, peer: Y7Id) -> Result<()> {
        self.send_control(&peer, messaging::ControlPayload::ChatDeleted)
            .await;
        crate::event_loop::wipe_conversation(&self.inner, &peer).await?;
        let _ = self.event_tx.send(AppEvent::RequestResolved {
            y7_id: peer.to_uri(),
            resolution: RequestResolution::Cancelled,
        });
        Ok(())
    }

    pub async fn list_contacts(&self) -> Result<Vec<ContactView>> {
        let rows = self.inner.db.contacts().list().await?;
        let presence_map = self.inner.presence.read().await;
        Ok(rows
            .into_iter()
            .map(|c| ContactView {
                y7_id: c.y7_id.to_uri(),
                nickname: c.nickname,
                status: c.status,
                added_at: c.added_at,
                presence: presence_map
                    .get(&c.y7_id)
                    .copied()
                    .unwrap_or(y7ke_core::ConnectionKind::Offline),
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

    /// Fire-and-forget control via /y7ke/msg/1.0.0. Used by reject + delete.
    pub(crate) async fn send_control(&self, peer: &Y7Id, payload: messaging::ControlPayload) {
        let Some(session) = self.inner.db.sessions().get(peer).await.ok().flatten() else {
            tracing::debug!(%peer, "no session for control — skipping");
            return;
        };
        let Ok((_mid, envelope, _ts)) = messaging::seal_control(
            &self.inner.me,
            &self.inner.my_pubkey,
            &session.session_key,
            &payload,
        ) else {
            tracing::warn!(%peer, "seal_control failed");
            return;
        };
        let Ok(peer_id) = peer_id_from_y7(peer) else {
            return;
        };
        let req = y7ke_net::protocol::MsgReq { envelope };
        let fut = self.inner.net.send_msg(peer_id, req);
        match tokio::time::timeout(SEND_TIMEOUT, fut).await {
            Ok(Ok(resp)) if resp.ack => {
                tracing::debug!(%peer, ?payload, "control delivered");
            }
            other => {
                tracing::warn!(%peer, ?other, "control delivery failed");
            }
        }
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
}
