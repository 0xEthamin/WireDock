//! Typed domain errors.
//!
//! One enum for the whole domain layer. `InfraError` from the infra layer
//! converts into `DomainError` via a `From` impl defined in the infra layer
//! so that the domain never names concrete transports.

use thiserror::Error;

/// Errors that can originate from or surface through the domain layer.
#[derive(Debug, Error)]
pub enum DomainError
{
    /// The user referenced an identifier that is not currently registered.
    #[error("unknown id: {0}")]
    UnknownId(String),

    /// The user tried to open a connection whose local id is already bound.
    #[error("id already in use: {0}")]
    IdAlreadyInUse(String),

    /// The user tried to label with a name already taken by another id.
    #[error("label already in use: {0}")]
    LabelAlreadyInUse(String),

    /// Hex payload parsing failed.
    #[error("invalid hex payload: {0}")]
    InvalidHex(String),

    /// Send to a server that currently has no accepted children.
    #[error("server {0} has no active accepted connections")]
    ServerHasNoChildren(String),

    /// Send targeted an empty payload.
    #[error("empty payload")]
    EmptyPayload,

    /// Wrapping of underlying infra (I/O) failures.
    #[error("io error: {0}")]
    Io(String),

    /// Generic logical error for cases that do not fit any of the above.
    #[error("logic error: {0}")]
    Logic(String),
}