//! Status enums shared across modules and the IPC surface.

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use ts_rs::TS;

/// Lifecycle of a message in the local outbox.
///
/// Stored in `messages.status` as the `i64` discriminant. Wire format is
/// also the i64 discriminant (via serde_repr) so the UI consumes it as a
/// number, matching the `MSG_SENDING / MSG_SENT / …` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize_repr, Deserialize_repr)]
#[repr(i64)]
pub enum MessageStatus {
    /// Encrypted, persisted locally, queued for first send.
    Sending = 0,
    /// Pushed to the peer on this device; awaiting peer-side acknowledgement.
    Sent = 1,
    /// V2 — peer has read it.
    Delivered = 2,
    /// Reconcile protocol confirmed both sides hold an identical copy.
    Synced = 3,
    /// Permanent failure after retry budget exhausted.
    Failed = 4,
}

impl MessageStatus {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            0 => Self::Sending,
            1 => Self::Sent,
            2 => Self::Delivered,
            3 => Self::Synced,
            4 => Self::Failed,
            _ => return None,
        })
    }
}

/// How a peer is currently reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub enum ConnectionKind {
    Offline,
    Connecting,
    /// LAN (mDNS-discovered).
    Lan,
    /// Direct internet connection (V2).
    Direct,
    /// Connected through a circuit relay (V2).
    Relayed,
}

/// State of a pending contact request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub enum RequestResolution {
    Accepted,
    Rejected,
    Cancelled,
}

/// State of a contact in the local address book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/lib/gen/")]
pub enum ContactStatus {
    Accepted,
    PendingOut,
    PendingIn,
    Blocked,
    Removed,
}

impl ContactStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::PendingOut => "pending_out",
            Self::PendingIn => "pending_in",
            Self::Blocked => "blocked",
            Self::Removed => "removed",
        }
    }
}

impl std::str::FromStr for ContactStatus {
    type Err = crate::error::AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "accepted" => Self::Accepted,
            "pending_out" => Self::PendingOut,
            "pending_in" => Self::PendingIn,
            "blocked" => Self::Blocked,
            "removed" => Self::Removed,
            other => {
                return Err(crate::error::AppError::invalid_input(format!(
                    "unknown ContactStatus {other:?}"
                )));
            }
        })
    }
}
