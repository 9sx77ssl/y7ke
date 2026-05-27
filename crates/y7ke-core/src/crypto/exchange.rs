//! X25519 ephemeral key exchange for session establishment.

use rand::rngs::OsRng;
use x25519_dalek::{
    EphemeralSecret as DalekEph, PublicKey as DalekPub, StaticSecret as DalekStatic,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{AppError, Result};

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SHARED_SECRET_LEN: usize = 32;

/// One-shot X25519 secret. Performing the Diffie-Hellman consumes it.
pub struct EphemeralSecret {
    inner: DalekEph,
}

impl EphemeralSecret {
    pub fn generate() -> Self {
        Self {
            inner: DalekEph::random_from_rng(OsRng),
        }
    }

    pub fn public_key(&self) -> ExchangePublicKey {
        ExchangePublicKey {
            inner: DalekPub::from(&self.inner),
        }
    }

    pub fn diffie_hellman(self, peer: &ExchangePublicKey) -> SharedSecret {
        let shared = self.inner.diffie_hellman(&peer.inner);
        let mut out = [0u8; SHARED_SECRET_LEN];
        out.copy_from_slice(shared.as_bytes());
        SharedSecret { bytes: out }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExchangePublicKey {
    inner: DalekPub,
}

impl ExchangePublicKey {
    pub fn from_bytes(b: [u8; PUBLIC_KEY_LEN]) -> Self {
        Self {
            inner: DalekPub::from(b),
        }
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LEN] {
        self.inner.to_bytes()
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SharedSecret {
    bytes: [u8; SHARED_SECRET_LEN],
}

impl SharedSecret {
    pub fn from_bytes(bytes: [u8; SHARED_SECRET_LEN]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; SHARED_SECRET_LEN] {
        &self.bytes
    }
}

/// Reusable X25519 key derived from the Ed25519 identity scalar.
/// Never stored — always derived on demand via `SigningKey::to_x25519_scalar`.
pub struct StaticKey {
    inner: DalekStatic,
}

impl StaticKey {
    pub fn from_scalar(scalar: [u8; 32]) -> Self {
        Self {
            inner: DalekStatic::from(scalar),
        }
    }

    pub fn diffie_hellman(&self, peer: &ExchangePublicKey) -> SharedSecret {
        let shared = self.inner.diffie_hellman(&peer.inner);
        let mut out = [0u8; SHARED_SECRET_LEN];
        out.copy_from_slice(shared.as_bytes());
        SharedSecret { bytes: out }
    }
}

/// Helper: parse a 32-byte buffer into an `ExchangePublicKey`.
pub fn parse_public_key(buf: &[u8]) -> Result<ExchangePublicKey> {
    if buf.len() != PUBLIC_KEY_LEN {
        return Err(AppError::crypto(format!(
            "x25519 public key must be {PUBLIC_KEY_LEN} bytes, got {}",
            buf.len()
        )));
    }
    let mut b = [0u8; PUBLIC_KEY_LEN];
    b.copy_from_slice(buf);
    Ok(ExchangePublicKey::from_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dh_shared_secret_matches() {
        let alice_sk = EphemeralSecret::generate();
        let alice_pk = alice_sk.public_key();

        let bob_sk = EphemeralSecret::generate();
        let bob_pk = bob_sk.public_key();

        let alice_shared = alice_sk.diffie_hellman(&bob_pk);
        let bob_shared = bob_sk.diffie_hellman(&alice_pk);

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn dh_yields_different_secrets_for_different_pairs() {
        let alice = EphemeralSecret::generate();
        let bob = EphemeralSecret::generate();
        let carol = EphemeralSecret::generate();

        let bob_pk = bob.public_key();
        let carol_pk = carol.public_key();

        let alice2 = EphemeralSecret::generate();
        // Same alice equivalent doesn't exist (consumed). Use two fresh secrets.
        let s1 = alice.diffie_hellman(&bob_pk);
        let s2 = alice2.diffie_hellman(&carol_pk);
        assert_ne!(s1.as_bytes(), s2.as_bytes());
    }
}
