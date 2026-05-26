//! HKDF-SHA256 key derivation.

use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{AppError, Result};

/// Derive `len` bytes from `ikm` keyed with `salt` and labelled with `info`.
pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = vec![0u8; len];
    hk.expand(info, &mut okm)
        .map_err(|e| AppError::crypto(format!("hkdf expand failed: {e}")))?;
    Ok(okm)
}

/// Derive a 32-byte symmetric key.
pub fn hkdf_sha256_32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; 32]> {
    let v = hkdf_sha256(salt, ikm, info, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = hkdf_sha256_32(b"salt", b"ikm", b"info").unwrap();
        let b = hkdf_sha256_32(b"salt", b"ikm", b"info").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn changes_with_salt() {
        let a = hkdf_sha256_32(b"salt-a", b"ikm", b"info").unwrap();
        let b = hkdf_sha256_32(b"salt-b", b"ikm", b"info").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn changes_with_info() {
        let a = hkdf_sha256_32(b"salt", b"ikm", b"info-a").unwrap();
        let b = hkdf_sha256_32(b"salt", b"ikm", b"info-b").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn returns_requested_length() {
        let v = hkdf_sha256(b"salt", b"ikm", b"info", 64).unwrap();
        assert_eq!(v.len(), 64);
    }
}
