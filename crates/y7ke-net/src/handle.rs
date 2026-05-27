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

use libp2p::{request_response::ResponseChannel, Multiaddr, PeerId};
use tokio::sync::{broadcast, mpsc, oneshot};

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
    /// addresses currently known for it via mDNS / identify.
    pub async fn dial(&self, y7_id: Y7Id) -> Result<(), AppError> {
        self.cmd_tx
            .send(NetCommand::Dial { y7_id })
            .await
            .map_err(|e| AppError::network(format!("command channel closed: {e}")))
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
    /// `PeerId` and uses any cached addresses.
    Dial { y7_id: Y7Id },
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
    /// Connection opened. `kind` reflects how we reached the peer.
    ConnectionEstablished { peer: PeerId, kind: ConnectionKind },
    /// Connection torn down. The application layer should mark the peer
    /// offline / move pending sends back to the retry queue.
    ConnectionClosed { peer: PeerId },
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
