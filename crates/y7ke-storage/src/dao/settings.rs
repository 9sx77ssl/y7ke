//! Single-row settings store. JSON-blobbed payload keeps schema churn off
//! the DB layer — adding a new dial-mode flag is a struct edit, not a
//! migration.

use sqlx::SqlitePool;
use y7ke_core::error::{AppError, Result};
use y7ke_core::settings::Settings;

use crate::db::now_ms;

pub struct SettingsDao<'db> {
    pool: &'db SqlitePool,
}

impl<'db> SettingsDao<'db> {
    pub(crate) fn new(pool: &'db SqlitePool) -> Self {
        Self { pool }
    }

    /// Read the single settings row. Migration 0004 seeds defaults, so a
    /// fresh install also returns `Ok(Settings::default())`.
    pub async fn get(&self) -> Result<Settings> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT payload_json FROM settings WHERE id = 1")
                .fetch_optional(self.pool)
                .await
                .map_err(|e| AppError::storage(format!("settings.get: {e}")))?;
        let json = match row {
            Some((j,)) => j,
            None => return Ok(Settings::default()),
        };
        serde_json::from_str(&json)
            .map_err(|e| AppError::storage(format!("settings.get decode: {e}")))
    }

    /// Serialize `settings` and overwrite row 1 with the current timestamp.
    pub async fn update(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)
            .map_err(|e| AppError::storage(format!("settings.update encode: {e}")))?;
        sqlx::query(
            "INSERT INTO settings (id, payload_json, updated_at) VALUES (1, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json, updated_at = excluded.updated_at",
        )
        .bind(&json)
        .bind(now_ms())
        .execute(self.pool)
        .await
        .map_err(|e| AppError::storage(format!("settings.update: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::{Db, DbConfig};
    use tempfile::TempDir;
    use y7ke_core::settings::{DialModes, Settings};

    #[tokio::test]
    async fn round_trips_default_update_get() {
        let dir = TempDir::new().unwrap();
        let db = Db::open(DbConfig::in_dir(dir.path())).await.unwrap();

        let initial = db.settings().get().await.unwrap();
        assert_eq!(initial.dial_modes, DialModes::default());
        assert!(initial.extra_bootstraps.is_empty());

        let updated = Settings {
            dial_modes: DialModes {
                lan: false,
                internet: true,
                relay: false,
                p2p: true,
            },
            extra_bootstraps: vec!["/ip4/127.0.0.1/tcp/9999/p2p/12D3KooWAaAaAaAa".into()],
        };
        db.settings().update(&updated).await.unwrap();

        let got = db.settings().get().await.unwrap();
        assert_eq!(got.dial_modes, updated.dial_modes);
        assert_eq!(got.extra_bootstraps, updated.extra_bootstraps);
    }
}
