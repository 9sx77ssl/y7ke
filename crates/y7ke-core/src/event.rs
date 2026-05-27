//! Events emitted from the Rust backend to the UI (and to integration tests).
//!
//! The `kind` tag is what the frontend matches on; payload variants are flat
//! to keep TypeScript ergonomics simple.

use serde::{Deserialize, Serialize};

use crate::id::ConversationId;
use crate::status::{ConnectionKind, MessageStatus, RequestResolution};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppEvent {
    /// First-launch identity finished generating / loaded on subsequent boots.
    IdentityReady { y7_id: String },

    /// Inbound contact request arrived; UI should surface a notification.
    RequestReceived {
        y7_id: String,
        greeting: Option<String>,
    },

    /// A previously-pending request was resolved by the local user or the peer.
    RequestResolved {
        y7_id: String,
        resolution: RequestResolution,
    },

    /// A new accepted contact landed in the address book.
    ContactAdded { y7_id: String },

    /// A contact (and any related chat state) was wiped — either by the
    /// local user or remotely via a ChatDeleted control. UI should refresh
    /// the contacts list and exit any chat with this peer.
    ContactRemoved { y7_id: String },

    /// A new message was persisted locally (either inbound or outbound).
    MessageReceived {
        conversation_id: String,
        message_id: String,
        sender_y7_id: String,
        timestamp_ms: i64,
        text: String,
    },

    /// A previously persisted message changed status (e.g., Sent → Synced).
    MessageStatusChanged {
        message_id: String,
        status: MessageStatus,
    },

    /// Peer presence changed.
    PresenceChanged {
        y7_id: String,
        connection: ConnectionKind,
    },

    /// Operator-visible error surfaced from a background task.
    BackgroundError { message: String },
}

impl AppEvent {
    pub fn name(&self) -> &'static str {
        match self {
            AppEvent::IdentityReady { .. } => "identity_ready",
            AppEvent::RequestReceived { .. } => "request_received",
            AppEvent::RequestResolved { .. } => "request_resolved",
            AppEvent::ContactAdded { .. } => "contact_added",
            AppEvent::ContactRemoved { .. } => "contact_removed",
            AppEvent::MessageReceived { .. } => "message_received",
            AppEvent::MessageStatusChanged { .. } => "message_status_changed",
            AppEvent::PresenceChanged { .. } => "presence_changed",
            AppEvent::BackgroundError { .. } => "background_error",
        }
    }
}

pub fn conversation_id_hex(c: &ConversationId) -> String {
    c.to_hex()
}
