//! Data-transfer objects exposed over Tauri commands. Kept separate from the
//! storage row types so the IPC contract is explicit and stable.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use y7ke_core::{ConnectionKind, ContactStatus};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct ContactView {
    pub y7_id: String,
    pub nickname: Option<String>,
    pub status: ContactStatus,
    #[ts(type = "number")]
    pub added_at: i64,
    pub presence: ConnectionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct RequestView {
    #[ts(type = "number")]
    pub id: i64,
    pub direction: String,
    pub peer_y7_id: String,
    pub initial_text: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub struct MessageView {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_y7_id: String,
    pub text: String,
    #[ts(type = "number")]
    pub timestamp_ms: i64,
    #[ts(type = "number")]
    pub status: i64,
    pub is_mine: bool,
}
