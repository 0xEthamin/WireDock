//! Domain layer: pure business core of wiredock.
//!
//! This layer defines identifiers, state, commands, events, errors, and the
//! I/O abstraction traits (`Connector`, `Listener`, `AsyncStream`). It MUST
//! NOT depend on `tokio::net` or any concrete transport. `tokio::io` traits
//! are allowed; `tokio::sync` channels are allowed. Everything else that
//! touches the OS lives in `infra`.

pub mod command;
pub mod endpoint;
pub mod error;
pub mod event;
pub mod hex;
pub mod id;
pub mod io;
pub mod logger;
pub mod registry;
pub mod state;
pub mod transport;

pub use error::DomainError;