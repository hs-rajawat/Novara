//! Single error type for the whole crate. Serializable so it can cross the
//! Tauri IPC boundary as a structured object (frontend can switch on `code`).

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("scanner error: {0}")]
    Scanner(String),

    #[error("metadata provider error: {0}")]
    Metadata(String),

    #[error("save manager error: {0}")]
    SaveMgr(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "db",
            Self::Migrate(_) => "migrate",
            Self::Io(_) => "io",
            Self::Serde(_) => "serde",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid",
            Self::Scanner(_) => "scanner",
            Self::Metadata(_) => "metadata",
            // Wire contract, not a module path. The frontend matches on this
            // string (`src/lib/errors.ts`, asserted in `errors.test.ts`), so it
            // stays `save_mgr` even though the module is now `saves::vault`.
            // Renaming it is a frontend-visible change, not a refactor.
            Self::SaveMgr(_) => "save_mgr",
            Self::Other(_) => "other",
        }
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WireError {
            code: self.code(),
            message: self.to_string(),
        }
        .serialize(s)
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
