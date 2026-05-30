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

    /// V2-A1 discovery chain. Behaviour is gated by the user's
    /// `DialMode`:
    ///
    /// - `LanOnly`: swarm address book (filtered to LAN-only) + cached
    ///   LAN-only addrs. No Kad lookup. If nothing LAN-reachable, give up.
    /// - `Internet`: all 4 steps. Step 3 (Kad) returns relay
    ///   multiaddrs naturally; direct dial preferred via
    ///   `sort_addrs_for_dial`.
    pub(crate) async fn dial_with_discovery(&self, peer: Y7Id) {
        let mode = match self.inner.db.settings().get().await {
            Ok(s) => s.dial_mode,
            Err(e) => {
                tracing::warn!(error = %e, "settings.get failed; using defaults");
                y7ke_core::settings::DialMode::default()
            }
        };
        tracing::info!(%peer, ?mode, "discovery: starting chain");

        let lan_only = matches!(mode, y7ke_core::settings::DialMode::LanOnly);

        // 1. Fast path: swarm address book. In LanOnly mode we gate the
        //    call on cached LAN-only addrs.
        let swarm_dial_ok = !lan_only || self.peer_has_lan_addr(&peer).await;
        if swarm_dial_ok {
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
            tracing::info!(%peer, "discovery: step 1 skipped — lan-only mode and no LAN addr known");
        }

        // 2. Cached addrs from a previous session.
        if let Ok(Some(state)) = self.inner.db.peer_state().get(&peer).await {
            if let Some(json) = state.last_addrs_json {
                if let Ok(addrs) = serde_json::from_str::<Vec<String>>(&json) {
                    // Drop cached circuit (relay) addrs from rows we
                    // haven't refreshed in over 24h: a decommissioned
                    // relay would otherwise have us dialing a dead
                    // /p2p-circuit path on every discovery forever. Direct
                    // addrs are kept (cheap to try) and Kad re-resolves
                    // the rest below.
                    let stale = circuit_cache_is_stale(state.last_seen_at, now_unix_ms());
                    if stale {
                        tracing::debug!(%peer, "discovery: step 2 — dropping stale (>24h) circuit addrs");
                    }
                    let parsed: Vec<libp2p::Multiaddr> = addrs
                        .iter()
                        .filter(|s| !(stale && s.contains("/p2p-circuit")))
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    let prioritised = y7ke_net::sort_addrs_for_dial(parsed);
                    let filtered = filter_addrs_for_mode(prioritised, mode);
                    tracing::info!(%peer, count = filtered.len(), "discovery: step 2 — trying cached addrs");
                    for m in filtered {
                        tracing::debug!(%peer, addr = %m, "dial chose cached addr");
                        if self.inner.net.dial_address(m).await.is_ok() {
                            tracing::info!(%peer, "discovery: step 2 cached addr dial issued");
                            return;
                        }
                    }
                }
            }
        }

        // LanOnly stops here — Kad lookups and bootstrap-assisted dials
        // require the bootstrap connection that LanOnly forbids.
        if lan_only {
            tracing::info!(%peer, "discovery: LAN-only mode and peer has no LAN address; not dialing");
            return;
        }

        // 3. Kad lookup. Bounded concurrency (KAD_LOOKUP_CONCURRENCY) so a
        //    reconnect storm can't fan out into dozens of simultaneous DHT
        //    provider queries. The permit is held only across the lookup,
        //    then dropped before the step-4 re-dial. An AcquireError means
        //    the semaphore closed (shutdown) — fall through unthrottled,
        //    the swarm is tearing down anyway.
        tracing::info!(%peer, "discovery: step 3 — Kad get_providers query");
        let lookup = {
            let _permit = self.inner.kad_lookups.acquire().await.ok();
            self.inner.net.find_peer(peer).await
        };
        match lookup {
            Ok(addrs) => {
                // V2-A5/A6: order direct QUIC > direct TCP > relay so
                // hole-punch-capable direct paths are tried before we
                // burn relay bandwidth.
                let prioritised = y7ke_net::sort_addrs_for_dial(addrs);
                let filtered = filter_addrs_for_mode(prioritised, mode);
                tracing::info!(%peer, count = filtered.len(), "discovery: step 3 Kad returned addrs (after mode filter)");
                for addr in filtered {
                    tracing::debug!(%peer, %addr, "dial chose addr");
                    if self.inner.net.dial_address(addr).await.is_ok() {
                        return;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%peer, error = %e, "discovery: step 3 — Kad lookup failed (peer not in DHT or unreachable)");
            }
        }

        // 4. Re-check the swarm — by now Kad may have populated routing.
        match self.inner.net.dial(peer).await {
            Ok(true) => {
                tracing::info!(%peer, "discovery: step 4 (re-check swarm) issued dial");
                return;
            }
            Ok(false) => {
                tracing::info!(%peer, "discovery: step 4 — still no direct addr");
            }
            Err(e) => {
                tracing::warn!(%peer, error = %e, "discovery: step 4 dial command failed");
            }
        }

        // 5. Port-stable relay-circuit fallback. After a restart the peer's
        //    direct addrs (ports) change and Kad may be cold, but a circuit
        //    address routes through the FIXED bootstrap relay and is
        //    independent of the peer's ports — the most reliable re-find for
        //    the NAT-bound ↔ public case. Construct
        //    `<bootstrap>/p2p-circuit/p2p/<target>` for each configured
        //    bootstrap and dial it; the relay forwards iff the target has a
        //    reservation (i.e. is online), so this self-limits and, once the
        //    peer is back online, re-establishes connectivity which then
        //    triggers ConnectionEstablished → queue drain + sync + DCUtR.
        let Ok(target) = y7ke_net::peer_id_from_y7(&peer) else {
            return;
        };
        let bootstraps = crate::config::load_bootstraps(&self.inner.db).await;
        let mut issued = 0usize;
        for relay in bootstraps {
            let circuit = relay
                .with(libp2p::multiaddr::Protocol::P2pCircuit)
                .with(libp2p::multiaddr::Protocol::P2p(target));
            tracing::debug!(%peer, addr = %circuit, "discovery: step 5 — relay-circuit fallback");
            if self.inner.net.dial_address(circuit).await.is_ok() {
                issued += 1;
            }
        }
        if issued > 0 {
            tracing::info!(%peer, issued, "discovery: step 5 — relay-circuit dials issued");
        } else {
            tracing::warn!(%peer, "discovery: all paths (incl. relay-circuit) exhausted");
        }
    }

    /// True if at least one cached addr for `peer` is LAN-private /
    /// loopback / link-local. Used to gate the swarm-dial step under
    /// `LanOnly`.
    async fn peer_has_lan_addr(&self, peer: &Y7Id) -> bool {
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
            .any(|m| y7ke_net::multiaddr_is_lan(&m))
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

    /// Wipe a conversation locally + reliably notify the peer.
    ///
    /// The `ChatDeleted` control must reach the peer even if they're offline
    /// right now (the UI promises "wiped on both sides the next time they come
    /// online"). We can't seal it after the wipe — that destroys the session —
    /// so seal it NOW and persist it to `pending_deletes`, which survives the
    /// wipe and is retried on the peer's next reconnect. A best-effort
    /// immediate send covers the common "still connected" case.
    pub async fn delete_contact(&self, peer: Y7Id) -> Result<()> {
        self.enqueue_chat_deleted(&peer).await;
        crate::event_loop::wipe_conversation(&self.inner, &peer).await?;
        let _ = self.event_tx.send(AppEvent::ContactRemoved {
            y7_id: peer.to_uri(),
        });
        // Immediate delivery attempt in the background so the local delete
        // stays instant; if it fails, the reconnect drain retries from the
        // persisted envelope.
        if let Ok(peer_id) = peer_id_from_y7(&peer) {
            let inner = std::sync::Arc::clone(&self.inner);
            tokio::spawn(async move {
                let _ = crate::event_loop::flush_pending_delete(&inner, &peer, peer_id).await;
                // Drop the socket AFTER the ChatDeleted send so a re-add
                // re-dials fresh (libp2p won't re-emit ConnectionEstablished
                // for a surviving connection → presence would stay Offline).
                // The peer sees ConnectionClosed too, so both ends reset. Flush
                // failure is non-fatal — pending_deletes retries on reconnect.
                let _ = inner.net.disconnect_peer(peer).await;
            });
        }
        Ok(())
    }

    /// Seal the `ChatDeleted` control while the session still exists and stash
    /// the sealed envelope in `pending_deletes` (survives the local wipe). No
    /// session → the peer never knew us, so there's nothing to propagate.
    async fn enqueue_chat_deleted(&self, peer: &Y7Id) {
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
            return;
        }
        let conv = y7ke_core::ConversationId::between(&self.inner.my_y7_id, peer);
        let Ok(conv_key) =
            messaging::derive_conv_key(&self.inner.me, peer.pubkey(), conv.as_bytes())
        else {
            tracing::warn!(%peer, "derive_conv_key failed for ChatDeleted");
            return;
        };
        let Ok((_mid, envelope, _ts)) = messaging::seal_control(
            &self.inner.me,
            &self.inner.my_pubkey,
            &conv_key,
            &messaging::ControlPayload::ChatDeleted,
        ) else {
            tracing::warn!(%peer, "seal_control failed for ChatDeleted");
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&envelope) else {
            tracing::warn!(%peer, "serialize ChatDeleted envelope failed");
            return;
        };
        if let Err(e) = self
            .inner
            .db
            .pending_deletes()
            .enqueue(peer, &bytes, y7ke_storage::now_ms())
            .await
        {
            tracing::warn!(%peer, error = %e, "persist pending ChatDeleted failed");
        }
    }

    pub async fn list_contacts(&self) -> Result<Vec<ContactView>> {
        let rows = self.inner.db.contacts().list().await?;
        let presence_map = self.inner.presence.read().await;
        let meta_map = self.inner.connection_meta.read().await;
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
                transport: meta_map.get(&c.y7_id).and_then(|m| m.transport),
                ip_version: meta_map.get(&c.y7_id).and_then(|m| m.ip_version),
                origin: meta_map.get(&c.y7_id).map(|m| m.origin).unwrap_or_default(),
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

/// Filter addresses by the current `DialMode`.
///
/// - `LanOnly` → keep LAN-only addrs.
/// - `Internet` → keep everything; ordering is decided by
///   `y7ke_net::sort_addrs_for_dial` at the caller.
fn filter_addrs_for_mode(
    addrs: Vec<libp2p::Multiaddr>,
    mode: y7ke_core::settings::DialMode,
) -> Vec<libp2p::Multiaddr> {
    use y7ke_core::settings::DialMode;
    match mode {
        DialMode::LanOnly => addrs
            .into_iter()
            .filter(y7ke_net::multiaddr_is_lan)
            .collect(),
        DialMode::Internet => addrs,
    }
}

/// Cached circuit (relay) addrs are treated as likely-dead once their
/// `peer_state` row hasn't been refreshed for this long.
const STALE_CIRCUIT_AGE_MS: i64 = 24 * 60 * 60 * 1000;

/// True if a `peer_state.last_seen_at` (Unix ms) is older than the
/// stale-circuit threshold relative to `now_ms`. A missing timestamp
/// counts as stale — we can't prove the cached relay path is fresh.
/// Pure for unit-testing.
fn circuit_cache_is_stale(last_seen_at: Option<i64>, now_ms: i64) -> bool {
    match last_seen_at {
        Some(ts) => now_ms.saturating_sub(ts) > STALE_CIRCUIT_AGE_MS,
        None => true,
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_cache_staleness() {
        let now = 100 * STALE_CIRCUIT_AGE_MS;
        // Just-seen → fresh.
        assert!(!circuit_cache_is_stale(Some(now), now));
        // Seen 23h ago → still fresh.
        assert!(!circuit_cache_is_stale(
            Some(now - 23 * 60 * 60 * 1000),
            now
        ));
        // Seen >24h ago → stale.
        assert!(circuit_cache_is_stale(
            Some(now - STALE_CIRCUIT_AGE_MS - 1),
            now
        ));
        // No timestamp → stale.
        assert!(circuit_cache_is_stale(None, now));
    }
}
