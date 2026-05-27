//! V2-A1 client configuration — bootstrap-node list resolution.
//!
//! Resolution order (first non-empty wins for the user-controlled list):
//!
//! 1. `Y7KE_BOOTSTRAP` env var (comma-separated multiaddrs).
//! 2. DB-stored `settings.extra_bootstraps`.
//! 3. `~/.config/y7ke/bootstrap.toml` — `peers = ["…", "…"]`.
//! 4. `y7ke_net::DEFAULT_BOOTSTRAPS` — hardcoded at build time.
//!
//! Regardless of which source wins, [`DEFAULT_RELAY_BOOTSTRAP`] is always
//! prepended (deduped) so the built-in relay is never lost — the UI may
//! show it but cannot delete it.

use std::path::PathBuf;

use libp2p::Multiaddr;
use serde::Deserialize;
use y7ke_core::settings::DEFAULT_RELAY_BOOTSTRAP;
use y7ke_storage::Db;

#[derive(Debug, Deserialize, Default)]
struct BootstrapFile {
    #[serde(default)]
    peers: Vec<String>,
}

/// Resolve the effective bootstrap multiaddr list at boot time.
///
/// Reads from env / DB / file / compile-time defaults in that priority,
/// then always prepends `DEFAULT_RELAY_BOOTSTRAP` (deduped). Malformed
/// entries are logged and skipped so a single typo cannot block boot.
pub async fn load_bootstraps(db: &Db) -> Vec<Multiaddr> {
    let mut result = load_user_sources(db).await;

    // Prepend hardcoded default, deduped.
    if let Ok(default) = DEFAULT_RELAY_BOOTSTRAP.parse::<Multiaddr>() {
        if !result.iter().any(|m| m == &default) {
            result.insert(0, default);
        }
    } else {
        tracing::error!("DEFAULT_RELAY_BOOTSTRAP failed to parse — check the constant");
    }
    result
}

/// Inner resolution without the hardcoded prepend.
async fn load_user_sources(db: &Db) -> Vec<Multiaddr> {
    // 1. Env var.
    if let Ok(env) = std::env::var("Y7KE_BOOTSTRAP") {
        let parsed = parse_multiaddrs(env.split(','), "Y7KE_BOOTSTRAP");
        if !parsed.is_empty() {
            tracing::info!(
                count = parsed.len(),
                "bootstraps loaded from Y7KE_BOOTSTRAP"
            );
            return parsed;
        }
    }

    // 2. DB-stored settings.extra_bootstraps.
    match db.settings().get().await {
        Ok(s) if !s.extra_bootstraps.is_empty() => {
            let parsed =
                parse_multiaddrs(s.extra_bootstraps.iter().map(String::as_str), "settings");
            if !parsed.is_empty() {
                tracing::info!(
                    count = parsed.len(),
                    "bootstraps loaded from DB settings.extra_bootstraps",
                );
                return parsed;
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "settings.get failed during boot"),
    }

    // 3. ~/.config/y7ke/bootstrap.toml
    if let Some(path) = bootstrap_toml_path() {
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match toml::from_str::<BootstrapFile>(&text) {
                    Ok(file) => {
                        let parsed = parse_multiaddrs(
                            file.peers.iter().map(String::as_str),
                            "bootstrap.toml",
                        );
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

    // 4. Compile-time defaults.
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
        tracing::info!("no user-source bootstraps — falling back to hardcoded default only");
    }
    defaults
}

fn parse_multiaddrs<'a, I: Iterator<Item = &'a str>>(it: I, source: &str) -> Vec<Multiaddr> {
    it.map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.parse::<Multiaddr>() {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!(addr = %s, source = %source, error = %e, "bootstrap entry rejected");
                None
            }
        })
        .collect()
}

fn bootstrap_toml_path() -> Option<PathBuf> {
    let proj = directories::ProjectDirs::from("com", "y7ke", "Y7KE")?;
    Some(proj.config_dir().join("bootstrap.toml"))
}
