//! Error boundary shared by the local API, persistence, media, and worker flows.

use std::io;

use thiserror::Error;

/// Describes a recoverable desktop-domain failure returned to the existing UI contract.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    External(String),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Result used by the Rust domain boundary rather than leaking provider or SQLite errors.
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// Match a domain failure to the HTTP-shaped status the browser UI already understands.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::External(_) | Self::Sql(_) | Self::Io(_) | Self::Json(_) => 500,
        }
    }
}
