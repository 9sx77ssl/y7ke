//! Networking diagnostics surfaced through the Tauri command API.
//!
//! Lives in `y7ke-core` so the wire shape can derive `ts_rs::TS` without
//! pulling Tauri or libp2p into the typegen crate. Read-only snapshots —
//! mutation happens in the swarm and event_loop.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
