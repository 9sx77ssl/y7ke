//! Identity: load or generate the local Ed25519 keypair.

use y7ke_core::crypto::SigningKey;
use y7ke_core::error::Result;
use y7ke_core::Y7Id;
use y7ke_storage::dao::users::NewUser;
use y7ke_storage::Db;

/// Combined view of the local identity (keypair + Y7 URI).
pub struct LocalIdentity {
    pub signing_key: SigningKey,
    pub y7_id: Y7Id,
}

/// Idempotent: returns the existing identity if one is persisted; otherwise
/// generates a new Ed25519 keypair, encrypts the private half with the DEK,
/// and writes it to `users`.
pub async fn ensure(db: &Db) -> Result<LocalIdentity> {
    if let Some(user) = db.users().get().await? {
        let signing_key = SigningKey::from_bytes(&user.ed25519_priv);
        return Ok(LocalIdentity {
            signing_key,
            y7_id: user.y7_id,
        });
    }

    let signing_key = SigningKey::generate();
    let pub_bytes = signing_key.verifying_key().to_bytes();
    let priv_bytes = signing_key.to_bytes();
    let y7_id = Y7Id::from_pubkey(pub_bytes);

    db.users()
        .insert(NewUser {
            y7_id,
            ed25519_pub: pub_bytes,
            ed25519_priv: priv_bytes,
        })
        .await?;

    tracing::info!(y7_id = %y7_id, "generated new identity");

    Ok(LocalIdentity { signing_key, y7_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use y7ke_storage::DbConfig;

    #[tokio::test]
    async fn generates_then_loads() {
        let dir = TempDir::new().unwrap();
        let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

        let id1 = ensure(&db).await.unwrap();
        let id2 = ensure(&db).await.unwrap();

        assert_eq!(id1.y7_id, id2.y7_id);
        assert_eq!(id1.signing_key.to_bytes(), id2.signing_key.to_bytes());
    }
}
