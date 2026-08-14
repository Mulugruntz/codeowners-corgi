use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CorgiError>;

#[derive(Debug, Error)]
pub enum CorgiError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error("invalid UTF-8 path: {0}")]
    Utf8Path(PathBuf),
    #[error("failed to parse CODEOWNERS line: {0}")]
    Parse(String),
    #[error("{0}")]
    Message(String),
}
