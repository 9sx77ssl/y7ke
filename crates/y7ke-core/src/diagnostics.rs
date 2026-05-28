//! Networking diagnostics surfaced through the Tauri command API.
//!
//! Lives in `y7ke-core` so the wire shape can derive `ts_rs::TS` without
//! pulling Tauri or libp2p into the typegen crate. Read-only snapshots —
//! mutation happens in the swarm and event_loop.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::status::{ConnectionKind, Transport};

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
}
