//! Cryptographic primitives wrapped behind opaque newtypes.
//!
//! All secret types implement `Zeroize` so memory is wiped on drop.

pub mod aead;
pub mod exchange;
pub mod kdf;
pub mod signing;

pub use aead::{generate_nonce, SymmetricKey, NONCE_LEN};
pub use exchange::{EphemeralSecret, ExchangePublicKey, SharedSecret};
pub use kdf::hkdf_sha256;
pub use signing::{Signature, SigningKey, VerifyingKey};
