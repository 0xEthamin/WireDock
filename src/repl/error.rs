//! REPL-layer errors.

use thiserror::Error;

use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum ReplError
{
    #[error("parse error: {0}")]
    Parse(String),

    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}