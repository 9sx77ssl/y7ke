//! Swarm construction and the owning task loop.
//!
//! The high-level lifecycle is:
//!
//! ```text
//! Y7 secret bytes ──► libp2p Keypair ──► Swarm<Y7Behaviour> ──► NetHandle
//!                                                           └── (background task)
//! ```
//!
//! `build_swarm` is synchronous and returns a configured `Swarm` ready to
//! be driven; `spawn_swarm` consumes that `Swarm`, takes ownership of it
//! in a dedicated `tokio::task`, and returns a [`NetHandle`] over which
//! the rest of the app issues commands and receives events.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    core::ConnectedPoint,
    identify, identity, mdns, noise,
    request_response::{self, OutboundRequestId},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use y7ke_core::{AppError, ConnectionKind, Y7Id};

use crate::behaviour::{Y7Behaviour, Y7BehaviourEvent};
use crate::handle::{
    NetCommand, NetEvent, NetHandle, TakeOnce, COMMAND_CHANNEL_CAPACITY, EVENT_CHANNEL_CAPACITY,
};
use crate::protocol::{HandshakeResp, MsgResp, SyncResp};

/// Default listen address (random TCP port on all interfaces).
pub const DEFAULT_LISTEN_ADDR: &str = "/ip4/0.0.0.0/tcp/0";

/// Idle connection timeout. Connections with no active substream are
/// dropped after this period.
const IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(300);

/// Lift a Y7KE Ed25519 32-byte secret into a libp2p `Keypair`.
///
/// The Y7KE long-term identity *is* the libp2p node identity in V1 —
/// using one key for both means the libp2p `PeerId` mirrors the
/// `Y7Id`, and a peer's protocol-version `info.public_key` from the
/// `identify` exchange round-trips back to the same `Y7Id`.
pub fn libp2p_keypair_from_y7_secret(secret: &[u8; 32]) -> Result<identity::Keypair, AppError> {
    // `ed25519_from_bytes` zeroizes the slice on success, so we hand it
    // a working copy and keep the caller's array untouched.
    let mut tmp = *secret;
    let kp = identity::Keypair::ed25519_from_bytes(&mut tmp[..])
        .map_err(|e| AppError::network(format!("invalid ed25519 secret: {e}")))?;
    Ok(kp)
}

/// Build a `Swarm<Y7Behaviour>` with TCP + Noise + Yamux transport and
/// the V1 behaviour stack. The swarm is **not yet listening** — the
/// returned value is positioned just before `swarm.listen_on(...)`.
pub fn build_swarm(keypair: identity::Keypair) -> Result<Swarm<Y7Behaviour>, AppError> {
    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| AppError::network(format!("tcp/noise/yamux setup: {e}")))?
        .with_behaviour(|kp| {
            Y7Behaviour::new(kp)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        })
        .map_err(|e| AppError::network(format!("behaviour setup: {e}")))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(IDLE_CONNECTION_TIMEOUT))
        .build();

    Ok(swarm)
}

/// Spawn the swarm task and return a [`NetHandle`] for talking to it.
///
/// The task listens on [`DEFAULT_LISTEN_ADDR`], emits a
/// `NetEvent::Listening` per bound address, then loops on
/// `tokio::select!` until either every `NetHandle` clone is dropped (the
/// command channel closes) or `NetCommand::Shutdown` is received.
pub fn spawn_swarm(mut swarm: Swarm<Y7Behaviour>) -> NetHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<NetCommand>(COMMAND_CHANNEL_CAPACITY);
    let (event_tx, event_rx) = broadcast::channel::<NetEvent>(EVENT_CHANNEL_CAPACITY);
    let event_tx_for_task = event_tx.clone();

    if let Err(e) = swarm.listen_on(
        DEFAULT_LISTEN_ADDR
            .parse()
            .expect("compile-time constant multiaddr"),
    ) {
        warn!("failed to start listener on {DEFAULT_LISTEN_ADDR}: {e}");
        let _ = event_tx.send(NetEvent::Error {
            message: format!("listen_on({DEFAULT_LISTEN_ADDR}) failed: {e}"),
        });
    }

    tokio::spawn(run_swarm(swarm, cmd_rx, event_tx_for_task));

    NetHandle {
        cmd_tx,
        event_rx,
        event_tx,
    }
}

/// Inner task body. Kept `async fn` (not a closure) so it has a
/// nameable type and can be unit-tested if needed.
async fn run_swarm(
    mut swarm: Swarm<Y7Behaviour>,
    mut cmd_rx: mpsc::Receiver<NetCommand>,
    event_tx: broadcast::Sender<NetEvent>,
) {
    let mut state = TaskState::default();

    loop {
        tokio::select! {
            // Process swarm events first to drain the network side
            // promptly under high load.
            biased;

            event = swarm.select_next_some() => {
                handle_swarm_event(&mut swarm, &mut state, &event_tx, event).await;
            }

            maybe_cmd = cmd_rx.recv() => {
                match maybe_cmd {
                    Some(NetCommand::Shutdown) => {
                        info!("net swarm task: shutdown requested");
                        break;
                    }
                    Some(cmd) => {
                        handle_command(&mut swarm, &mut state, &event_tx, cmd);
                    }
                    None => {
                        info!("net swarm task: command channel closed, exiting");
                        break;
                    }
                }
            }
        }
    }
}

/// Cached state for the swarm task — never escapes the task.
#[derive(Default)]
struct TaskState {
    /// Best-known addresses for each peer, populated from mDNS and
    /// identify events.
    address_book: HashMap<PeerId, Vec<Multiaddr>>,

    /// Pending response slots for outbound requests, keyed by the
    /// per-protocol `OutboundRequestId`. Each entry is the `oneshot`
    /// sender the caller is awaiting on.
    pending_handshake: HashMap<OutboundRequestId, oneshot::Sender<Result<HandshakeResp, AppError>>>,
    pending_msg: HashMap<OutboundRequestId, oneshot::Sender<Result<MsgResp, AppError>>>,
    pending_sync: HashMap<OutboundRequestId, oneshot::Sender<Result<SyncResp, AppError>>>,
}

impl TaskState {
    fn remember_address(&mut self, peer: PeerId, addr: Multiaddr) {
        let entry = self.address_book.entry(peer).or_default();
        if !entry.contains(&addr) {
            entry.push(addr);
        }
    }
}

// --------------------------------------------------------------------------
// Command handling
// --------------------------------------------------------------------------

fn handle_command(
    swarm: &mut Swarm<Y7Behaviour>,
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    cmd: NetCommand,
) {
    match cmd {
        NetCommand::Dial { y7_id } => {
            let peer = match peer_id_from_y7(&y7_id) {
                Ok(p) => p,
                Err(e) => {
                    debug!(error = %e, "dial: invalid Y7Id pubkey");
                    emit(
                        event_tx,
                        NetEvent::Error {
                            message: e.to_string(),
                        },
                    );
                    return;
                }
            };
            let addrs = state.address_book.get(&peer).cloned().unwrap_or_default();
            if addrs.is_empty() {
                debug!(%peer, "dial requested but no addresses known");
                emit(
                    event_tx,
                    NetEvent::Error {
                        message: format!("no known addresses for {peer}"),
                    },
                );
                return;
            }
            debug!(%peer, addr_count = addrs.len(), "dialing");
            // request_response::Behaviour::send_request will use peer
            // addresses from the swarm's own peer-address store; we
            // mirror them here so direct dials work.
            for addr in addrs {
                if let Err(e) = swarm.dial(addr.clone()) {
                    warn!(%peer, %addr, "dial failed: {e}");
                    emit(
                        event_tx,
                        NetEvent::Error {
                            message: format!("dial to {peer} at {addr} failed: {e}"),
                        },
                    );
                }
            }
        }

        NetCommand::DialAddress { address } => {
            // If the multiaddr ends with `/p2p/<peer-id>`, extract the
            // peer id so we can populate our address book — this lets
            // subsequent `send_*` calls re-use the address without a
            // fresh dial.
            if let Some(peer) = peer_id_from_multiaddr(&address) {
                state.remember_address(peer, address.clone());
                swarm.add_peer_address(peer, address.clone());
            }
            debug!(%address, "dialing multiaddr");
            if let Err(e) = swarm.dial(address.clone()) {
                warn!(%address, "dial failed: {e}");
                emit(
                    event_tx,
                    NetEvent::Error {
                        message: format!("dial to {address} failed: {e}"),
                    },
                );
            }
        }

        NetCommand::SendHandshake {
            peer,
            request,
            response_tx,
        } => {
            let id = swarm.behaviour_mut().handshake.send_request(&peer, request);
            state.pending_handshake.insert(id, response_tx);
        }

        NetCommand::SendMsg {
            peer,
            request,
            response_tx,
        } => {
            let id = swarm.behaviour_mut().msg.send_request(&peer, request);
            state.pending_msg.insert(id, response_tx);
        }

        NetCommand::SendSync {
            peer,
            request,
            response_tx,
        } => {
            let id = swarm.behaviour_mut().sync.send_request(&peer, request);
            state.pending_sync.insert(id, response_tx);
        }

        NetCommand::RespondHandshake { channel, response } => {
            if swarm
                .behaviour_mut()
                .handshake
                .send_response(channel, response)
                .is_err()
            {
                warn!("handshake response channel closed before send");
            }
        }
        NetCommand::RespondMsg { channel, response } => {
            if swarm
                .behaviour_mut()
                .msg
                .send_response(channel, response)
                .is_err()
            {
                warn!("msg response channel closed before send");
            }
        }
        NetCommand::RespondSync { channel, response } => {
            if swarm
                .behaviour_mut()
                .sync
                .send_response(channel, response)
                .is_err()
            {
                warn!("sync response channel closed before send");
            }
        }

        NetCommand::Shutdown => {
            // Handled by the caller of `handle_command`. Should never
            // arrive here in practice; if it does, we just no-op.
        }
    }
}

// --------------------------------------------------------------------------
// Swarm event handling
// --------------------------------------------------------------------------

async fn handle_swarm_event(
    swarm: &mut Swarm<Y7Behaviour>,
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: SwarmEvent<Y7BehaviourEvent>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!(%address, "listening");
            emit(event_tx, NetEvent::Listening { addr: address });
        }

        SwarmEvent::ConnectionEstablished {
            peer_id, endpoint, ..
        } => {
            let addr = endpoint.get_remote_address().clone();
            state.remember_address(peer_id, addr.clone());
            let kind = connection_kind_for(&endpoint);
            info!(%peer_id, %addr, ?kind, "connection established");
            emit(
                event_tx,
                NetEvent::ConnectionEstablished {
                    peer: peer_id,
                    kind,
                },
            );
        }

        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
            info!(%peer_id, ?cause, "connection closed");
            emit(event_tx, NetEvent::ConnectionClosed { peer: peer_id });
        }

        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            let peer = peer_id
                .map(|p| p.to_string())
                .unwrap_or_else(|| "<unknown>".into());
            warn!(%peer, "outgoing connection error: {error}");
            emit(
                event_tx,
                NetEvent::Error {
                    message: format!("dial to {peer} failed: {error}"),
                },
            );
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Mdns(event)) => {
            handle_mdns(swarm, state, event_tx, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Identify(event)) => {
            handle_identify(state, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Ping(_)) => {
            // RTT/liveness is interesting but doesn't drive a NetEvent
            // in V1. The application layer derives presence from
            // ConnectionEstablished / ConnectionClosed.
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Handshake(event)) => {
            handle_handshake_event(state, event_tx, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Msg(event)) => {
            handle_msg_event(state, event_tx, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Sync(event)) => {
            handle_sync_event(state, event_tx, event);
        }

        _ => {
            // Other variants (Dialing, IncomingConnection, ExternalAddr*, ...) are
            // not load-bearing in V1.
        }
    }
}

fn handle_mdns(
    swarm: &mut Swarm<Y7Behaviour>,
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: mdns::Event,
) {
    match event {
        mdns::Event::Discovered(peers) => {
            let mut grouped: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
            for (peer, addr) in peers {
                state.remember_address(peer, addr.clone());
                swarm.add_peer_address(peer, addr.clone());
                grouped.entry(peer).or_default().push(addr);
            }
            for (peer, addrs) in grouped {
                debug!(%peer, "mDNS discovered");
                emit(
                    event_tx,
                    NetEvent::PeerDiscovered {
                        peer,
                        addrs,
                        y7_id: y7_id_from_peer_id(&peer),
                    },
                );
            }
        }
        mdns::Event::Expired(peers) => {
            for (peer, addr) in peers {
                if let Some(addrs) = state.address_book.get_mut(&peer) {
                    addrs.retain(|a| a != &addr);
                }
                debug!(%peer, %addr, "mDNS expired");
            }
        }
    }
}

fn handle_identify(state: &mut TaskState, event: identify::Event) {
    if let identify::Event::Received { peer_id, info, .. } = event {
        debug!(
            %peer_id,
            agent = %info.agent_version,
            protocol = %info.protocol_version,
            listen_addr_count = info.listen_addrs.len(),
            "identify received"
        );
        for addr in info.listen_addrs {
            state.remember_address(peer_id, addr);
        }
    }
}

fn handle_handshake_event(
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: request_response::Event<crate::protocol::HandshakeReq, HandshakeResp>,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                emit(
                    event_tx,
                    NetEvent::HandshakeReceived {
                        peer,
                        request,
                        channel: TakeOnce::new(channel),
                    },
                );
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                if let Some(tx) = state.pending_handshake.remove(&request_id) {
                    let _ = tx.send(Ok(response));
                }
            }
        },
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => {
            if let Some(tx) = state.pending_handshake.remove(&request_id) {
                let _ = tx.send(Err(AppError::network(format!(
                    "handshake outbound failure: {error}"
                ))));
            }
        }
        request_response::Event::InboundFailure {
            peer,
            request_id,
            error,
            ..
        } => {
            warn!(%peer, %request_id, "handshake inbound failure: {error}");
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

fn handle_msg_event(
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: request_response::Event<crate::protocol::MsgReq, MsgResp>,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                emit(
                    event_tx,
                    NetEvent::MsgReceived {
                        peer,
                        request,
                        channel: TakeOnce::new(channel),
                    },
                );
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                if let Some(tx) = state.pending_msg.remove(&request_id) {
                    let _ = tx.send(Ok(response));
                }
            }
        },
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => {
            if let Some(tx) = state.pending_msg.remove(&request_id) {
                let _ = tx.send(Err(AppError::network(format!(
                    "msg outbound failure: {error}"
                ))));
            }
        }
        request_response::Event::InboundFailure {
            peer,
            request_id,
            error,
            ..
        } => {
            warn!(%peer, %request_id, "msg inbound failure: {error}");
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

fn handle_sync_event(
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: request_response::Event<crate::protocol::SyncReq, SyncResp>,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                emit(
                    event_tx,
                    NetEvent::SyncReceived {
                        peer,
                        request,
                        channel: TakeOnce::new(channel),
                    },
                );
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                if let Some(tx) = state.pending_sync.remove(&request_id) {
                    let _ = tx.send(Ok(response));
                }
            }
        },
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => {
            if let Some(tx) = state.pending_sync.remove(&request_id) {
                let _ = tx.send(Err(AppError::network(format!(
                    "sync outbound failure: {error}"
                ))));
            }
        }
        request_response::Event::InboundFailure {
            peer,
            request_id,
            error,
            ..
        } => {
            warn!(%peer, %request_id, "sync inbound failure: {error}");
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

// --------------------------------------------------------------------------
// Glue helpers
// --------------------------------------------------------------------------

fn emit(event_tx: &broadcast::Sender<NetEvent>, event: NetEvent) {
    // `broadcast::Sender::send` errors only when there are no
    // subscribers, which is benign and frequent (e.g. before the UI
    // attaches). Discard the error rather than log it.
    let _ = event_tx.send(event);
}

/// Classify an endpoint into a `y7ke-core::ConnectionKind`.
///
/// V1 has only LAN-style connections (the swarm only listens on
/// `/ip4/0.0.0.0/tcp/0` and discovers peers via mDNS), so we always
/// report `Lan`. V2 will add `Direct` and `Relayed` when DCUtR and
/// circuit relay land.
fn connection_kind_for(_endpoint: &ConnectedPoint) -> ConnectionKind {
    ConnectionKind::Lan
}

/// Build a libp2p `PeerId` from a Y7 identifier.
///
/// In V1 the long-term Ed25519 keypair is shared between Y7KE and libp2p,
/// so the mapping is simply `PeerId::from(PublicKey::Ed25519(y7_id.pubkey))`.
///
/// Returns `Err` if the underlying 32 bytes are not a valid Ed25519 point.
/// `Y7Id::parse` already validates input, so user-facing URIs land here as
/// valid; this fallback only fires on bytes obtained through unchecked
/// constructors (e.g. `Y7Id::from_pubkey(...)` with arbitrary input or a
/// corrupted DB row).
pub fn peer_id_from_y7(y7_id: &Y7Id) -> Result<PeerId, AppError> {
    let pubkey = identity::ed25519::PublicKey::try_from_bytes(y7_id.pubkey())
        .map_err(|e| AppError::network(format!("invalid Ed25519 pubkey in Y7Id: {e}")))?;
    let libp2p_pub: identity::PublicKey = identity::PublicKey::from(pubkey);
    Ok(libp2p_pub.to_peer_id())
}

/// Extract a `PeerId` from a multiaddr that ends with
/// `/p2p/<peer-id>`. Returns `None` if no `/p2p/` component is present.
fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    for proto in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer) = proto {
            return Some(peer);
        }
    }
    None
}

/// Recover the `Y7Id` from a libp2p `PeerId`.
///
/// Works whenever the peer's public key is *inlined* in the multihash
/// digest (which is the case for any Ed25519 key with libp2p — keys of
/// at most 42 bytes use the `identity` multihash). Returns `None` if
/// the digest is a SHA-256 fingerprint instead (i.e. the peer is using
/// a non-Ed25519 or unusually-large key — not a thing in V1).
pub fn y7_id_from_peer_id(peer: &PeerId) -> Option<Y7Id> {
    // `PeerId::to_bytes()` returns the multihash; we re-parse it to
    // access the digest. There is currently no stable accessor for the
    // raw digest, so we go through bytes.
    let bytes = peer.to_bytes();
    let mh = libp2p::multihash::Multihash::<64>::from_bytes(&bytes).ok()?;
    // 0x00 is the `identity` multihash code: digest == raw pubkey bytes.
    if mh.code() != 0x00 {
        return None;
    }
    let pubkey = identity::PublicKey::try_decode_protobuf(mh.digest()).ok()?;
    let ed = pubkey.try_into_ed25519().ok()?;
    Some(Y7Id::from_pubkey(ed.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_round_trips_through_y7_secret() {
        let secret = [7u8; 32];
        let kp1 = libp2p_keypair_from_y7_secret(&secret).unwrap();
        let kp2 = libp2p_keypair_from_y7_secret(&secret).unwrap();
        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }

    #[test]
    fn keypair_secret_byte_array_is_not_mutated() {
        let original = [7u8; 32];
        let copy = original;
        let _ = libp2p_keypair_from_y7_secret(&copy).unwrap();
        // `libp2p_keypair_from_y7_secret` works on an internal clone,
        // so the caller's buffer is untouched.
        assert_eq!(copy, original);
    }

    #[test]
    fn peer_id_round_trip_through_y7_id() {
        let secret = [42u8; 32];
        let kp = libp2p_keypair_from_y7_secret(&secret).unwrap();
        let peer_id = kp.public().to_peer_id();
        let ed_pub = kp.public().try_into_ed25519().unwrap();
        let y7 = Y7Id::from_pubkey(ed_pub.to_bytes());

        // Y7Id → PeerId
        assert_eq!(peer_id_from_y7(&y7).unwrap(), peer_id);
        // PeerId → Y7Id
        assert_eq!(y7_id_from_peer_id(&peer_id), Some(y7));
    }

    #[test]
    fn build_swarm_succeeds_with_random_key() {
        // mDNS sets up an UDP socket via the tokio reactor, so this test
        // must run inside a tokio runtime even though it never awaits.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("tokio runtime");
        rt.block_on(async {
            let kp = identity::Keypair::generate_ed25519();
            let _swarm = build_swarm(kp).expect("build_swarm");
        });
    }
}
