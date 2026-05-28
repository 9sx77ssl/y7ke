//! Runtime user settings: dial mode + user-added bootstrap multiaddrs.
//!
//! Persisted in `settings` (single-row table). Default values must match
//! the seed JSON after migrations 0004 + 0005 run, so fresh installs and
//! an `.update(Settings::default())` round-trip both land at the same
//! `payload_json` semantically (after JSON decode).

use serde::{Deserialize, Serialize};

/// Hardcoded fallback bootstrap multiaddr. Always present in
/// `list_bootstraps` output as the immutable entry — UI renders it
/// read-only.
pub const DEFAULT_RELAY_BOOTSTRAP: &str =
    "/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo";

/// Dial strategy. Two modes: LAN-only, or the full "Y7net" path. The
/// former `P2p` variant was a behavioural duplicate of `Internet` (same
/// dial chain; DCUtR runs automatically regardless of mode), so it was
/// removed. Legacy `"P2p"` settings rows are rewritten to `Internet` by
/// migration 0006.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub enum DialMode {
    /// LAN-only. mDNS + cached LAN-only addrs. No Kad provider record,
    /// no bootstrap dial, no `/p2p-circuit` listen.
    LanOnly,
    /// Full mode ("Y7net" in the UI): Kad lookup + direct dial (with
    /// automatic DCUtR hole-punch upgrade) and circuit-relay-v2 fallback.
    #[default]
    Internet,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct Settings {
    pub dial_mode: DialMode,
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
