//! Composition root: AppHandle owns storage + swarm + identity + event bus.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

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
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

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
    /// Presence cache populated by event_loop. Read by list_contacts to
    /// survive the boot race where PresenceChanged fires before UI listener.
    /// Reflects the *best* kind currently active for each peer (max of
    /// `connection_kinds`).
    pub presence: tokio::sync::RwLock<HashMap<Y7Id, ConnectionKind>>,
    /// Active connection kinds per peer. When a peer holds two connections
    /// (e.g. LAN + Relayed) we keep both; UI presence reflects the best
    /// one. Updated alongside `presence` on every Established/Closed.
    pub connection_kinds: tokio::sync::RwLock<HashMap<Y7Id, HashSet<ConnectionKind>>>,
    pub rate_limiter: RateLimiter,
    /// Last-known ping state per bootstrap multiaddr (keyed by the full
    /// multiaddr string). Empty at boot — populated by `ping_all_bootstraps`.
    pub bootstrap_pings: tokio::sync::RwLock<HashMap<String, BootstrapPingState>>,
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
        let net = spawn_swarm_with_bootstraps(swarm, bootstraps);
        let event_rx_for_loop = net.try_clone_event_rx();

        let inner = Arc::new(AppInner {
            db,
            net,
            me: local.signing_key,
            my_pubkey,
            my_y7_id,
            presence: tokio::sync::RwLock::new(HashMap::new()),
            connection_kinds: tokio::sync::RwLock::new(HashMap::new()),
            rate_limiter: RateLimiter::default_limits(),
            bootstrap_pings: tokio::sync::RwLock::new(HashMap::new()),
        });

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let loop_inner = Arc::clone(&inner);
        let loop_event_tx = event_tx.clone();
        tokio::spawn(async move {
            event_loop::run(loop_inner, loop_event_tx, event_rx_for_loop).await;
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

/// Best kind in `set`, or Offline if the set is empty.
pub(crate) fn best_kind(set: &HashSet<ConnectionKind>) -> ConnectionKind {
    set.iter()
        .copied()
        .max_by_key(|k| connection_kind_precedence(*k))
        .unwrap_or(ConnectionKind::Offline)
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
