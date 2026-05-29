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

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    core::{transport::ListenerId, ConnectedPoint},
    dcutr, identify, identity, kad, mdns, noise, relay,
    request_response::{self, OutboundRequestId},
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        DialError, SwarmEvent,
    },
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use y7ke_core::settings::DialMode;
use y7ke_core::{AppError, ConnectionKind, Y7Id};

use crate::behaviour::{Y7Behaviour, Y7BehaviourEvent};
use crate::handle::{
    NetCommand, NetEvent, NetHandle, TakeOnce, COMMAND_CHANNEL_CAPACITY, EVENT_CHANNEL_CAPACITY,
};
use crate::protocol::{HandshakeResp, MsgResp, SyncResp};

/// Bootstrap peer multiaddrs hardcoded into release builds. Each entry
/// is an independent stable peer with its own PeerId — there's no
/// clustering and no shared state. Kad replicates routing between
/// whichever entries the client reaches.
///
/// The application layer (`y7ke-app::Config::load`) overrides this from
/// `~/.config/y7ke/bootstrap.toml` or the `Y7KE_BOOTSTRAP` env var.
// Transport-agnostic shorthand: the app layer expands it (via
// `y7ke_core::expand_bootstrap`) into BOTH a TCP and a QUIC multiaddr.
pub const DEFAULT_BOOTSTRAPS: &[&str] =
    &["/dns4/bootstrap1.y7v.lol/4101/p2p/12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo"];

/// Default TCP listen address (random port on all interfaces).
pub const DEFAULT_LISTEN_ADDR: &str = "/ip4/0.0.0.0/tcp/0";

/// Default QUIC (v1) listen address (random UDP port on all interfaces).
/// V2-A6 — bound in parallel with the TCP listener.
pub const DEFAULT_QUIC_LISTEN_ADDR: &str = "/ip4/0.0.0.0/udp/0/quic-v1";

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
        // V2-A6: QUIC alongside TCP. libp2p-quic provides its own Noise +
        // muxer, so no upgrade closures are needed here.
        .with_quic()
        .with_dns()
        .map_err(|e| AppError::network(format!("dns transport setup: {e}")))?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|e| AppError::network(format!("relay-client setup: {e}")))?
        .with_behaviour(|kp, relay_client| {
            Y7Behaviour::new(kp, relay_client)
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
pub fn spawn_swarm(swarm: Swarm<Y7Behaviour>) -> NetHandle {
    spawn_swarm_with_bootstraps(swarm, Vec::new(), DialMode::default())
}

/// Like [`spawn_swarm`] but seeds the Kademlia routing table with the
/// given bootstrap multiaddrs. Each entry must include `/p2p/<peer-id>`
/// so the routing table can map peer_id → addresses without a separate
/// identify round-trip. Entries without `/p2p/` are warned about and
/// skipped.
pub fn spawn_swarm_with_bootstraps(
    mut swarm: Swarm<Y7Behaviour>,
    bootstraps: Vec<Multiaddr>,
    initial_mode: DialMode,
) -> NetHandle {
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
    // V2-A6: bind QUIC in parallel. Best-effort — if UDP/0 fails (e.g.
    // sandbox without UDP), TCP remains operational.
    if let Err(e) = swarm.listen_on(
        DEFAULT_QUIC_LISTEN_ADDR
            .parse()
            .expect("compile-time constant multiaddr"),
    ) {
        warn!("failed to start listener on {DEFAULT_QUIC_LISTEN_ADDR}: {e}");
        let _ = event_tx.send(NetEvent::Error {
            message: format!("listen_on({DEFAULT_QUIC_LISTEN_ADDR}) failed: {e}"),
        });
    }

    // Seed the Kad routing table from configured bootstraps. We always
    // record the peer→addr map so a later mode switch can pick them up,
    // but only dial + bootstrap when not in LanOnly mode.
    let lan_only = matches!(initial_mode, DialMode::LanOnly);
    // Group bootstrap addrs by PeerId: one peer can have several transports
    // (TCP + QUIC from the same descriptor), all dialed together so libp2p
    // races them (QUIC wins on UDP-open networks → direct hole-punch path).
    let mut bootstrap_peers: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
    for addr in &bootstraps {
        match peer_id_from_multiaddr(addr) {
            Some(peer) => {
                let entry = bootstrap_peers.entry(peer).or_default();
                if !entry.contains(addr) {
                    entry.push(addr.clone());
                }
            }
            None => warn!(%addr, "bootstrap multiaddr lacks /p2p/<peer-id>; ignoring"),
        }
    }
    if !lan_only {
        for (peer, addrs) in &bootstrap_peers {
            for addr in addrs {
                swarm.behaviour_mut().kad.add_address(peer, addr.clone());
            }
            // Single DialOpts carrying every transport for this peer →
            // libp2p races them and keeps the first to connect.
            let opts = DialOpts::peer_id(*peer)
                .addresses(addrs.clone())
                .condition(PeerCondition::DisconnectedAndNotDialing)
                .build();
            match swarm.dial(opts) {
                Ok(()) => debug!(%peer, count = addrs.len(), "dialing bootstrap (tcp+quic race)"),
                Err(e) => warn!(%peer, "bootstrap dial failed: {e}"),
            }
        }
    }
    if !bootstraps.is_empty() && !lan_only {
        if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
            warn!("kad.bootstrap() failed: {e}");
        }
    }
    if lan_only {
        // Don't advertise providing while LAN-only.
        swarm.behaviour_mut().kad.set_mode(Some(kad::Mode::Client));
    }

    tokio::spawn(run_swarm(
        swarm,
        cmd_rx,
        event_tx_for_task,
        bootstrap_peers,
        initial_mode,
    ));

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
    bootstrap_peers: HashMap<PeerId, Vec<Multiaddr>>,
    initial_mode: DialMode,
) {
    let mut state = TaskState {
        bootstrap_peers,
        dial_mode: initial_mode,
        ..TaskState::default()
    };

    // V2-A4: probe bootstraps every BOOTSTRAP_RECONNECT_INTERVAL and
    // redial any that aren't currently connected. Without this a single
    // bootstrap restart leaves clients orphaned until Kad's 5-min
    // periodic bootstrap kicks in.
    let mut reconnect_tick = tokio::time::interval(BOOTSTRAP_RECONNECT_INTERVAL);
    reconnect_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    reconnect_tick.tick().await; // skip the immediate first tick

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

            _ = reconnect_tick.tick() => {
                if !matches!(state.dial_mode, DialMode::LanOnly) {
                    reconnect_lost_bootstraps(&mut swarm, &state);
                }
            }
        }
    }
}

const BOOTSTRAP_RECONNECT_INTERVAL: Duration = Duration::from_secs(15);

fn reconnect_lost_bootstraps(swarm: &mut Swarm<Y7Behaviour>, state: &TaskState) {
    for (peer, addrs) in &state.bootstrap_peers {
        if swarm.is_connected(peer) {
            continue;
        }
        // One DialOpts carrying every transport for this peer, gated by
        // `DisconnectedAndNotDialing` so a slow dial that hasn't resolved
        // isn't re-issued on the next 15-s tick (no redial pile-up during
        // an outage). DisconnectedAndNotDialing also collapses the
        // multi-addr race into a single in-flight attempt per peer, so
        // adding QUIC doesn't multiply dial pressure on the VPS.
        let opts = DialOpts::peer_id(*peer)
            .addresses(addrs.clone())
            .condition(PeerCondition::DisconnectedAndNotDialing)
            .build();
        match swarm.dial(opts) {
            Ok(()) => debug!(%peer, count = addrs.len(), "redialing lost bootstrap (tcp+quic)"),
            Err(DialError::DialPeerConditionFalse(_)) => {
                debug!(%peer, "bootstrap redial skipped — dial already in progress")
            }
            Err(e) => debug!(%peer, error = %e, "bootstrap redial failed"),
        }
    }
}

/// Oneshot slot for an in-flight Kad `FindPeer` query.
type PendingFind = (PeerId, oneshot::Sender<Result<Vec<Multiaddr>, AppError>>);

/// Cached state for the swarm task — never escapes the task.
#[derive(Default)]
struct TaskState {
    /// Best-known addresses for each peer, populated from mDNS,
    /// identify, and Kad-routing events.
    address_book: HashMap<PeerId, Vec<Multiaddr>>,

    /// Pending response slots for outbound requests, keyed by the
    /// per-protocol `OutboundRequestId`. Each entry is the `oneshot`
    /// sender the caller is awaiting on.
    pending_handshake: HashMap<OutboundRequestId, oneshot::Sender<Result<HandshakeResp, AppError>>>,
    pending_msg: HashMap<OutboundRequestId, oneshot::Sender<Result<MsgResp, AppError>>>,
    pending_sync: HashMap<OutboundRequestId, oneshot::Sender<Result<SyncResp, AppError>>>,

    /// Pending `FindPeer` queries — keyed by Kad `QueryId`. Each entry
    /// is the target PeerId we're looking for and the oneshot to
    /// resolve when the addresses are known (or the query completes).
    pending_find_peer: HashMap<kad::QueryId, PendingFind>,

    /// True once `kad.start_providing(self)` has been issued. Deferred
    /// until the routing table has at least one peer — calling it
    /// against an empty table fails with `NoKnownPeers`.
    provided_self: bool,

    /// Bootstrap peers we configured at startup → their full multiaddr.
    /// Used by `ConnectionEstablished` to know which connections deserve
    /// a `listen_on(<addr>/p2p-circuit)` reservation request.
    bootstrap_peers: HashMap<PeerId, Vec<Multiaddr>>,

    /// Bootstrap peers we have already issued a reservation `listen_on`
    /// for in this task's lifetime — guards against re-issuing every
    /// time the connection drops and reconnects.
    relay_reserved: HashSet<PeerId>,

    /// `/p2p-circuit` listeners we successfully started, keyed by the
    /// relay PeerId. Tracked so `ApplyDialMode(LanOnly)` can call
    /// `swarm.remove_listener(listener_id)` to tear them down.
    circuit_listeners: HashMap<PeerId, ListenerId>,

    /// Current live `DialMode`. Mirrors the user's setting; flipped by
    /// `NetCommand::ApplyDialMode`.
    dial_mode: DialMode,
}

/// Max cached dial addresses per peer. A peer rotating ephemeral source
/// ports would otherwise grow its address_book Vec without bound; keep
/// the most-recently-learned few (direct QUIC/TCP + a circuit is plenty).
const MAX_ADDRS_PER_PEER: usize = 16;

impl TaskState {
    fn remember_address(&mut self, peer: PeerId, addr: Multiaddr) {
        let entry = self.address_book.entry(peer).or_default();
        if !entry.contains(&addr) {
            entry.push(addr);
            if entry.len() > MAX_ADDRS_PER_PEER {
                entry.remove(0);
            }
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
        NetCommand::Dial { y7_id, response_tx } => {
            let peer = match peer_id_from_y7(&y7_id) {
                Ok(p) => p,
                Err(e) => {
                    debug!(error = %e, "dial: invalid Y7Id pubkey");
                    let _ = response_tx.send(Err(e));
                    return;
                }
            };
            let mut addrs = state.address_book.get(&peer).cloned().unwrap_or_default();
            // Under LanOnly, never dial a /p2p-circuit (relay) address even
            // if identify-push or Kad re-seeded one into the address book
            // after the mode-transition prune. This is the dial-decision
            // chokepoint, so it closes the LanOnly relay-dial leak fully
            // (the transition-time prune alone left a re-population window).
            if matches!(state.dial_mode, DialMode::LanOnly) {
                addrs.retain(|a| {
                    !a.iter()
                        .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit))
                });
            }
            if addrs.is_empty() {
                debug!(%peer, "dial requested but no addresses known");
                let _ = response_tx.send(Ok(false));
                return;
            }
            // Reconnect-storm guard: issue a SINGLE dial attempt for the
            // peer carrying every known address, gated by
            // `DisconnectedAndNotDialing`. libp2p dedups against an
            // already-established connection or an in-flight dial, so N
            // ticker/UI redials of the same peer collapse into at most one
            // socket — no N×addrs fan-out. A condition-false result means
            // we're already connected or mid-dial: treat it as success so
            // the discovery chain doesn't fall through to a redundant Kad
            // lookup.
            let addr_count = addrs.len();
            let opts = DialOpts::peer_id(peer)
                .addresses(addrs)
                .condition(PeerCondition::DisconnectedAndNotDialing)
                .build();
            match swarm.dial(opts) {
                Ok(()) => {
                    debug!(%peer, addr_count, "dial issued (by Y7Id)");
                    let _ = response_tx.send(Ok(true));
                }
                Err(DialError::DialPeerConditionFalse(_)) => {
                    debug!(%peer, "dial skipped — already connected or dialing");
                    let _ = response_tx.send(Ok(true));
                }
                Err(DialError::NoAddresses) => {
                    let _ = response_tx.send(Ok(false));
                }
                Err(e) => {
                    warn!(%peer, error = %e, "dial failed");
                    emit(
                        event_tx,
                        NetEvent::Error {
                            message: format!("dial to {peer} failed: {e}"),
                        },
                    );
                    // Addresses existed and an attempt was made; don't
                    // re-trigger discovery on a transport-level error.
                    let _ = response_tx.send(Ok(true));
                }
            }
        }

        NetCommand::FindPeer { y7_id, response_tx } => {
            let peer = match peer_id_from_y7(&y7_id) {
                Ok(p) => p,
                Err(e) => {
                    let _ = response_tx.send(Err(e));
                    return;
                }
            };
            // Fast path: if we already know addresses (mDNS / identify
            // / earlier Kad), short-circuit without issuing a query.
            if let Some(addrs) = state.address_book.get(&peer) {
                if !addrs.is_empty() {
                    let _ = response_tx.send(Ok(addrs.clone()));
                    return;
                }
            }
            let key = kad::RecordKey::new(&peer.to_bytes());
            let qid = swarm.behaviour_mut().kad.get_providers(key);
            state.pending_find_peer.insert(qid, (peer, response_tx));
            debug!(%peer, "kad find_peer issued");
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
            let class = addr_class(&address);
            debug!(%address, class, "dialing multiaddr");
            if let Err(e) = swarm.dial(address.clone()) {
                warn!(%address, class, "dial failed: {e}");
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

        NetCommand::UpdateBootstraps { addresses } => {
            // Incoming addresses are already expanded (config.rs) — group
            // by PeerId so a bootstrap's TCP + QUIC addrs share one entry.
            let mut next: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
            for addr in &addresses {
                if let Some(peer) = peer_id_from_multiaddr(addr) {
                    let e = next.entry(peer).or_default();
                    if !e.contains(addr) {
                        e.push(addr.clone());
                    }
                } else {
                    warn!(%addr, "update_bootstraps: addr missing /p2p/<peer-id>; ignoring");
                }
            }
            // Seed Kad + dial anything new. While LanOnly we only update
            // the recorded map and skip dialing.
            let lan_only = matches!(state.dial_mode, DialMode::LanOnly);
            for (peer, addrs) in &next {
                if state.bootstrap_peers.contains_key(peer) || lan_only {
                    continue;
                }
                for addr in addrs {
                    swarm.behaviour_mut().kad.add_address(peer, addr.clone());
                }
                let opts = DialOpts::peer_id(*peer)
                    .addresses(addrs.clone())
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .build();
                if let Err(e) = swarm.dial(opts) {
                    warn!(%peer, "update_bootstraps: dial failed: {e}");
                } else {
                    debug!(%peer, count = addrs.len(), "update_bootstraps: dialing new bootstrap");
                }
            }
            state.bootstrap_peers = next;
            if !lan_only && !state.bootstrap_peers.is_empty() {
                if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
                    debug!("update_bootstraps: kad.bootstrap() failed: {e}");
                }
            }
        }

        NetCommand::ApplyDialMode { mode } => {
            let prev = state.dial_mode;
            state.dial_mode = mode;
            info!(?prev, next = ?mode, "applying dial_mode");
            match mode {
                DialMode::LanOnly => {
                    // Tear down every active circuit listener.
                    let listener_ids: Vec<(PeerId, ListenerId)> =
                        state.circuit_listeners.drain().collect();
                    for (peer, lid) in listener_ids {
                        if swarm.remove_listener(lid) {
                            info!(%peer, ?lid, "relay: dropped circuit listener");
                        }
                    }
                    state.relay_reserved.clear();
                    // Drop cached /p2p-circuit addrs from the address
                    // book. NetCommand::Dial replays every known addr
                    // unfiltered, so a leftover circuit entry would let a
                    // soft-redial issue a relay dial under LanOnly,
                    // breaking the LanOnly contract. Re-entering Internet
                    // mode repopulates them via remember_address.
                    for addrs in state.address_book.values_mut() {
                        addrs.retain(|a| {
                            !a.iter()
                                .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit))
                        });
                    }
                    state.address_book.retain(|_, addrs| !addrs.is_empty());
                    // Disconnect bootstrap peers.
                    let peers: Vec<PeerId> = state.bootstrap_peers.keys().copied().collect();
                    for peer in peers {
                        let _ = swarm.disconnect_peer_id(peer);
                        debug!(%peer, "lan-only: disconnecting bootstrap");
                    }
                    // Move Kad to Client mode so we don't advertise.
                    swarm.behaviour_mut().kad.set_mode(Some(kad::Mode::Client));
                    state.provided_self = false;
                }
                DialMode::Internet => {
                    // Re-enable Kad advertising; routing-updated will
                    // start_providing again next time it fires.
                    swarm.behaviour_mut().kad.set_mode(Some(kad::Mode::Server));
                    // Re-seed Kad + immediately probe bootstraps so the
                    // user doesn't wait BOOTSTRAP_RECONNECT_INTERVAL for
                    // the first dial.
                    for (peer, addrs) in state.bootstrap_peers.clone() {
                        for addr in addrs {
                            swarm.behaviour_mut().kad.add_address(&peer, addr);
                        }
                    }
                    reconnect_lost_bootstraps(swarm, state);
                    if !state.bootstrap_peers.is_empty() {
                        if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
                            debug!("apply_dial_mode: kad.bootstrap() failed: {e}");
                        }
                    }
                }
            }
        }

        NetCommand::CheckLive { y7_id, response_tx } => {
            let result = peer_id_from_y7(&y7_id).map(|peer| swarm.is_connected(&peer));
            let _ = response_tx.send(result);
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
            peer_id,
            connection_id,
            endpoint,
            ..
        } => {
            let addr = endpoint.get_remote_address().clone();
            state.remember_address(peer_id, addr.clone());
            let kind = connection_kind_for(&endpoint);
            info!(%peer_id, %addr, ?kind, ?connection_id, "connection established");
            emit(
                event_tx,
                NetEvent::ConnectionEstablished {
                    peer: peer_id,
                    connection_id,
                    kind,
                    endpoint_addr: addr.clone(),
                },
            );

            // V2-A4: bootstrap connections double as relay servers. Ask
            // the relay client to listen on `/p2p-circuit` via this
            // bootstrap so other clients can dial us through it.
            // Skipped while LanOnly — a circuit listen has no point if
            // we're not soliciting internet traffic.
            if !matches!(state.dial_mode, DialMode::LanOnly)
                && !endpoint.is_relayed()
                && state.bootstrap_peers.contains_key(&peer_id)
                && state.relay_reserved.insert(peer_id)
            {
                // Reserve a circuit over WHATEVER transport actually
                // connected (QUIC if QUIC won the race) by deriving the
                // circuit addr from the live endpoint, not a stored addr.
                // Strip any trailing /p2p, re-add the relay's /p2p, then
                // /p2p-circuit → `<transport>/p2p/<relay>/p2p-circuit`.
                use libp2p::multiaddr::Protocol;
                let mut relay_addr = addr.clone();
                if matches!(relay_addr.iter().last(), Some(Protocol::P2p(_))) {
                    relay_addr.pop();
                }
                let circuit_addr = relay_addr
                    .with(Protocol::P2p(peer_id))
                    .with(Protocol::P2pCircuit);
                match swarm.listen_on(circuit_addr.clone()) {
                    Ok(lid) => {
                        state.circuit_listeners.insert(peer_id, lid);
                        info!(%peer_id, %circuit_addr, ?lid, "relay: requesting reservation");
                    }
                    Err(e) => {
                        state.relay_reserved.remove(&peer_id);
                        warn!(%peer_id, error = %e, "relay: listen_on circuit failed");
                    }
                }
            }
        }

        SwarmEvent::ConnectionClosed {
            peer_id,
            connection_id,
            cause,
            ..
        } => {
            info!(%peer_id, ?cause, ?connection_id, "connection closed");
            // Bootstrap dropped — clear the relay-reservation guard so
            // the next reconnect re-runs `listen_on(<addr>/p2p-circuit)`.
            if state.bootstrap_peers.contains_key(&peer_id) {
                state.relay_reserved.remove(&peer_id);
                if let Some(lid) = state.circuit_listeners.remove(&peer_id) {
                    // The listener self-terminates when the underlying
                    // connection drops; calling remove_listener defends
                    // against the rare case where it lingers.
                    swarm.remove_listener(lid);
                }
            } else if !swarm.is_connected(&peer_id) {
                // Non-bootstrap peer fully disconnected: drop its dial
                // cache so address_book doesn't accumulate an entry for
                // every peer ever surfaced by DHT/identify churn. It's an
                // L1 cache only — contacts re-dial from the peer_state DB
                // cache + a Kad lookup.
                state.address_book.remove(&peer_id);
            }
            emit(
                event_tx,
                NetEvent::ConnectionClosed {
                    peer: peer_id,
                    connection_id,
                },
            );
        }

        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            let peer = peer_id
                .map(|p| p.to_string())
                .unwrap_or_else(|| "<unknown>".into());
            // Dial errors are routine: Kad's periodic routing
            // maintenance probes every peer it ever heard of, and
            // long-dead local addresses (172.17.x.x docker bridges,
            // 192.168.x.x home LANs of strangers, Tailscale interfaces)
            // refuse the connection. libp2p evicts stale entries on
            // its own; logging each one at WARN drowns out the
            // signal. User-initiated dials surface via
            // `AppError::Network` in the calling command instead.
            debug!(%peer, "outgoing connection error: {error}");
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

        SwarmEvent::Behaviour(Y7BehaviourEvent::Kad(event)) => {
            handle_kad(swarm, state, event_tx, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::RelayClient(event)) => {
            handle_relay_client(event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::Dcutr(event)) => {
            handle_dcutr(event_tx, event);
        }

        SwarmEvent::Behaviour(Y7BehaviourEvent::AutonatClient(event)) => {
            handle_autonat(event_tx, event);
        }

        _ => {
            // Other variants (Dialing, IncomingConnection, ExternalAddr*, ...) are
            // not load-bearing in V1.
        }
    }
}

/// V2-A5: forward DCUtR upgrade outcomes to the app layer. On success
/// the swarm has a fresh direct connection to the peer; on failure the
/// existing relayed connection stays in place untouched.
fn handle_dcutr(event_tx: &broadcast::Sender<NetEvent>, event: dcutr::Event) {
    match event.result {
        Ok(conn_id) => {
            info!(peer = %event.remote_peer_id, ?conn_id, "dcutr: direct upgrade succeeded");
            emit(
                event_tx,
                NetEvent::ConnectionUpgraded {
                    peer: event.remote_peer_id,
                    connection_id: conn_id,
                    kind: ConnectionKind::Direct,
                },
            );
        }
        Err(e) => {
            info!(peer = %event.remote_peer_id, error = %e, "dcutr: direct upgrade failed (staying on relay)");
            emit(
                event_tx,
                NetEvent::ConnectionUpgradeFailed {
                    peer: event.remote_peer_id,
                    error: e.to_string(),
                },
            );
        }
    }
}

/// V2-A3: forward AutoNAT v2 probe verdicts. A successful test confirms
/// `tested_addr` is externally reachable; failure means the server's
/// fresh outbound dial couldn't reach us at that address. The app layer
/// aggregates these into a single `NatReachability` for UI display and
/// upgrade-loop gating.
fn handle_autonat(
    event_tx: &broadcast::Sender<NetEvent>,
    event: libp2p::autonat::v2::client::Event,
) {
    let reachable = event.result.is_ok();
    let outcome = if reachable {
        "reachable"
    } else {
        "unreachable"
    };
    info!(
        peer = %event.server,
        addr = %event.tested_addr,
        bytes_sent = event.bytes_sent,
        outcome,
        "autonat: probe result"
    );
    emit(
        event_tx,
        NetEvent::NatStatus {
            tested_addr: event.tested_addr,
            server: event.server,
            reachable,
        },
    );
}

fn handle_relay_client(event: relay::client::Event) {
    match event {
        relay::client::Event::ReservationReqAccepted {
            relay_peer_id,
            renewal,
            ..
        } => {
            info!(%relay_peer_id, renewal, "relay: reservation accepted");
        }
        relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
            info!(%relay_peer_id, "relay: outbound circuit established");
        }
        relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
            info!(%src_peer_id, "relay: inbound circuit established");
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

fn handle_kad(
    swarm: &mut Swarm<Y7Behaviour>,
    state: &mut TaskState,
    event_tx: &broadcast::Sender<NetEvent>,
    event: kad::Event,
) {
    match event {
        kad::Event::RoutingUpdated {
            peer, addresses, ..
        } => {
            debug!(%peer, addr_count = addresses.len(), "kad routing updated");
            for addr in addresses.iter() {
                state.remember_address(peer, addr.clone());
                swarm.add_peer_address(peer, addr.clone());
            }
            // First time we have anyone in the routing table → safe to
            // advertise self.
            if !state.provided_self {
                let own_peer = *swarm.local_peer_id();
                let key = kad::RecordKey::new(&own_peer.to_bytes());
                match swarm.behaviour_mut().kad.start_providing(key) {
                    Ok(_) => {
                        state.provided_self = true;
                        info!(%own_peer, "kad: started providing self");
                    }
                    Err(e) => debug!("start_providing deferred: {e}"),
                }
            }
        }
        kad::Event::OutboundQueryProgressed {
            id, result, step, ..
        } => {
            if let kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                providers,
                ..
            })) = &result
            {
                // Borrow target separately from the mutable remove() below.
                let target = state.pending_find_peer.get(&id).map(|(t, _)| *t);
                if let Some(target) = target {
                    if providers.contains(&target) {
                        let addrs = state.address_book.get(&target).cloned().unwrap_or_default();
                        if !addrs.is_empty() {
                            if let Some((_, tx)) = state.pending_find_peer.remove(&id) {
                                debug!(%target, addr_count = addrs.len(), "find_peer: resolved via Kad");
                                let _ = tx.send(Ok(addrs));
                            }
                        }
                        // If we matched the target but don't yet have
                        // their addresses in the address_book, wait for
                        // `RoutingUpdated` to populate them; the
                        // query's final step (below) will give up if
                        // nothing arrives.
                    }
                }
            }
            // Query terminated (success or failure) — drain any pending
            // entry that didn't already resolve.
            if step.last {
                if let Some((target, tx)) = state.pending_find_peer.remove(&id) {
                    let addrs = state.address_book.get(&target).cloned().unwrap_or_default();
                    if addrs.is_empty() {
                        debug!(%target, "find_peer: Kad query exhausted with no addresses");
                        let _ = tx.send(Err(AppError::NotFound));
                    } else {
                        let _ = tx.send(Ok(addrs));
                    }
                }
            }
        }
        kad::Event::InboundRequest { .. } | kad::Event::ModeChanged { .. } => {
            // Informational; nothing to do.
        }
        other => {
            debug!(?other, "kad event");
        }
    }
    // Silence the unused-warning for event_tx — we may emit
    // PeerDiscovered from Kad routing updates in a future iteration.
    let _ = event_tx;
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
/// V2-A1 distinguishes LAN-private (RFC 1918 / link-local / loopback)
/// addrs — what mDNS would have surfaced anyway — from anything else,
/// which we mark `Internet`. The DCUtR-upgraded `Direct` and the
/// relay-routed `Relayed` variants land later.
fn connection_kind_for(endpoint: &ConnectedPoint) -> ConnectionKind {
    // For an inbound relayed connection the remote address is just
    // `/p2p/<src>` (no `p2p-circuit` component) — the circuit marker
    // lives in `local_addr` instead. `ConnectedPoint::is_relayed`
    // handles both endpoint roles correctly.
    let remote = endpoint.get_remote_address();
    let kind = if endpoint.is_relayed() {
        ConnectionKind::Relayed
    } else if multiaddr_is_lan(remote) {
        ConnectionKind::Lan
    } else {
        ConnectionKind::Internet
    };
    debug!(addr = %remote, ?kind, "connection_kind_for: classified");
    kind
}

/// Classify a `Multiaddr` into a coarse transport bucket for debug
/// logs at each `swarm.dial(...)` site.
fn addr_class(addr: &Multiaddr) -> &'static str {
    let is_circuit = addr
        .iter()
        .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2pCircuit));
    if is_circuit {
        "relay"
    } else if multiaddr_is_lan(addr) {
        "lan"
    } else {
        "internet"
    }
}

/// Best-effort check: is `addr` a LAN-private or loopback address?
/// We look at the first `Ip4` / `Ip6` component and apply RFC 1918 /
/// IPv6 unique-local rules. Anything else (DNS names, public IPs,
/// relay multiaddrs) is treated as non-LAN.
pub fn multiaddr_is_lan(addr: &Multiaddr) -> bool {
    for proto in addr.iter() {
        match proto {
            libp2p::multiaddr::Protocol::Ip4(ip) => {
                return ip.is_loopback()
                    || ip.is_private()
                    || ip.is_link_local()
                    || ip.is_unspecified();
            }
            libp2p::multiaddr::Protocol::Ip6(ip) => {
                if ip.is_loopback() || ip.is_unspecified() {
                    return true;
                }
                // Unique-local fc00::/7 + link-local fe80::/10.
                let seg = ip.segments()[0];
                return (seg & 0xfe00) == 0xfc00 || (seg & 0xffc0) == 0xfe80;
            }
            _ => continue,
        }
    }
    false
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
