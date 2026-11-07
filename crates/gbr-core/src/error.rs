use std::path::PathBuf;

/// Unified error type for the Git Backup Restore core engine.
#[derive(Debug, thiserror::Error)]
pub enum GbrError {
    #[error("GitHub authentication required: {0}")]
    AuthRequired(String),

    #[error("GitHub API error (HTTP {status}): {message}")]
    GitHub { status: u16, message: String },

    #[error("GitHub API rate limit exceeded; resets at {reset_at}")]
    RateLimited { reset_at: String },

    #[error("git command failed: {0}")]
    Git(String),

    #[error("backup not found: {0}")]
    BackupNotFound(String),

    #[error("backup verification failed: {0}")]
    Verification(String),

    #[error("destination conflict: {0}")]
    Conflict(String),

    #[error("insufficient disk space: need {needed} bytes, {available} bytes available")]
    DiskSpace { needed: u64, available: u64 },

    #[error("unsafe path rejected: {0}")]
    UnsafePath(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("credential store error: {0}")]
    Credential(String),

    #[error("database error: {0}")]
    Db(String),

    #[error("io error on {path}: {source}")]
    IoPath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GbrError>;

impl GbrError {
    pub fn io_at(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        GbrError::IoPath {
            path: path.into(),
            source,
        }
    }
}

impl From<sqlx::Error> for GbrError {
    fn from(e: sqlx::Error) -> Self {
        GbrError::Db(e.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for GbrError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        GbrError::Db(format!("migration: {e}"))
    }
}

impl From<reqwest::Error> for GbrError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            GbrError::GitHub {
                status: 0,
                message: "request timed out".into(),
            }
        } else {
            GbrError::GitHub {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                message: e.to_string(),
            }
        }
    }
}

impl From<serde_json::Error> for GbrError {
    fn from(e: serde_json::Error) -> Self {
        GbrError::Other(format!("json: {e}"))
    }
}
