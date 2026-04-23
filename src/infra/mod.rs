//! Infra layer: concrete adapters for tokio::net, filesystem, stderr.
//!
//! This is the only place where `tokio::net`, `std::fs`, `std::os::unix`,
//! and platform-specific code is allowed. The domain imports only the
//! traits implemented here.

pub mod connector;
pub mod error;
pub mod fs_cleanup;
pub mod listener;
pub mod logger;
pub mod peercred;

pub use connector::TokioConnector;
pub use listener::TokioListener;
pub use logger::StderrLogger;