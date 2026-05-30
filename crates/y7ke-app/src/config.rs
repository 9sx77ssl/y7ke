//! V2-A1 client configuration — bootstrap-node list resolution.
//!
//! Bootstraps come from exactly TWO places (the `Y7KE_BOOTSTRAP` env var and
//! the `bootstrap.toml` config file were removed — extra bootstraps are added
//! ONLY via the in-app Settings :3 page):
//!
//! 1. DB-stored `settings.extra_bootstraps` (edited in Settings).
//! 2. `y7ke_net::DEFAULT_BOOTSTRAPS` — one hardcoded node, baked at build time.
//!
//! Regardless, [`DEFAULT_RELAY_BOOTSTRAP`] is always prepended (deduped) so the
//! built-in relay is never lost — the UI shows it readonly and can't delete it.

use libp2p::Multiaddr;
use y7ke_core::settings::DEFAULT_RELAY_BOOTSTRAP;
use y7ke_storage::Db;

/// Resolve the effective bootstrap multiaddr list at boot time: the user's
/// Settings list if any, else the hardcoded default — then always prepend
/// `DEFAULT_RELAY_BOOTSTRAP` (deduped). Malformed entries are logged and
/// skipped so a single typo cannot block boot.
pub async fn load_bootstraps(db: &Db) -> Vec<Multiaddr> {
    let mut result = load_user_sources(db).await;

    // Prepend the hardcoded default (expanded to its TCP+QUIC pair), deduped.
    // .rev() so the pair keeps its tcp-then-quic order after the inserts.
    let defaults = parse_multiaddrs(std::iter::once(DEFAULT_RELAY_BOOTSTRAP), "default-relay");
    if defaults.is_empty() {
        tracing::error!("DEFAULT_RELAY_BOOTSTRAP expanded to nothing — check the constant");
    }
    for d in defaults.into_iter().rev() {
        if !result.iter().any(|m| m == &d) {
            result.insert(0, d);
        }
    }
    result
}

/// Inner resolution without the hardcoded prepend: the user's Settings list
/// if non-empty, else the compile-time default. (Env var + config file were
/// removed — extra bootstraps are managed only via the Settings page.)
async fn load_user_sources(db: &Db) -> Vec<Multiaddr> {
    // 1. DB-stored settings.extra_bootstraps (the in-app Settings :3 page).
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

    // 2. Compile-time default (one hardcoded node; also a descriptor — expand).
    let defaults: Vec<Multiaddr> = parse_multiaddrs(
        y7ke_net::DEFAULT_BOOTSTRAPS.iter().copied(),
        "compile-default",
    );
    if defaults.is_empty() {
        tracing::info!("no user-source bootstraps — falling back to hardcoded default only");
    }
    defaults
}

/// Parse bootstrap descriptor strings into multiaddrs. Each descriptor is
/// first run through `expand_bootstrap`, so a transport-agnostic shorthand
/// (`/dns4/host/4101/p2p/<id>`) becomes BOTH a TCP and a QUIC multiaddr;
/// explicit multiaddrs pass through unchanged. Invalid entries are logged
/// and skipped. The result is deduped so the explicit and shorthand forms
/// of the same addr don't double-dial.
fn parse_multiaddrs<'a, I: Iterator<Item = &'a str>>(it: I, source: &str) -> Vec<Multiaddr> {
    let mut out: Vec<Multiaddr> = Vec::new();
    for entry in it.map(str::trim).filter(|s| !s.is_empty()) {
        for expanded in y7ke_core::expand_bootstrap(entry) {
            match expanded.parse::<Multiaddr>() {
                Ok(m) => {
                    if !out.contains(&m) {
                        out.push(m);
                    }
                }
                Err(e) => {
                    tracing::warn!(addr = %expanded, source = %source, error = %e, "bootstrap entry rejected");
                }
            }
        }
    }
    out
}
