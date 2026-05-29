//! Runtime user settings: dial mode + user-added bootstrap multiaddrs.
//!
//! Persisted in `settings` (single-row table). Default values must match
//! the seed JSON after migrations 0004 + 0005 run, so fresh installs and
//! an `.update(Settings::default())` round-trip both land at the same
//! `payload_json` semantically (after JSON decode).

use serde::{Deserialize, Serialize};

/// Hardcoded fallback bootstrap descriptor. Always present in
/// `list_bootstraps` output as the immutable entry — UI renders it
/// read-only. This is the transport-AGNOSTIC shorthand
/// (`/dns4/host/<port>/p2p/<id>`, no /tcp or /udp): the client expands it
/// to BOTH a TCP and a QUIC multiaddr and races them (QUIC wins on
/// UDP-open networks, enabling direct hole-punch; TCP is the fallback).
pub const DEFAULT_RELAY_BOOTSTRAP: &str =
    "/dns4/bootstrap1.y7v.lol/4101/p2p/12D3KooWEVq9A1w4xk1paGxywwPNy4vz8D92wxE4XKBh8DpA8fSo";

/// Expand a bootstrap descriptor into the multiaddr STRINGS to dial.
///
/// Accepts our transport-agnostic shorthand
/// `/{dns4|dns6|ip4|ip6}/<host>/<port>/p2p/<peer-id>` (no transport
/// keyword) and expands it to BOTH `/.../tcp/<port>/p2p/<id>` and
/// `/.../udp/<port>/quic-v1/p2p/<id>`. An already-explicit multiaddr
/// (one that names `/tcp` or `/udp`) is returned unchanged, so existing
/// settings rows and the loopback tests keep working. Returned as
/// strings so this crate stays libp2p-free; the net/app layer parses
/// them with `.parse::<Multiaddr>()` (and logs+skips anything invalid).
pub fn expand_bootstrap(s: &str) -> Vec<String> {
    let s = s.trim();
    // Leading '/' yields an empty first element: ["", net, host, port, "p2p", id].
    let parts: Vec<&str> = s.split('/').collect();
    let is_shorthand = parts.len() == 6
        && matches!(parts[1], "dns4" | "dns6" | "ip4" | "ip6")
        && !parts[2].is_empty()
        && !parts[3].is_empty()
        && parts[3].chars().all(|c| c.is_ascii_digit())
        && parts[4] == "p2p"
        && !parts[5].is_empty();
    if is_shorthand {
        let (net, host, port, id) = (parts[1], parts[2], parts[3], parts[5]);
        return vec![
            format!("/{net}/{host}/tcp/{port}/p2p/{id}"),
            format!("/{net}/{host}/udp/{port}/quic-v1/p2p/{id}"),
        ];
    }
    // Explicit multiaddr (or anything else): pass through; downstream
    // `.parse::<Multiaddr>()` validates it.
    vec![s.to_string()]
}

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

#[cfg(test)]
mod tests {
    use super::expand_bootstrap;

    #[test]
    fn shorthand_expands_to_tcp_and_quic() {
        let v = expand_bootstrap("/dns4/bootstrap1.y7v.lol/4101/p2p/12D3KooWAaAa");
        assert_eq!(
            v,
            vec![
                "/dns4/bootstrap1.y7v.lol/tcp/4101/p2p/12D3KooWAaAa".to_string(),
                "/dns4/bootstrap1.y7v.lol/udp/4101/quic-v1/p2p/12D3KooWAaAa".to_string(),
            ]
        );
    }

    #[test]
    fn ip4_shorthand_expands() {
        let v = expand_bootstrap("/ip4/89.35.130.67/4101/p2p/12D3KooWAaAa");
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("/tcp/4101/"));
        assert!(v[1].contains("/udp/4101/quic-v1/"));
    }

    #[test]
    fn explicit_tcp_passes_through() {
        let v = expand_bootstrap("/dns4/h/tcp/4101/p2p/12D3KooWAaAa");
        assert_eq!(v, vec!["/dns4/h/tcp/4101/p2p/12D3KooWAaAa".to_string()]);
    }

    #[test]
    fn explicit_quic_passes_through() {
        let v = expand_bootstrap("/ip4/1.2.3.4/udp/4101/quic-v1/p2p/12D3KooWAaAa");
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("/quic-v1/"));
    }

    #[test]
    fn non_numeric_port_is_not_shorthand() {
        // "tcp" in the port slot → not shorthand → passthrough (1 entry).
        let v = expand_bootstrap("/dns4/h/tcp/4101/p2p/12D3KooWAaAa");
        assert_eq!(v.len(), 1);
    }
}
