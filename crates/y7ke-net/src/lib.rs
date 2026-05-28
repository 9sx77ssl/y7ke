//! libp2p swarm, session handshake, and offline sync protocol for Y7KE.
//!
//! V1 is LAN-only: TCP + Noise + Yamux + mDNS + ping + identify + three
//! `request_response` protocols (`/y7ke/handshake/1.0.0`, `/y7ke/msg/1.0.0`,
//! `/y7ke/sync/1.0.0`). No QUIC, no Kademlia, no AutoNAT/DCUtR, no relay —
//! those land in V2.
//!
//! # Surface
//!
//! - [`protocol`] — wire types, protocol IDs.
//! - [`behaviour`] — the composite `NetworkBehaviour`.
//! - [`swarm`] — swarm construction + the owning task loop.
//! - [`handle`] — `NetHandle`, `NetCommand`, `NetEvent`, `TakeOnce`.
//!
//! The typical caller does:
//!
//! ```no_run
//! use y7ke_net::{build_swarm, libp2p_keypair_from_y7_secret, spawn_swarm};
//!
//! # async fn run(secret: [u8; 32]) -> Result<(), y7ke_core::AppError> {
//! let keypair = libp2p_keypair_from_y7_secret(&secret)?;
//! let swarm = build_swarm(keypair)?;
//! let handle = spawn_swarm(swarm);
//! // `handle.send_*`, `handle.respond_*`, subscribe to `handle.event_rx()`.
//! # Ok(())
//! # }
//! ```

pub mod behaviour;
pub mod handle;
pub mod protocol;
pub mod swarm;

pub use behaviour::{Y7Behaviour, Y7BehaviourEvent};
pub use handle::{NetCommand, NetEvent, NetHandle, TakeOnce};
pub use protocol::{
    ConversationDigest, HandshakeReq, HandshakeResp, MessageEnvelope, MsgReq, MsgResp, SyncReq,
    SyncResp, HANDSHAKE_PROTOCOL, IDENTIFY_AGENT_VERSION, IDENTIFY_PROTOCOL_VERSION, MSG_PROTOCOL,
    SYNC_PROTOCOL,
};
pub use swarm::{
    build_swarm, libp2p_keypair_from_y7_secret, multiaddr_is_lan, peer_id_from_y7, spawn_swarm,
    spawn_swarm_with_bootstraps, y7_id_from_peer_id, DEFAULT_BOOTSTRAPS, DEFAULT_LISTEN_ADDR,
    DEFAULT_QUIC_LISTEN_ADDR,
};

// Re-export the libp2p types the public API exposes, so downstream
// crates don't have to track a libp2p version themselves.
pub use libp2p::{Multiaddr, PeerId};
