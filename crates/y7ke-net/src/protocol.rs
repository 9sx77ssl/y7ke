//! Wire types for the three Y7KE `request_response` protocols.
//!
//! V1 LAN messenger uses three CBOR-encoded protocols:
//!
//! - [`HANDSHAKE_PROTOCOL`] — one-shot session handshake between two contacts.
//! - [`MSG_PROTOCOL`] — live delivery of an individual [`MessageEnvelope`].
//! - [`SYNC_PROTOCOL`] — multi-round reconcile protocol for offline messages.
//!
//! All wire types derive `Serialize, Deserialize, Debug, Clone` so the
//! `request_response::cbor` codec can encode them and so call sites can
//! freely move them across `tokio::mpsc` and `tokio::broadcast` channels.
//!
//! The types here intentionally hold raw byte arrays (`[u8; 32]`,
//! `[u8; 64]`, ...) rather than the strongly-typed crypto wrappers from
//! `y7ke-core::crypto`. The networking layer is dumb: it does not produce
//! or verify signatures. The composition root (`y7ke-app`) is the only
//! place that holds an unlocked `SigningKey` and can perform the
//! signature math. Keeping the wire types byte-flat means the network
//! crate has no dependency on the crypto primitives and protocol bytes
//! can be safely logged / persisted without leaking secret material.
//!
//! ## Signature inputs (informational)
//!
//! - `HandshakeReq.sig` is Ed25519 over
//!   `initiator_eph_x25519_pub || responder_pubkey`.
//! - `HandshakeResp.sig` is Ed25519 over
//!   `responder_eph_x25519_pub || initiator_pubkey`.
//! - `MessageEnvelope.sig` is Ed25519 over
//!   `message_id || timestamp_ms.to_le_bytes() || ciphertext`.

use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};

/// `/y7ke/handshake/1.0.0` — session-establishment request/response.
pub const HANDSHAKE_PROTOCOL: StreamProtocol = StreamProtocol::new("/y7ke/handshake/1.0.0");

/// `/y7ke/msg/1.0.0` — live single-message delivery.
pub const MSG_PROTOCOL: StreamProtocol = StreamProtocol::new("/y7ke/msg/1.0.0");

/// `/y7ke/sync/1.0.0` — offline-sync reconcile (3 logical rounds, single codec).
pub const SYNC_PROTOCOL: StreamProtocol = StreamProtocol::new("/y7ke/sync/1.0.0");

/// `identify` protocol-version string advertised to peers.
pub const IDENTIFY_PROTOCOL_VERSION: &str = "/y7ke/0.1.0";

/// `identify` agent string, included in the `Info` advertisement.
pub const IDENTIFY_AGENT_VERSION: &str = concat!("y7ke-net/", env!("CARGO_PKG_VERSION"));

// --------------------------------------------------------------------------
// /y7ke/handshake/1.0.0
// --------------------------------------------------------------------------

/// Handshake request emitted by the initiator side.
///
/// The signature commits the initiator's ephemeral X25519 public key to the
/// responder's long-term Ed25519 identity, so the message cannot be replayed
/// against a different responder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeReq {
    /// Initiator's long-term Ed25519 public key (the initiator's `Y7Id`).
    pub initiator_ed25519_pub: [u8; 32],
    /// Initiator's ephemeral X25519 public key.
    pub initiator_eph_x25519_pub: [u8; 32],
    /// Ed25519 signature over `initiator_eph_x25519_pub || responder_pubkey`.
    pub sig: [u8; 64],
    /// Optional human-readable greeting attached to a contact request.
    pub greeting: Option<String>,
}

/// Handshake response emitted by the responder side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResp {
    /// Responder's ephemeral X25519 public key.
    pub responder_eph_x25519_pub: [u8; 32],
    /// Ed25519 signature over `responder_eph_x25519_pub || initiator_pubkey`.
    pub sig: [u8; 64],
    /// `true` if the responder accepted the contact request.
    pub accept: bool,
}

// --------------------------------------------------------------------------
// /y7ke/msg/1.0.0
// --------------------------------------------------------------------------

/// Encrypted, signed envelope for a single message.
///
/// Mirrors `y7ke-core` documentation but kept as raw bytes so this crate has
/// no dependency on the crypto module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// UUIDv7, raw 16-byte form. Lexicographic order matches send-time order.
    pub message_id: [u8; 16],
    /// Sender's long-term Ed25519 public key.
    pub sender_pub: [u8; 32],
    /// Send timestamp in Unix milliseconds (matches the UUIDv7 prefix).
    pub timestamp_ms: i64,
    /// ChaCha20-Poly1305 nonce, random per message.
    pub nonce: [u8; 12],
    /// ChaCha20-Poly1305 ciphertext + tag of the UTF-8 plaintext.
    pub ciphertext: Vec<u8>,
    /// Ed25519 signature over `message_id || timestamp_ms.to_le_bytes() || ciphertext`.
    pub sig: [u8; 64],
}

/// Live-delivery request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgReq {
    pub envelope: MessageEnvelope,
}

/// Live-delivery response. `ack=true` means the receiver persisted the
/// envelope (or already had it) and the sender may transition the message
/// from `Sending → Sent`. `ack=false` is reserved for explicit refusal
/// (e.g. unknown sender); senders must treat it the same as a transport
/// failure for retry purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgResp {
    pub ack: bool,
}

// --------------------------------------------------------------------------
// /y7ke/sync/1.0.0
// --------------------------------------------------------------------------

/// Per-conversation digest exchanged in the first round of sync.
///
/// `highest_outbound_msg_id` is the most recent UUIDv7 we sent in this
/// conversation; `highest_inbound_msg_id` is the most recent UUIDv7 we
/// received. The counterparty compares these against its own values and
/// decides what (if anything) to `Pull`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDigest {
    pub conversation_id: [u8; 16],
    pub highest_outbound_msg_id: Option<[u8; 16]>,
    pub highest_inbound_msg_id: Option<[u8; 16]>,
}

/// 3-round reconcile protocol expressed as a single discriminator-tagged
/// request type. Each round is one round-trip on the `/y7ke/sync/1.0.0`
/// `request_response` stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncReq {
    /// Round 1: announce per-conversation digests.
    Header { conversations: Vec<ConversationDigest> },
    /// Round 2: request missing envelopes for a single conversation.
    /// `since` is the last UUIDv7 the caller already holds (exclusive); a
    /// `None` means "from the beginning of the conversation."
    Pull {
        conversation_id: [u8; 16],
        since: Option<[u8; 16]>,
        limit: u16,
    },
    /// Round 3: confirm which message IDs the caller persisted; the sender
    /// uses this to transition its rows from `Sent → Synced` and drop them
    /// from the retry queue.
    Ack {
        conversation_id: [u8; 16],
        confirmed_ids: Vec<[u8; 16]>,
    },
}

/// Response variants paired by position with [`SyncReq`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResp {
    /// Reply to [`SyncReq::Header`] — the responder's own digests.
    HeaderAck { ours: Vec<ConversationDigest> },
    /// Reply to [`SyncReq::Pull`] — envelopes the caller is missing.
    /// `has_more=true` signals the caller should issue another `Pull` with
    /// `since` set to the last returned `message_id`.
    Pull {
        envelopes: Vec<MessageEnvelope>,
        has_more: bool,
    },
    /// Reply to [`SyncReq::Ack`] — empty acknowledgement.
    Ack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_ids_are_stable() {
        assert_eq!(HANDSHAKE_PROTOCOL.as_ref(), "/y7ke/handshake/1.0.0");
        assert_eq!(MSG_PROTOCOL.as_ref(), "/y7ke/msg/1.0.0");
        assert_eq!(SYNC_PROTOCOL.as_ref(), "/y7ke/sync/1.0.0");
    }

    #[test]
    fn handshake_req_round_trips_through_cbor() {
        let req = HandshakeReq {
            initiator_ed25519_pub: [1u8; 32],
            initiator_eph_x25519_pub: [2u8; 32],
            sig: [3u8; 64],
            greeting: Some("hello".into()),
        };
        let bytes = serde_cbor_round_trip(&req);
        let back: HandshakeReq = bytes;
        assert_eq!(back.initiator_ed25519_pub, req.initiator_ed25519_pub);
        assert_eq!(back.initiator_eph_x25519_pub, req.initiator_eph_x25519_pub);
        assert_eq!(back.sig, req.sig);
        assert_eq!(back.greeting.as_deref(), Some("hello"));
    }

    #[test]
    fn sync_req_round_trips_each_variant() {
        let header = SyncReq::Header {
            conversations: vec![ConversationDigest {
                conversation_id: [9u8; 16],
                highest_outbound_msg_id: Some([1u8; 16]),
                highest_inbound_msg_id: None,
            }],
        };
        let _: SyncReq = serde_cbor_round_trip(&header);

        let pull = SyncReq::Pull {
            conversation_id: [9u8; 16],
            since: Some([1u8; 16]),
            limit: 50,
        };
        let _: SyncReq = serde_cbor_round_trip(&pull);

        let ack = SyncReq::Ack {
            conversation_id: [9u8; 16],
            confirmed_ids: vec![[1u8; 16], [2u8; 16]],
        };
        let _: SyncReq = serde_cbor_round_trip(&ack);
    }

    /// CBOR encode then decode `value` using `serde_json` as a stand-in so
    /// we don't pull in a new dev-dependency; serde_json round-trips
    /// exercise the same `Serialize`/`Deserialize` impls the production
    /// CBOR codec uses.
    fn serde_cbor_round_trip<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let s = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&s).expect("deserialize")
    }
}
