//! Composition root: AppHandle owns storage + swarm + identity + event bus.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use libp2p::swarm::ConnectionId;
use tokio::sync::broadcast;
use y7ke_core::crypto::SigningKey;
use y7ke_core::error::Result;
use y7ke_core::{AppEvent, ConnectionKind, Y7Id};
use y7ke_net::{
    build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm_with_bootstraps, NetHandle,
};
use y7ke_storage::{Db, DbConfig};

use crate::rate_limit::RateLimiter;
use crate::{event_loop, identity};

mod contacts;
mod messages;
mod settings;

/// In-memory ping cache entry. Updated only by `ping_all_bootstraps`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BootstrapPingState {
    pub last_ping_ms: Option<u64>,
    pub last_ping_failed: bool,
}

pub const EVENT_CHANNEL_CAPACITY: usize = 256;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
// Outer guard on the first send. Must sit ABOVE the request-response
// timeout (15 s, behaviour.rs) — a shorter cap abandons the oneshot while
// the wire request is still alive, discarding a slow-but-succeeding ack and
// spuriously re-queueing (the "every other message hangs at sending" bug).
// 20 s leaves headroom past the 15 s rr timeout while still bounding a
// genuinely wedged oneshot.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

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

pub(crate) struct AppInner {
    pub db: Db,
    pub net: NetHandle,
    pub me: SigningKey,
    pub my_pubkey: [u8; 32],
    pub my_y7_id: Y7Id,
    /// Presence cache: the *best* kind currently active per peer. A
    /// derived view of `connections`, recomputed on every connection
    /// event. Read by list_contacts to survive the boot race where
    /// PresenceChanged fires before the UI listener attaches.
    pub presence: tokio::sync::RwLock<HashMap<Y7Id, ConnectionKind>>,
    /// Authoritative per-connection state: one entry per live libp2p
    /// connection, keyed by `ConnectionId`. A peer holding several
    /// connections (relay + direct after a DCUtR upgrade, or LAN +
    /// relay) keeps one entry each, so a single `ConnectionClosed`
    /// removes only that connection and presence is recomputed from the
    /// survivors — a relay drop can never hide a live direct path.
    /// `presence` and `connection_meta` are derived caches of this map.
    pub connections: tokio::sync::RwLock<HashMap<Y7Id, HashMap<ConnectionId, ConnEntry>>>,
    pub rate_limiter: RateLimiter,
    /// Last-known ping state per bootstrap multiaddr (keyed by the full
    /// multiaddr string). Empty at boot — populated by `ping_all_bootstraps`.
    pub bootstrap_pings: tokio::sync::RwLock<HashMap<String, BootstrapPingState>>,
    /// AutoNAT v2 aggregate reachability verdict. Populated by event_loop
    /// rolling individual probe results into a verdict with a small
    /// flap-resistance window (≥3 failures before downgrading from
    /// Public). Drives the connectivity-debug UI pill and the
    /// upgrade-from-relay loop.
    pub nat_status: tokio::sync::RwLock<NatStatusState>,
    /// DCUtR upgrade counters — incremented by event_loop on every
    /// NetEvent::ConnectionUpgraded / ConnectionUpgradeFailed. Read by
    /// the Tauri `get_dcutr_stats` command for the Connectivity debug
    /// pane. AtomicU64 so the Tauri command can read without acquiring
    /// any RwLock.
    pub dcutr_attempts: std::sync::atomic::AtomicU64,
    pub dcutr_successes: std::sync::atomic::AtomicU64,
    pub dcutr_failures: std::sync::atomic::AtomicU64,
    /// Last few DCUtR failure reason strings (bounded ring, oldest first).
    /// The counters above say how many failed; this says why. Surfaced in
    /// the diagnostics export via `diagnostics_detail`.
    pub dcutr_recent_failures: tokio::sync::Mutex<std::collections::VecDeque<String>>,
    /// Inbound RPCs refused by the rate limiter since boot, per protocol.
    /// Counted at the warn sites in event_loop so a silent drop storm is
    /// visible in the diagnostics export. Atomic for lock-free command reads.
    pub rl_drops_handshake: std::sync::atomic::AtomicU64,
    pub rl_drops_msg: std::sync::atomic::AtomicU64,
    pub rl_drops_sync: std::sync::atomic::AtomicU64,
    /// (tested_addr, probe-server PeerId) of the last AutoNAT probe absorbed.
    /// Kept out of `nat_status` so that stays `Copy`; surfaced in the export.
    pub nat_probe_detail: tokio::sync::RwLock<Option<(String, String)>>,
    /// Wake signal for the presence ticker. Fired by `update_settings`
    /// (mode change) and by `handle_nat_status` (verdict flip) so
    /// presence/relay-upgrade work happens within ~1 s of the event
    /// instead of waiting for the next 30-s tick. The ticker
    /// `select!`s on this alongside the interval timer; either source
    /// triggers the same body.
    pub wake_notify: tokio::sync::Notify,
    /// Per-Relayed-peer state for the upgrade-from-relay loop: counts
    /// how many DCUtR attempts we've absorbed since the last
    /// observed-addr or NAT-verdict change. Used to apply
    /// exponential backoff so we don't re-dial a peer on every tick
    /// once we've established the relay path is the only one
    /// available. Cleared per peer on a successful upgrade or on a
    /// signal change.
    pub upgrade_backoff: tokio::sync::RwLock<HashMap<Y7Id, u32>>,
    /// Per-peer exponential backoff for the presence ticker's Offline
    /// arm. Bounds the otherwise-unbounded "re-dial every offline
    /// contact every tick" into a 30s→10min schedule; reset per peer on
    /// `ConnectionEstablished`. See `crate::reconnect`.
    pub reconnect_backoff: tokio::sync::RwLock<HashMap<Y7Id, crate::reconnect::Backoff>>,
    /// Caps concurrent Kad `find_peer` lookups from `dial_with_discovery`
    /// so a reconnect storm (50 contacts returning after a suspend)
    /// can't flood the DHT with simultaneous provider queries.
    pub kad_lookups: tokio::sync::Semaphore,
    /// Peers with a sync reconcile in flight. A reconnect burst (relay +
    /// DCUtR ConnectionEstablished + PeerDiscovered) would otherwise spawn
    /// several reconciles for the same peer, racing each other through its
    /// inbound sync rate-limit bucket and truncating. One at a time.
    pub syncing: tokio::sync::Mutex<std::collections::HashSet<Y7Id>>,
    /// Per-peer metadata about the *current* best-kind connection,
    /// derived from the libp2p multiaddr on `ConnectionEstablished`.
    /// Surfaces in the Connectivity debug pane via
    /// `list_active_connections`. Cleared by `ConnectionClosed`.
    pub connection_meta: tokio::sync::RwLock<HashMap<Y7Id, ConnectionMeta>>,
}

/// Per-active-connection metadata exposed via the Connectivity pane.
#[derive(Debug, Clone, Default)]
pub struct ConnectionMeta {
    /// For Relayed connections: the relay's host portion (DNS name or
    /// IP) extracted from the multiaddr before `/p2p-circuit`.
    pub via_host: Option<String>,
    /// Underlying transport (TCP or QUIC) extracted from the multiaddr.
    pub transport: Option<y7ke_core::Transport>,
}

/// One live libp2p connection's facts, stored in `AppInner::connections`
/// keyed by `ConnectionId`. `kind` is set from the endpoint on
/// `ConnectionEstablished` and relabelled `Direct` on a DCUtR upgrade;
/// `meta` carries the transport/relay-host for the Connectivity pane.
#[derive(Debug, Clone)]
pub struct ConnEntry {
    pub kind: ConnectionKind,
    pub meta: ConnectionMeta,
}

/// Extract the host segment (e.g. `bootstrap1.y7v.lol`) from a relayed
/// multiaddr, looking at the `/dns4|6` / `/ip4|6` immediately before
/// `/p2p-circuit`. Returns `None` if the multiaddr isn't a circuit or
/// the leading transport component can't be parsed.
pub fn extract_relay_via_host(addr: &libp2p::Multiaddr) -> Option<String> {
    use libp2p::multiaddr::Protocol;
    let mut host: Option<String> = None;
    for p in addr.iter() {
        match p {
            Protocol::Dns4(n) | Protocol::Dns6(n) | Protocol::Dns(n) => {
                host = Some(n.to_string());
            }
            Protocol::Ip4(ip) => {
                host = Some(ip.to_string());
            }
            Protocol::Ip6(ip) => {
                host = Some(ip.to_string());
            }
            Protocol::P2pCircuit => return host,
            _ => {}
        }
    }
    None
}

/// Classify the underlying transport (TCP vs QUIC) from a multiaddr.
pub fn extract_transport(addr: &libp2p::Multiaddr) -> Option<y7ke_core::Transport> {
    use libp2p::multiaddr::Protocol;
    for p in addr.iter() {
        match p {
            Protocol::QuicV1 | Protocol::Quic => return Some(y7ke_core::Transport::Quic),
            Protocol::Tcp(_) => return Some(y7ke_core::Transport::Tcp),
            _ => {}
        }
    }
    None
}

/// Aggregate state derived from AutoNAT v2 probe results.
///
/// `verdict` starts `Unknown`; flips to `Public` on any reachable probe,
/// or to `Private` after enough consecutive failures (see
/// `event_loop::handle_nat_status` for the precise FSM). UI reads
/// `verdict` exclusively; `consecutive_failures` is internal flap
/// suppression.
#[derive(Debug, Clone, Copy, Default)]
pub struct NatStatusState {
    pub verdict: y7ke_core::NatReachability,
    pub consecutive_failures: u32,
}

pub struct AppHandle {
    pub(crate) inner: Arc<AppInner>,
    pub(crate) event_tx: broadcast::Sender<AppEvent>,
}

impl AppHandle {
    pub async fn boot(config: AppConfig) -> Result<Self> {
        let started = std::time::Instant::now();
        let db = Db::open(config.db).await?;
        let local = identity::ensure(&db).await?;
        let my_pubkey = local.signing_key.verifying_key().to_bytes();
        let my_y7_id = local.y7_id;
        let secret = local.signing_key.to_bytes();

        let keypair = libp2p_keypair_from_y7_secret(&secret)?;
        let swarm = build_swarm(keypair)?;
        let bootstraps = crate::config::load_bootstraps(&db).await;
        // Start the swarm in the user's persisted mode so a LanOnly user
        // doesn't briefly leak a bootstrap dial before the first
        // apply_dial_mode would land.
        let initial_mode = match db.settings().get().await {
            Ok(s) => s.dial_mode,
            Err(e) => {
                tracing::warn!(error = %e, "settings.get failed at boot; using default mode");
                y7ke_core::settings::DialMode::default()
            }
        };
        let net = spawn_swarm_with_bootstraps(swarm, bootstraps, initial_mode);
        let event_rx_for_loop = net.try_clone_event_rx();

        let inner = Arc::new(AppInner {
            db,
            net,
            me: local.signing_key,
            my_pubkey,
            my_y7_id,
            presence: tokio::sync::RwLock::new(HashMap::new()),
            connections: tokio::sync::RwLock::new(HashMap::new()),
            rate_limiter: RateLimiter::default_limits(),
            bootstrap_pings: tokio::sync::RwLock::new(HashMap::new()),
            nat_status: tokio::sync::RwLock::new(NatStatusState::default()),
            dcutr_attempts: std::sync::atomic::AtomicU64::new(0),
            dcutr_successes: std::sync::atomic::AtomicU64::new(0),
            dcutr_failures: std::sync::atomic::AtomicU64::new(0),
            dcutr_recent_failures: tokio::sync::Mutex::new(std::collections::VecDeque::new()),
            rl_drops_handshake: std::sync::atomic::AtomicU64::new(0),
            rl_drops_msg: std::sync::atomic::AtomicU64::new(0),
            rl_drops_sync: std::sync::atomic::AtomicU64::new(0),
            nat_probe_detail: tokio::sync::RwLock::new(None),
            wake_notify: tokio::sync::Notify::new(),
            upgrade_backoff: tokio::sync::RwLock::new(HashMap::new()),
            reconnect_backoff: tokio::sync::RwLock::new(HashMap::new()),
            kad_lookups: tokio::sync::Semaphore::new(KAD_LOOKUP_CONCURRENCY),
            syncing: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            connection_meta: tokio::sync::RwLock::new(HashMap::new()),
        });

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let loop_inner = Arc::clone(&inner);
        let loop_event_tx = event_tx.clone();
        tokio::spawn(async move {
            event_loop::run(loop_inner, loop_event_tx, event_rx_for_loop).await;
        });

        // Periodic presence liveness check. Holds a Weak<AppInner> so
        // the task exits cleanly when the last AppHandle drops.
        let presence_inner = Arc::downgrade(&inner);
        let presence_event_tx = event_tx.clone();
        tokio::spawn(async move {
            run_presence_ticker(presence_inner, presence_event_tx).await;
        });

        // Proactive reconnect on launch: dial every Accepted contact through
        // the full Kad/relay-circuit discovery chain so queued messages drain
        // and presence reflects reality WITHOUT the user having to manually
        // send first. Without this, boot only connects to the bootstrap and
        // reservation; contacts weren't re-dialed until a manual send or the
        // 30s ticker. Bounded internally by the kad_lookups semaphore + the
        // per-peer backoff; LanOnly short-circuits inside dial_with_discovery.
        let boot_handle = Self {
            inner: Arc::clone(&inner),
            event_tx: event_tx.clone(),
        };
        tokio::spawn(async move {
            let contacts = match boot_handle.inner.db.contacts().list().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(error = %e, "boot reconnect: contacts.list failed");
                    return;
                }
            };
            for c in contacts
                .into_iter()
                .filter(|c| c.status == y7ke_core::ContactStatus::Accepted)
            {
                // Small jitter so N contacts don't fan out on the same instant.
                let j = std::time::Duration::from_millis(rand::random::<u64>() % 750);
                tokio::time::sleep(j).await;
                boot_handle.dial_with_discovery(c.y7_id).await;
            }
        });

        // Best-effort; no subscribers yet at boot.
        let _ = event_tx.send(AppEvent::IdentityReady {
            y7_id: my_y7_id.to_uri(),
        });

        tracing::info!(
            y7_id = %my_y7_id,
            boot_ms = started.elapsed().as_millis() as u64,
            rss_kb = process_rss_kb(),
            "y7ke-app booted",
        );

        Ok(Self { inner, event_tx })
    }

    pub fn my_y7_id(&self) -> &Y7Id {
        &self.inner.my_y7_id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.event_tx.subscribe()
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner.net.shutdown().await
    }
}

/// Order ConnectionKind values best-first. Used by event_loop to pick
/// which kind to publish when a peer has multiple active connections
/// (e.g. LAN + Relayed). Higher precedence wins.
pub(crate) fn connection_kind_precedence(k: ConnectionKind) -> u8 {
    match k {
        ConnectionKind::Direct => 5,
        ConnectionKind::Lan => 4,
        ConnectionKind::Internet => 3,
        ConnectionKind::Relayed => 2,
        ConnectionKind::Connecting => 1,
        ConnectionKind::Offline => 0,
    }
}

/// Best (kind, meta) across a peer's live connections, or
/// (Offline, default) if it has none. Presence shows the highest-ranked
/// kind; the pane shows that winning connection's transport/relay-host.
pub(crate) fn best_conn(
    conns: &HashMap<ConnectionId, ConnEntry>,
) -> (ConnectionKind, ConnectionMeta) {
    conns
        .values()
        .max_by_key(|e| connection_kind_precedence(e.kind))
        .map(|e| (e.kind, e.meta.clone()))
        .unwrap_or((ConnectionKind::Offline, ConnectionMeta::default()))
}

/// Recompute the `presence` + `connection_meta` derived caches for `y7`
/// from the authoritative `connections` map and return the new best kind
/// plus its transport. Single place the two caches are written, so they
/// can't desync. Offline *removes* the entries rather than storing an
/// `Offline` row — otherwise a `ConnectionClosed` arriving after a contact
/// was deleted would resurrect a ghost presence entry (absence already
/// reads as Offline via `unwrap_or` at the call sites).
pub(crate) async fn refresh_presence(
    inner: &AppInner,
    y7: Y7Id,
) -> (ConnectionKind, Option<y7ke_core::Transport>) {
    let (best, meta) = {
        let conns = inner.connections.read().await;
        conns
            .get(&y7)
            .map(best_conn)
            .unwrap_or((ConnectionKind::Offline, ConnectionMeta::default()))
    };
    let transport = meta.transport;
    if matches!(best, ConnectionKind::Offline) {
        inner.presence.write().await.remove(&y7);
        inner.connection_meta.write().await.remove(&y7);
    } else {
        inner.presence.write().await.insert(y7, best);
        inner.connection_meta.write().await.insert(y7, meta);
    }
    (best, transport)
}

const PRESENCE_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// Max simultaneous Kad `find_peer` lookups (reconnect-storm bound).
const KAD_LOOKUP_CONCURRENCY: usize = 4;

/// Relay→direct upgrade schedule in presence-tick units: an early burst
/// at ticks 0/5/10 (observed addrs freshest just after the relay path
/// forms), then one attempt every 20 ticks (~10 min) indefinitely.
/// Periodic-not-permanent keeps "relay is temporary" alive without the
/// every-tick storm. Pure so the cadence is unit-testable.
fn should_attempt_upgrade(attempts: u32) -> bool {
    matches!(attempts, 0 | 5 | 10) || (attempts > 10 && attempts % 20 == 0)
}

/// Background task: every 30 s OR on a `wake_notify` signal, walk
/// Accepted contacts. Three jobs per iteration:
///
/// 1. **Live-ness check** for currently-online peers; demote to
///    Offline if `swarm.is_connected` says the socket is gone.
/// 2. **Soft redial** Offline peers via `net.dial` (no Kad lookup —
///    that's expensive and happens on user demand).
/// 3. **Upgrade-from-relay** for any peer currently on a Relayed
///    connection: re-issue `net.dial` to give libp2p a chance to
///    pick a fresher direct address it may have learned via identify
///    push since the relay path opened. If the direct dial succeeds
///    DCUtR's automatic trigger fires; if libp2p only has the relay
///    address, the existing relay connection is reused (idempotent).
///    Exponential backoff per peer (1×, 5×, 10× the tick interval)
///    so we don't hammer permanent-relay pairs. Backoff resets on
///    AutoNAT verdict flip (which fires `wake_notify`).
///
/// The 30-s interval is short enough that "I just unlocked my phone"
/// → discovery rerun feels near-instant; the wake_notify path makes
/// settings changes / AutoNAT verdict flips effectively immediate
/// (~1 s).
async fn run_presence_ticker(
    inner: std::sync::Weak<AppInner>,
    event_tx: broadcast::Sender<AppEvent>,
) {
    let mut tick = tokio::time::interval(PRESENCE_TICK_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the immediate first tick — boot already published presence
    // via NetEvent::ConnectionEstablished.
    tick.tick().await;

    loop {
        // Whichever fires first wins; the body is the same.
        {
            let Some(inner) = inner.upgrade() else {
                tracing::debug!("presence ticker: AppInner dropped; exiting");
                return;
            };
            tokio::select! {
                _ = tick.tick() => {}
                _ = inner.wake_notify.notified() => {
                    tracing::debug!("presence ticker: woken by notify (settings/NAT change)");
                }
            }
        }
        let Some(inner) = inner.upgrade() else {
            tracing::debug!("presence ticker: AppInner dropped; exiting");
            return;
        };

        let contacts = match inner.db.contacts().list().await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "presence ticker: contacts.list failed");
                continue;
            }
        };
        for c in contacts {
            if c.status != y7ke_core::ContactStatus::Accepted {
                continue;
            }
            let current_presence = inner
                .presence
                .read()
                .await
                .get(&c.y7_id)
                .copied()
                .unwrap_or(ConnectionKind::Offline);

            match current_presence {
                ConnectionKind::Offline => {
                    // Full discovery chain (swarm book → cached addrs → Kad →
                    // relay-circuit fallback) — NOT the old address-book-only
                    // soft dial, which no-ops after a restart (ports changed,
                    // book empty) and so never re-found a moved peer. Gated by
                    // a per-peer exponential backoff so a permanently-gone
                    // contact decays from once-per-tick to once-per-10min, and
                    // the chain itself is bounded by the kad_lookups semaphore.
                    let now = std::time::Instant::now();
                    // 0–500ms jitter desyncs peers that all went offline
                    // together (suspend/resume) so they don't re-dial on
                    // the same tick.
                    let jitter = std::time::Duration::from_millis(rand::random::<u64>() % 500);
                    let should_dial = {
                        let mut bo = inner.reconnect_backoff.write().await;
                        let entry = bo
                            .entry(c.y7_id)
                            .or_insert_with(|| crate::reconnect::Backoff::ready(now));
                        if entry.due(now) {
                            entry.record(now, jitter);
                            true
                        } else {
                            false
                        }
                    };
                    if should_dial {
                        let handle = AppHandle {
                            inner: Arc::clone(&inner),
                            event_tx: event_tx.clone(),
                        };
                        tracing::debug!(y7_id = %c.y7_id, "presence ticker: discovery redial");
                        handle.dial_with_discovery(c.y7_id).await;
                    }
                }
                ConnectionKind::Relayed => {
                    // Demote-if-dead FIRST. Previously the relayed arm never
                    // checked liveness, so a relayed peer whose socket died
                    // stayed falsely ONLINE — and, being non-Offline, it also
                    // skipped the Offline redial path, stranding it. Mirror
                    // the `_` arm: snapshot conn ids, and on a dead socket
                    // remove them + recompute presence so the GUI flips
                    // promptly and the next tick can re-discover.
                    let snapshot: Vec<ConnectionId> = inner
                        .connections
                        .read()
                        .await
                        .get(&c.y7_id)
                        .map(|m| m.keys().copied().collect())
                        .unwrap_or_default();
                    match inner.net.check_live(c.y7_id).await {
                        Ok(false) => {
                            {
                                let mut conns = inner.connections.write().await;
                                if let Some(by_id) = conns.get_mut(&c.y7_id) {
                                    for id in &snapshot {
                                        by_id.remove(id);
                                    }
                                    if by_id.is_empty() {
                                        conns.remove(&c.y7_id);
                                    }
                                }
                            }
                            let (best, transport) =
                                crate::app::refresh_presence(&inner, c.y7_id).await;
                            tracing::info!(
                                y7_id = %c.y7_id,
                                "presence ticker: relayed socket gone → offline"
                            );
                            let _ = event_tx.send(AppEvent::PresenceChanged {
                                y7_id: c.y7_id.to_uri(),
                                connection: best,
                                transport,
                            });
                        }
                        Ok(true) => {
                            // Alive — V2-A5 "relay is temporary": attempt a
                            // relay→direct upgrade in an early burst (ticks 0,
                            // 5, 10, freshest observed addrs) then once every
                            // ~20 ticks. Counter resets on AutoNAT flip /
                            // ConnectionClosed.
                            let attempts = {
                                let mut bo = inner.upgrade_backoff.write().await;
                                let n = bo.entry(c.y7_id).or_insert(0);
                                let old = *n;
                                *n = n.saturating_add(1);
                                old
                            };
                            if should_attempt_upgrade(attempts) {
                                if let Ok(true) = inner.net.dial(c.y7_id).await {
                                    tracing::debug!(
                                        y7_id = %c.y7_id,
                                        attempts,
                                        "presence ticker: relay→direct upgrade dial issued"
                                    );
                                }
                            }
                            // Self-heal: re-drive any due queued message over
                            // the live (relayed) path so a send that stranded
                            // mid-flap reaches a terminal status without
                            // waiting for a full reconnect. Schedule-respecting
                            // so backoff still decays.
                            drain_due_for_connected_peer(&inner, &event_tx, c.y7_id).await;
                        }
                        Err(e) => {
                            tracing::debug!(y7_id = %c.y7_id, error = %e, "relayed check_live failed");
                        }
                    }
                }
                _ => {
                    // Snapshot the connection ids we know for this peer
                    // BEFORE the awaiting liveness check. If check_live
                    // says dead we remove only these — a fresh connection
                    // the event loop inserts concurrently (a reconnect
                    // landing mid-tick) is newer than our snapshot and must
                    // survive, else a live peer is stranded Offline forever
                    // (the Offline arm only soft-redials, which no-ops when
                    // already connected, so it never self-heals).
                    let snapshot: Vec<ConnectionId> = inner
                        .connections
                        .read()
                        .await
                        .get(&c.y7_id)
                        .map(|m| m.keys().copied().collect())
                        .unwrap_or_default();
                    match inner.net.check_live(c.y7_id).await {
                        Ok(true) => {
                            // Still connected — re-drive any due queued message
                            // so a send stranded by a transient routing flap
                            // self-heals to a terminal status. Schedule-
                            // respecting so backoff decays for a wedged peer.
                            drain_due_for_connected_peer(&inner, &event_tx, c.y7_id).await;
                        }
                        Ok(false) => {
                            {
                                let mut conns = inner.connections.write().await;
                                if let Some(by_id) = conns.get_mut(&c.y7_id) {
                                    for id in &snapshot {
                                        by_id.remove(id);
                                    }
                                    if by_id.is_empty() {
                                        conns.remove(&c.y7_id);
                                    }
                                }
                            }
                            let (best, transport) =
                                crate::app::refresh_presence(&inner, c.y7_id).await;
                            if matches!(best, ConnectionKind::Offline) {
                                tracing::info!(
                                    y7_id = %c.y7_id,
                                    prev = ?current_presence,
                                    "presence ticker: socket gone → Offline"
                                );
                            }
                            let _ = event_tx.send(AppEvent::PresenceChanged {
                                y7_id: c.y7_id.to_uri(),
                                connection: best,
                                transport,
                            });
                        }
                        Err(e) => {
                            tracing::debug!(
                                y7_id = %c.y7_id,
                                error = %e,
                                "presence ticker: check_live failed"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Periodic-ticker helper: re-drive due queued messages to an already-
/// connected peer. Schedule-respecting (only rows whose `next_retry_at` has
/// elapsed) so a wedged message self-heals without out-running its backoff.
/// Errors are logged, never fatal — the next tick retries.
async fn drain_due_for_connected_peer(
    inner: &Arc<AppInner>,
    event_tx: &broadcast::Sender<AppEvent>,
    y7: Y7Id,
) {
    let peer_id = match y7ke_net::peer_id_from_y7(&y7) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(y7_id = %y7, error = %e, "drain: peer_id_from_y7 failed");
            return;
        }
    };
    if let Err(e) =
        crate::event_loop::drain_queue_for_peer(inner, event_tx, &y7, peer_id, true).await
    {
        tracing::debug!(y7_id = %y7, error = %e, "drain: queue drain failed");
    }
}

/// Linux-only best-effort RSS reading via /proc/self/status. Boot telemetry only.
fn process_rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                return rest.split_whitespace().next()?.parse().ok();
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ConnectionKind) -> ConnEntry {
        ConnEntry {
            kind,
            meta: ConnectionMeta::default(),
        }
    }

    #[test]
    fn best_conn_empty_is_offline() {
        let conns: HashMap<ConnectionId, ConnEntry> = HashMap::new();
        assert_eq!(best_conn(&conns).0, ConnectionKind::Offline);
    }

    #[test]
    fn best_conn_prefers_direct_over_relayed() {
        let mut conns = HashMap::new();
        conns.insert(
            ConnectionId::new_unchecked(1),
            entry(ConnectionKind::Relayed),
        );
        conns.insert(
            ConnectionId::new_unchecked(2),
            entry(ConnectionKind::Direct),
        );
        assert_eq!(best_conn(&conns).0, ConnectionKind::Direct);
    }

    #[test]
    fn upgrade_schedule_is_burst_then_periodic_not_every_tick() {
        // Early burst.
        assert!(should_attempt_upgrade(0));
        assert!(should_attempt_upgrade(5));
        assert!(should_attempt_upgrade(10));
        // Quiet between the burst points.
        assert!(!should_attempt_upgrade(1));
        assert!(!should_attempt_upgrade(11));
        assert!(!should_attempt_upgrade(15));
        // Periodic every-20 from the 10-min mark — NOT every tick (the
        // old `>= 20` bug fired on 20,21,22,…).
        assert!(should_attempt_upgrade(20));
        assert!(!should_attempt_upgrade(21));
        assert!(!should_attempt_upgrade(39));
        assert!(should_attempt_upgrade(40));
    }

    #[test]
    fn relay_survives_when_better_path_closes() {
        // The core regression: a peer holds two connections; closing the
        // higher-ranked one must leave the survivor, not blank to Offline.
        let mut conns = HashMap::new();
        let relay = ConnectionId::new_unchecked(1);
        let lan = ConnectionId::new_unchecked(2);
        conns.insert(relay, entry(ConnectionKind::Relayed));
        conns.insert(lan, entry(ConnectionKind::Lan));
        assert_eq!(best_conn(&conns).0, ConnectionKind::Lan);

        // LAN connection closes → relay still live, peer stays online.
        conns.remove(&lan);
        assert_eq!(best_conn(&conns).0, ConnectionKind::Relayed);

        // Relay also closes → now genuinely Offline.
        conns.remove(&relay);
        assert_eq!(best_conn(&conns).0, ConnectionKind::Offline);
    }
}
