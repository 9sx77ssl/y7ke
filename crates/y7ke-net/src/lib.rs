//! libp2p swarm, session handshake, and offline sync protocol for Y7KE.
//!
//! V1 is LAN-only: TCP + Noise + Yamux + mDNS + ping + identify + three
//! `request_response` protocols (`/y7ke/handshake/1.0.0`, `/y7ke/msg/1.0.0`,
//! `/y7ke/sync/1.0.0`). No QUIC, no Kademlia, no AutoNAT/DCUtR, no relay —
//! those land in V2.
