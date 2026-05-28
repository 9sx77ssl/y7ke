//! Shared types, errors, IDs, events, and cryptographic primitives for Y7KE.
//!
//! Every other Y7KE crate depends on this one. Keep it small and stable.

pub mod crypto;
pub mod diagnostics;
pub mod error;
pub mod event;
pub mod id;
pub mod settings;
pub mod status;

pub use diagnostics::DcutrStats;
pub use error::{AppError, Result};
pub use event::AppEvent;
pub use id::{ConversationId, MessageId, Y7Id};
pub use settings::{BootstrapEntry, DialMode, Settings, DEFAULT_RELAY_BOOTSTRAP};
pub use status::{
    ConnectionKind, ContactStatus, MessageStatus, NatReachability, RequestResolution,
};
