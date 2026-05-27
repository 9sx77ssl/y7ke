//! Session establishment via the `/y7ke/handshake/1.0.0` request_response
//! protocol.
//!
//! Both peers prove ownership of their Ed25519 long-term identity by signing
//! over `ephemeral_pub || counterparty_pubkey`. After exchanging X25519
//! ephemerals, both derive a 32-byte `session_key` via HKDF-SHA256.

use y7ke_core::crypto::{
    hkdf_sha256, EphemeralSecret, ExchangePublicKey, Signature, SigningKey, SymmetricKey,
    VerifyingKey,
};
use y7ke_core::error::{AppError, Result};
use y7ke_core::Y7Id;
use y7ke_net::protocol::{HandshakeReq, HandshakeResp};

/// HKDF info tag for session-key derivation. Bumping this is a session-key
/// format break — bump the protocol version at the same time.
pub const SESSION_KDF_INFO: &[u8] = b"y7ke-session-v1";

/// Salt for the HKDF — `blake3(sort(pub_a, pub_b))` makes the salt symmetric
/// across both peers regardless of who initiated.
pub fn session_salt(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = blake3::Hasher::new();
    hasher.update(lo);
    hasher.update(hi);
    *hasher.finalize().as_bytes()
}

/// Caller-side: produce the `HandshakeReq` for an outbound handshake. The
/// returned `EphemeralSecret` is consumed in `finalize_initiator` once the
/// matching `HandshakeResp` arrives — keep it on the stack between calls.
pub fn open_initiator(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    peer_pubkey: &[u8; 32],
    greeting: Option<String>,
) -> (HandshakeReq, EphemeralSecret) {
    let eph = EphemeralSecret::generate();
    let eph_pub = eph.public_key();
    let mut signed = [0u8; 64];
    signed[..32].copy_from_slice(&eph_pub.to_bytes());
    signed[32..].copy_from_slice(peer_pubkey);
    let sig = me.sign(&signed);
    let req = HandshakeReq {
        initiator_ed25519_pub: *my_pubkey,
        initiator_eph_x25519_pub: eph_pub.to_bytes(),
        sig: sig.to_bytes(),
        greeting,
    };
    (req, eph)
}

/// Caller-side: after receiving `resp`, verify the peer's signature and
/// derive the shared session key.
pub fn finalize_initiator(
    my_eph: EphemeralSecret,
    my_pubkey: &[u8; 32],
    peer_pubkey: &[u8; 32],
    resp: &HandshakeResp,
) -> Result<SymmetricKey> {
    let peer_verifying = VerifyingKey::from_bytes(peer_pubkey)?;
    let mut signed = [0u8; 64];
    signed[..32].copy_from_slice(&resp.responder_eph_x25519_pub);
    signed[32..].copy_from_slice(my_pubkey);
    let sig = Signature::from_bytes(&resp.sig);
    peer_verifying.verify(&signed, &sig)?;

    let peer_eph_x = ExchangePublicKey::from_bytes(resp.responder_eph_x25519_pub);
    let shared = my_eph.diffie_hellman(&peer_eph_x);
    let salt = session_salt(my_pubkey, peer_pubkey);
    let key = hkdf_sha256(&salt, shared.as_bytes(), SESSION_KDF_INFO, 32)?;
    let arr: [u8; 32] = key
        .try_into()
        .map_err(|_| AppError::crypto("hkdf produced wrong length"))?;
    Ok(SymmetricKey::new(arr))
}

/// Responder side: verify the inbound `HandshakeReq` and produce both the
/// response and the derived session key.
pub fn respond(
    me: &SigningKey,
    my_pubkey: &[u8; 32],
    req: &HandshakeReq,
) -> Result<(HandshakeResp, SymmetricKey, Y7Id)> {
    let initiator_id = Y7Id::from_pubkey(req.initiator_ed25519_pub);

    // Verify the initiator's signature over (eph || my_pubkey).
    let initiator_verifying = VerifyingKey::from_bytes(&req.initiator_ed25519_pub)?;
    let mut signed = [0u8; 64];
    signed[..32].copy_from_slice(&req.initiator_eph_x25519_pub);
    signed[32..].copy_from_slice(my_pubkey);
    let sig = Signature::from_bytes(&req.sig);
    initiator_verifying.verify(&signed, &sig)?;

    // Generate our own ephemeral, derive shared, build response.
    let my_eph = EphemeralSecret::generate();
    let my_eph_pub = my_eph.public_key();

    let mut resp_signed = [0u8; 64];
    resp_signed[..32].copy_from_slice(&my_eph_pub.to_bytes());
    resp_signed[32..].copy_from_slice(&req.initiator_ed25519_pub);
    let resp_sig = me.sign(&resp_signed);

    let peer_eph_x = ExchangePublicKey::from_bytes(req.initiator_eph_x25519_pub);
    let shared = my_eph.diffie_hellman(&peer_eph_x);
    let salt = session_salt(&req.initiator_ed25519_pub, my_pubkey);
    let session_bytes = hkdf_sha256(&salt, shared.as_bytes(), SESSION_KDF_INFO, 32)?;
    let session_arr: [u8; 32] = session_bytes
        .try_into()
        .map_err(|_| AppError::crypto("hkdf produced wrong length"))?;

    let resp = HandshakeResp {
        responder_eph_x25519_pub: my_eph_pub.to_bytes(),
        sig: resp_sig.to_bytes(),
        accept: true,
    };
    Ok((resp, SymmetricKey::new(session_arr), initiator_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_derive_same_session_key() {
        let alice = SigningKey::generate();
        let bob = SigningKey::generate();
        let alice_pub = alice.verifying_key().to_bytes();
        let bob_pub = bob.verifying_key().to_bytes();

        // Alice opens the handshake.
        let (req, alice_eph) = open_initiator(&alice, &alice_pub, &bob_pub, Some("hi".into()));

        // Bob receives, verifies, responds.
        let (resp, bob_key, alice_id) = respond(&bob, &bob_pub, &req).unwrap();
        assert_eq!(alice_id, Y7Id::from_pubkey(alice_pub));

        // Alice finalizes.
        let alice_key = finalize_initiator(alice_eph, &alice_pub, &bob_pub, &resp).unwrap();

        assert_eq!(alice_key.as_bytes(), bob_key.as_bytes());
    }

    #[test]
    fn rejects_bad_initiator_signature() {
        let alice = SigningKey::generate();
        let attacker = SigningKey::generate();
        let bob = SigningKey::generate();
        let alice_pub = alice.verifying_key().to_bytes();
        let bob_pub = bob.verifying_key().to_bytes();

        // Initiator advertises alice's pubkey but signs with the attacker.
        let (mut req, _eph) = open_initiator(&alice, &alice_pub, &bob_pub, None);
        let bad_sig = attacker.sign(b"unrelated payload");
        req.sig = bad_sig.to_bytes();

        assert!(respond(&bob, &bob_pub, &req).is_err());
    }
}
