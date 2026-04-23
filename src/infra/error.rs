//! Infra-layer errors and conversion to DomainError at the boundary.

use thiserror::Error;

use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum InfraError
{
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("address already in use: {0}")]
    AddressInUse(String),

    #[error("peercred failed: {0}")]
    PeerCred(String),

    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),
}

impl From<InfraError> for DomainError
{
    fn from(e: InfraError) -> Self
    {
        match &e
        {
            InfraError::AddressInUse(s) => DomainError::IdAlreadyInUse(s.clone()),
            _                           => DomainError::Io(e.to_string()),
        }
    }
}