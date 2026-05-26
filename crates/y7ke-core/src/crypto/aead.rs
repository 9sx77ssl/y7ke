//! ChaCha20-Poly1305 AEAD wrapper used for at-rest column encryption and
//! over-the-wire session encryption.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{AppError, Result};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;

/// 32-byte ChaCha20-Poly1305 symmetric key.
#[derive(Zeroize, ZeroizeOnDrop, Clone)]
pub struct SymmetricKey {
    bytes: [u8; KEY_LEN],
}

impl SymmetricKey {
    pub fn new(bytes: [u8; KEY_LEN]) -> Self {
        Self { bytes }
    }

    pub fn random() -> Self {
        let mut bytes = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// AEAD encrypt `plaintext` with associated data `aad`. The nonce is
    /// generated randomly per call; caller stores it next to the ciphertext.
    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_LEN])> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.bytes));
        let nonce_bytes = generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| AppError::crypto(format!("encrypt failed: {e}")))?;
        Ok((ciphertext, nonce_bytes))
    }

    pub fn open(&self, nonce: &[u8; NONCE_LEN], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.bytes));
        let nonce = Nonce::from_slice(nonce);
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|e| AppError::crypto(format!("decrypt failed: {e}")))
    }
}

pub fn generate_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip() {
        let key = SymmetricKey::random();
        let msg = b"the quick brown fox jumps over the lazy dog";
        let aad = b"associated data here";
        let (ct, nonce) = key.seal(msg, aad).unwrap();
        let pt = key.open(&nonce, &ct, aad).unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn rejects_tampered_ciphertext() {
        let key = SymmetricKey::random();
        let (mut ct, nonce) = key.seal(b"original", b"").unwrap();
        ct[0] ^= 0xff;
        assert!(key.open(&nonce, &ct, b"").is_err());
    }

    #[test]
    fn rejects_wrong_aad() {
        let key = SymmetricKey::random();
        let (ct, nonce) = key.seal(b"data", b"correct-aad").unwrap();
        assert!(key.open(&nonce, &ct, b"wrong-aad").is_err());
    }

    #[test]
    fn rejects_wrong_key() {
        let k1 = SymmetricKey::random();
        let k2 = SymmetricKey::random();
        let (ct, nonce) = k1.seal(b"secret", b"").unwrap();
        assert!(k2.open(&nonce, &ct, b"").is_err());
    }
}
