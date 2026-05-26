//! Top-level error type. Every public function returns `Result<_, AppError>`.

use std::io;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("invalid Y7 identifier: {0}")]
    InvalidY7Id(String),

    #[error("invalid public key")]
    InvalidPublicKey,

    #[error("invalid signature")]
    InvalidSignature,

    #[error("cryptography failed: {0}")]
    Crypto(String),

    #[error("storage failed: {0}")]
    Storage(String),

    #[error("network failed: {0}")]
    Network(String),

    #[error("not found")]
    NotFound,

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error("serialization: {0}")]
    Serialization(String),
}

impl AppError {
    pub fn crypto(msg: impl Into<String>) -> Self {
        Self::Crypto(msg.into())
    }

    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
