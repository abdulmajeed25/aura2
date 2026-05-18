use serde::Serialize;
use thiserror::Error;

/// Internal error type for backend operations.
#[derive(Debug, Error)]
pub enum AuraError {
    #[error("vault is not open")]
    NoVault,

    #[error("path is outside the active vault: {0}")]
    PathOutsideVault(String),

    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("{0}")]
    Other(String),
}

impl From<libsql::Error> for AuraError {
    fn from(e: libsql::Error) -> Self {
        AuraError::Db(e.to_string())
    }
}

impl From<anyhow::Error> for AuraError {
    fn from(e: anyhow::Error) -> Self {
        AuraError::Other(e.to_string())
    }
}

/// Wire-friendly result returned from `#[tauri::command]` functions.
pub type CmdResult<T> = Result<T, AuraErrorWire>;

/// Serializable error representation sent to the frontend.
#[derive(Debug, Serialize)]
pub struct AuraErrorWire {
    pub code: String,
    pub message: String,
}

impl From<AuraError> for AuraErrorWire {
    fn from(err: AuraError) -> Self {
        let code = match &err {
            AuraError::NoVault => "no_vault",
            AuraError::PathOutsideVault(_) => "path_outside_vault",
            AuraError::FileNotFound(_) => "file_not_found",
            AuraError::InvalidPath(_) => "invalid_path",
            AuraError::Io(_) => "io",
            AuraError::Db(_) => "db",
            AuraError::Other(_) => "other",
        }
        .to_string();
        AuraErrorWire {
            code,
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for AuraErrorWire {
    fn from(e: std::io::Error) -> Self {
        AuraError::from(e).into()
    }
}

impl From<libsql::Error> for AuraErrorWire {
    fn from(e: libsql::Error) -> Self {
        AuraError::from(e).into()
    }
}

impl From<anyhow::Error> for AuraErrorWire {
    fn from(e: anyhow::Error) -> Self {
        AuraError::from(e).into()
    }
}
