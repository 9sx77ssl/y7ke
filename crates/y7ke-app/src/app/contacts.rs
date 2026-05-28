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

        // Dedup: if an outgoing pending request to this peer already
        // exists, skip the insert. Without this, each retry click from
        // a UI flow where the dial failed produced a fresh row, piling
        // up duplicate "pending…" cards in the Requests view.
        let outgoing = self
            .inner
            .db
            .requests()
            .list_pending(Some(RequestDirection::Outgoing))
            .await?;
        let already_pending = outgoing.iter().any(|r| r.peer_y7_id == peer);
        if !already_pending {
            self.inner
                .db
                .requests()
                .insert(NewRequest {
                    direction: RequestDirection::Outgoing,
                    peer_y7_id: peer,
                    initial_text: greeting.clone(),
                })
                .await?;
        }
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

        self.dial_with_discovery(peer).await;
        let (req, eph) = crate::handshake::open_initiator(
            &self.inner.me,
            &self.inner.my_pubkey,
            peer.pubkey(),
            greeting,
        );
        let resp = self.inner.net.send_handshake(peer_id, req).await?;
        crate::handshake::finalize_initiator(eph, &self.inner.my_pubkey, peer.pubkey(), &resp)?;
        self.inner.db.sessions().upsert(&peer).await?;
        Ok(())
    }

    /// V2-A1 discovery chain: mDNS cache → cached `peer_state.last_addrs`
    /// → Kademlia DHT lookup → direct dial. All four steps are best-
    /// effort; any successful dial enqueues a handshake later. Each step
    /// is gated by the user's current `DialModes`.
    async fn dial_with_discovery(&self, peer: Y7Id) {
        let modes = match self.inner.db.settings().get().await {
            Ok(s) => s.dial_modes,
            Err(e) => {
                tracing::warn!(error = %e, "settings.get failed; using defaults");
                y7ke_core::settings::DialModes::default()
            }
        };
        tracing::info!(%peer, ?modes, "discovery: starting chain");

        if !modes.lan && !modes.internet && !modes.relay && !modes.p2p {
            tracing::warn!(%peer, "discovery: all dial modes disabled — skipping");
            return;
        }
        if modes.p2p {
            tracing::info!(
                %peer,
                "p2p hole-punching requested but not implemented yet (V2-A5)"
            );
        }

        // 1. Fast path: swarm address book. When `lan` is off we still
        //    want to dial INTERNET-reachable known addrs, so we only
        //    skip the call if every known addr is LAN-only.
        if modes.lan || self.peer_has_non_lan_addr(&peer).await {
            match self.inner.net.dial(peer).await {
                Ok(true) => {
                    tracing::info!(%peer, "discovery: step 1 (swarm address book) issued dial");
                    return;
                }
                Ok(false) => {
                    tracing::info!(%peer, "discovery: step 1 — no known addresses in swarm");
                }
                Err(e) => {
                    tracing::warn!(%peer, error = %e, "discovery: step 1 — dial command failed");
                }
            }
        } else {
            tracing::info!(%peer, "discovery: step 1 skipped — lan dial mode off and only LAN addrs known");
        }

        // 2. Cached addrs from a previous session, filtered by mode.
        if let Ok(Some(state)) = self.inner.db.peer_state().get(&peer).await {
            if let Some(json) = state.last_addrs_json {
                if let Ok(addrs) = serde_json::from_str::<Vec<String>>(&json) {
                    let parsed: Vec<libp2p::Multiaddr> =
                        addrs.iter().filter_map(|s| s.parse().ok()).collect();
                    let filtered = filter_addrs_by_mode(parsed, &modes);
                    tracing::info!(%peer, count = filtered.len(), "discovery: step 2 — trying cached addrs");
                    for m in filtered {
                        if self.inner.net.dial_address(m).await.is_ok() {
                            tracing::info!(%peer, "discovery: step 2 cached addr dial issued");
                            return;
                        }
                    }
                }
            }
        }

        // 3. Kad lookup. find_peer either resolves to addrs we can dial
        //    or returns NotFound after a 10 s window.
        tracing::info!(%peer, "discovery: step 3 — Kad get_providers query");
        match self.inner.net.find_peer(peer).await {
            Ok(addrs) => {
                let mut filtered = filter_addrs_by_mode(addrs, &modes);
                // Prefer direct (non-circuit, non-LAN-only) first, then
                // circuit (relay) fallbacks, with LAN-looking addrs
                // last — Kad can return both the peer's public addr
                // AND its 192.168.x.x interface, and the latter won't
                // reach across NATs.
                filtered.sort_by_key(sort_addr_priority);
                tracing::info!(%peer, count = filtered.len(), "discovery: step 3 Kad returned addrs (after mode filter + sort)");
                for addr in filtered {
                    if self.inner.net.dial_address(addr).await.is_ok() {
                        return;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%peer, error = %e, "discovery: step 3 — Kad lookup failed (peer not in DHT or unreachable)");
            }
        }

        // 4. Last resort: ask the swarm one more time — by now Kad may
        //    have populated its routing table.
        if modes.lan || self.peer_has_non_lan_addr(&peer).await {
            match self.inner.net.dial(peer).await {
                Ok(true) => {
                    tracing::info!(%peer, "discovery: step 4 (re-check swarm) issued dial");
                }
                Ok(false) => {
                    tracing::warn!(%peer, "discovery: all 4 paths exhausted — peer unreachable");
                }
                Err(e) => {
                    tracing::warn!(%peer, error = %e, "discovery: step 4 dial command failed");
                }
            }
        } else {
            tracing::warn!(%peer, "discovery: all paths exhausted (lan-only addrs but lan mode off)");
        }
    }

    /// True if at least one cached addr for `peer` is non-LAN. Used to
    /// decide whether step 1 / step 4 swarm-dials should run when
    /// `lan = false`.
    async fn peer_has_non_lan_addr(&self, peer: &Y7Id) -> bool {
        let Ok(Some(state)) = self.inner.db.peer_state().get(peer).await else {
            return false;
        };
        let Some(json) = state.last_addrs_json else {
            return false;
        };
        let Ok(addrs) = serde_json::from_str::<Vec<String>>(&json) else {
            return false;
        };
        addrs
            .iter()
            .filter_map(|s| s.parse::<libp2p::Multiaddr>().ok())
            .any(|m| !y7ke_net::multiaddr_is_lan(&m))
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
        let _ = self.event_tx.send(AppEvent::ContactRemoved {
            y7_id: peer.to_uri(),
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
        if self
            .inner
            .db
            .sessions()
            .get(peer)
            .await
            .ok()
            .flatten()
            .is_none()
        {
            tracing::debug!(%peer, "no session for control — skipping");
            return;
        }
        let conv = y7ke_core::ConversationId::between(&self.inner.my_y7_id, peer);
        let Ok(conv_key) =
            messaging::derive_conv_key(&self.inner.me, peer.pubkey(), conv.as_bytes())
        else {
            tracing::warn!(%peer, "derive_conv_key failed for control");
            return;
        };
        let Ok((_mid, envelope, _ts)) =
            messaging::seal_control(&self.inner.me, &self.inner.my_pubkey, &conv_key, &payload)
        else {
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

/// Sort key for dial ordering. Lower value = try first.
/// Direct public addrs first, circuit relays second, LAN-only addrs
/// last (those usually don't reach across NATs).
fn sort_addr_priority(addr: &libp2p::Multiaddr) -> u8 {
    let is_circuit = addr
        .iter()
        .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit));
    let is_lan = y7ke_net::multiaddr_is_lan(addr);
    match (is_circuit, is_lan) {
        (false, false) => 0, // direct internet
        (true, _) => 1,      // relay fallback
        (false, true) => 2,  // LAN-only (rarely useful for cross-NAT)
    }
}

/// Drop addrs whose transport class is disabled in `modes`.
///
/// - `relay = false` → drop addrs containing `/p2p-circuit`.
/// - `internet = false` → drop non-LAN, non-circuit addrs.
/// - `lan = false` → drop LAN addrs (loopback / private / link-local).
fn filter_addrs_by_mode(
    addrs: Vec<libp2p::Multiaddr>,
    modes: &y7ke_core::settings::DialModes,
) -> Vec<libp2p::Multiaddr> {
    addrs
        .into_iter()
        .filter(|m| {
            let is_circuit = m
                .iter()
                .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit));
            let is_lan = y7ke_net::multiaddr_is_lan(m);
            if is_circuit {
                return modes.relay;
            }
            if is_lan {
                return modes.lan;
            }
            modes.internet
        })
        .collect()
}
