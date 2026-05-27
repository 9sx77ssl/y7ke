//! Thin helpers for encrypting / decrypting SQLite column values with the
//! master DEK. Each call yields a fresh 12-byte nonce; caller persists it
//! alongside the ciphertext.

use y7ke_core::crypto::SymmetricKey;
use y7ke_core::error::Result;

pub fn seal(dek: &SymmetricKey, plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let (ct, nonce) = dek.seal(plaintext, aad)?;
    Ok((ct, nonce.to_vec()))
}

pub fn open(dek: &SymmetricKey, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let nonce_arr: [u8; 12] = nonce.try_into().map_err(|_| {
        y7ke_core::AppError::storage(format!("field nonce must be 12 bytes, got {}", nonce.len()))
    })?;
    dek.open(&nonce_arr, ciphertext, aad)
}
