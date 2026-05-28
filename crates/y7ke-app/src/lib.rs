//! Composition root tying `y7ke-core`, `y7ke-storage`, and `y7ke-net` together.
//!
//! `src-tauri` depends only on this crate. A headless harness binary at
//! `examples/swarm_harness.rs` uses the same API without Tauri so multi-client
//! integration tests can run in CI.

pub mod app;
pub mod config;
pub mod event_loop;
pub mod handshake;
pub mod identity;
pub mod messaging;
pub mod rate_limit;
pub(crate) mod reconnect;
pub mod views;

pub use app::{AppConfig, AppHandle, EVENT_CHANNEL_CAPACITY};
pub use views::{ContactView, MessageView, RequestView};
