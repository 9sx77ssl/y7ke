//! Per-message encryption / decryption + envelope construction. The session
//! key is supplied by the caller (it lives in `sessions`).

use y7ke_core::crypto::{Signature, SigningKey, SymmetricKey, VerifyingKey};
use y7ke_core::error::{AppError, Result};
use y7ke_core::MessageId;
use y7ke_net::protocol::MessageEnvelope;

/// Build the canonical bytes that the sender signs (and the receiver verifies)
/// for a [`MessageEnvelope`].
pub fn signed_bytes(message_id: &[u8; 16], timestamp_ms: i64, ciphertext: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + 8 + ciphertext.len());
    v.extend_from_slice(message_id);
    v.extend_from_slice(&timestamp_ms.to_le_bytes());
    v.extend_from_slice(ciphertext);
    v
}

/// Encrypt `text` with `session_key`, sign with `me`, and produce the wire
/// envelope plus the `MessageId` it was assigned (UUIDv7 from `now()`).
pub fn seal_outgoing(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    session_key: &SymmetricKey,
    text: &str,
) -> Result<(MessageId, MessageEnvelope, i64)> {
    let message_id = MessageId::new_v7();
    let timestamp_ms = message_id.timestamp_ms().ok_or_else(|| {
        AppError::crypto("UUIDv7 had no embedded timestamp — should be unreachable")
    })?;
    let (ciphertext, nonce) = session_key.seal(text.as_bytes(), my_pubkey)?;
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

/// Verify the envelope's signature with `sender_verifying`, then decrypt with
/// `session_key`. The AAD binds the ciphertext to the sender's pubkey.
pub fn open_envelope(
    envelope: &MessageEnvelope,
    sender_verifying: &VerifyingKey,
    session_key: &SymmetricKey,
) -> Result<String> {
    let sig = Signature::from_bytes(&envelope.sig);
    let payload = signed_bytes(
        &envelope.message_id,
        envelope.timestamp_ms,
        &envelope.ciphertext,
    );
    sender_verifying.verify(&payload, &sig)?;
    let plaintext =
        session_key.open(&envelope.nonce, &envelope.ciphertext, &envelope.sender_pub)?;
    String::from_utf8(plaintext)
        .map_err(|e| AppError::crypto(format!("message plaintext is not utf-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_envelope() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session = SymmetricKey::random();
        let text = "encrypted hi from the other side";

        let (mid, envelope, ts) = seal_outgoing(&me, &pub_bytes, &session, text).unwrap();
        assert_eq!(envelope.message_id, *mid.as_bytes());
        assert_eq!(envelope.sender_pub, pub_bytes);
        assert!(ts > 1_700_000_000_000);

        let verifying = me.verifying_key();
        let decoded = open_envelope(&envelope, &verifying, &session).unwrap();
        assert_eq!(decoded, text);
    }

    #[test]
    fn rejects_tampered_ciphertext() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session = SymmetricKey::random();

        let (_mid, mut env, _ts) = seal_outgoing(&me, &pub_bytes, &session, "hi").unwrap();
        env.ciphertext[0] ^= 0xff;
        assert!(open_envelope(&env, &me.verifying_key(), &session).is_err());
    }

    #[test]
    fn rejects_wrong_session_key() {
        let me = SigningKey::generate();
        let pub_bytes = me.verifying_key().to_bytes();
        let session_a = SymmetricKey::random();
        let session_b = SymmetricKey::random();

        let (_mid, env, _ts) = seal_outgoing(&me, &pub_bytes, &session_a, "hi").unwrap();
        assert!(open_envelope(&env, &me.verifying_key(), &session_b).is_err());
    }
}
