//! Composition root: AppHandle owns storage + swarm + identity + event bus.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::broadcast;
use y7ke_core::crypto::SigningKey;
use y7ke_core::error::Result;
use y7ke_core::{AppEvent, ConnectionKind, Y7Id};
use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm, NetHandle};
use y7ke_storage::{Db, DbConfig};

use crate::rate_limit::RateLimiter;
use crate::{event_loop, identity};

mod contacts;
mod messages;

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
    pub presence: tokio::sync::RwLock<std::collections::HashMap<Y7Id, ConnectionKind>>,
    pub rate_limiter: RateLimiter,
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
        let net = spawn_swarm(swarm);
        let event_rx_for_loop = net.try_clone_event_rx();

        let inner = Arc::new(AppInner {
            db,
            net,
            me: local.signing_key,
            my_pubkey,
            my_y7_id,
            presence: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            rate_limiter: RateLimiter::default_limits(),
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
