//! SQLite persistence, master-DEK file, and field encryption for Y7KE.
//!
//! V1 stores the master DEK as a 32-byte file under the OS app-data directory
//! (mode 0600 on Unix). OS-keyring integration is V2.

// sqlx's `query_as::<_, (T, T, ...)>` tuple-row pattern is intentionally used
// across DAOs in place of single-use `FromRow` structs.
#![allow(clippy::type_complexity)]

pub mod dao;
pub mod db;
pub mod dek;
pub mod field_crypto;

pub use db::{now_ms, Db, DbConfig};
pub use dek::{Dek, DekError};
