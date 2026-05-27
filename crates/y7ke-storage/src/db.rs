//! Database handle. Owns the sqlx connection pool and the loaded DEK.

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use y7ke_core::crypto::SymmetricKey;
use y7ke_core::error::{AppError, Result};

use crate::dao;
use crate::dek::Dek;

/// Configuration for opening the database. Use `DbConfig::default_for_app`
/// to get OS-appropriate paths (`~/.local/share/y7ke/` on Linux, etc.).
#[derive(Clone, Debug)]
pub struct DbConfig {
    pub db_path: PathBuf,
    pub dek_path: PathBuf,
}

impl DbConfig {
    /// Resolve the standard per-OS app data directory and place
    /// `y7ke.db` + `master.dek` inside it.
    pub fn default_for_app() -> Result<Self> {
        let proj = directories::ProjectDirs::from("com", "y7ke", "Y7KE")
            .ok_or_else(|| AppError::storage("could not resolve app data directory"))?;
        let dir = proj.data_dir();
        Ok(Self {
            db_path: dir.join("y7ke.db"),
            dek_path: dir.join("master.dek"),
        })
    }

    /// Both DB and DEK inside `dir`. Useful for tests and multi-instance
    /// local harnesses.
    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            db_path: dir.join("y7ke.db"),
            dek_path: dir.join("master.dek"),
        }
    }
}

/// Y7KE persistence layer: a connection pool plus the master DEK.
pub struct Db {
    pool: SqlitePool,
    dek: SymmetricKey,
    dek_path: PathBuf,
}

impl Db {
    /// Open the database, loading or generating the DEK, and applying any
    /// pending migrations.
    pub async fn open(config: DbConfig) -> Result<Self> {
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::storage(format!("create_dir_all: {e}")))?;
        }

        let dek = Dek::load_or_create(&config.dek_path)
            .map_err(|e| AppError::storage(format!("load DEK: {e}")))?;

        let options = SqliteConnectOptions::new()
            .filename(&config.db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|e| AppError::storage(format!("connect: {e}")))?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::storage(format!("migrate: {e}")))?;

        tracing::info!(
            db = %config.db_path.display(),
            dek = %dek.path().display(),
            "y7ke storage opened",
        );

        Ok(Self {
            pool,
            dek: dek.key().clone(),
            dek_path: dek.path().to_path_buf(),
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn dek(&self) -> &SymmetricKey {
        &self.dek
    }

    pub fn dek_path(&self) -> &Path {
        &self.dek_path
    }

    pub fn users(&self) -> dao::users::UsersDao<'_> {
        dao::users::UsersDao::new(&self.pool, &self.dek)
    }

    pub fn contacts(&self) -> dao::contacts::ContactsDao<'_> {
        dao::contacts::ContactsDao::new(&self.pool)
    }

    pub fn requests(&self) -> dao::requests::RequestsDao<'_> {
        dao::requests::RequestsDao::new(&self.pool, &self.dek)
    }

    pub fn messages(&self) -> dao::messages::MessagesDao<'_> {
        dao::messages::MessagesDao::new(&self.pool)
    }

    pub fn sessions(&self) -> dao::sessions::SessionsDao<'_> {
        dao::sessions::SessionsDao::new(&self.pool, &self.dek)
    }

    pub fn sync_queue(&self) -> dao::sync_queue::SyncQueueDao<'_> {
        dao::sync_queue::SyncQueueDao::new(&self.pool)
    }

    pub fn peer_state(&self) -> dao::peer_state::PeerStateDao<'_> {
        dao::peer_state::PeerStateDao::new(&self.pool)
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
