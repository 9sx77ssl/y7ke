//! Runtime user settings: dial modes + user-added bootstrap multiaddrs.
//!
//! Persisted in `settings` (single-row table). Default values must match
//! the seed JSON in `migrations/0004_settings.sql` byte-for-byte so a
//! fresh install and an `.update(Settings::default())` round-trip both
//! land at the same `payload_json`.

use serde::{Deserialize, Serialize};

/// Hardcoded fallback bootstrap multiaddr. Always present in
/// `list_bootstraps` output as the immutable entry — UI renders it
/// read-only.
pub const DEFAULT_RELAY_BOOTSTRAP: &str =
    "/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct DialModes {
    /// mDNS LAN discovery (works only on same WiFi/LAN).
    pub lan: bool,
    /// Direct internet dial via Kad-resolved addresses.
    pub internet: bool,
    /// Circuit Relay v2 (V2-A4) — works through NAT/CGNAT.
    pub relay: bool,
    /// Direct P2P via DCUtR hole-punching (V2-A5, not yet implemented).
    /// Currently a no-op stub; toggling has no effect beyond UI.
    pub p2p: bool,
}

impl Default for DialModes {
    fn default() -> Self {
        Self {
            lan: true,
            internet: true,
            relay: true,
            p2p: false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct Settings {
    pub dial_modes: DialModes,
    /// User-added bootstrap multiaddrs (NOT including the hardcoded
    /// default — that one is returned separately by list_bootstraps).
    pub extra_bootstraps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct BootstrapEntry {
    /// Full multiaddr including /p2p/<peer-id>.
    pub multiaddr: String,
    /// True for the built-in y7v.lol bootstrap that ships hardcoded.
    /// The UI must render it as read-only (cannot delete, cannot edit).
    pub is_default: bool,
    /// Last measured RTT in milliseconds, if `ping_all_bootstraps` has run.
    #[ts(type = "number | null")]
    pub last_ping_ms: Option<u64>,
    /// True if the last ping attempt could not complete (timeout / dial error).
    pub last_ping_failed: bool,
}
