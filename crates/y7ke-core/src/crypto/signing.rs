//! Ed25519 long-term signing keys.

use ed25519_dalek::{
    Signature as DalekSig, Signer, SigningKey as DalekSigning, Verifier,
    VerifyingKey as DalekVerifying,
};
use rand::rngs::OsRng;

use crate::error::{AppError, Result};

pub const SIGNATURE_LEN: usize = 64;
pub const PUBLIC_KEY_LEN: usize = 32;
pub const SECRET_KEY_LEN: usize = 32;

/// Long-term signing key. Wraps `ed25519_dalek::SigningKey`, which zeroizes
/// on drop via the `zeroize` feature.
pub struct SigningKey {
    inner: DalekSigning,
}

impl SigningKey {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        Self {
            inner: DalekSigning::generate(&mut rng),
        }
    }

    pub fn from_bytes(secret: &[u8; SECRET_KEY_LEN]) -> Self {
        Self {
            inner: DalekSigning::from_bytes(secret),
        }
    }

    pub fn to_bytes(&self) -> [u8; SECRET_KEY_LEN] {
        self.inner.to_bytes()
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature {
            inner: self.inner.sign(message),
        }
    }

    /// Derive the X25519 scalar from this Ed25519 seed.
    /// Uses SHA-512(seed)[..32] with RFC 7748 clamping — same scalar as signing.
    pub fn to_x25519_scalar(&self) -> [u8; 32] {
        use sha2::{Digest, Sha512};
        let hash = Sha512::digest(self.inner.to_bytes());
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(&hash[..32]);
        scalar[0] &= 248;
        scalar[31] &= 127;
        scalar[31] |= 64;
        scalar
    }
}

/// Long-term verification key (the "public key" half).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VerifyingKey {
    inner: DalekVerifying,
}

impl VerifyingKey {
    pub fn from_bytes(pubkey: &[u8; PUBLIC_KEY_LEN]) -> Result<Self> {
        DalekVerifying::from_bytes(pubkey)
            .map(|inner| Self { inner })
            .map_err(|_| AppError::InvalidPublicKey)
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LEN] {
        self.inner.to_bytes()
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        self.inner
            .verify(message, &signature.inner)
            .map_err(|_| AppError::InvalidSignature)
    }

    /// Convert this Ed25519 public key to its X25519 (Montgomery) form.
    pub fn to_x25519_public(&self) -> [u8; 32] {
        self.inner.to_montgomery().to_bytes()
    }
}

/// Ed25519 signature (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    inner: DalekSig,
}

impl Signature {
    pub fn from_bytes(bytes: &[u8; SIGNATURE_LEN]) -> Self {
        Self {
            inner: DalekSig::from_bytes(bytes),
        }
    }

    pub fn to_bytes(&self) -> [u8; SIGNATURE_LEN] {
        self.inner.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_round_trip() {
        let sk = SigningKey::generate();
        let vk = sk.verifying_key();
        let msg = b"the rain in spain";
        let sig = sk.sign(msg);
        vk.verify(msg, &sig).expect("signature should verify");
    }

    #[test]
    fn detects_tampered_message() {
        let sk = SigningKey::generate();
        let vk = sk.verifying_key();
        let sig = sk.sign(b"original message");
        assert!(vk.verify(b"tampered message", &sig).is_err());
    }

    #[test]
    fn detects_wrong_key() {
        let sk1 = SigningKey::generate();
        let sk2 = SigningKey::generate();
        let sig = sk1.sign(b"hello");
        assert!(sk2.verifying_key().verify(b"hello", &sig).is_err());
    }

    #[test]
    fn bytes_round_trip() {
        let sk = SigningKey::generate();
        let sk_bytes = sk.to_bytes();
        let restored = SigningKey::from_bytes(&sk_bytes);
        let msg = b"persistence test";
        let sig = sk.sign(msg);
        restored
            .verifying_key()
            .verify(msg, &sig)
            .expect("restored key derives the same verifier");
    }
}
