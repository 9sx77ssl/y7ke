//! Runtime settings: get / update / bootstrap-list / ping.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use libp2p::Multiaddr;
use tokio::net::TcpStream;
use tokio::time::timeout;
use y7ke_core::error::Result;
use y7ke_core::settings::{BootstrapEntry, Settings, DEFAULT_RELAY_BOOTSTRAP};
use y7ke_core::AppEvent;

use super::{AppHandle, BootstrapPingState};

const PING_TIMEOUT: Duration = Duration::from_secs(5);

impl AppHandle {
    pub async fn get_settings(&self) -> Result<Settings> {
        self.inner.db.settings().get().await
    }

    /// Persist `settings`, push the new bootstrap list to the swarm, and
    /// emit `AppEvent::SettingsChanged`.
    pub async fn update_settings(&self, settings: Settings) -> Result<()> {
        self.inner.db.settings().update(&settings).await?;

        // Reuse the same loader so env-var / file precedence is preserved.
        let addrs = crate::config::load_bootstraps(&self.inner.db).await;
        if let Err(e) = self.inner.net.update_bootstraps(addrs).await {
            tracing::warn!(error = %e, "swarm rejected update_bootstraps");
        }

        let _ = self.event_tx.send(AppEvent::SettingsChanged);
        Ok(())
    }

    /// Built-in default first, then user extras. Ping state is filled in
    /// from the in-memory cache (empty until `ping_all_bootstraps` runs).
    pub async fn list_bootstraps(&self) -> Result<Vec<BootstrapEntry>> {
        let settings = self.inner.db.settings().get().await?;
        let pings = self.inner.bootstrap_pings.read().await;
        let mut out = Vec::with_capacity(1 + settings.extra_bootstraps.len());
        out.push(make_entry(DEFAULT_RELAY_BOOTSTRAP, true, &pings));
        for s in &settings.extra_bootstraps {
            if s == DEFAULT_RELAY_BOOTSTRAP {
                continue; // never duplicate the immutable default
            }
            out.push(make_entry(s, false, &pings));
        }
        Ok(out)
    }

    /// TCP-connect to each bootstrap in parallel with a 5 s timeout and
    /// record the result in the in-memory cache. Returns the freshly
    /// updated bootstrap list.
    pub async fn ping_all_bootstraps(&self) -> Result<Vec<BootstrapEntry>> {
        let settings = self.inner.db.settings().get().await?;
        let mut addrs: Vec<String> = Vec::new();
        addrs.push(DEFAULT_RELAY_BOOTSTRAP.to_string());
        for s in &settings.extra_bootstraps {
            if s != DEFAULT_RELAY_BOOTSTRAP {
                addrs.push(s.clone());
            }
        }

        let results = futures_join_all(addrs.clone().into_iter().map(|addr| async move {
            let state = ping_one(&addr).await;
            (addr, state)
        }))
        .await;

        let mut cache = self.inner.bootstrap_pings.write().await;
        for (addr, state) in &results {
            cache.insert(addr.clone(), *state);
        }
        drop(cache);

        let pings = self.inner.bootstrap_pings.read().await;
        let mut out = Vec::with_capacity(addrs.len());
        for (i, addr) in addrs.iter().enumerate() {
            out.push(make_entry(addr, i == 0, &pings));
        }
        Ok(out)
    }

    /// Lowest-RTT non-failed bootstrap; default on full failure.
    pub async fn select_best_bootstrap(&self) -> Option<String> {
        let entries = match self.ping_all_bootstraps().await {
            Ok(v) => v,
            Err(_) => return Some(DEFAULT_RELAY_BOOTSTRAP.to_string()),
        };
        let mut best: Option<(&BootstrapEntry, u64)> = None;
        for e in &entries {
            if e.last_ping_failed {
                continue;
            }
            if let Some(rtt) = e.last_ping_ms {
                match best {
                    Some((_, cur)) if cur <= rtt => {}
                    _ => best = Some((e, rtt)),
                }
            }
        }
        match best {
            Some((e, _)) => Some(e.multiaddr.clone()),
            None => Some(DEFAULT_RELAY_BOOTSTRAP.to_string()),
        }
    }
}

fn make_entry(
    multiaddr: &str,
    is_default: bool,
    pings: &HashMap<String, BootstrapPingState>,
) -> BootstrapEntry {
    let state = pings.get(multiaddr).copied().unwrap_or_default();
    BootstrapEntry {
        multiaddr: multiaddr.to_string(),
        is_default,
        last_ping_ms: state.last_ping_ms,
        last_ping_failed: state.last_ping_failed,
    }
}

/// Resolve `/dns4|/dns6|/ip4|/ip6` + `/tcp/<port>` from a multiaddr and
/// open a TCP connection, timing the round-trip. Anything we can't
/// resolve to `host:port` counts as a failure.
async fn ping_one(addr_str: &str) -> BootstrapPingState {
    let addr = match addr_str.parse::<Multiaddr>() {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!(addr = %addr_str, error = %e, "ping: multiaddr parse failed");
            return BootstrapPingState {
                last_ping_ms: None,
                last_ping_failed: true,
            };
        }
    };
    let Some((host, port)) = host_port_from_multiaddr(&addr) else {
        tracing::debug!(addr = %addr_str, "ping: no /tcp + host component");
        return BootstrapPingState {
            last_ping_ms: None,
            last_ping_failed: true,
        };
    };

    let started = Instant::now();
    let target = format!("{host}:{port}");
    match timeout(PING_TIMEOUT, TcpStream::connect(&target)).await {
        Ok(Ok(_stream)) => {
            let rtt = started.elapsed().as_millis() as u64;
            BootstrapPingState {
                last_ping_ms: Some(rtt),
                last_ping_failed: false,
            }
        }
        Ok(Err(e)) => {
            tracing::debug!(target = %target, error = %e, "ping: connect failed");
            BootstrapPingState {
                last_ping_ms: None,
                last_ping_failed: true,
            }
        }
        Err(_) => {
            tracing::debug!(target = %target, "ping: connect timed out");
            BootstrapPingState {
                last_ping_ms: None,
                last_ping_failed: true,
            }
        }
    }
}

fn host_port_from_multiaddr(addr: &Multiaddr) -> Option<(String, u16)> {
    use libp2p::multiaddr::Protocol;
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ip) => host = Some(ip.to_string()),
            Protocol::Ip6(ip) => host = Some(format!("[{ip}]")),
            Protocol::Dns(name) | Protocol::Dns4(name) | Protocol::Dns6(name) => {
                host = Some(name.to_string())
            }
            Protocol::Tcp(p) => port = Some(p),
            _ => {}
        }
    }
    match (host, port) {
        (Some(h), Some(p)) => Some((h, p)),
        _ => None,
    }
}

/// Tiny join-all helper so we don't need to pull `futures` for this one
/// call. Polls each future serially in spawn order — fine because each
/// ping is bounded by PING_TIMEOUT and they run concurrently via
/// `tokio::spawn`.
async fn futures_join_all<F, T>(iter: impl IntoIterator<Item = F>) -> Vec<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = iter.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(v) => out.push(v),
            Err(e) => {
                tracing::warn!(error = %e, "ping task panicked");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{AppConfig, AppHandle};
    use tempfile::TempDir;
    use y7ke_core::settings::{DialModes, Settings, DEFAULT_RELAY_BOOTSTRAP};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_get_list_round_trip() {
        let dir = TempDir::new().unwrap();
        let app = AppHandle::boot(AppConfig::in_dir(dir.path()))
            .await
            .unwrap();

        let initial = app.get_settings().await.unwrap();
        assert_eq!(initial.dial_modes, DialModes::default());
        assert!(initial.extra_bootstraps.is_empty());

        let extra = "/ip4/127.0.0.1/tcp/9999/p2p/12D3KooWAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAa";
        let updated = Settings {
            dial_modes: DialModes {
                lan: false,
                internet: true,
                relay: true,
                p2p: false,
            },
            extra_bootstraps: vec![extra.into()],
        };
        app.update_settings(updated.clone()).await.unwrap();

        let got = app.get_settings().await.unwrap();
        assert_eq!(got.dial_modes, updated.dial_modes);
        assert_eq!(got.extra_bootstraps, updated.extra_bootstraps);

        let list = app.list_bootstraps().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].multiaddr, DEFAULT_RELAY_BOOTSTRAP);
        assert!(list[0].is_default);
        assert_eq!(list[1].multiaddr, extra);
        assert!(!list[1].is_default);
        // ping cache is empty until ping_all_bootstraps runs.
        assert!(list.iter().all(|e| e.last_ping_ms.is_none()));
        assert!(list.iter().all(|e| !e.last_ping_failed));

        app.shutdown().await.ok();
    }
}
