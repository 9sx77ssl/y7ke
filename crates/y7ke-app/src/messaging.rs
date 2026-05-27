//! Encrypt/decrypt + envelope construction. Plaintext starts with 1-byte tag:
//! 0x00 = utf-8 text, 0x01 = control (json payload). Tag is part of AEAD'd bytes.

use serde::{Deserialize, Serialize};

use y7ke_core::crypto::{Signature, SigningKey, SymmetricKey, VerifyingKey};
use y7ke_core::error::{AppError, Result};
use y7ke_core::MessageId;
use y7ke_net::protocol::MessageEnvelope;

pub const TAG_TEXT: u8 = 0x00;
pub const TAG_CONTROL: u8 = 0x01;

/// Out-of-band signals piggybacked on /y7ke/msg/1.0.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlPayload {
    /// Peer rejected our contact request.
    RejectedRequest,
    /// Peer wiped this conversation; receiver should mirror the wipe.
    ChatDeleted,
}

/// One of: plain UTF-8 text or a structured control payload.
#[derive(Debug, Clone)]
pub enum PlaintextKind {
    Text(String),
    Control(ControlPayload),
}

/// Canonical signed bytes: id || ts(le) || ciphertext.
pub fn signed_bytes(message_id: &[u8; 16], timestamp_ms: i64, ciphertext: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + 8 + ciphertext.len());
    v.extend_from_slice(message_id);
    v.extend_from_slice(&timestamp_ms.to_le_bytes());
    v.extend_from_slice(ciphertext);
    v
}

fn seal_kind(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    session_key: &SymmetricKey,
    plaintext: &[u8],
) -> Result<(MessageId, MessageEnvelope, i64)> {
    let message_id = MessageId::new_v7();
    let timestamp_ms = message_id
        .timestamp_ms()
        .ok_or_else(|| AppError::crypto("UUIDv7 had no embedded timestamp"))?;
    let (ciphertext, nonce) = session_key.seal(plaintext, my_pubkey)?;
    let sig = me.sign(&signed_bytes(
        message_id.as_bytes(),
        timestamp_ms,
        &ciphertext,
    ));
    Ok((
        message_id,
        MessageEnvelope {
            message_id: *message_id.as_bytes(),
            sender_pub: *my_pubkey,
            timestamp_ms,
            nonce,
            ciphertext,
            sig: sig.to_bytes(),
        },
        timestamp_ms,
    ))
}

/// Encrypt a regular text message.
pub fn seal_outgoing(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    session_key: &SymmetricKey,
    text: &str,
) -> Result<(MessageId, MessageEnvelope, i64)> {
    let mut buf = Vec::with_capacity(1 + text.len());
    buf.push(TAG_TEXT);
    buf.extend_from_slice(text.as_bytes());
    seal_kind(me, my_pubkey, session_key, &buf)
}

/// Encrypt a control payload (e.g. RejectedRequest, ChatDeleted).
pub fn seal_control(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    session_key: &SymmetricKey,
    payload: &ControlPayload,
) -> Result<(MessageId, MessageEnvelope, i64)> {
    let json = serde_json::to_vec(payload)
        .map_err(|e| AppError::Serialization(format!("control: {e}")))?;
    let mut buf = Vec::with_capacity(1 + json.len());
    buf.push(TAG_CONTROL);
    buf.extend_from_slice(&json);
    seal_kind(me, my_pubkey, session_key, &buf)
}

/// Verify sig, decrypt, split tag byte. Returns Text or Control.
pub fn open_envelope(
    envelope: &MessageEnvelope,
    sender_verifying: &VerifyingKey,
    session_key: &SymmetricKey,
) -> Result<PlaintextKind> {
    let sig = Signature::from_bytes(&envelope.sig);
    let payload = signed_bytes(
        &envelope.message_id,
        envelope.timestamp_ms,
        &envelope.ciphertext,
    );
    sender_verifying.verify(&payload, &sig)?;
    let plaintext =
        session_key.open(&envelope.nonce, &envelope.ciphertext, &envelope.sender_pub)?;
    if plaintext.is_empty() {
        return Err(AppError::crypto("plaintext is empty"));
    }
    match plaintext[0] {
        TAG_TEXT => {
            let text = String::from_utf8(plaintext[1..].to_vec())
                .map_err(|e| AppError::crypto(format!("text not utf-8: {e}")))?;
            Ok(PlaintextKind::Text(text))
        }
        TAG_CONTROL => {
            let p: ControlPayload = serde_json::from_slice(&plaintext[1..])
                .map_err(|e| AppError::Serialization(format!("control decode: {e}")))?;
            Ok(PlaintextKind::Control(p))
        }
        other => Err(AppError::crypto(format!("unknown plaintext tag {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session = SymmetricKey::random();

        let (mid, env, ts) = seal_outgoing(&me, &pub_bytes, &session, "hello").unwrap();
        assert_eq!(env.message_id, *mid.as_bytes());
        assert!(ts > 1_700_000_000_000);

        let pt = open_envelope(&env, &me.verifying_key(), &session).unwrap();
        match pt {
            PlaintextKind::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn control_round_trip() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session = SymmetricKey::random();

        let (_mid, env, _ts) =
            seal_control(&me, &pub_bytes, &session, &ControlPayload::RejectedRequest).unwrap();
        let pt = open_envelope(&env, &me.verifying_key(), &session).unwrap();
        match pt {
            PlaintextKind::Control(ControlPayload::RejectedRequest) => {}
            _ => panic!("expected RejectedRequest control"),
        }
    }

    #[test]
    fn rejects_tampered() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session = SymmetricKey::random();
        let (_, mut env, _) = seal_outgoing(&me, &pub_bytes, &session, "hi").unwrap();
        env.ciphertext[0] ^= 0xff;
        assert!(open_envelope(&env, &me.verifying_key(), &session).is_err());
    }

    #[test]
    fn rejects_wrong_key() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let a = SymmetricKey::random();
        let b = SymmetricKey::random();
        let (_, env, _) = seal_outgoing(&me, &pub_bytes, &a, "hi").unwrap();
        assert!(open_envelope(&env, &me.verifying_key(), &b).is_err());
    }
}
