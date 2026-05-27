//! V2-A1 client configuration — currently just the bootstrap-node list.
//!
//! Resolution order (first non-empty wins):
//!
//! 1. `Y7KE_BOOTSTRAP` env var (comma-separated multiaddrs).
//! 2. `~/.config/y7ke/bootstrap.toml` — `peers = ["…", "…"]`.
//! 3. `y7ke_net::DEFAULT_BOOTSTRAPS` — hardcoded at build time.
//!
//! An empty result (no env, no file, no defaults) is a perfectly valid
//! configuration — the client just stays LAN-only via mDNS.

use std::path::PathBuf;

use libp2p::Multiaddr;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
struct BootstrapFile {
    #[serde(default)]
    peers: Vec<String>,
}

/// Resolve the effective bootstrap multiaddr list at boot time.
///
/// Any malformed entry is logged and skipped — a single typo in the
/// config file must not prevent the app from starting in LAN mode.
pub fn load_bootstraps() -> Vec<Multiaddr> {
    // 1. Env var.
    if let Ok(env) = std::env::var("Y7KE_BOOTSTRAP") {
        let parsed: Vec<Multiaddr> = env
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|s| match s.parse::<Multiaddr>() {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(addr = %s, error = %e, "Y7KE_BOOTSTRAP entry rejected");
                    None
                }
            })
            .collect();
        if !parsed.is_empty() {
            tracing::info!(
                count = parsed.len(),
                "bootstraps loaded from Y7KE_BOOTSTRAP"
            );
            return parsed;
        }
    }

    // 2. ~/.config/y7ke/bootstrap.toml
    if let Some(path) = bootstrap_toml_path() {
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match toml::from_str::<BootstrapFile>(&text) {
                    Ok(file) => {
                        let parsed: Vec<Multiaddr> = file
                            .peers
                            .into_iter()
                            .filter_map(|s| match s.parse::<Multiaddr>() {
                                Ok(m) => Some(m),
                                Err(e) => {
                                    tracing::warn!(addr = %s, error = %e, "bootstrap.toml entry rejected");
                                    None
                                }
                            })
                            .collect();
                        if !parsed.is_empty() {
                            tracing::info!(
                                count = parsed.len(),
                                path = %path.display(),
                                "bootstraps loaded from config file",
                            );
                            return parsed;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "bootstrap.toml parse failed")
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "bootstrap.toml read failed")
                }
            }
        }
    }

    // 3. Compile-time defaults.
    let defaults: Vec<Multiaddr> = y7ke_net::DEFAULT_BOOTSTRAPS
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    if !defaults.is_empty() {
        tracing::info!(
            count = defaults.len(),
            "bootstraps loaded from compile-time defaults"
        );
    } else {
        tracing::info!("no bootstraps configured — LAN-only discovery (mDNS)");
    }
    defaults
}

fn bootstrap_toml_path() -> Option<PathBuf> {
    let proj = directories::ProjectDirs::from("com", "y7ke", "Y7KE")?;
    Some(proj.config_dir().join("bootstrap.toml"))
}
