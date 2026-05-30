//! Networking diagnostics surfaced through the Tauri command API.
//!
//! Lives in `y7ke-core` so the wire shape can derive `ts_rs::TS` without
//! pulling Tauri or libp2p into the typegen crate. Read-only snapshots —
//! mutation happens in the swarm and event_loop.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::status::{ConnectionKind, ConnectionOrigin, IpVersion, NatReachability, Transport};

/// Aggregate DCUtR upgrade counters since the AppHandle booted.
///
/// `attempts` counts every libp2p `dcutr::Event` outcome (successful +
/// failed). `successes` is the subset where `Result::Ok(_)` arrived;
/// `failures` is the complement. UI displays a "N / M (X %)" line in
/// the Connectivity pane.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct DcutrStats {
    #[ts(type = "number")]
    pub attempts: u64,
    #[ts(type = "number")]
    pub successes: u64,
    #[ts(type = "number")]
    pub failures: u64,
}

/// One row in the Connectivity debug pane. Aggregates everything the UI
/// needs to show about a single active peer connection: identity, the
/// transport class, the relay path (if any), and the underlying
/// transport (TCP / QUIC). Built on-demand by
/// `AppHandle::list_active_connections`; no persistence.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct ConnectionView {
    /// `y7:` URI of the peer (`Y7Id::to_uri()`).
    pub y7_id: String,
    /// Best-precedence ConnectionKind currently active.
    pub kind: ConnectionKind,
    /// For Relayed connections: the relay's host portion extracted
    /// from the multiaddr (e.g. `bootstrap1.y7v.lol`). `None` for
    /// non-relayed or when extraction fails.
    pub via_host: Option<String>,
    /// Underlying transport. `None` when no current connection (e.g.
    /// Offline) or when the multiaddr couldn't be parsed.
    pub transport: Option<Transport>,
    /// IP family of the remote endpoint. `None` for relay / DNS-only.
    pub ip_version: Option<IpVersion>,
    /// HOW the connection was established (the "how did we get here?" axis).
    pub origin: ConnectionOrigin,
}

/// One recorded connection-state transition for a peer — the lineage ring.
/// A snapshot says what a connection IS *now*; this says how it GOT there
/// over the session (None→Relayed→Direct via DCUtR, or a Direct→Relayed
/// downgrade after a path drop). The single most useful artifact when a
/// user reports "it was direct, then went slow": the export shows the exact
/// sequence and timing without reproducing the session.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct TransportTransition {
    /// `y7:` URI of the peer this transition is about.
    pub y7_id: String,
    /// Connection kind BEFORE the transition. `None` for the first connect.
    pub from: Option<ConnectionKind>,
    /// Connection kind AFTER the transition.
    pub to: ConnectionKind,
    /// Transport of the new winning connection (QUIC / TCP), if known.
    pub transport: Option<Transport>,
    /// IP family of the new winning connection (v4 / v6), if known.
    pub ip_version: Option<IpVersion>,
    /// HOW the new connection was established.
    pub origin: ConnectionOrigin,
    /// Wall-clock ms when recorded — lets the export order + age transitions.
    #[ts(type = "number")]
    pub at_ms: i64,
}

/// Extra debug signal that doesn't fit the compact always-on UI but is
/// load-bearing in the copy-diagnostics export — the only debug surface a
/// non-technical user can reach. Built on-demand; no persistence.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct DiagnosticsDetail {
    /// Last few DCUtR hole-punch failure reasons (oldest → newest). The
    /// aggregate `DcutrStats` only says how many failed; this says WHY
    /// (symmetric NAT / timeout / no observed addr) — the key to triaging
    /// a pair that never upgrades from Relayed to Direct.
    pub recent_dcutr_failures: Vec<String>,
    /// Connection-state lineage across the session (oldest → newest), the
    /// "how did each peer's path evolve?" axis. Bounded ring.
    pub recent_transitions: Vec<TransportTransition>,
    /// Inbound RPCs refused by the rate limiter since boot, per protocol.
    /// Surfaces an otherwise-silent drop storm (reconnect storm / hostile
    /// peer exhausting a bucket) that would make "active connections" look
    /// healthy while messages/syncs vanish.
    pub rate_limit_drops: RateLimitDrops,
    /// AutoNAT probe detail behind the aggregate verdict.
    pub nat_detail: NatDetail,
}

/// Inbound-RPC rate-limit refusals since boot, split by protocol.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct RateLimitDrops {
    #[ts(type = "number")]
    pub handshake: u64,
    #[ts(type = "number")]
    pub msg: u64,
    #[ts(type = "number")]
    pub sync: u64,
}

/// The supporting detail behind the aggregate `NatReachability` verdict,
/// for the diagnostics export (the always-on UI shows only the verdict).
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct NatDetail {
    pub verdict: NatReachability,
    #[ts(type = "number")]
    pub consecutive_failures: u32,
    /// Last own-address AutoNAT probed, if any.
    pub last_tested_addr: Option<String>,
    /// PeerId of the last AutoNAT server that ran a probe for us.
    pub last_probe_server: Option<String>,
}
