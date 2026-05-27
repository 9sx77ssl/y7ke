//! Identifier types: `Y7Id`, `MessageId`, `ConversationId`.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::error::{AppError, Result};

/// Public identity of a Y7KE user. Wraps the 32-byte Ed25519 public key.
///
/// Wire form: `y7:<base58>` where `base58` encodes the raw pubkey using
/// the Bitcoin alphabet (no `0`, `O`, `I`, `l`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Y7Id {
    pubkey: [u8; 32],
}

impl Y7Id {
    pub const URI_PREFIX: &'static str = "y7:";

    pub fn from_pubkey(pubkey: [u8; 32]) -> Self {
        Self { pubkey }
    }

    pub fn pubkey(&self) -> &[u8; 32] {
        &self.pubkey
    }

    pub fn into_pubkey(self) -> [u8; 32] {
        self.pubkey
    }

    pub fn to_uri(&self) -> String {
        format!(
            "{}{}",
            Self::URI_PREFIX,
            bs58::encode(self.pubkey).into_string()
        )
    }

    /// Parse a `y7:<base58>` URI. Validates structure (length, base58
    /// alphabet) but does NOT confirm the decoded bytes form a valid Ed25519
    /// point — that check happens at the network boundary via
    /// [`crate::crypto::VerifyingKey::from_bytes`] and at the libp2p mapping
    /// via `peer_id_from_y7`. Use [`Self::parse_strict`] at user-facing
    /// entry points (Tauri commands) to fail-fast on garbage URIs.
    pub fn parse(s: &str) -> Result<Self> {
        let body = s.strip_prefix(Self::URI_PREFIX).ok_or_else(|| {
            AppError::InvalidY7Id(format!("missing '{}' prefix", Self::URI_PREFIX))
        })?;
        let mut buf = [0u8; 32];
        let written = bs58::decode(body)
            .onto(&mut buf)
            .map_err(|e| AppError::InvalidY7Id(format!("base58 decode: {e}")))?;
        if written != 32 {
            return Err(AppError::InvalidY7Id(format!(
                "expected 32 bytes, got {written}"
            )));
        }
        Ok(Self { pubkey: buf })
    }

    /// Strict variant of [`Self::parse`] that ALSO verifies the decoded bytes
    /// form a valid Ed25519 public key. Use at IPC boundaries (Tauri commands)
    /// so adversarial input is rejected before it can panic libp2p or trigger
    /// expensive downstream operations.
    pub fn parse_strict(s: &str) -> Result<Self> {
        let y7 = Self::parse(s)?;
        crate::crypto::VerifyingKey::from_bytes(&y7.pubkey).map_err(|_| {
            AppError::InvalidY7Id("decoded bytes are not a valid Ed25519 public key".into())
        })?;
        Ok(y7)
    }
}

impl fmt::Display for Y7Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uri())
    }
}

impl fmt::Debug for Y7Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Y7Id({})", self.to_uri())
    }
}

/// Per-message unique identifier — UUIDv7 (timestamp-sortable, collision-free).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(Uuid);

impl MessageId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_bytes(b: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(b))
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }

    /// Returns the embedded UUIDv7 timestamp in milliseconds since Unix epoch.
    /// Returns `None` if the underlying UUID is not a v7 (should not happen
    /// for IDs created via [`MessageId::new_v7`]).
    pub fn timestamp_ms(&self) -> Option<i64> {
        let ts = self.0.get_timestamp()?;
        let (secs, nanos) = ts.to_unix();
        Some(secs as i64 * 1000 + (nanos / 1_000_000) as i64)
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MessageId({})", self.0)
    }
}

/// 16-byte identifier for the two-party conversation between two `Y7Id`s.
///
/// Derived as `blake3(sort(pubkey_a, pubkey_b))[..16]` so both peers compute
/// the same value regardless of who initiated.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationId(pub [u8; 16]);

impl ConversationId {
    pub fn between(a: &Y7Id, b: &Y7Id) -> Self {
        let (lo, hi) = if a.pubkey() <= b.pubkey() {
            (a.pubkey(), b.pubkey())
        } else {
            (b.pubkey(), a.pubkey())
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(lo);
        hasher.update(hi);
        let digest = hasher.finalize();
        let mut out = [0u8; 16];
        out.copy_from_slice(&digest.as_bytes()[..16]);
        Self(out)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConversationId({})", self.to_hex())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y7id_round_trip() {
        let bytes = [7u8; 32];
        let id = Y7Id::from_pubkey(bytes);
        let s = id.to_uri();
        assert!(s.starts_with("y7:"));
        let back = Y7Id::parse(&s).unwrap();
        assert_eq!(back.pubkey(), &bytes);
    }

    #[test]
    fn y7id_rejects_garbage() {
        assert!(Y7Id::parse("not-a-y7-uri").is_err());
        assert!(Y7Id::parse("y7:000").is_err());
    }

    #[test]
    fn parse_strict_rejects_non_curve_points() {
        // Round-trip a synthetic 32-byte sequence; `parse` accepts it but
        // `parse_strict` MUST reject if it's not on the Ed25519 curve.
        let synthetic = Y7Id::from_pubkey([7u8; 32]).to_uri();
        assert!(Y7Id::parse(&synthetic).is_ok());
        assert!(
            Y7Id::parse_strict(&synthetic).is_err(),
            "parse_strict must reject non-curve-point inputs"
        );

        // A real Ed25519 pubkey from SigningKey passes both.
        let sk = crate::crypto::SigningKey::generate();
        let valid = Y7Id::from_pubkey(sk.verifying_key().to_bytes()).to_uri();
        assert!(Y7Id::parse(&valid).is_ok());
        assert!(Y7Id::parse_strict(&valid).is_ok());
    }

    #[test]
    fn message_id_is_v7() {
        let id = MessageId::new_v7();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let ts = id.timestamp_ms().unwrap();
        assert!(ts > 1_700_000_000_000);
    }

    #[test]
    fn conversation_id_is_symmetric() {
        let a = Y7Id::from_pubkey([1u8; 32]);
        let b = Y7Id::from_pubkey([2u8; 32]);
        assert_eq!(
            ConversationId::between(&a, &b),
            ConversationId::between(&b, &a)
        );
    }

    #[test]
    fn conversation_id_distinguishes_pairs() {
        let a = Y7Id::from_pubkey([1u8; 32]);
        let b = Y7Id::from_pubkey([2u8; 32]);
        let c = Y7Id::from_pubkey([3u8; 32]);
        assert_ne!(
            ConversationId::between(&a, &b),
            ConversationId::between(&a, &c)
        );
    }
}
