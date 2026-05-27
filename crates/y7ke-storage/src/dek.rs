//! Master Data Encryption Key (DEK) — 32 random bytes in a file under the
//! OS app-data directory, mode 0600. V1 only; V2 layers OS-keyring on top.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rand::rngs::OsRng;
use rand::RngCore;
use y7ke_core::crypto::SymmetricKey;

#[derive(thiserror::Error, Debug)]
pub enum DekError {
    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error("dek file has unexpected size {actual} (expected 32)")]
    InvalidSize { actual: usize },

    #[error("dek file path has no parent: {0}")]
    NoParent(PathBuf),
}

/// Wrapper that owns a loaded master DEK plus the path it was read from.
pub struct Dek {
    key: SymmetricKey,
    path: PathBuf,
}

impl Dek {
    /// Load the DEK from disk, generating a fresh one if the file does not
    /// exist. Parent directory is created (mode 0700 on Unix).
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, DekError> {
        let path = path.as_ref().to_path_buf();

        if path.exists() {
            let bytes = fs::read(&path)?;
            if bytes.len() != 32 {
                return Err(DekError::InvalidSize {
                    actual: bytes.len(),
                });
            }
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            return Ok(Self {
                key: SymmetricKey::new(k),
                path,
            });
        }

        let parent = path
            .parent()
            .ok_or_else(|| DekError::NoParent(path.clone()))?;
        fs::create_dir_all(parent)?;
        restrict_dir_perms(parent)?;

        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);

        write_file_private(&path, &key)?;

        Ok(Self {
            key: SymmetricKey::new(key),
            path,
        })
    }

    pub fn key(&self) -> &SymmetricKey {
        &self.key
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn write_file_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    use std::io::Write;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_file_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::write(path, bytes)
}

#[cfg(unix)]
fn restrict_dir_perms(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(dir)?.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(dir, perms)
}

#[cfg(not(unix))]
fn restrict_dir_perms(_dir: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generates_on_first_call_and_reloads_on_second() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir").join("master.dek");

        let dek1 = Dek::load_or_create(&path).unwrap();
        let k1 = *dek1.key().as_bytes();

        let dek2 = Dek::load_or_create(&path).unwrap();
        let k2 = *dek2.key().as_bytes();

        assert_eq!(k1, k2);
    }

    #[test]
    fn rejects_corrupt_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("master.dek");
        std::fs::write(&path, b"too short").unwrap();
        assert!(matches!(
            Dek::load_or_create(&path),
            Err(DekError::InvalidSize { .. })
        ));
    }
}
