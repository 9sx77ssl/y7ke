//! SQLite persistence, master-DEK file, and field encryption for Y7KE.
//!
//! V1 stores the master DEK as a 32-byte file under the OS app-data directory
//! (mode 0600 on Unix). OS-keyring integration is V2.
