//! Handle / command / event surface for the network task.
//!
//! The swarm task owns the libp2p `Swarm<Y7Behaviour>` exclusively — that
//! type is `!Sync` and the canonical pattern is to wrap it in a dedicated
//! task and talk to it over channels. The [`NetHandle`] returned by
//! `crate::swarm::spawn_swarm` is the only public surface to that task.
//!
//! Channel sizing:
//! - [`NetCommand`] is `mpsc(64)` — bursts of dial/send commands from the
//!   app layer are bounded by the number of pending user actions; 64 is
//!   plenty of headroom and small enough to surface back-pressure.
//! - [`NetEvent`] is `broadcast(256)` — the UI plus headless test
//!   subscribers can all observe the same event stream. A slow subscriber
//!   that lags will drop old events (broadcast semantics), not block
//!   the swarm.
//!
//! ## `ResponseChannel` and broadcast
//!
//! `libp2p::request_response::ResponseChannel<T>` is `Send` but not
//! `Clone`. `tokio::sync::broadcast` requires its payload to be `Clone`.
//! To reconcile the two, the `*Received` event variants carry a
//! [`TakeOnce<ResponseChannel<_>>`] — a thin
//! `Arc<Mutex<Option<ResponseChannel<_>>>>` wrapper. Only the first
//! subscriber that calls [`TakeOnce::take`] obtains the channel; later
//! subscribers see `None`. This is the correct semantic anyway: a
//! request must be answered exactly once.

use std::sync::{Arc, Mutex};

use libp2p::{request_response::ResponseChannel, swarm::ConnectionId, Multiaddr, PeerId};
use tokio::sync::{broadcast, mpsc, oneshot};

use y7ke_core::settings::DialMode;
use y7ke_core::{AppError, ConnectionKind, Y7Id};

use crate::protocol::{HandshakeReq, HandshakeResp, MsgReq, MsgResp, SyncReq, SyncResp};

/// Container that allows shipping a non-`Clone` value through a
/// broadcast channel: every subscriber sees a `Clone`-able handle, but
/// only the first to call [`TakeOnce::take`] actually obtains the value.
pub struct TakeOnce<T> {
    inner: Arc<Mutex<Option<T>>>,
}

impl<T> TakeOnce<T> {
    /// Wrap `value`. Subsequent broadcast subscribers receive Arc clones.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(value))),
        }
    }

    /// Remove and return the wrapped value. Returns `None` if some
    /// earlier subscriber already took it (or if the lock is poisoned).
    pub fn take(&self) -> Option<T> {
        self.inner.lock().ok().and_then(|mut guard| guard.take())
    }

    /// Check whether the inner value is still present without removing
    /// it. Returns `false` on poisoned-lock as a conservative default.
    pub fn is_some(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }
}

impl<T> Clone for TakeOnce<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> std::fmt::Debug for TakeOnce<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TakeOnce {{ present: {} }}", self.is_some())
    }
}

/// Default capacity for the broadcast event channel.
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 256;
/// Default capacity for the mpsc command channel.
pub(crate) const COMMAND_CHANNEL_CAPACITY: usize = 64;

/// Owned handle to the running swarm task.
///
/// Cheap to clone via [`NetHandle::try_clone_event_rx`] for additional
/// subscribers; [`NetHandle::clone_command_sender`] for additional command
/// emitters.
pub struct NetHandle {
    pub(crate) cmd_tx: mpsc::Sender<NetCommand>,
    pub(crate) event_rx: broadcast::Receiver<NetEvent>,
    pub(crate) event_tx: broadcast::Sender<NetEvent>,
}

impl NetHandle {
    /// Subscribe a second consumer to the event stream. The new receiver
    /// only sees events published *after* it subscribes — historical
    /// events are not replayed.
    pub fn try_clone_event_rx(&self) -> broadcast::Receiver<NetEvent> {
        self.event_tx.subscribe()
    }

    /// Take the primary receiver out of the handle (consumes it). Useful
    /// when there is exactly one consumer that wants ownership.
    pub fn into_event_rx(self) -> broadcast::Receiver<NetEvent> {
        self.event_rx
    }

    /// Borrow the primary receiver mutably.
    pub fn event_rx(&mut self) -> &mut broadcast::Receiver<NetEvent> {
        &mut self.event_rx
    }

    /// Clone the command sender so multiple emitters can drive the swarm.
    pub fn clone_command_sender(&self) -> mpsc::Sender<NetCommand> {
        self.cmd_tx.clone()
    }

    /// Fire-and-forget dial by `Y7Id`. The swarm task derives the libp2p
    /// `PeerId` from the contained Ed25519 public key and dials any
    /// addresses currently known for it via mDNS / identify. Returns
    /// `Ok(true)` if a dial was issued, `Ok(false)` if the swarm has no
    /// known addresses for the peer (caller should fall through to
    /// discovery).
    pub async fn dial(&self, y7_id: Y7Id) -> Result<bool, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::Dial {
                y7_id,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(AppError::network("dial response channel dropped")),
        }
    }

    /// Fire-and-forget dial of a fully-qualified `Multiaddr` (typically
    /// `<transport-addr>/p2p/<peer-id>`). Useful when the address comes
    /// from outside mDNS — for example, a hand-pasted bootstrap or a
    /// test that wants to skip discovery.
    pub async fn dial_address(&self, address: Multiaddr) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::DialAddress { address })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// Look up a peer's currently-known multiaddrs via the Kademlia DHT
    /// (V2-A1). Returns the addresses Kad has recorded for the peer, or
    /// `AppError::NotFound` if the lookup completes without finding any.
    /// Times out after 10 seconds.
    pub async fn find_peer(&self, y7_id: Y7Id) -> Result<Vec<Multiaddr>, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::FindPeer {
                y7_id,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(Ok(addrs))) => Ok(addrs),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(AppError::network("find_peer response channel dropped")),
            Err(_) => Err(AppError::network("find_peer timed out after 10s")),
        }
    }

    /// Send a handshake request and await the matching response. Returns
    /// `AppError::Network` on timeout, transport failure, or task
    /// shutdown.
    pub async fn send_handshake(
        &self,
        peer: PeerId,
        request: HandshakeReq,
    ) -> Result<HandshakeResp, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::SendHandshake {
                peer,
                request,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        rx.await
            .map_err(|e| AppError::network(format!("response channel dropped: {e}")))?
    }

    /// Send a single message envelope. Same error semantics as
    /// [`Self::send_handshake`].
    pub async fn send_msg(&self, peer: PeerId, request: MsgReq) -> Result<MsgResp, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::SendMsg {
                peer,
                request,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        rx.await
            .map_err(|e| AppError::network(format!("response channel dropped: {e}")))?
    }

    /// Drive one round of the sync protocol.
    pub async fn send_sync(&self, peer: PeerId, request: SyncReq) -> Result<SyncResp, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::SendSync {
                peer,
                request,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        rx.await
            .map_err(|e| AppError::network(format!("response channel dropped: {e}")))?
    }

    /// Send a handshake response back through a previously-received
    /// `ResponseChannel`. Returns `Err` if the swarm task has already
    /// dropped the channel (typically because the request timed out
    /// before the responder finished its work).
    pub async fn respond_handshake(
        &self,
        channel: ResponseChannel<HandshakeResp>,
        response: HandshakeResp,
    ) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::RespondHandshake { channel, response })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// Send a handshake response by first claiming the channel from a
    /// [`TakeOnce`] envelope. Convenience for the common
    /// `event_rx.recv()` → respond pattern.
    pub async fn respond_handshake_take(
        &self,
        channel: TakeOnce<ResponseChannel<HandshakeResp>>,
        response: HandshakeResp,
    ) -> Result<(), AppError> {
        let channel = channel.take().ok_or_else(|| {
            AppError::network("handshake response channel already taken or expired")
        })?;
        self.respond_handshake(channel, response).await
    }

    /// Send a message-protocol response.
    pub async fn respond_msg(
        &self,
        channel: ResponseChannel<MsgResp>,
        response: MsgResp,
    ) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::RespondMsg { channel, response })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// See [`Self::respond_handshake_take`].
    pub async fn respond_msg_take(
        &self,
        channel: TakeOnce<ResponseChannel<MsgResp>>,
        response: MsgResp,
    ) -> Result<(), AppError> {
        let channel = channel
            .take()
            .ok_or_else(|| AppError::network("msg response channel already taken or expired"))?;
        self.respond_msg(channel, response).await
    }

    /// Send a sync-protocol response.
    pub async fn respond_sync(
        &self,
        channel: ResponseChannel<SyncResp>,
        response: SyncResp,
    ) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::RespondSync { channel, response })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// See [`Self::respond_handshake_take`].
    pub async fn respond_sync_take(
        &self,
        channel: TakeOnce<ResponseChannel<SyncResp>>,
        response: SyncResp,
    ) -> Result<(), AppError> {
        let channel = channel
            .take()
            .ok_or_else(|| AppError::network("sync response channel already taken or expired"))?;
        self.respond_sync(channel, response).await
    }

    /// Replace the swarm's bootstrap-peer map. New addrs are added to Kad
    /// and dialed; removed addrs are forgotten (live connections stay
    /// until they drop naturally). Used on settings change.
    pub async fn update_bootstraps(&self, addresses: Vec<Multiaddr>) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::UpdateBootstraps { addresses })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// Switch the swarm to a new `DialMode` immediately. `LanOnly` drops
    /// circuit listeners + disconnects bootstraps; `Internet`
    /// re-dial bootstraps and re-request relay reservations on connect.
    pub async fn apply_dial_mode(&self, mode: DialMode) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::ApplyDialMode { mode })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// Cheap, network-free check: does the swarm currently hold any
    /// active connection to `y7_id`? Used by the periodic presence
    /// ticker to spot peers whose socket has died without a
    /// `ConnectionClosed` (e.g. mom's laptop hibernated). When this
    /// returns `false` for a previously-online peer, the caller
    /// downgrades presence to Offline; libp2p's own `ping::Behaviour`
    /// drives the actual socket health check on a 20 s interval.
    pub async fn check_live(&self, y7_id: Y7Id) -> Result<bool, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(NetCommand::CheckLive {
                y7_id,
                response_tx: tx,
            })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))?;
        rx.await
            .map_err(|e| AppError::network(format!("check_live response dropped: {e}")))?
    }

    /// Force-close every connection to `y7_id`. Fire-and-forget: the next
    /// dial yields a fresh `ConnectionEstablished` that repopulates presence.
    pub async fn disconnect_peer(&self, y7_id: Y7Id) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::DisconnectPeer { y7_id })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
    }

    /// Request graceful shutdown. The swarm task will drain in-flight
    /// requests and exit. Subsequent commands return
    /// `AppError::Network("command channel closed")`.
    pub async fn shutdown(&self) -> Result<(), AppError> {
        // If the channel is already closed (task ended), shutting down is
        // already what we wanted, so we ignore that one error variant.
        let _ = self.cmd_tx.send(NetCommand::Shutdown).await;
        Ok(())
    }
}

/// Commands accepted by the swarm task. Most carry a `oneshot::Sender`
/// for the matching response.
#[derive(Debug)]
pub enum NetCommand {
    /// Dial a known contact by their Y7 identifier (i.e. by their long-
    /// term Ed25519 public key). The swarm task derives the libp2p
    /// `PeerId` and uses any cached addresses. `response_tx` resolves to
    /// `Ok(true)` if a dial was actually issued, `Ok(false)` if the
    /// peer has no known addresses (so a fallback discovery path should
    /// run), or `Err` on an invalid Y7Id.
    Dial {
        y7_id: Y7Id,
        response_tx: oneshot::Sender<Result<bool, AppError>>,
    },
    /// Dial an arbitrary, fully-qualified `Multiaddr`. The address is
    /// also recorded in the per-peer address book so subsequent
    /// `send_*` calls can re-use it.
    DialAddress { address: Multiaddr },
    /// Issue a Kademlia `get_providers` query for the peer's record key
    /// and resolve `response_tx` with the addresses Kad gathers en route.
    FindPeer {
        y7_id: Y7Id,
        response_tx: oneshot::Sender<Result<Vec<Multiaddr>, AppError>>,
    },
    /// Open `/y7ke/handshake/1.0.0` to `peer` and await the response.
    SendHandshake {
        peer: PeerId,
        request: HandshakeReq,
        response_tx: oneshot::Sender<Result<HandshakeResp, AppError>>,
    },
    /// Open `/y7ke/msg/1.0.0`.
    SendMsg {
        peer: PeerId,
        request: MsgReq,
        response_tx: oneshot::Sender<Result<MsgResp, AppError>>,
    },
    /// Open `/y7ke/sync/1.0.0`.
    SendSync {
        peer: PeerId,
        request: SyncReq,
        response_tx: oneshot::Sender<Result<SyncResp, AppError>>,
    },
    /// Reply to a previously-received handshake request.
    RespondHandshake {
        channel: ResponseChannel<HandshakeResp>,
        response: HandshakeResp,
    },
    /// Reply to a previously-received message request.
    RespondMsg {
        channel: ResponseChannel<MsgResp>,
        response: MsgResp,
    },
    /// Reply to a previously-received sync request.
    RespondSync {
        channel: ResponseChannel<SyncResp>,
        response: SyncResp,
    },
    /// Replace the swarm's tracked bootstrap-peer set. The swarm task
    /// adds new entries to Kad + dials them; entries no longer in
    /// `addresses` are removed from the redial loop (existing
    /// connections persist).
    UpdateBootstraps { addresses: Vec<Multiaddr> },
    /// Resolve `response_tx` synchronously with
    /// `swarm.is_connected(&peer_id)` for the derived libp2p PeerId.
    /// Used by the app-layer presence ticker to detect dead sockets
    /// that haven't yet surfaced a `ConnectionClosed` event.
    CheckLive {
        y7_id: Y7Id,
        response_tx: oneshot::Sender<Result<bool, AppError>>,
    },
    /// Force-close ALL libp2p connections to the peer derived from `y7_id`.
    /// Used on chat delete (drop the stale socket so both ends re-dial fresh)
    /// and by the presence ticker to clear a map-vs-socket desync — libp2p
    /// won't re-emit `ConnectionEstablished` for an already-open connection,
    /// so the only way to repopulate presence is to drop it and dial again.
    DisconnectPeer { y7_id: Y7Id },
    /// Switch the live `DialMode`. Triggers immediate side-effects:
    /// `LanOnly` drops `/p2p-circuit` listeners + disconnects bootstraps;
    /// `Internet` re-enable bootstrap reconnect and relay reservations.
    ApplyDialMode { mode: DialMode },
    /// Stop the swarm task.
    Shutdown,
}

/// Events published by the swarm task on the broadcast channel.
///
/// `broadcast::channel` requires `T: Clone`. The non-`Clone`
/// `ResponseChannel` returned by libp2p is therefore wrapped in
/// [`TakeOnce`] — every subscriber sees the same handle, but only the
/// first to call `.take()` claims the underlying channel.
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// New local listen address (informational).
    Listening { addr: Multiaddr },
    /// mDNS surfaced a new peer (or refreshed an existing one). `y7_id`
    /// is populated when the peer's Ed25519 pubkey could be recovered
    /// from the PeerId (V1 always uses inlined Ed25519 keys, so this is
    /// effectively always `Some`).
    PeerDiscovered {
        peer: PeerId,
        addrs: Vec<Multiaddr>,
        y7_id: Option<Y7Id>,
    },
    /// Connection opened. `kind` reflects how we reached the peer;
    /// `endpoint_addr` is the full remote multiaddr so the app layer
    /// can derive transport (TCP vs QUIC) + relay host for the
    /// Connectivity debug pane without re-querying the swarm.
    /// `connection_id` keys the app's per-connection state so a peer
    /// that holds several concurrent connections (e.g. relay + direct)
    /// is tracked faithfully and a single close can't blank it.
    ConnectionEstablished {
        peer: PeerId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
        endpoint_addr: Multiaddr,
    },
    /// Connection torn down. Carries the `connection_id` so the app
    /// removes only that connection and recomputes presence from any
    /// survivors — a relay drop must not hide a live direct path.
    ConnectionClosed {
        peer: PeerId,
        connection_id: ConnectionId,
    },
    /// DCUtR (V2-A5) reported a successful direct-connection upgrade.
    /// `connection_id` is the freshly hole-punched direct connection
    /// (libp2p also fired a `ConnectionEstablished` for it, classified
    /// by endpoint as Internet); the app relabels that connection
    /// `Direct` so `best_kind` promotes it. The underlying relayed
    /// circuit lingers briefly while libp2p folds traffic over.
    ConnectionUpgraded {
        peer: PeerId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
    },
    /// DCUtR upgrade failed (one of: peer didn't respond in time,
    /// observed-address mismatch, both peers behind symmetric NAT, etc.).
    /// The existing Relayed connection stays in place. The
    /// upgrade-from-relay loop consumes this as a signal to schedule a
    /// retry after the next observed-address change / AutoNAT verdict
    /// flip.
    ConnectionUpgradeFailed { peer: PeerId, error: String },
    /// AutoNAT v2 verdict — a tested external address either came back
    /// reachable or not. The app layer aggregates these into a single
    /// `NatReachability` per-app, used to gate the upgrade-from-relay
    /// loop. Fires once per AutoNAT probe (every ~5s by default while
    /// candidates exist).
    NatStatus {
        tested_addr: Multiaddr,
        server: PeerId,
        reachable: bool,
    },
    /// Inbound handshake request awaiting a response.
    HandshakeReceived {
        peer: PeerId,
        request: HandshakeReq,
        channel: TakeOnce<ResponseChannel<HandshakeResp>>,
    },
    /// Inbound message envelope awaiting an ack.
    MsgReceived {
        peer: PeerId,
        request: MsgReq,
        channel: TakeOnce<ResponseChannel<MsgResp>>,
    },
    /// Inbound sync request awaiting a response.
    SyncReceived {
        peer: PeerId,
        request: SyncReq,
        channel: TakeOnce<ResponseChannel<SyncResp>>,
    },
    /// Operator-visible failure (dial error, missing-address, etc.). Not
    /// a fatal error for the task.
    Error { message: String },
}
